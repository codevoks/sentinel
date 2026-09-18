//! The required adversarial campaign (`docs/phases/phase-03-rpc.md` §23):
//! FI-09, FI-10, FI-11, FI-21, FI-22, each with the *specific* assertion
//! the spec names — "returned an error" is insufficient where the error
//! class or state transition is part of the contract.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use sentinel_core::{Commitment, Slot};
use sentinel_rpc::budget::BudgetConfig;
use sentinel_rpc::fault::{FaultInjectingProvider, InjectedFault};
use sentinel_rpc::fixtures::{all_classes, full_capabilities, FixtureProvider, ScriptedResponse};
use sentinel_rpc::pool::{PoolConfig, ResolvedProviderSpec};
use sentinel_rpc::provider::RpcMethodResponse;
use sentinel_rpc::{BreakerConfig, ProviderId, RequestClass, RpcError, RpcMethodCall, RpcPool};

/// FI-09 — stale `context.slot`: rejected as `STALE`, never used as
/// canonical, and the provider's health degrades (RPC-04).
#[tokio::test]
async fn fi09_stale_response_is_rejected_and_degrades_health() {
    let inner = Arc::new(
        FixtureProvider::new("stale-provider", full_capabilities(), all_classes()).with_response(
            "getLatestBlockhash",
            ScriptedResponse::Ok(RpcMethodResponse::LatestBlockhash {
                blockhash: "stalehash".into(),
                last_valid_block_height: 100,
                context_slot: Slot(10), // far behind the required minimum
            }),
        ),
    );
    let provider_id = ProviderId("stale-provider".to_string());

    let pool = RpcPool::from_providers(
        vec![ResolvedProviderSpec {
            provider: inner,
            capabilities: full_capabilities(),
            configured_classes: all_classes(),
            budget: BudgetConfig::new(1000, 1000),
        }],
        BreakerConfig::default(),
        PoolConfig {
            backoff_base: Duration::from_millis(1),
            backoff_cap: Duration::from_millis(5),
            call_deadline: Duration::from_millis(300),
            ..Default::default()
        },
    )
    .expect("pool must build");

    let result = pool
        .call_with_min_slot(
            RpcMethodCall::GetLatestBlockhash,
            RequestClass::Execution,
            Commitment::Confirmed,
            Some(Slot(1_000)), // required far ahead of the fixture's Slot(10)
        )
        .await;

    // (1) never returned to the caller as a valid/Ok response.
    assert!(
        result.is_err(),
        "a stale response must never be surfaced as Ok"
    );
    // (2)+(3) specifically classified STALE, not merely "an error".
    let err = result.err().map(|e| {
        matches!(e, RpcError::Stale { .. })
            || matches!(
                e,
                RpcError::Fatal {
                    code: "SEN-RPC-000",
                    ..
                }
            )
    });
    // The pool may retry against no other provider and end in
    // NoHealthyProvider (Fatal) once the single provider is excluded after
    // a Stale rejection, or return the Stale error directly if attempts run
    // out first — either way, the caller never receives Ok, and staleness
    // was detected (checked directly below via health, not inferred).
    assert!(
        err.unwrap_or(false),
        "must fail via Stale (or NoHealthyProvider after a Stale exclusion), never any other class"
    );

    // (4) provider health degraded per policy (staleness -> Degraded).
    let health = pool.health();
    let snap = health
        .iter()
        .find(|h| h.provider_id == provider_id.0)
        .expect("health entry must exist");
    assert!(
        snap.stale_rejections > 0,
        "stale_rejections must be recorded: {snap:?}"
    );
    assert_eq!(
        snap.breaker_state, "degraded",
        "CB staleness must degrade the breaker: {snap:?}"
    );
}

/// FI-10 — two providers return different blocks for the same slot: both
/// surfaced to the caller, no silent merge.
#[tokio::test]
async fn fi10_divergent_blocks_for_one_slot_are_both_surfaced_never_merged() {
    let provider_a = Arc::new(
        FixtureProvider::new("prov-a", full_capabilities(), all_classes()).with_block(
            42,
            "hashA111111111111111111111111111111111111",
            serde_json::json!({"transactions": ["tx-from-a"]}),
        ),
    );
    let provider_b = Arc::new(
        FixtureProvider::new("prov-b", full_capabilities(), all_classes()).with_block(
            42,
            "hashB222222222222222222222222222222222222", // different blockhash for the same slot
            serde_json::json!({"transactions": ["tx-from-b"]}),
        ),
    );

    let pool = RpcPool::from_providers(
        vec![
            ResolvedProviderSpec {
                provider: provider_a,
                capabilities: full_capabilities(),
                configured_classes: all_classes(),
                budget: BudgetConfig::new(1000, 1000),
            },
            ResolvedProviderSpec {
                provider: provider_b,
                capabilities: full_capabilities(),
                configured_classes: all_classes(),
                budget: BudgetConfig::new(1000, 1000),
            },
        ],
        BreakerConfig::default(),
        PoolConfig::default(),
    )
    .expect("pool must build");

    let id_a = ProviderId("prov-a".to_string());
    let id_b = ProviderId("prov-b".to_string());
    let result = pool.cross_check_block(Slot(42), &id_a, &id_b).await;

    let err = result.expect_err("divergent blocks must be an error, not a merged Ok");
    match err {
        RpcError::ContentDivergence { detail, .. } => {
            assert_eq!(detail.slot, Some(Slot(42)));
            // No canonical blockhash chosen — divergence at the hash level
            // itself means neither side is silently preferred.
            assert_eq!(
                detail.blockhash, None,
                "must not pick one blockhash as canonical"
            );
        }
        _other => panic!("expected ContentDivergence, got a different error class"),
    }

    // Health on BOTH providers reflects the divergence — neither is
    // silently exonerated.
    let health = pool.health();
    for id in [&id_a, &id_b] {
        let snap = health
            .iter()
            .find(|h| h.provider_id == id.0)
            .expect("health entry must exist");
        assert!(
            snap.divergence_events > 0,
            "{id} must record a divergence event: {snap:?}"
        );
        assert_eq!(
            snap.breaker_state, "open",
            "{id}'s breaker must trip immediately (CB-4): {snap:?}"
        );
    }
}

/// FI-11 — two providers return different content for the same
/// blockhash: severe divergence, breaker trips immediately (CB-4), and it
/// is a distinct case from ordinary transport failure.
#[tokio::test]
async fn fi11_same_blockhash_different_content_trips_breaker_immediately_cb4() {
    const SHARED_HASH: &str = "sharedhash11111111111111111111111111111111";
    let provider_a = Arc::new(
        FixtureProvider::new("prov-a", full_capabilities(), all_classes()).with_block(
            7,
            SHARED_HASH,
            serde_json::json!({"transactions": ["tx-1"], "blockTime": 1000}),
        ),
    );
    let provider_b = Arc::new(
        FixtureProvider::new("prov-b", full_capabilities(), all_classes())
            // Same blockhash, but different reported content — the "lying
            // provider" case, strictly more severe than a plain transport
            // failure.
            .with_block(
                7,
                SHARED_HASH,
                serde_json::json!({"transactions": ["tx-1", "tx-2"], "blockTime": 1000}),
            ),
    );

    let pool = RpcPool::from_providers(
        vec![
            ResolvedProviderSpec {
                provider: provider_a,
                capabilities: full_capabilities(),
                configured_classes: all_classes(),
                budget: BudgetConfig::new(1000, 1000),
            },
            ResolvedProviderSpec {
                provider: provider_b,
                capabilities: full_capabilities(),
                configured_classes: all_classes(),
                budget: BudgetConfig::new(1000, 1000),
            },
        ],
        BreakerConfig {
            // A very high min_sample_size proves the trip is NOT a
            // failure-ratio trip in disguise — CB-4 fires with zero prior
            // samples.
            min_sample_size: 10_000,
            ..Default::default()
        },
        PoolConfig::default(),
    )
    .expect("pool must build");

    let id_a = ProviderId("prov-a".to_string());
    let id_b = ProviderId("prov-b".to_string());

    let result = pool.cross_check_block(Slot(7), &id_a, &id_b).await;
    let err = result.expect_err("same-blockhash content divergence must be an error");
    match err {
        RpcError::ContentDivergence { detail, .. } => {
            assert_eq!(
                detail.blockhash,
                Some(SHARED_HASH.to_string()),
                "the shared blockhash must be recorded — this is the FI-11 case specifically"
            );
        }
        _ => panic!("expected ContentDivergence for FI-11"),
    }

    let health = pool.health();
    for id in [&id_a, &id_b] {
        let snap = health
            .iter()
            .find(|h| h.provider_id == id.0)
            .expect("health entry must exist");
        assert_eq!(
            snap.breaker_state, "open",
            "{id}'s breaker must be Open immediately despite zero prior failure samples (CB-4, min_sample_size=10000): {snap:?}"
        );
    }
}

/// FI-21 — aggressive rate limiting: budget adaptation/isolation works;
/// `Execution` stays protected while lower classes are the ones that
/// starve. (`budget.rs`'s own unit test proves the general ordering
/// property across all five classes; this test proves the specific,
/// pool-level acceptance claim — that under simulated aggressive rate
/// limiting from the provider itself, Execution calls still succeed.)
#[tokio::test]
async fn fi21_execution_class_survives_aggressive_rate_limiting() {
    let inner = Arc::new(
        FixtureProvider::new("rl-provider", full_capabilities(), all_classes())
            .with_response(
                "getSlot",
                ScriptedResponse::Ok(RpcMethodResponse::Slot(Slot(99))),
            )
            .with_response(
                "getLatestBlockhash",
                ScriptedResponse::Ok(RpcMethodResponse::LatestBlockhash {
                    blockhash: "h".into(),
                    last_valid_block_height: 1,
                    context_slot: Slot(99),
                }),
            ),
    );
    let faulty = Arc::new(FaultInjectingProvider::wrapping(inner));
    // Saturate ScheduledScan's own client-side bucket with rate-limit
    // faults far beyond its capacity — simulating the provider aggressively
    // rate limiting the lowest-priority traffic.
    faulty.script(
        "getSlot",
        (0..50)
            .map(|_| InjectedFault::RateLimitedNoRetryAfter)
            .collect(),
    );

    let mut classes = all_classes();
    classes.insert(RequestClass::Execution);

    let pool = RpcPool::from_providers(
        vec![ResolvedProviderSpec {
            provider: faulty,
            capabilities: full_capabilities(),
            configured_classes: classes,
            budget: BudgetConfig::new(50, 50),
        }],
        BreakerConfig {
            min_sample_size: 10_000, // isolate budget behavior from breaker behavior
            ..Default::default()
        },
        PoolConfig {
            backoff_base: Duration::from_millis(1),
            backoff_cap: Duration::from_millis(3),
            call_deadline: Duration::from_millis(500),
            ..Default::default()
        },
    )
    .expect("pool must build");

    // Hammer ScheduledScan's getSlot until its own budget is visibly
    // exhausted (denied calls never reaching the provider).
    for _ in 0..30 {
        let _ = pool
            .call(
                RpcMethodCall::GetSlot,
                RequestClass::ScheduledScan,
                Commitment::Confirmed,
            )
            .await;
    }

    // Execution, drawing from its own disjoint, larger reservation and
    // untouched by ScheduledScan's script (different method), must still
    // succeed.
    let exec_result = pool
        .call(
            RpcMethodCall::GetLatestBlockhash,
            RequestClass::Execution,
            Commitment::Confirmed,
        )
        .await;
    assert!(
        exec_result.is_ok(),
        "Execution must remain unaffected by ScheduledScan's rate-limit pressure: {exec_result:?}"
    );
}

/// FI-22 — all providers open: a distinct, loud failure
/// (`NoHealthyProvider`-equivalent), never silently treated as idle/no-work
/// (CB-6, RPC-07).
#[tokio::test]
async fn fi22_all_providers_open_is_a_distinct_loud_failure() {
    let mut providers = Vec::new();
    let mut classes = HashSet::new();
    classes.insert(RequestClass::Backfill);
    for name in ["p1", "p2", "p3"] {
        let inner = Arc::new(FixtureProvider::new(
            name,
            full_capabilities(),
            classes.clone(),
        ));
        let faulty = Arc::new(FaultInjectingProvider::wrapping(inner));
        faulty.always_fail(InjectedFault::ProviderFailure);
        providers.push(ResolvedProviderSpec {
            provider: faulty,
            capabilities: full_capabilities(),
            configured_classes: classes.clone(),
            budget: BudgetConfig::new(1000, 1000),
        });
    }

    let pool = RpcPool::from_providers(
        providers,
        BreakerConfig {
            min_sample_size: 1,
            failure_ratio_threshold: 0.0, // trip on the very first failure — get to "all open" fast
            ..Default::default()
        },
        PoolConfig {
            backoff_base: Duration::from_millis(1),
            backoff_cap: Duration::from_millis(2),
            call_deadline: Duration::from_millis(500),
            ..Default::default()
        },
    )
    .expect("pool must build");

    // Drive every provider's breaker to Open.
    for _ in 0..3 {
        let _ = pool
            .call(
                RpcMethodCall::GetBlockHeight,
                RequestClass::Backfill,
                Commitment::Confirmed,
            )
            .await;
    }

    let health_before = pool.health();
    assert!(
        health_before.iter().all(|h| h.breaker_state == "open"),
        "all three providers must actually be Open before the loud-failure assertion: {health_before:?}"
    );

    let result = pool
        .call(
            RpcMethodCall::GetBlockHeight,
            RequestClass::Backfill,
            Commitment::Confirmed,
        )
        .await;
    match result {
        Err(RpcError::Fatal {
            code: "SEN-RPC-000",
            ..
        }) => {} // the distinct NoHealthyProvider signal
        other => {
            panic!("expected the distinct NoHealthyProvider Fatal signal (CB-6), got {other:?}")
        }
    }
}
