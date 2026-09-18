//! Client-side request budgets, enforced **before** a request is issued
//! (`docs/rpc-strategy.md` §4). A 429 is treated as a bug in Sentinel's own
//! accounting, not as the normal way to discover a limit.
//!
//! Each provider carries one [`ProviderBudget`]: **one independent token
//! bucket and concurrency gate per request class**, each sized as a
//! configured weighted slice of the provider's total `max_rps` /
//! `max_concurrent`. Classes never share a pool, so one class's pressure
//! can never consume another's capacity — `docs/rpc-strategy.md` §4's
//! priority table ("Execution: never starved, dedicated slice" /
//! "ScheduledScan: starves first, by design") is realised directly as
//! `Execution`'s weight being the largest and `ScheduledScan`'s the
//! smallest, so under symmetric heavy pressure every class's success rate
//! falls in exactly priority order (FI-21).

use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::provider::RequestClass;

#[derive(Debug, Clone, Copy)]
pub struct BudgetConfig {
    /// Total requests/second across all classes, split by
    /// [`class_weight_pct`] into five disjoint per-class buckets.
    pub max_rps: u32,
    /// Total concurrent in-flight requests across all classes, split the
    /// same way.
    pub max_concurrent: u32,
}

/// Each class's fixed percentage of the provider's total budget. Sums to
/// 100. Strictly decreasing in priority order so that, under symmetric
/// pressure across every class, each class's absolute capacity — and so its
/// success rate — is strictly ordered highest-priority first: `Execution`,
/// then `RealtimeCompleteness`, `GapRepair`, `Backfill`, `ScheduledScan`
/// (`docs/rpc-strategy.md` §4, FI-21).
fn class_weight_pct(class: RequestClass) -> u64 {
    match class {
        RequestClass::Execution => 40,
        RequestClass::RealtimeCompleteness => 25,
        RequestClass::GapRepair => 15,
        RequestClass::Backfill => 12,
        RequestClass::ScheduledScan => 8,
    }
}

impl BudgetConfig {
    pub fn new(max_rps: u32, max_concurrent: u32) -> Self {
        BudgetConfig {
            max_rps,
            max_concurrent,
        }
    }

    /// A configured non-zero total is never rounded down to a zero share
    /// for any class (a floor of 1) — but a genuinely-configured zero total
    /// means zero, not "at least one anyway" (`exhausted_budget_denies_...`
    /// depends on this: `max_rps = 0` must deny every class).
    fn class_rps(&self, class: RequestClass) -> u32 {
        if self.max_rps == 0 {
            return 0;
        }
        ((self.max_rps as u64 * class_weight_pct(class)) / 100).max(1) as u32
    }

    fn class_concurrent(&self, class: RequestClass) -> u32 {
        if self.max_concurrent == 0 {
            return 0;
        }
        ((self.max_concurrent as u64 * class_weight_pct(class)) / 100).max(1) as u32
    }
}

/// A simple token bucket: capacity tokens, refilled continuously at
/// `rate`/second, never exceeding capacity. `try_take` is the client-side
/// gate — the request is only issued if it returns `true`.
struct TokenBucket {
    capacity: f64,
    rate_per_sec: f64,
    tokens: Mutex<(f64, Instant)>,
}

impl TokenBucket {
    fn new(rate_per_sec: u32) -> Self {
        let rate = rate_per_sec as f64;
        TokenBucket {
            capacity: rate.max(1.0),
            rate_per_sec: rate,
            tokens: Mutex::new((rate.max(1.0), Instant::now())),
        }
    }

    fn try_take(&self) -> bool {
        if self.rate_per_sec <= 0.0 {
            return false;
        }
        let mut guard = self.tokens.lock().unwrap_or_else(|e| e.into_inner());
        let (tokens, last) = &mut *guard;
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(*last).as_secs_f64();
        *tokens = (*tokens + elapsed * self.rate_per_sec).min(self.capacity);
        *last = now;
        if *tokens >= 1.0 {
            *tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// A bounded concurrency gate (never an unbounded queue — `phase-03-rpc.md`
/// §8). `try_acquire` fails immediately rather than waiting when the limit
/// is reached; the caller (pool) treats that as budget exhaustion.
struct ConcurrencyGate {
    limit: u32,
    inflight: AtomicU32,
}

impl ConcurrencyGate {
    fn new(limit: u32) -> Self {
        ConcurrencyGate {
            limit,
            inflight: AtomicU32::new(0),
        }
    }

    fn try_acquire(&self) -> bool {
        if self.limit == 0 {
            return false;
        }
        loop {
            let current = self.inflight.load(Ordering::Acquire);
            if current >= self.limit {
                return false;
            }
            if self
                .inflight
                .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return true;
            }
        }
    }

    fn release(&self) {
        self.inflight.fetch_sub(1, Ordering::AcqRel);
    }
}

/// An RAII guard releasing the concurrency slot the budget granted. Holding
/// this for the lifetime of the underlying provider call is what makes
/// "budget enforced before the request is made, and released after" true.
pub struct BudgetPermit<'a> {
    gate: &'a ConcurrencyGate,
}

impl Drop for BudgetPermit<'_> {
    fn drop(&mut self) {
        self.gate.release();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetOutcome {
    /// Rate-limited (token bucket empty) — retryable at the pool level via
    /// normal backoff, without ever having called the provider.
    RateExhausted,
    /// Concurrency limit reached.
    ConcurrencyExhausted,
}

struct ClassBudget {
    rate: TokenBucket,
    concurrency: ConcurrencyGate,
    granted: AtomicI64,
    rejected: AtomicI64,
}

pub struct ProviderBudget {
    by_class: [ClassBudget; 5],
}

fn class_index(class: RequestClass) -> usize {
    (class.priority() - 1) as usize
}

impl ProviderBudget {
    pub fn new(config: BudgetConfig) -> Self {
        let make = |class: RequestClass| ClassBudget {
            rate: TokenBucket::new(config.class_rps(class)),
            concurrency: ConcurrencyGate::new(config.class_concurrent(class)),
            granted: AtomicI64::new(0),
            rejected: AtomicI64::new(0),
        };
        ProviderBudget {
            by_class: RequestClass::ALL.map(make),
        }
    }

    /// Attempts to reserve budget for `class`. On `Granted`, returns a
    /// [`BudgetPermit`] that MUST be held until the underlying provider
    /// call completes — dropping it releases the concurrency slot. Every
    /// class draws from its own disjoint bucket (`class_weight_pct`), so
    /// pressure on one class can never exhaust another's.
    pub fn try_acquire(&self, class: RequestClass) -> Result<BudgetPermit<'_>, BudgetOutcome> {
        let cb = &self.by_class[class_index(class)];
        if !cb.rate.try_take() {
            cb.rejected.fetch_add(1, Ordering::Relaxed);
            return Err(BudgetOutcome::RateExhausted);
        }
        if !cb.concurrency.try_acquire() {
            cb.rejected.fetch_add(1, Ordering::Relaxed);
            return Err(BudgetOutcome::ConcurrencyExhausted);
        }
        cb.granted.fetch_add(1, Ordering::Relaxed);
        Ok(BudgetPermit {
            gate: &cb.concurrency,
        })
    }

    pub fn granted(&self, class: RequestClass) -> i64 {
        self.by_class[class_index(class)]
            .granted
            .load(Ordering::Relaxed)
    }

    pub fn rejected(&self, class: RequestClass) -> i64 {
        self.by_class[class_index(class)]
            .rejected
            .load(Ordering::Relaxed)
    }
}

/// A test/production-shared helper: sleeps until a token bucket would grant
/// again, bounded, used only by tests that want to observe recovery rather
/// than busy-poll.
pub fn approx_refill_interval(rate_per_sec: u32) -> Duration {
    if rate_per_sec == 0 {
        Duration::from_secs(1)
    } else {
        Duration::from_secs_f64(1.0 / rate_per_sec as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhausted_budget_denies_without_a_provider_call() {
        let budget = ProviderBudget::new(BudgetConfig::new(0, 0));
        // With rps=0 and concurrency computed from 0, every class must be
        // denied immediately — proving "budget enforced before the request
        // is made" (phase-03-rpc.md §8): the caller never gets a permit to
        // pass into a provider call.
        let outcome = budget.try_acquire(RequestClass::Backfill);
        assert!(outcome.is_err());
    }

    #[test]
    fn execution_class_has_a_protected_reservation() {
        // ScheduledScan's bucket is 8% of 10 = max(1, 0) = 1; Execution's is
        // 40% of 10 = 4. Each class's bucket is disjoint from every other's.
        let budget = ProviderBudget::new(BudgetConfig::new(10, 10));
        // Drain ScheduledScan's own bucket entirely.
        let mut scan_grants = Vec::new();
        while let Ok(permit) = budget.try_acquire(RequestClass::ScheduledScan) {
            scan_grants.push(permit);
            if scan_grants.len() > 100 {
                break;
            }
        }
        assert!(
            budget.rejected(RequestClass::ScheduledScan) > 0,
            "ScheduledScan's own bucket must actually exhaust"
        );
        // Execution must still be grantable — it draws from a disjoint pool.
        let exec = budget.try_acquire(RequestClass::Execution);
        assert!(
            exec.is_ok(),
            "Execution must remain available while ScheduledScan is starved (FI-21)"
        );
    }

    #[test]
    fn concurrency_permit_release_frees_the_slot() {
        let budget = ProviderBudget::new(BudgetConfig::new(1000, 1));
        // Every class's concurrency floor is max(1, weight% * 1) = 1 here;
        // Execution is used arbitrarily since each class's gate is disjoint.
        let acquired = budget.try_acquire(RequestClass::Execution);
        assert!(acquired.is_ok(), "first acquire must succeed");
        let permit = match acquired {
            Ok(p) => p,
            Err(_) => unreachable!(),
        };
        assert!(
            budget.try_acquire(RequestClass::Execution).is_err(),
            "concurrency=1 must deny a second concurrent acquire"
        );
        drop(permit);
        assert!(
            budget.try_acquire(RequestClass::Execution).is_ok(),
            "dropping the permit must release the slot"
        );
    }

    #[test]
    fn success_rate_falls_in_strict_priority_order_under_symmetric_pressure_fi21() {
        // Every class hammers its own bucket the same number of times, far
        // beyond any class's capacity (max weight is 40% of 1000 = 400).
        // Because the buckets are disjoint and sized by class_weight_pct
        // (strictly decreasing in priority order), each class's success
        // rate must land in exactly priority order: Execution highest,
        // ScheduledScan lowest — the concrete form of "lower classes may
        // starve/degrade first while Execution remains protected" (FI-21).
        let budget = ProviderBudget::new(BudgetConfig::new(1000, 1_000_000));
        const ATTEMPTS: i64 = 1000;
        for class in RequestClass::ALL {
            for _ in 0..ATTEMPTS {
                let _ = budget.try_acquire(class);
            }
        }
        let rate = |class: RequestClass| budget.granted(class) as f64 / ATTEMPTS as f64;
        let rates: Vec<(RequestClass, f64)> =
            RequestClass::ALL.iter().map(|&c| (c, rate(c))).collect();

        for pair in rates.windows(2) {
            let (class_a, rate_a) = pair[0];
            let (class_b, rate_b) = pair[1];
            assert!(
                rate_a > rate_b,
                "expected {class_a} (rate={rate_a}) to strictly beat {class_b} (rate={rate_b}) — full rates: {rates:?}"
            );
        }
        assert!(
            budget.rejected(RequestClass::ScheduledScan) > 0,
            "ScheduledScan must actually be denied under this pressure, not merely slower"
        );
    }
}
