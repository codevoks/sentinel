//! P-BOUND-1 (`docs/phases/phase-03-rpc.md` §10, RPC-02): for generated
//! error patterns, retry behavior remains bounded in attempt count and in
//! total elapsed/scheduled time.
//!
//! This environment could not reach `crates.io`'s API during this phase
//! (`docs/project-status.md` records the finding, consistent with Phase
//! 1/2's own disclosed finding for the same host) to add `proptest`, so
//! this is a hand-rolled deterministic property test: a small splitmix64
//! generator (fixed seed, no external randomness — `phase-03-rpc.md` §18
//! requires fault injection be deterministic anyway) produces many
//! adversarial retryable-error sequences, each run through the **real**
//! `RpcPool` retry loop end to end, asserting the bound holds for every one.
//! This is still a real property test — it fails the moment the bound is
//! violated for ANY generated case — just without the shrinking/reporting
//! machinery a crate like `proptest` would add.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sentinel_core::Commitment;
use sentinel_rpc::budget::BudgetConfig;
use sentinel_rpc::fault::{FaultInjectingProvider, InjectedFault};
use sentinel_rpc::fixtures::{full_capabilities, FixtureProvider};
use sentinel_rpc::pool::{PoolConfig, ResolvedProviderSpec};
use sentinel_rpc::{BreakerConfig, ProviderId, RequestClass, RpcMethodCall, RpcPool};

/// A tiny, fixed-seed splitmix64 generator — deterministic across runs and
/// platforms, with no external dependency.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    fn next_range(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn generate_fault(rng: &mut SplitMix64) -> InjectedFault {
    match rng.next_range(4) {
        0 => InjectedFault::Timeout,
        1 => {
            InjectedFault::RateLimitedWithRetryAfter(Duration::from_millis(rng.next_range(40) + 1))
        }
        2 => InjectedFault::RateLimitedNoRetryAfter,
        _ => InjectedFault::ServerError(500 + (rng.next_range(4) as u16) * 100),
    }
}

fn max_attempts_for(class: RequestClass) -> u32 {
    // Mirrors pool.rs's private `max_attempts` table (RT-1) — duplicated
    // here deliberately so this test asserts against the frozen contract's
    // numbers, not against whatever the implementation happens to do.
    match class {
        RequestClass::Execution => 5,
        RequestClass::RealtimeCompleteness => 4,
        RequestClass::GapRepair => 4,
        RequestClass::Backfill => 3,
        RequestClass::ScheduledScan => 2,
    }
}

#[tokio::test]
async fn every_generated_fault_sequence_produces_a_bounded_retry_pbound1() {
    const CASES: u64 = 60;
    const DEADLINE: Duration = Duration::from_millis(800);
    // Generous multiple of DEADLINE, accounting for scheduling jitter in a
    // shared CI runner — the point under test is "bounded", not "exactly
    // DEADLINE to the millisecond".
    let elapsed_ceiling = DEADLINE * 4;

    for class in RequestClass::ALL {
        let bound = max_attempts_for(class);
        for case in 0..CASES {
            let seed = (class.priority() as u64) * 1_000_003 + case;
            let (attempts, elapsed) = run_one_case_via_health(class, seed, DEADLINE).await;
            assert!(
                attempts <= bound,
                "class={class} seed={seed}: attempts={attempts} exceeded bound={bound}"
            );
            assert!(
                elapsed <= elapsed_ceiling,
                "class={class} seed={seed}: elapsed={elapsed:?} exceeded ceiling={elapsed_ceiling:?}"
            );
        }
    }
}

/// Builds a fresh single-provider pool scripted with `fault_count` (>
/// the class's retry bound) generated retryable faults, drives one
/// `pool.call`, and reads the attempt count back from the pool's own
/// `provider_health` snapshot (`requests` counter) — the real, load-bearing
/// measurement of how many attempts the retry loop actually made.
async fn run_one_case_via_health(
    class: RequestClass,
    seed: u64,
    deadline: Duration,
) -> (u32, Duration) {
    let mut rng = SplitMix64(seed);
    let fault_count = max_attempts_for(class) as u64 + 3 + rng.next_range(10);
    let faults: Vec<InjectedFault> = (0..fault_count).map(|_| generate_fault(&mut rng)).collect();

    let mut classes = HashSet::new();
    classes.insert(class);
    let inner = Arc::new(FixtureProvider::new(
        format!("p-bound-{seed}"),
        full_capabilities(),
        classes.clone(),
    ));
    let faulty = Arc::new(FaultInjectingProvider::wrapping(inner));
    faulty.script("getSlot", faults);
    let provider_id = ProviderId(format!("p-bound-{seed}"));

    let pool = RpcPool::from_providers(
        vec![ResolvedProviderSpec {
            provider: faulty,
            capabilities: full_capabilities(),
            configured_classes: classes,
            budget: BudgetConfig::new(10_000, 10_000),
        }],
        BreakerConfig {
            min_sample_size: 10_000,
            open_cooldown: Duration::from_secs(3_600),
            ..Default::default()
        },
        PoolConfig {
            backoff_base: Duration::from_millis(1),
            backoff_cap: Duration::from_millis(20),
            call_deadline: deadline,
            ..Default::default()
        },
    )
    .expect("pool must build");

    let started = Instant::now();
    let _ = pool
        .call(RpcMethodCall::GetSlot, class, Commitment::Confirmed)
        .await;
    let elapsed = started.elapsed();

    let attempts = pool
        .health()
        .into_iter()
        .find(|h| h.provider_id == provider_id.0)
        .map(|h| h.requests as u32)
        .unwrap_or(0);
    (attempts, elapsed)
}
