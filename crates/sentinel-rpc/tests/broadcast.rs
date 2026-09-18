//! `RpcPool::broadcast` (RT-5 / RPC-05 / `docs/phases/phase-03-rpc.md` §16):
//! fan-out submission, never sequential failover. Acceptance criteria
//! (§9): "`broadcast()` fans out to all healthy providers; a single
//! failure does not fail the broadcast."

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use sentinel_rpc::budget::BudgetConfig;
use sentinel_rpc::fault::{FaultInjectingProvider, InjectedFault};
use sentinel_rpc::fixtures::{full_capabilities, FixtureProvider, ScriptedResponse};
use sentinel_rpc::pool::{PoolConfig, ResolvedProviderSpec};
use sentinel_rpc::provider::RpcMethodResponse;
use sentinel_rpc::{BreakerConfig, ProviderId, RequestClass, RpcPool};

fn execution_only() -> HashSet<RequestClass> {
    let mut s = HashSet::new();
    s.insert(RequestClass::Execution);
    s
}

#[tokio::test]
async fn broadcast_fans_out_to_every_eligible_provider_and_survives_one_failure() {
    let ok_a = Arc::new(
        FixtureProvider::new("ok-a", full_capabilities(), execution_only()).with_response(
            "sendTransaction",
            ScriptedResponse::Ok(RpcMethodResponse::SendTransaction {
                signature: "sigA".into(),
            }),
        ),
    );
    let ok_b = Arc::new(
        FixtureProvider::new("ok-b", full_capabilities(), execution_only()).with_response(
            "sendTransaction",
            ScriptedResponse::Ok(RpcMethodResponse::SendTransaction {
                signature: "sigB".into(),
            }),
        ),
    );
    let failing = Arc::new(FaultInjectingProvider::wrapping(Arc::new(
        FixtureProvider::new("fails", full_capabilities(), execution_only()),
    )));
    failing.always_fail(InjectedFault::ProviderFailure);

    let pool = RpcPool::from_providers(
        vec![
            ResolvedProviderSpec {
                provider: ok_a.clone(),
                capabilities: full_capabilities(),
                configured_classes: execution_only(),
                budget: BudgetConfig::new(1000, 1000),
            },
            ResolvedProviderSpec {
                provider: ok_b.clone(),
                capabilities: full_capabilities(),
                configured_classes: execution_only(),
                budget: BudgetConfig::new(1000, 1000),
            },
            ResolvedProviderSpec {
                provider: failing.clone(),
                capabilities: full_capabilities(),
                configured_classes: execution_only(),
                budget: BudgetConfig::new(1000, 1000),
            },
        ],
        BreakerConfig::default(),
        PoolConfig::default(),
    )
    .expect("pool must build");

    let outcome = pool.broadcast("AA==").await;

    // Fan-out reached all THREE eligible providers, not just one.
    assert_eq!(
        outcome.per_provider.len(),
        3,
        "broadcast must attempt every eligible provider: {outcome:?}"
    );
    let attempted: HashSet<String> = outcome
        .per_provider
        .iter()
        .map(|(id, _)| id.0.clone())
        .collect();
    for id in ["ok-a", "ok-b", "fails"] {
        assert!(
            attempted.contains(id),
            "provider {id} must have been attempted: {attempted:?}"
        );
    }

    // Each provider's own fixture was actually invoked exactly once — the
    // direct proof that this is real fan-out and not a sequential
    // "try A, stop on first success" masquerading as broadcast (which
    // would leave ok-b's call_count at 0 once ok-a succeeded).
    assert_eq!(
        ok_a.call_count(),
        1,
        "ok-a must have been called even though it wasn't first"
    );
    assert_eq!(
        ok_b.call_count(),
        1,
        "ok-b must have been called even though ok-a already succeeded"
    );

    // A single provider's failure does not fail the whole broadcast.
    assert!(
        outcome.any_success(),
        "at least one provider succeeded, so the broadcast overall must reflect that: {outcome:?}"
    );
    let signatures = outcome.signatures();
    assert!(signatures.contains(&"sigA".to_string()) && signatures.contains(&"sigB".to_string()));

    // The failing provider's specific error is surfaced per-provider, not
    // swallowed or allowed to fail the others.
    let failing_result = outcome
        .per_provider
        .iter()
        .find(|(id, _)| id.0 == "fails")
        .map(|(_, r)| r);
    assert!(
        matches!(failing_result, Some(Err(_))),
        "the failing provider's own error must be surfaced: {failing_result:?}"
    );
}

#[tokio::test]
async fn broadcast_excludes_an_open_provider_and_all_failures_surface_as_overall_failure() {
    let dead = Arc::new(FaultInjectingProvider::wrapping(Arc::new(
        FixtureProvider::new("dead", full_capabilities(), execution_only()),
    )));
    dead.always_fail(InjectedFault::ProviderFailure);
    let flaky = Arc::new(FaultInjectingProvider::wrapping(Arc::new(
        FixtureProvider::new("flaky", full_capabilities(), execution_only()),
    )));
    flaky.always_fail(InjectedFault::ProviderFailure);

    let pool = RpcPool::from_providers(
        vec![
            ResolvedProviderSpec {
                provider: dead.clone(),
                capabilities: full_capabilities(),
                configured_classes: execution_only(),
                budget: BudgetConfig::new(1000, 1000),
            },
            ResolvedProviderSpec {
                provider: flaky.clone(),
                capabilities: full_capabilities(),
                configured_classes: execution_only(),
                budget: BudgetConfig::new(1000, 1000),
            },
        ],
        BreakerConfig {
            min_sample_size: 1,
            failure_ratio_threshold: 0.0,
            ..Default::default()
        },
        PoolConfig {
            backoff_base: Duration::from_millis(1),
            backoff_cap: Duration::from_millis(2),
            call_deadline: Duration::from_millis(200),
            ..Default::default()
        },
    )
    .expect("pool must build");

    // Drive ONLY `dead`'s breaker to Open, directly via its own health
    // tracker — not through `pool.call()`, which would select among BOTH
    // providers for retries and could open `flaky`'s breaker too, making
    // this test's setup ambiguous about which provider is supposed to be
    // excluded.
    let dead_id = ProviderId("dead".to_string());
    let flaky_id = ProviderId("flaky".to_string());
    let dead_tracker = pool
        .provider_health_tracker(&dead_id)
        .expect("dead's tracker must exist");
    dead_tracker.record_failure("SEN-RPC-TEST", false, false);
    let health = pool.health();
    let dead_state = health
        .iter()
        .find(|h| h.provider_id == dead_id.0)
        .map(|h| h.breaker_state.clone());
    assert_eq!(
        dead_state.as_deref(),
        Some("open"),
        "test setup: `dead` must actually be Open before broadcasting"
    );
    let flaky_state = health
        .iter()
        .find(|h| h.provider_id == flaky_id.0)
        .map(|h| h.breaker_state.clone());
    assert_eq!(
        flaky_state.as_deref(),
        Some("closed"),
        "test setup: `flaky` must still be Closed/eligible"
    );

    let outcome = pool.broadcast("AA==").await;

    // `dead` is excluded entirely (CB-2: Open excludes every class,
    // including Execution) — never even attempted.
    let attempted: HashSet<String> = outcome
        .per_provider
        .iter()
        .map(|(id, _)| id.0.clone())
        .collect();
    assert!(
        !attempted.contains(&dead_id.0),
        "an Open provider must never be attempted by broadcast: {attempted:?}"
    );
    assert!(attempted.contains(&flaky_id.0));

    // The only eligible provider also fails: the overall outcome must
    // reflect total failure, loudly, not a false "ok".
    assert!(!outcome.any_success());
    assert!(outcome.all_failed());
}
