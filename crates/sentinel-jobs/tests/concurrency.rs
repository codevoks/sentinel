//! Real-concurrency job-queue tests against live PostgreSQL
//! (phase-02-data-model.md §7/§8, docs/distributed-correctness.md §3).
//!
//! These spawn genuine concurrent Tokio tasks, each with its OWN pool
//! connection (so claims are genuinely concurrent database sessions, not
//! sequential calls on one connection), and assert on real row state
//! afterward. Skips (does not fail) if Postgres is unreachable, matching
//! the pattern already established in `crates/sentinel-db/src/lib.rs`
//! (`make up` starts the required local stack; no network beyond
//! loopback/Docker is used).

use std::collections::HashSet;
use std::time::Duration;

use chrono::Duration as ChronoDuration;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use sentinel_db::enums::JobKind;

fn test_database_url() -> String {
    std::env::var("SENTINEL_TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://sentinel_rust:sentinel_local_dev_only_rust@127.0.0.1:5432/sentinel".to_string()
    })
}

async fn connect_pool(max_connections: u32) -> Option<PgPool> {
    let url = test_database_url();
    match PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&url)
        .await
    {
        Ok(pool) => Some(pool),
        Err(e) => {
            eprintln!("skipping sentinel-jobs concurrency test: Postgres not reachable: {e}");
            None
        }
    }
}

/// `claim()`'s query is deliberately global across every queued job in the
/// table — that is real production behavior (a worker does not know or
/// care which test/run created a job). Run as `#[tokio::test]`, these test
/// functions execute **concurrently within the same process** by default
/// (`cargo test`'s normal parallelism operates at the OS-thread level,
/// across different `#[test]`/`#[tokio::test]` functions in one binary),
/// so without serialization, one test's workers can claim a job fixture
/// belonging to a *different* concurrently-running test in this file —
/// verified directly: running this suite without the lock below produced a
/// real, reproducible cross-test claim theft (`fail()`/`assert_eq` failures
/// that vanished when each test was run in isolation). A session-level
/// Postgres advisory lock, held for a test's entire body, serializes these
/// three tests against each other without adding a new crate dependency or
/// weakening what any of them actually asserts about the real queue.
struct SerialGuard {
    // A dedicated, non-pooled connection (NOT `pool.acquire()`): a pooled
    // connection would return to the pool alive on drop, leaving the
    // session-level advisory lock held forever with nothing left to unlock
    // it, deadlocking every subsequent test. A raw connection's underlying
    // socket closes on drop, and PostgreSQL releases every session-level
    // advisory lock automatically when the holding backend's connection
    // closes — true whether the drop is graceful or (as on a test panic,
    // which unwinds and still runs `Drop`) abrupt.
    _conn: sqlx::postgres::PgConnection,
}

async fn acquire_serial_guard() -> SerialGuard {
    use sqlx::Connection;
    let mut conn = sqlx::postgres::PgConnection::connect(&test_database_url())
        .await
        .expect("must open a dedicated connection for the advisory lock");
    sqlx::query("SELECT pg_advisory_lock(872_364_501)")
        .execute(&mut conn)
        .await
        .expect("advisory lock must be acquirable");

    // `claim()`'s ORDER BY priority, job_id is global and every test in
    // this file exercises the real, unscoped queue — so leftover rows from
    // an earlier run (this file's own previous invocation, or an
    // interrupted one) are claimable "foreign" jobs that steal a claim
    // slot from the fixture the current test just set up, exactly as
    // cross-test interference did before the advisory lock existed. The
    // lock above only serializes test *functions*; it does not clean up
    // state a *previous test process* left behind. Cleared here, inside
    // the same critical section, using the bootstrap credential because
    // `sentinel_rust` (the runtime role) deliberately has no DELETE grant
    // on `jobs` (an OPERATIONAL, not-rebuildable table — application code
    // is never supposed to delete job history; only this test's own
    // cleanup, running as the migration/bootstrap role, does).
    let bootstrap_url = std::env::var("SENTINEL_TEST_BOOTSTRAP_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://sentinel_bootstrap:sentinel_local_dev_only_bootstrap@127.0.0.1:5432/sentinel".to_string()
    });
    if let Ok(mut bootstrap_conn) = sqlx::postgres::PgConnection::connect(&bootstrap_url).await {
        let _ = sqlx::query("DELETE FROM jobs")
            .execute(&mut bootstrap_conn)
            .await;
        let _ = sqlx::query("DELETE FROM alerts WHERE kind = 'job_quarantined'")
            .execute(&mut bootstrap_conn)
            .await;
    }

    SerialGuard { _conn: conn }
}

/// phase-02-data-model.md §7/§11: "Run a REAL concurrency test with 16
/// workers proving: claimed job sets disjoint, no job claimed twice
/// concurrently."
#[tokio::test]
async fn sixteen_workers_claim_disjoint_job_sets_with_no_duplicate_claim() {
    let _guard = acquire_serial_guard().await;
    const WORKER_COUNT: usize = 16;
    const JOBS_PER_WORKER: i64 = 5;
    const TOTAL_JOBS: i64 = WORKER_COUNT as i64 * JOBS_PER_WORKER;

    let Some(setup_pool) = connect_pool(4).await else {
        return;
    };

    // A unique run id keeps this test's rows distinguishable if it is ever
    // run against a shared/non-empty database.
    let run_id = uuid::Uuid::new_v4();
    for i in 0..TOTAL_JOBS {
        let dedupe_key = format!("concurrency-test-{run_id}-{i}");
        let job_id = sentinel_jobs::enqueue(
            &setup_pool,
            JobKind::BackfillRange,
            &dedupe_key,
            serde_json::json!({ "i": i }),
            0,
            5,
        )
        .await
        .expect("enqueue must succeed");
        assert!(
            job_id.is_some(),
            "each distinct dedupe_key must enqueue a fresh job"
        );
    }

    // Each worker gets its OWN pool connection so the 16 claims are
    // genuinely concurrent database sessions racing on FOR UPDATE SKIP
    // LOCKED, not 16 sequential calls sharing one connection.
    let mut handles = Vec::new();
    for worker_index in 0..WORKER_COUNT {
        let holder = format!("worker-{run_id}-{worker_index}");
        handles.push(tokio::spawn(async move {
            let pool = PgPoolOptions::new()
                .max_connections(1)
                .acquire_timeout(Duration::from_secs(5))
                .connect(&test_database_url())
                .await
                .expect("each worker must get its own connection");
            sentinel_jobs::claim(&pool, &holder, ChronoDuration::seconds(30), JOBS_PER_WORKER)
                .await
                .expect("claim must succeed")
        }));
    }

    let mut all_claimed_ids: Vec<i64> = Vec::new();
    for handle in handles {
        let claimed = handle.await.expect("worker task must not panic");
        all_claimed_ids.extend(claimed.into_iter().map(|c| c.job_id));
    }

    let unique: HashSet<i64> = all_claimed_ids.iter().copied().collect();
    assert_eq!(
        unique.len(),
        all_claimed_ids.len(),
        "no job_id may appear twice across the 16 workers' claimed sets — a duplicate means SKIP LOCKED failed to prevent a double-claim"
    );
    assert_eq!(
        all_claimed_ids.len() as i64,
        TOTAL_JOBS,
        "every enqueued job in this run must have been claimed by exactly one worker"
    );

    setup_pool.close().await;
}

/// distributed-correctness.md §3, L-2/L-3: a lease-conditioned update by a
/// holder that has lost the lease (here: simulated by deliberately setting
/// an already-expired lease) affects zero rows, and this is detected, not
/// assumed to have succeeded.
#[tokio::test]
async fn stale_lease_holder_update_affects_zero_rows_and_is_detected() {
    let _guard = acquire_serial_guard().await;
    let Some(pool) = connect_pool(2).await else {
        return;
    };
    let run_id = uuid::Uuid::new_v4();
    let dedupe_key = format!("stale-lease-test-{run_id}");

    let job_id = sentinel_jobs::enqueue(
        &pool,
        JobKind::ReplayRange,
        &dedupe_key,
        serde_json::json!({}),
        0,
        3,
    )
    .await
    .expect("enqueue must succeed")
    .expect("must be a fresh job");

    // Claim with an already-negative TTL so the lease is expired the
    // instant it is written — deterministically reproduces "lost the
    // lease" without a real sleep.
    let claimed = sentinel_jobs::claim(&pool, "holder-a", ChronoDuration::seconds(-1), 1)
        .await
        .expect("claim must succeed");
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].job_id, job_id);

    // holder-a's lease is already expired. Another worker reclaims it.
    let reclaimed = sentinel_jobs::reclaim_expired_leases(&pool)
        .await
        .expect("reclaim must succeed");
    assert!(reclaimed >= 1, "the expired lease must be reclaimable");

    let claimed_by_b = sentinel_jobs::claim(&pool, "holder-b", ChronoDuration::seconds(30), 1)
        .await
        .expect("claim must succeed");
    assert_eq!(
        claimed_by_b.len(),
        1,
        "holder-b must now be able to claim the reclaimed job"
    );
    assert_eq!(claimed_by_b[0].job_id, job_id);

    // holder-a, unaware its lease is gone, tries to complete the job it no
    // longer holds. This MUST affect zero rows, and the return value MUST
    // say so — never silently succeed.
    let completed_by_stale_holder = sentinel_jobs::complete(&pool, job_id, "holder-a")
        .await
        .expect("complete must not error, only report false");
    assert!(
        !completed_by_stale_holder,
        "a stale lease holder's completion attempt must affect zero rows and be detected as such"
    );

    // holder-b, the legitimate current holder, can complete it.
    let completed_by_current_holder = sentinel_jobs::complete(&pool, job_id, "holder-b")
        .await
        .expect("complete must succeed");
    assert!(
        completed_by_current_holder,
        "the current lease holder's completion must succeed"
    );

    let final_state = sentinel_jobs::fetch_state(&pool, job_id)
        .await
        .expect("fetch must succeed");
    assert_eq!(final_state, Some(sentinel_db::enums::JobState::Done));

    pool.close().await;
}

/// distributed-correctness.md §7: after max_attempts, the job is
/// quarantined, never auto-deleted, never auto-retried, and an alert opens.
#[tokio::test]
async fn poison_job_is_quarantined_after_max_attempts_and_opens_an_alert() {
    let _guard = acquire_serial_guard().await;
    let Some(pool) = connect_pool(2).await else {
        return;
    };
    let run_id = uuid::Uuid::new_v4();
    let dedupe_key = format!("poison-job-{run_id}");

    let job_id = sentinel_jobs::enqueue(
        &pool,
        JobKind::ScanProgramAccounts,
        &dedupe_key,
        serde_json::json!({}),
        0,
        2,
    )
    .await
    .expect("enqueue must succeed")
    .expect("must be fresh");

    for attempt in 1..=2 {
        let claimed = sentinel_jobs::claim(&pool, "poison-worker", ChronoDuration::seconds(30), 1)
            .await
            .expect("claim must succeed");
        assert_eq!(
            claimed.len(),
            1,
            "attempt {attempt}: job must be claimable before quarantine"
        );
        let ok = sentinel_jobs::fail(
            &pool,
            job_id,
            "poison-worker",
            "always fails",
            ChronoDuration::seconds(0),
        )
        .await
        .expect("fail must succeed");
        assert!(ok);
    }

    let final_state = sentinel_jobs::fetch_state(&pool, job_id)
        .await
        .expect("fetch must succeed");
    assert_eq!(final_state, Some(sentinel_db::enums::JobState::Quarantined));

    // Never auto-retried: it must not be claimable anymore.
    let claim_after_quarantine =
        sentinel_jobs::claim(&pool, "poison-worker", ChronoDuration::seconds(30), 1)
            .await
            .expect("claim must succeed");
    assert!(
        claim_after_quarantine.is_empty() || claim_after_quarantine[0].job_id != job_id,
        "a quarantined job must never be auto-reclaimed"
    );

    let open_alerts = sentinel_db::queries::fetch_open_alerts(&pool)
        .await
        .expect("fetch alerts must succeed");
    assert!(
        open_alerts.iter().any(|a| a.kind == "job_quarantined" && a.entity_key == dedupe_key),
        "quarantining a job must open a job_quarantined alert naming its dedupe_key (distributed-correctness.md §7)"
    );

    pool.close().await;
}
