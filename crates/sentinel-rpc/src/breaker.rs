//! The circuit breaker (`docs/rpc-strategy.md` §6): `Closed / Degraded /
//! Open / HalfOpen`, with the exact transition rules CB-1..CB-6.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakerState {
    Closed,
    Degraded,
    Open,
    HalfOpen,
}

impl BreakerState {
    pub fn as_str(self) -> &'static str {
        match self {
            BreakerState::Closed => "closed",
            BreakerState::Degraded => "degraded",
            BreakerState::Open => "open",
            BreakerState::HalfOpen => "half_open",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BreakerConfig {
    /// CB-1: minimum number of samples in the window before a failure
    /// ratio can trip the breaker. Two startup failures must never trip a
    /// ratio-based breaker.
    pub min_sample_size: u32,
    /// Failure ratio (0.0..=1.0) over the window that trips `Open`.
    pub failure_ratio_threshold: f64,
    /// Rolling window size, in samples, over which the ratio is computed.
    pub window_size: u32,
    /// p95 latency threshold (ms) that trips `Degraded` when exceeded.
    pub p95_latency_degraded_ms: u64,
    /// Time `Open` must elapse before allowing a `HalfOpen` probe.
    pub open_cooldown: Duration,
    /// Consecutive half-open probe successes required to close.
    pub half_open_success_threshold: u32,
}

impl Default for BreakerConfig {
    fn default() -> Self {
        BreakerConfig {
            min_sample_size: 10,
            failure_ratio_threshold: 0.5,
            window_size: 50,
            p95_latency_degraded_ms: 2_000,
            open_cooldown: Duration::from_secs(30),
            half_open_success_threshold: 3,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Sample {
    Success { latency: Duration },
    Failure,
}

/// One provider's breaker. `record_*` methods are the only mutation
/// surface; `state()` is read by the selector on every call.
pub struct Breaker {
    config: BreakerConfig,
    state: BreakerState,
    window: VecDeque<Sample>,
    opened_at: Option<Instant>,
    half_open_consecutive_successes: u32,
    /// CB-5: every transition is recorded as an event so the caller
    /// (health.rs / metrics) can log + alert.
    transitions: Vec<(BreakerState, BreakerState, &'static str)>,
}

impl Breaker {
    pub fn new(config: BreakerConfig) -> Self {
        Breaker {
            config,
            state: BreakerState::Closed,
            window: VecDeque::new(),
            opened_at: None,
            half_open_consecutive_successes: 0,
            transitions: Vec::new(),
        }
    }

    pub fn state(&self) -> BreakerState {
        self.state
    }

    pub fn transitions(&self) -> &[(BreakerState, BreakerState, &'static str)] {
        &self.transitions
    }

    fn transition(&mut self, to: BreakerState, reason: &'static str) {
        if self.state != to {
            self.transitions.push((self.state, to, reason));
            tracing::warn!(
                from = self.state.as_str(),
                to = to.as_str(),
                reason,
                "circuit breaker state transition"
            );
            self.state = to;
        }
    }

    fn push_sample(&mut self, sample: Sample) {
        self.window.push_back(sample);
        while self.window.len() > self.config.window_size as usize {
            self.window.pop_front();
        }
    }

    fn failure_ratio(&self) -> Option<(f64, u32)> {
        let n = self.window.len() as u32;
        if n == 0 {
            return None;
        }
        let failures = self
            .window
            .iter()
            .filter(|s| matches!(s, Sample::Failure))
            .count() as f64;
        Some((failures / n as f64, n))
    }

    fn p95_latency_ms(&self) -> Option<u64> {
        let mut latencies: Vec<u64> = self
            .window
            .iter()
            .filter_map(|s| match s {
                Sample::Success { latency } => Some(latency.as_millis() as u64),
                Sample::Failure => None,
            })
            .collect();
        if latencies.is_empty() {
            return None;
        }
        latencies.sort_unstable();
        let idx = ((latencies.len() as f64) * 0.95).ceil() as usize;
        Some(latencies[idx.saturating_sub(1).min(latencies.len() - 1)])
    }

    /// Records a successful call. In `HalfOpen`, this is a probe result;
    /// CB-3 requires the probe itself be cheap/read-only, which is enforced
    /// by the caller only ever issuing `getSlot`/`getHealth` while
    /// `HalfOpen` (see `pool.rs`).
    pub fn record_success(&mut self, latency: Duration) {
        match self.state {
            BreakerState::HalfOpen => {
                self.half_open_consecutive_successes += 1;
                if self.half_open_consecutive_successes >= self.config.half_open_success_threshold {
                    self.window.clear();
                    self.opened_at = None;
                    self.half_open_consecutive_successes = 0;
                    self.transition(BreakerState::Closed, "half_open_probes_succeeded");
                }
            }
            _ => {
                self.push_sample(Sample::Success { latency });
                if let Some(p95) = self.p95_latency_ms() {
                    if p95 > self.config.p95_latency_degraded_ms
                        && self.state == BreakerState::Closed
                    {
                        self.transition(BreakerState::Degraded, "p95_latency_threshold_exceeded");
                    } else if p95 <= self.config.p95_latency_degraded_ms
                        && self.state == BreakerState::Degraded
                    {
                        if let Some((ratio, n)) = self.failure_ratio() {
                            if n < self.config.min_sample_size
                                || ratio <= self.config.failure_ratio_threshold
                            {
                                self.transition(BreakerState::Closed, "metrics_recovered");
                            }
                        }
                    }
                }
                // A success can be the sample that pushes the window past
                // `min_sample_size` (e.g. failures then a success filling
                // the last slot) — the ratio trip must be evaluated on
                // every sample, not only on failures.
                self.evaluate_ratio_trip();
            }
        }
    }

    /// CB-1: if the window has reached the minimum sample size and the
    /// failure ratio exceeds the threshold, opens the breaker. Shared by
    /// both `record_success` and `record_failure` so the trip is never
    /// missed depending on which kind of sample happens to complete the
    /// window.
    fn evaluate_ratio_trip(&mut self) {
        if matches!(self.state, BreakerState::Open | BreakerState::HalfOpen) {
            return;
        }
        if let Some((ratio, n)) = self.failure_ratio() {
            if n >= self.config.min_sample_size && ratio > self.config.failure_ratio_threshold {
                self.opened_at = Some(Instant::now());
                let reason = if self.state == BreakerState::Degraded {
                    "failures_escalated_from_degraded"
                } else {
                    "failure_ratio_threshold_exceeded"
                };
                self.transition(BreakerState::Open, reason);
            }
        }
    }

    /// Marks the provider `Degraded` due to detected staleness (FI-09),
    /// independent of ordinary failure-ratio accounting.
    pub fn record_staleness(&mut self) {
        if self.state == BreakerState::Closed {
            self.transition(BreakerState::Degraded, "staleness_detected");
        }
    }

    /// Records a failed call and evaluates CB-1 (minimum sample size) /
    /// escalation from `Degraded` to `Open`.
    pub fn record_failure(&mut self) {
        match self.state {
            BreakerState::HalfOpen => {
                self.half_open_consecutive_successes = 0;
                self.window.clear();
                self.opened_at = Some(Instant::now());
                self.transition(BreakerState::Open, "half_open_probe_failed");
            }
            _ => {
                self.push_sample(Sample::Failure);
                // CB-1: minimum sample size required before a ratio can
                // trip anything. Two startup failures alone (n < min) must
                // never open the breaker — enforced inside
                // `evaluate_ratio_trip`.
                self.evaluate_ratio_trip();
            }
        }
    }

    /// CB-4: content divergence trips the breaker immediately, regardless
    /// of sample size — a provider returning different bytes for the same
    /// natural key is faulty or lying, and neither is something to average
    /// out.
    pub fn record_content_divergence(&mut self) {
        self.opened_at = Some(Instant::now());
        self.window.clear();
        self.transition(BreakerState::Open, "content_divergence");
    }

    /// Called periodically (or lazily before selection) to let a cooled-down
    /// `Open` breaker advance to `HalfOpen`.
    pub fn tick(&mut self) {
        if self.state == BreakerState::Open {
            if let Some(opened_at) = self.opened_at {
                if opened_at.elapsed() >= self.config.open_cooldown {
                    self.half_open_consecutive_successes = 0;
                    self.transition(BreakerState::HalfOpen, "cooldown_elapsed");
                }
            }
        }
    }

    /// CB-2: `Open` excludes the provider from all classes. `Degraded`
    /// excludes it from `Execution` and `RealtimeCompleteness` only.
    pub fn eligible_for(&self, class: crate::provider::RequestClass) -> bool {
        use crate::provider::RequestClass::*;
        match self.state {
            BreakerState::Open => false,
            BreakerState::HalfOpen => false, // only the probe call itself uses a half-open provider; see pool.rs
            BreakerState::Degraded => !matches!(class, Execution | RealtimeCompleteness),
            BreakerState::Closed => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::RequestClass;

    #[test]
    fn two_startup_failures_do_not_trip_a_ratio_breaker_cb1() {
        let mut b = Breaker::new(BreakerConfig::default());
        b.record_failure();
        b.record_failure();
        assert_eq!(
            b.state(),
            BreakerState::Closed,
            "CB-1 violated: {:?}",
            b.transitions()
        );
    }

    #[test]
    fn failure_ratio_above_threshold_with_min_sample_opens_cb1() {
        let mut b = Breaker::new(BreakerConfig {
            min_sample_size: 10,
            failure_ratio_threshold: 0.5,
            window_size: 20,
            ..Default::default()
        });
        for _ in 0..6 {
            b.record_failure();
        }
        for _ in 0..4 {
            b.record_success(Duration::from_millis(10));
        }
        assert_eq!(b.state(), BreakerState::Open);
    }

    #[test]
    fn content_divergence_trips_immediately_regardless_of_sample_size_cb4() {
        let mut b = Breaker::new(BreakerConfig::default());
        b.record_success(Duration::from_millis(5));
        b.record_content_divergence();
        assert_eq!(b.state(), BreakerState::Open);
    }

    #[test]
    fn open_excludes_every_class_cb2() {
        let mut b = Breaker::new(BreakerConfig::default());
        for _ in 0..20 {
            b.record_failure();
        }
        assert_eq!(b.state(), BreakerState::Open);
        for class in RequestClass::ALL {
            assert!(!b.eligible_for(class), "Open must exclude {class:?}");
        }
    }

    #[test]
    fn degraded_excludes_only_execution_and_realtime_cb2() {
        let mut b = Breaker::new(BreakerConfig::default());
        b.record_staleness();
        assert_eq!(b.state(), BreakerState::Degraded);
        assert!(!b.eligible_for(RequestClass::Execution));
        assert!(!b.eligible_for(RequestClass::RealtimeCompleteness));
        assert!(b.eligible_for(RequestClass::GapRepair));
        assert!(b.eligible_for(RequestClass::Backfill));
        assert!(b.eligible_for(RequestClass::ScheduledScan));
    }

    #[test]
    fn half_open_after_cooldown_then_closes_on_consecutive_successes() {
        let mut b = Breaker::new(BreakerConfig {
            open_cooldown: Duration::from_millis(1),
            half_open_success_threshold: 2,
            ..Default::default()
        });
        for _ in 0..20 {
            b.record_failure();
        }
        assert_eq!(b.state(), BreakerState::Open);
        std::thread::sleep(Duration::from_millis(5));
        b.tick();
        assert_eq!(b.state(), BreakerState::HalfOpen);
        b.record_success(Duration::from_millis(1));
        assert_eq!(
            b.state(),
            BreakerState::HalfOpen,
            "needs 2 consecutive successes"
        );
        b.record_success(Duration::from_millis(1));
        assert_eq!(b.state(), BreakerState::Closed);
    }

    #[test]
    fn half_open_probe_failure_reopens() {
        let mut b = Breaker::new(BreakerConfig {
            open_cooldown: Duration::from_millis(1),
            ..Default::default()
        });
        for _ in 0..20 {
            b.record_failure();
        }
        std::thread::sleep(Duration::from_millis(5));
        b.tick();
        assert_eq!(b.state(), BreakerState::HalfOpen);
        b.record_failure();
        assert_eq!(b.state(), BreakerState::Open);
    }

    #[test]
    fn every_transition_is_a_recorded_event_cb5() {
        let mut b = Breaker::new(BreakerConfig::default());
        b.record_content_divergence();
        assert!(!b.transitions().is_empty());
        let (from, to, reason) = b.transitions()[0];
        assert_eq!(from, BreakerState::Closed);
        assert_eq!(to, BreakerState::Open);
        assert_eq!(reason, "content_divergence");
    }
}
