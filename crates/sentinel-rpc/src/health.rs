//! Per-provider health tracking, selection scoring, and persistence into the
//! Phase-2 canonical `provider_health` table (`docs/data-model.md` §4,
//! `docs/rpc-strategy.md` §9). This module owns the *metrics*; `breaker.rs`
//! owns the *state machine* they feed.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use sentinel_db::queries::{self, ProviderHealthUpdate};

use crate::breaker::{Breaker, BreakerConfig, BreakerState};
use crate::provider::{ProviderId, RequestClass};

/// A public, cheap-to-clone snapshot of one provider's health — what
/// `RpcPool::health()` returns (`docs/rpc-strategy.md` §1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub provider_id: String,
    pub breaker_state: String,
    pub requests: i64,
    pub errors: i64,
    pub timeouts: i64,
    pub rate_limited: i64,
    pub stale_rejections: i64,
    pub divergence_events: i64,
    pub p50_ms: Option<i64>,
    pub p95_ms: Option<i64>,
    pub last_error_code: Option<String>,
    /// WS reconnect count (W-5) — surfaced here so the same `health()`
    /// snapshot metrics/dashboards read already carries it, rather than a
    /// second parallel accessor.
    pub reconnects: i64,
}

/// Bounded latency sample buffer — never unbounded (AGENTS.md §7.6).
const MAX_LATENCY_SAMPLES: usize = 512;

struct Counters {
    requests: AtomicI64,
    errors: AtomicI64,
    timeouts: AtomicI64,
    rate_limited: AtomicI64,
    stale_rejections: AtomicI64,
    divergence_events: AtomicI64,
    reconnects: AtomicI64,
    last_error_code: Mutex<Option<String>>,
    latencies_ms: Mutex<Vec<u64>>,
    last_known_context_slot: AtomicU64,
}

impl Default for Counters {
    fn default() -> Self {
        Counters {
            requests: AtomicI64::new(0),
            errors: AtomicI64::new(0),
            timeouts: AtomicI64::new(0),
            rate_limited: AtomicI64::new(0),
            stale_rejections: AtomicI64::new(0),
            divergence_events: AtomicI64::new(0),
            reconnects: AtomicI64::new(0),
            last_error_code: Mutex::new(None),
            latencies_ms: Mutex::new(Vec::new()),
            last_known_context_slot: AtomicU64::new(0),
        }
    }
}

/// One provider's health tracker: rolling counters plus the breaker they
/// feed. Owned by the pool, one per configured provider.
pub struct ProviderHealthTracker {
    provider_id: ProviderId,
    window_start: Mutex<DateTime<Utc>>,
    counters: Counters,
    breaker: Mutex<Breaker>,
}

impl ProviderHealthTracker {
    pub fn new(provider_id: ProviderId, breaker_config: BreakerConfig) -> Self {
        ProviderHealthTracker {
            provider_id,
            window_start: Mutex::new(Utc::now()),
            counters: Counters::default(),
            breaker: Mutex::new(Breaker::new(breaker_config)),
        }
    }

    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    pub fn record_success(&self, latency_ms: u64) {
        self.counters.requests.fetch_add(1, Ordering::Relaxed);
        let mut lat = self
            .counters
            .latencies_ms
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        lat.push(latency_ms);
        if lat.len() > MAX_LATENCY_SAMPLES {
            lat.remove(0);
        }
        drop(lat);
        self.breaker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record_success(std::time::Duration::from_millis(latency_ms));
    }

    pub fn record_failure(&self, error_code: &str, is_timeout: bool, is_rate_limited: bool) {
        self.counters.requests.fetch_add(1, Ordering::Relaxed);
        self.counters.errors.fetch_add(1, Ordering::Relaxed);
        if is_timeout {
            self.counters.timeouts.fetch_add(1, Ordering::Relaxed);
        }
        if is_rate_limited {
            self.counters.rate_limited.fetch_add(1, Ordering::Relaxed);
        }
        *self
            .counters
            .last_error_code
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(error_code.to_string());
        self.breaker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record_failure();
    }

    pub fn record_stale(&self) {
        self.counters.requests.fetch_add(1, Ordering::Relaxed);
        self.counters
            .stale_rejections
            .fetch_add(1, Ordering::Relaxed);
        self.breaker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record_staleness();
    }

    pub fn record_content_divergence(&self) {
        self.counters
            .divergence_events
            .fetch_add(1, Ordering::Relaxed);
        self.breaker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record_content_divergence();
    }

    pub fn record_reconnect(&self) {
        self.counters.reconnects.fetch_add(1, Ordering::Relaxed);
    }

    pub fn reconnects(&self) -> i64 {
        self.counters.reconnects.load(Ordering::Relaxed)
    }

    pub fn record_known_context_slot(&self, slot: u64) {
        self.counters
            .last_known_context_slot
            .fetch_max(slot, Ordering::Relaxed);
    }

    pub fn last_known_context_slot(&self) -> u64 {
        self.counters
            .last_known_context_slot
            .load(Ordering::Relaxed)
    }

    pub fn breaker_state(&self) -> BreakerState {
        self.breaker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .state()
    }

    /// Advances a cooled-down `Open` breaker to `HalfOpen`. Called by the
    /// pool before every selection.
    pub fn tick_breaker(&self) {
        self.breaker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .tick();
    }

    pub fn eligible_for(&self, class: RequestClass) -> bool {
        self.breaker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .eligible_for(class)
    }

    pub fn record_half_open_probe_success(&self, latency_ms: u64) {
        self.breaker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record_success(std::time::Duration::from_millis(latency_ms));
    }

    pub fn record_half_open_probe_failure(&self) {
        self.breaker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record_failure();
    }

    fn percentile_ms(&self, pct: f64) -> Option<i64> {
        let lat = self
            .counters
            .latencies_ms
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if lat.is_empty() {
            return None;
        }
        let mut sorted = lat.clone();
        sorted.sort_unstable();
        let idx = ((sorted.len() as f64) * pct).ceil() as usize;
        Some(sorted[idx.saturating_sub(1).min(sorted.len() - 1)] as i64)
    }

    pub fn p50_ms(&self) -> Option<i64> {
        self.percentile_ms(0.50)
    }

    pub fn p95_ms(&self) -> Option<i64> {
        self.percentile_ms(0.95)
    }

    fn success_rate(&self) -> f64 {
        let requests = self.counters.requests.load(Ordering::Relaxed);
        if requests == 0 {
            return 1.0; // no evidence of failure yet — do not penalize a fresh provider
        }
        let errors = self.counters.errors.load(Ordering::Relaxed);
        1.0 - (errors as f64 / requests as f64)
    }

    /// `score = f(success_rate, p95_latency, budget_headroom,
    /// context_slot_freshness)` (`docs/rpc-strategy.md` §5.1). Higher is
    /// better. `budget_headroom` and `known_head_slot` are supplied by the
    /// caller (pool) since they are pool-scoped, not provider-scoped.
    pub fn score(&self, budget_headroom: f64, known_head_slot: u64) -> f64 {
        let success = self.success_rate();
        let p95 = self.p95_ms().unwrap_or(0) as f64;
        let latency_score = 1.0 / (1.0 + p95 / 1000.0); // decays smoothly, never negative/zero
        let freshness_score = if known_head_slot == 0 {
            1.0
        } else {
            let lag = known_head_slot.saturating_sub(self.last_known_context_slot());
            1.0 / (1.0 + lag as f64 / 50.0)
        };
        let breaker_penalty = match self.breaker_state() {
            BreakerState::Closed => 1.0,
            BreakerState::Degraded => 0.4,
            BreakerState::HalfOpen | BreakerState::Open => 0.0,
        };
        (0.4 * success
            + 0.25 * latency_score
            + 0.15 * budget_headroom.clamp(0.0, 1.0)
            + 0.2 * freshness_score)
            * breaker_penalty
    }

    pub fn snapshot(&self) -> ProviderHealth {
        ProviderHealth {
            provider_id: self.provider_id.0.clone(),
            breaker_state: self.breaker_state().as_str().to_string(),
            requests: self.counters.requests.load(Ordering::Relaxed),
            errors: self.counters.errors.load(Ordering::Relaxed),
            timeouts: self.counters.timeouts.load(Ordering::Relaxed),
            rate_limited: self.counters.rate_limited.load(Ordering::Relaxed),
            stale_rejections: self.counters.stale_rejections.load(Ordering::Relaxed),
            divergence_events: self.counters.divergence_events.load(Ordering::Relaxed),
            p50_ms: self.p50_ms(),
            p95_ms: self.p95_ms(),
            last_error_code: self
                .counters
                .last_error_code
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
            reconnects: self.reconnects(),
        }
    }

    pub fn window_start(&self) -> DateTime<Utc> {
        *self.window_start.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Persists the current window's snapshot into `provider_health`
    /// (Phase-2 canonical table, owner `sentinel-rpc`) via `sentinel-db`'s
    /// typed access layer — no new datastore, no bypass of the existing
    /// ownership rules (`phase-03-rpc.md` §20).
    pub async fn persist<'e, E>(&self, exec: E) -> Result<(), sentinel_db::queries::QueryError>
    where
        E: sqlx::PgExecutor<'e>,
    {
        let snap = self.snapshot();
        queries::upsert_provider_health(
            exec,
            ProviderHealthUpdate {
                provider_id: &snap.provider_id,
                window_start: self.window_start(),
                requests: snap.requests,
                errors: snap.errors,
                timeouts: snap.timeouts,
                rate_limited: snap.rate_limited,
                p50_ms: snap.p50_ms,
                p95_ms: snap.p95_ms,
                breaker_state: &snap.breaker_state,
                last_error_code: snap.last_error_code.as_deref(),
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker() -> ProviderHealthTracker {
        ProviderHealthTracker::new(ProviderId("p1".into()), BreakerConfig::default())
    }

    #[test]
    fn healthy_provider_scores_higher_than_degraded_one() {
        let healthy = tracker();
        let degraded = tracker();
        for _ in 0..5 {
            healthy.record_success(10);
        }
        degraded.record_stale();
        assert!(
            healthy.score(1.0, 0) > degraded.score(1.0, 0),
            "healthy={} degraded={}",
            healthy.score(1.0, 0),
            degraded.score(1.0, 0)
        );
    }

    #[test]
    fn open_provider_scores_zero() {
        let t = tracker();
        for _ in 0..20 {
            t.record_failure("SEN-RPC-001", false, false);
        }
        assert_eq!(t.breaker_state(), BreakerState::Open);
        assert_eq!(t.score(1.0, 0), 0.0);
    }

    #[test]
    fn snapshot_reflects_recorded_counters() {
        let t = tracker();
        t.record_success(5);
        t.record_failure("SEN-RPC-429", false, true);
        let snap = t.snapshot();
        assert_eq!(snap.requests, 2);
        assert_eq!(snap.errors, 1);
        assert_eq!(snap.rate_limited, 1);
        assert_eq!(snap.last_error_code.as_deref(), Some("SEN-RPC-429"));
    }
}
