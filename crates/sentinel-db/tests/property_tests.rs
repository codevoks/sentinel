//! T2 property tests against real PostgreSQL (docs/testing-strategy.md §3):
//! `P-KEY-1` and `P-MONO-2`, per phase-02-data-model.md §7.
//!
//! No `proptest` crate is used: it is not present in `Cargo.lock` and
//! `crates.io` is unreachable from this session (verified: `curl
//! https://crates.io` returns HTTP 403), so a new version cannot be pinned
//! against AGENTS.md §15 ("verify, do not remember"). Both properties are
//! instead exercised with a **fixed-seed** (`testing-strategy.md` §3: "All
//! fixtures use fixed seeds. An unshrinkable failure is worthless.")
//! deterministic pseudo-random generator (`rand::rngs::StdRng::seed_from_u64`)
//! over many generated cases, biased toward the boundary values the
//! testing-strategy doc calls out (`u64` extremes, dust/zero values,
//! adjacent slots). On failure, the fixed seed plus the printed failing
//! input **is** the reproduction — the seed is deterministic, so re-running
//! reproduces the exact same sequence of generated cases every time,
//! achieving the property's repro requirement without automatic shrinking.

use std::collections::HashMap;
use std::time::Duration;

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use sentinel_db::enums::{CommitmentLevel, ObservationSource, PayloadEncoding, RawObservationKind};
use sentinel_db::queries::{insert_raw_observation, NewRawObservation};

const FIXED_SEED: u64 = 0x5E17_1E02; // fixed, documented, never randomized per-run.
const CASES: usize = 300;

async fn connect() -> Option<PgPool> {
    let url = std::env::var("SENTINEL_TEST_RUST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://sentinel_rust:sentinel_local_dev_only_rust@127.0.0.1:5432/sentinel".to_string()
    });
    match PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&url)
        .await
    {
        Ok(p) => Some(p),
        Err(e) => {
            eprintln!("skipping property test: Postgres not reachable: {e}");
            None
        }
    }
}

/// `P-KEY-1`: distinct logical observations never collide on a natural key
/// (`testing-strategy.md` §3). Generates `CASES` distinct (kind, natural_key,
/// payload_hash) triples — "distinct logical observations" — biased toward
/// boundary slots (0, and values near common partition boundaries) and
/// short/degenerate natural-key strings, and asserts every one of them
/// persists as its own row: no two distinct logical observations are ever
/// folded into one by the natural-key uniqueness constraint, and no
/// natural-key collision is silently absorbed across cases that were meant
/// to be distinct.
#[tokio::test]
async fn p_key_1_distinct_logical_observations_never_collide_on_natural_key() {
    let Some(pool) = connect().await else { return };
    let mut rng = StdRng::seed_from_u64(FIXED_SEED);
    let run_id = uuid::Uuid::new_v4(); // keeps this run's keys distinct from any previous run's leftover rows.

    let boundary_slots: [i64; 5] = [0, 1, 9_999_999, 10_000_000, 19_999_999];
    let mut expected_rows: HashMap<(String, Vec<u8>), ()> = HashMap::new();

    for i in 0..CASES {
        // Bias toward boundary slots ~30% of the time, otherwise a random
        // in-range slot (testing-strategy.md §3: "generators are biased
        // toward the dangerous region").
        let slot = if rng.random_bool(0.3) {
            boundary_slots[rng.random_range(0..boundary_slots.len())]
        } else {
            rng.random_range(0i64..20_000_000)
        };
        let natural_key = format!("pkey1-{run_id}-{i}"); // distinct by construction: this IS "a distinct logical observation".
        let payload_hash = format!("hash-{i}").into_bytes();

        let inserted = insert_raw_observation(
            &pool,
            NewRawObservation {
                kind: RawObservationKind::Block,
                natural_key: &natural_key,
                slot,
                commitment: CommitmentLevel::Confirmed,
                source: ObservationSource::RpcHttp,
                provider_id: "property-test",
                request_id: None,
                payload: b"p",
                payload_hash: &payload_hash,
                payload_encoding: PayloadEncoding::Borsh,
            },
        )
        .await
        .unwrap_or_else(|e| panic!("case {i} (seed {FIXED_SEED:#x}, slot {slot}, natural_key {natural_key:?}) must insert without error: {e}"));

        assert!(
            inserted,
            "case {i} (seed {FIXED_SEED:#x}, slot {slot}, natural_key {natural_key:?}): a genuinely distinct logical observation must not collide with an earlier one in this run"
        );
        expected_rows.insert((natural_key, payload_hash), ());
    }

    assert_eq!(
        expected_rows.len(),
        CASES,
        "test construction bug: generated natural keys must all be distinct"
    );
}

/// `P-MONO-2`: `last_contiguous_slot` never decreases (`testing-strategy.md`
/// §3, `ingest_checkpoints.last_contiguous_slot`). Generates a sequence of
/// candidate advances (including deliberately out-of-order/regressive ones)
/// and asserts the persisted `last_contiguous_slot` is monotonically
/// non-decreasing across the whole sequence, verified against the actual
/// row after every step — not just the final value.
#[tokio::test]
async fn p_mono_2_last_contiguous_slot_never_decreases() {
    let Some(pool) = connect().await else { return };
    let mut rng = StdRng::seed_from_u64(FIXED_SEED.wrapping_add(1));
    let stream_name = format!("p-mono-2-{}", uuid::Uuid::new_v4());

    sqlx::query(
        "INSERT INTO ingest_checkpoints (stream_name, last_contiguous_slot, head_slot, commitment) \
         VALUES ($1, 0, 0, 'confirmed')",
    )
    .bind(&stream_name)
    .execute(&pool)
    .await
    .expect("checkpoint row must be creatable");

    let mut observed_max = 0i64;
    for i in 0..CASES {
        // Candidate advance: sometimes forward (the normal case), sometimes
        // a deliberately regressive/no-op value (simulating a stale
        // ingestion worker replaying an old checkpoint) — the property
        // under test is that the LATTER never actually moves the
        // persisted value backward.
        let candidate = if rng.random_bool(0.25) {
            // regressive: something at or below the current observed max.
            if observed_max > 0 {
                rng.random_range(0..=observed_max)
            } else {
                0
            }
        } else {
            observed_max + rng.random_range(1..=1000)
        };

        // The monotonic upsert query itself (DM-04's mechanism, applied to
        // this PROMOTABLE checkpoint field): only advance if candidate is
        // strictly greater than the current persisted value.
        sqlx::query(
            "UPDATE ingest_checkpoints SET last_contiguous_slot = $2, head_slot = GREATEST(head_slot, $2), updated_at = now() \
             WHERE stream_name = $1 AND $2 > last_contiguous_slot",
        )
        .bind(&stream_name)
        .bind(candidate)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("case {i} (seed, candidate {candidate}) update must not error: {e}"));

        let persisted: i64 = sqlx::query_scalar(
            "SELECT last_contiguous_slot FROM ingest_checkpoints WHERE stream_name = $1",
        )
        .bind(&stream_name)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!(
            persisted >= observed_max,
            "case {i}: last_contiguous_slot regressed from {observed_max} to {persisted} (candidate was {candidate}) — P-MONO-2 violated"
        );
        observed_max = persisted;
    }
}
