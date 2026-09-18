//! `provider_health` persistence against real PostgreSQL
//! (`docs/phases/phase-03-rpc.md` §20): the canonical Phase-2 table, owner
//! `sentinel-rpc`, accessed only through `sentinel-db`'s existing typed
//! layer — no new datastore invented, Redis never made load-bearing.
//!
//! Skips (never fails) when Postgres is unreachable — the same pattern
//! `sentinel-db`'s own tests use.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use sentinel_rpc::budget::BudgetConfig;
use sentinel_rpc::fixtures::{full_capabilities, FixtureProvider};
use sentinel_rpc::pool::{PoolConfig, ResolvedProviderSpec};
use sentinel_rpc::{BreakerConfig, ProviderId, RequestClass, RpcMethodCall, RpcPool};

fn rust_url() -> String {
    std::env::var("SENTINEL_TEST_RUST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://sentinel_rust:sentinel_local_dev_only_rust@127.0.0.1:5432/sentinel".to_string()
    })
}

/// `sentinel_rust` deliberately has no DELETE grant on `provider_health`
/// (least privilege — DM ownership rules never grant app roles DELETE).
/// Test cleanup therefore connects as the bootstrap/admin role, exactly
/// the same split `sentinel-db`'s own `adversarial.rs` tests use.
fn bootstrap_url() -> String {
    std::env::var("SENTINEL_TEST_BOOTSTRAP_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://sentinel_bootstrap:sentinel_local_dev_only_bootstrap@127.0.0.1:5432/sentinel".to_string())
}

async fn connect(url: &str) -> Option<PgPool> {
    match PgPoolOptions::new()
        .max_connections(3)
        .acquire_timeout(Duration::from_secs(3))
        .connect(url)
        .await
    {
        Ok(p) => Some(p),
        Err(e) => {
            eprintln!("skipping provider_health_persistence: Postgres not reachable at {url}: {e}\nstart it with `make up` and `make migrate`");
            None
        }
    }
}

#[tokio::test]
async fn provider_health_transitions_are_persisted_and_readable_via_sentinel_db() {
    let Some(db) = connect(&rust_url()).await else {
        return;
    };

    let mut classes = HashSet::new();
    classes.insert(RequestClass::Backfill);
    let provider_id = format!("test-provider-{}", uuid::Uuid::new_v4());
    let inner = Arc::new(FixtureProvider::new(
        provider_id.clone(),
        full_capabilities(),
        classes.clone(),
    ));

    let pool = RpcPool::from_providers(
        vec![ResolvedProviderSpec {
            provider: inner,
            capabilities: full_capabilities(),
            configured_classes: classes,
            budget: BudgetConfig::new(1000, 1000),
        }],
        BreakerConfig::default(),
        PoolConfig::default(),
    )
    .expect("pool must build");

    // Generate a few real health events (a success and a would-be lookup
    // for a method this FixtureProvider was never scripted for, i.e. a
    // real Fatal failure) before persisting.
    let _ = pool
        .call(
            RpcMethodCall::GetBlockHeight,
            RequestClass::Backfill,
            sentinel_core::Commitment::Confirmed,
        )
        .await; // Fatal: no scripted response — still a real, recorded failure

    pool.persist_health(&db)
        .await
        .expect("persisting provider_health must succeed against real Postgres");

    let row = sentinel_db::queries::fetch_provider_health(
        &db,
        &provider_id,
        pool.provider_health_tracker(&ProviderId(provider_id.clone()))
            .expect("tracker must exist")
            .window_start(),
    )
    .await
    .expect("fetch must succeed")
    .expect("a row must have been written for this provider_id/window_start");

    assert_eq!(row.provider_id, provider_id);
    assert!(
        row.requests >= 1,
        "requests must reflect the real call made above: {row:?}"
    );
    assert!(
        row.errors >= 1,
        "the unscripted call must have recorded as an error: {row:?}"
    );
    assert!(!row.breaker_state.is_empty());

    // Persisting again (a later window snapshot at the same window_start)
    // must upsert cleanly, not violate the (provider_id, window_start)
    // primary key — proving the ON CONFLICT DO UPDATE path really is
    // exercised, not merely present in the SQL text.
    pool.persist_health(&db)
        .await
        .expect("re-persisting the same window must upsert cleanly");

    // Clean up this test's row so repeated runs do not accumulate garbage
    // in a shared local database. sentinel_rust has no DELETE grant on
    // provider_health by design, so cleanup uses the bootstrap connection.
    if let Some(admin_db) = connect(&bootstrap_url()).await {
        sqlx::query("DELETE FROM provider_health WHERE provider_id = $1")
            .bind(&provider_id)
            .execute(&admin_db)
            .await
            .expect("test cleanup delete must succeed");
    }
}
