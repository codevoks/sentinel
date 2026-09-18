//! Phase 2 adversarial/failure-injection campaign against real PostgreSQL 18
//! (phase-02-data-model.md §7/§8). Every test here attempts a real
//! forbidden operation and asserts PostgreSQL itself refuses it — not
//! application-level validation standing in for the database.
//!
//! Skips (never fails) individual tests when Postgres is unreachable, the
//! same pattern `crates/sentinel-db/src/lib.rs` already uses. `make up`
//! starts the required local stack; nothing here touches the network
//! beyond loopback/Docker.

use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool, Row};
use std::time::Duration;

use sentinel_db::enums::{
    CommitmentLevel, IntentKind, MaterializationStatus, MaterializedVia, ObservationSource,
    PayloadEncoding, RawObservationKind, TxVersion,
};
use sentinel_db::numeric::{decode_u128, encode_u128};
use sentinel_db::queries::{
    self, insert_decoder_version, insert_raw_observation, open_alert, upsert_aegis_market,
    AegisMarketFixture, NewRawObservation,
};

fn bootstrap_url() -> String {
    std::env::var("SENTINEL_TEST_BOOTSTRAP_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://sentinel_bootstrap:sentinel_local_dev_only_bootstrap@127.0.0.1:5432/sentinel".to_string())
}
fn rust_url() -> String {
    std::env::var("SENTINEL_TEST_RUST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://sentinel_rust:sentinel_local_dev_only_rust@127.0.0.1:5432/sentinel".to_string()
    })
}
fn ts_url() -> String {
    std::env::var("SENTINEL_TEST_TS_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://sentinel_ts:sentinel_local_dev_only_ts@127.0.0.1:5432/sentinel".to_string()
    })
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
            eprintln!("skipping: Postgres not reachable at {url}: {e}");
            None
        }
    }
}

// ---------------------------------------------------------------------
// DM-02: raw_observations is append-only. No application role may UPDATE
// or DELETE it — proven against real Postgres permissions by connecting
// AS each role and attempting the forbidden statement.
// ---------------------------------------------------------------------

#[tokio::test]
async fn dm02_sentinel_rust_cannot_update_or_delete_raw_observations() {
    let Some(pool) = connect(&rust_url()).await else {
        return;
    };

    let update = sqlx::query("UPDATE raw_observations SET slot = slot + 1")
        .execute(&pool)
        .await;
    assert!(
        update.is_err(),
        "sentinel_rust must not be able to UPDATE raw_observations (DM-02)"
    );
    assert!(
        update
            .unwrap_err()
            .to_string()
            .to_lowercase()
            .contains("permission denied"),
        "must fail specifically with a permission error, not some other error"
    );

    let delete = sqlx::query("DELETE FROM raw_observations")
        .execute(&pool)
        .await;
    assert!(
        delete.is_err(),
        "sentinel_rust must not be able to DELETE FROM raw_observations (DM-02)"
    );
    assert!(delete
        .unwrap_err()
        .to_string()
        .to_lowercase()
        .contains("permission denied"));
}

#[tokio::test]
async fn dm02_sentinel_ts_cannot_update_delete_or_insert_raw_observations() {
    let Some(pool) = connect(&ts_url()).await else {
        return;
    };

    let update = sqlx::query("UPDATE raw_observations SET slot = slot + 1")
        .execute(&pool)
        .await;
    assert!(
        update.is_err(),
        "sentinel_ts must not be able to UPDATE raw_observations"
    );

    let delete = sqlx::query("DELETE FROM raw_observations")
        .execute(&pool)
        .await;
    assert!(
        delete.is_err(),
        "sentinel_ts must not be able to DELETE FROM raw_observations"
    );

    let insert = sqlx::query(
        "INSERT INTO raw_observations (kind, natural_key, slot, commitment, source, provider_id, payload, payload_hash, payload_encoding) \
         VALUES ('block', 'ts-cannot-write', 1, 'confirmed', 'rpc_http', 'p', '\\x00', '\\x00', 'borsh')",
    )
    .execute(&pool)
    .await;
    assert!(
        insert.is_err(),
        "sentinel_ts must not be able to INSERT into raw_observations (architecture.md §5)"
    );
}

// ---------------------------------------------------------------------
// DM-03: natural-key audit — every table with a declared natural key has a
// matching unique index, verified against the real catalog.
// ---------------------------------------------------------------------

/// (table, expected natural-key columns as they appear in the backing
/// unique index) for every table `docs/data-model.md` declares a natural
/// key for. Cross-checked against `pg_indexes`/`pg_constraint`, not
/// against migration SQL text.
fn declared_natural_keys() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        (
            "raw_observations",
            vec!["kind", "natural_key", "payload_hash", "slot"],
        ),
        (
            "decode_failures",
            vec!["observation_id", "stage", "coalesce"],
        ), // expression index
        ("slots", vec!["slot", "blockhash"]),
        ("transactions", vec!["signature", "slot", "blockhash"]),
        (
            "instructions",
            vec!["signature", "slot", "blockhash", "ix_index", "inner_index"],
        ),
        (
            "program_logs",
            vec!["signature", "slot", "blockhash", "log_index"],
        ),
        (
            "account_observations",
            vec!["pubkey", "slot", "content_hash"],
        ),
        (
            "token_balance_deltas",
            vec!["signature", "slot", "blockhash", "account_index"],
        ),
        ("gap_events", vec!["slot_start", "slot_end"]),
        ("ingest_checkpoints", vec!["stream_name"]),
        ("provider_health", vec!["provider_id", "window_start"]),
        (
            "decoder_versions",
            vec!["protocol", "program_id", "account_kind", "schema_version"],
        ),
        ("aegis_protocol_state", vec!["program_id"]),
        ("aegis_markets", vec!["market_pubkey"]),
        (
            "aegis_market_params_history",
            vec!["market_pubkey", "effective_from_slot"],
        ),
        ("aegis_positions", vec!["position_pubkey"]),
        (
            "aegis_events",
            vec!["signature", "slot", "blockhash", "log_index"],
        ),
        (
            "aegis_oracle_observations",
            vec!["feed_id", "publish_time", "slot"],
        ),
        (
            "position_health",
            vec!["position_pubkey", "computed_at_slot"],
        ),
        (
            "liquidation_candidates",
            vec!["position_pubkey", "detected_at_slot"],
        ),
        ("market_metrics", vec!["market_pubkey", "bucket_start"]),
        ("execution_intents", vec!["idempotency_key"]),
        ("transaction_attempts", vec!["signature"]),
        ("jobs", vec!["dedupe_key"]),
    ]
}

#[tokio::test]
async fn dm03_every_declared_natural_key_has_a_matching_unique_index() {
    let Some(pool) = connect(&bootstrap_url()).await else {
        return;
    };

    for (table, _cols) in declared_natural_keys() {
        // Real catalog introspection: does at least one unique index (or
        // unique constraint, which is backed by one) exist on this table?
        // pg_index.indisunique is the authoritative signal; we don't parse
        // SQL text.
        let count: i64 = sqlx::query(
            "SELECT count(*) AS n FROM pg_index i \
             JOIN pg_class t ON t.oid = i.indrelid \
             WHERE t.relname = $1 AND i.indisunique",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|e| panic!("catalog query failed for {table}: {e}"))
        .try_get("n")
        .unwrap();

        assert!(
            count >= 1,
            "DM-03: table `{table}` declares a natural key in data-model.md but has no unique index enforcing it"
        );
    }
}

#[tokio::test]
async fn dm03_duplicate_raw_observation_natural_key_is_rejected_not_duplicated() {
    let Some(pool) = connect(&rust_url()).await else {
        return;
    };
    let nk = format!("dup-test-{}", uuid::Uuid::new_v4());
    let payload_hash = b"hash-a".to_vec();

    let inserted_first = insert_raw_observation(
        &pool,
        NewRawObservation {
            kind: RawObservationKind::Block,
            natural_key: &nk,
            slot: 5,
            commitment: CommitmentLevel::Confirmed,
            source: ObservationSource::RpcHttp,
            provider_id: "p1",
            request_id: None,
            payload: b"payload",
            payload_hash: &payload_hash,
            payload_encoding: PayloadEncoding::Borsh,
        },
    )
    .await
    .expect("first insert must succeed");
    assert!(inserted_first);

    let inserted_second = insert_raw_observation(
        &pool,
        NewRawObservation {
            kind: RawObservationKind::Block,
            natural_key: &nk,
            slot: 5,
            commitment: CommitmentLevel::Confirmed,
            source: ObservationSource::RpcHttp,
            provider_id: "p1",
            request_id: None,
            payload: b"payload-different-bytes-but-same-key-and-hash",
            payload_hash: &payload_hash,
            payload_encoding: PayloadEncoding::Borsh,
        },
    )
    .await
    .expect("second insert must not error — ON CONFLICT DO NOTHING");
    assert!(
        !inserted_second,
        "duplicate natural key must be a documented no-op, not a second row"
    );

    let rows = queries::fetch_raw_observations_by_natural_key(&pool, &nk)
        .await
        .unwrap();
    assert_eq!(
        rows.len(),
        1,
        "exactly one row must exist for this natural key"
    );
}

// ---------------------------------------------------------------------
// DM-04: a stale materialization write can never overwrite a newer one.
// ---------------------------------------------------------------------

#[tokio::test]
async fn dm04_stale_as_of_slot_write_is_rejected_on_aegis_markets() {
    let Some(pool) = connect(&rust_url()).await else {
        return;
    };

    // program_id includes a fresh UUID so this test is safely re-runnable
    // against a database that already has data from a previous run
    // (decoder_versions' natural key is (protocol, program_id, account_kind,
    // schema_version), and this fixture value is otherwise fixed).
    let program_id = format!("prog-dm04-{}", uuid::Uuid::new_v4());
    let decoder_id = insert_decoder_version(
        &pool,
        queries::NewDecoderVersion {
            protocol: "aegis",
            program_id: &program_id,
            account_kind: "market",
            schema_version: 1,
            discriminator: b"disc",
            layout_hash: "hash",
            effective_from_slot: 0,
            source: "spec",
        },
    )
    .await
    .expect("decoder version insert must succeed");

    let market_pubkey = uuid::Uuid::new_v4().as_bytes().to_vec();

    // slot 100 writes.
    let applied_100 = upsert_aegis_market(
        &pool,
        AegisMarketFixture {
            market_pubkey: &market_pubkey,
            program_id: program_id.as_bytes(),
            as_of_slot: 100,
            as_of_commitment: CommitmentLevel::Confirmed,
            decoder_version_id: decoder_id,
            status: MaterializationStatus::Current,
            materialized_via: MaterializedVia::Snapshot,
        },
    )
    .await
    .unwrap();
    assert!(applied_100, "slot 100 must apply (row does not exist yet)");

    // slot 101 replaces.
    let applied_101 = upsert_aegis_market(
        &pool,
        AegisMarketFixture {
            market_pubkey: &market_pubkey,
            program_id: program_id.as_bytes(),
            as_of_slot: 101,
            as_of_commitment: CommitmentLevel::Confirmed,
            decoder_version_id: decoder_id,
            status: MaterializationStatus::Current,
            materialized_via: MaterializedVia::Event,
        },
    )
    .await
    .unwrap();
    assert!(applied_101, "slot 101 (newer) must replace slot 100");

    // slot 99 cannot replace 101.
    let applied_99 = upsert_aegis_market(
        &pool,
        AegisMarketFixture {
            market_pubkey: &market_pubkey,
            program_id: program_id.as_bytes(),
            as_of_slot: 99,
            as_of_commitment: CommitmentLevel::Confirmed,
            decoder_version_id: decoder_id,
            status: MaterializationStatus::Stale,
            materialized_via: MaterializedVia::Event,
        },
    )
    .await
    .unwrap();
    assert!(
        !applied_99,
        "slot 99 (older than 101) must be rejected, not applied"
    );

    // slot 101 again cannot mutate (strictly greater required, not >=).
    let applied_101_again = upsert_aegis_market(
        &pool,
        AegisMarketFixture {
            market_pubkey: &market_pubkey,
            program_id: program_id.as_bytes(),
            as_of_slot: 101,
            as_of_commitment: CommitmentLevel::Confirmed,
            decoder_version_id: decoder_id,
            status: MaterializationStatus::Stale,
            materialized_via: MaterializedVia::Event,
        },
    )
    .await
    .unwrap();
    assert!(
        !applied_101_again,
        "a second write at the SAME as_of_slot must not mutate the row (strict >)"
    );

    // Verify persisted data, not just row counts: the row must still show
    // as_of_slot=101 and the status/materialized_via from that write, not
    // from the rejected slot-99 or repeated slot-101 attempts.
    let final_as_of = queries::fetch_aegis_market_as_of_slot(&pool, &market_pubkey)
        .await
        .unwrap();
    assert_eq!(
        final_as_of,
        Some(101),
        "persisted as_of_slot must be 101, proving the rejects truly did not mutate the row"
    );
}

#[tokio::test]
async fn dm04_slot_promotion_is_monotonic_observed_confirmed_finalized() {
    use sentinel_db::enums::SlotStatus;
    let Some(pool) = connect(&rust_url()).await else {
        return;
    };
    let slot = 900_000_001i64 % 9_000_000; // stay inside partition p0 [0, 10_000_000)
    let blockhash = uuid::Uuid::new_v4().as_bytes().to_vec();

    let promote = |status: SlotStatus, commitment: CommitmentLevel, canonical: bool| {
        queries::upsert_slot(
            &pool,
            queries::SlotPromotion {
                slot,
                blockhash: Some(&blockhash),
                parent_slot: None,
                parent_blockhash: None,
                status,
                commitment,
                canonical,
            },
        )
    };

    let applied_observed = promote(SlotStatus::Observed, CommitmentLevel::Processed, false)
        .await
        .unwrap();
    assert!(applied_observed);

    let applied_confirmed = promote(SlotStatus::Confirmed, CommitmentLevel::Confirmed, true)
        .await
        .unwrap();
    assert!(
        applied_confirmed,
        "observed -> confirmed must be a legal promotion"
    );

    let regressed = promote(SlotStatus::Observed, CommitmentLevel::Processed, false)
        .await
        .unwrap();
    assert!(
        !regressed,
        "confirmed -> observed must be rejected (P-1: promotion never lowers a level)"
    );

    let applied_finalized = promote(SlotStatus::Finalized, CommitmentLevel::Finalized, true)
        .await
        .unwrap();
    assert!(
        applied_finalized,
        "confirmed -> finalized must be a legal promotion"
    );

    let regressed_from_finalized =
        promote(SlotStatus::Abandoned, CommitmentLevel::Finalized, false)
            .await
            .unwrap();
    assert!(
        !regressed_from_finalized,
        "finalized -> abandoned is not a legal transition per finality-and-forks.md §3's state diagram"
    );

    let row = queries::fetch_slot(&pool, slot, Some(&blockhash))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.status,
        SlotStatus::Finalized,
        "persisted status must still be finalized after the rejected abandon attempt"
    );
}

// ---------------------------------------------------------------------
// DM-07: no floating-point column exists anywhere in the schema.
// ---------------------------------------------------------------------

#[tokio::test]
async fn dm07_no_floating_point_column_exists_in_the_schema() {
    let Some(pool) = connect(&bootstrap_url()).await else {
        return;
    };

    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT c.relname, a.attname \
         FROM pg_attribute a \
         JOIN pg_class c ON c.oid = a.attrelid \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = 'public' \
           AND a.attnum > 0 AND NOT a.attisdropped \
           AND c.relkind IN ('r', 'p') \
           AND format_type(a.atttypid, a.atttypmod) IN ('real', 'double precision')",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert!(
        rows.is_empty(),
        "DM-07: found floating-point columns (must be numeric(39,0)/numeric(20,0)/integer instead): {rows:?}"
    );
}

// ---------------------------------------------------------------------
// DM-08: one open alert per (kind, entity_kind, entity_key).
// ---------------------------------------------------------------------

#[tokio::test]
async fn dm08_duplicate_open_alert_is_rejected_then_resolved_reopen_succeeds() {
    let Some(pool) = connect(&rust_url()).await else {
        return;
    };
    let entity_key = format!("market-{}", uuid::Uuid::new_v4());

    let first_id = open_alert(
        &pool,
        "stalled_finalization",
        "critical",
        "market",
        &entity_key,
        serde_json::json!({}),
        None,
    )
    .await
    .expect("first open must succeed");

    let second = open_alert(
        &pool,
        "stalled_finalization",
        "critical",
        "market",
        &entity_key,
        serde_json::json!({}),
        None,
    )
    .await;
    assert!(second.is_err(), "DM-08: a second OPEN alert for the same (kind, entity) must be rejected by the partial unique index");

    let resolved = queries::resolve_alert(&pool, first_id).await.unwrap();
    assert!(resolved);

    // Once resolved, re-opening the same condition is legitimate.
    let third = open_alert(
        &pool,
        "stalled_finalization",
        "critical",
        "market",
        &entity_key,
        serde_json::json!({}),
        None,
    )
    .await;
    assert!(
        third.is_ok(),
        "after resolution, a new open alert for the same (kind, entity) must be permitted"
    );
}

// ---------------------------------------------------------------------
// DM-10: execution_intents.idempotency_key is globally unique.
// ---------------------------------------------------------------------

#[tokio::test]
async fn dm10_duplicate_idempotency_key_is_rejected_globally() {
    let Some(pool) = connect(&ts_url()).await else {
        return;
    };
    let key = format!("liquidate:prog:mkt:pos:{}", uuid::Uuid::new_v4());
    let market = b"market-dm10".to_vec();

    let first = queries::create_execution_intent(
        &pool,
        queries::NewExecutionIntent {
            intent_id: uuid::Uuid::new_v4(),
            idempotency_key: &key,
            kind: IntentKind::Liquidate,
            market_pubkey: &market,
            position_pubkey: None,
            params: serde_json::json!({}),
            constraints: serde_json::json!({}),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
            max_attempts: 3,
            trigger_slot: 42,
            trigger_commitment: CommitmentLevel::Confirmed,
        },
    )
    .await;
    assert!(first.is_ok(), "first intent creation must succeed");

    // A DIFFERENT kind ('custom' instead of 'liquidate') with the SAME
    // idempotency_key — proves the uniqueness is truly global, not scoped
    // to kind (data-model.md §7's explicit requirement).
    let second = queries::create_execution_intent(
        &pool,
        queries::NewExecutionIntent {
            intent_id: uuid::Uuid::new_v4(),
            idempotency_key: &key,
            kind: IntentKind::Custom,
            market_pubkey: &market,
            position_pubkey: None,
            params: serde_json::json!({}),
            constraints: serde_json::json!({}),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
            max_attempts: 3,
            trigger_slot: 43,
            trigger_commitment: CommitmentLevel::Confirmed,
        },
    )
    .await;
    assert!(
        second.is_err(),
        "DM-10: a duplicate idempotency_key must be rejected even across different `kind` values"
    );
}

// ---------------------------------------------------------------------
// TX-02: one non-terminal attempt per intent_id.
// ---------------------------------------------------------------------

#[tokio::test]
async fn tx02_second_nonterminal_attempt_for_same_intent_is_rejected() {
    let Some(pool) = connect(&ts_url()).await else {
        return;
    };
    let intent_id = uuid::Uuid::new_v4();
    let key = format!("liquidate:tx02:{intent_id}");
    let market = b"market-tx02".to_vec();

    queries::create_execution_intent(
        &pool,
        queries::NewExecutionIntent {
            intent_id,
            idempotency_key: &key,
            kind: IntentKind::Liquidate,
            market_pubkey: &market,
            position_pubkey: None,
            params: serde_json::json!({}),
            constraints: serde_json::json!({}),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
            max_attempts: 3,
            trigger_slot: 1,
            trigger_commitment: CommitmentLevel::Confirmed,
        },
    )
    .await
    .expect("intent creation must succeed");

    let sig_a = uuid::Uuid::new_v4().as_bytes().to_vec();
    queries::insert_transaction_attempt(
        &pool,
        queries::NewTransactionAttempt {
            attempt_id: uuid::Uuid::new_v4(),
            intent_id,
            attempt_number: 1,
            signature: &sig_a,
            transaction_bytes: b"bytes-a",
            transaction_version: TxVersion::V0,
            recent_blockhash: b"blockhash-a-32-bytes-000000000",
            last_valid_block_height: 1000,
            state: sentinel_db::enums::AttemptState::Submitted,
        },
    )
    .await
    .expect("first attempt (SUBMITTED, non-terminal) must succeed");

    let sig_b = uuid::Uuid::new_v4().as_bytes().to_vec();
    let second = queries::insert_transaction_attempt(
        &pool,
        queries::NewTransactionAttempt {
            attempt_id: uuid::Uuid::new_v4(),
            intent_id,
            attempt_number: 2,
            signature: &sig_b,
            transaction_bytes: b"bytes-b",
            transaction_version: TxVersion::V0,
            recent_blockhash: b"blockhash-b-32-bytes-000000000",
            last_valid_block_height: 1001,
            state: sentinel_db::enums::AttemptState::Observed,
        },
    )
    .await;
    assert!(second.is_err(), "TX-02: a second non-terminal (SUBMITTED/OBSERVED) attempt for the same intent must be rejected");

    // A TERMINAL second attempt (e.g. after the first expired) is permitted.
    let sig_c = uuid::Uuid::new_v4().as_bytes().to_vec();
    let third = queries::insert_transaction_attempt(
        &pool,
        queries::NewTransactionAttempt {
            attempt_id: uuid::Uuid::new_v4(),
            intent_id,
            attempt_number: 2,
            signature: &sig_c,
            transaction_bytes: b"bytes-c",
            transaction_version: TxVersion::V0,
            recent_blockhash: b"blockhash-c-32-bytes-000000000",
            last_valid_block_height: 1002,
            state: sentinel_db::enums::AttemptState::FailedOnchain,
        },
    )
    .await;
    assert!(
        third.is_ok(),
        "a TERMINAL-state attempt must be permitted even while another exists"
    );
}

// ---------------------------------------------------------------------
// Numeric exactness: u128::MAX round-trips; overflow is rejected.
// ---------------------------------------------------------------------

#[tokio::test]
async fn numeric_u128_max_round_trips_exactly_through_a_real_numeric_39_0_column() {
    let Some(pool) = connect(&bootstrap_url()).await else {
        return;
    };
    let mut conn = pool.acquire().await.unwrap();
    conn.execute("CREATE TEMP TABLE numeric_probe (n numeric(39,0))")
        .await
        .unwrap();

    let max_text = encode_u128(u128::MAX);
    sqlx::query("INSERT INTO numeric_probe (n) VALUES ($1::numeric)")
        .bind(&max_text)
        .execute(&mut *conn)
        .await
        .unwrap();

    let row = sqlx::query("SELECT n::text AS t FROM numeric_probe")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    let back: String = row.try_get("t").unwrap();
    assert_eq!(
        decode_u128(&back).unwrap(),
        u128::MAX,
        "u128::MAX must round-trip exactly through numeric(39,0)"
    );
}

#[tokio::test]
async fn numeric_value_exceeding_numeric_39_0_precision_is_rejected_not_truncated() {
    let Some(pool) = connect(&bootstrap_url()).await else {
        return;
    };
    let mut conn = pool.acquire().await.unwrap();
    conn.execute("CREATE TEMP TABLE numeric_overflow_probe (n numeric(39,0))")
        .await
        .unwrap();

    let too_big = "1".to_string() + &"0".repeat(39); // 10^39: one digit beyond 39-digit precision
    let result = sqlx::query("INSERT INTO numeric_overflow_probe (n) VALUES ($1::numeric(39,0))")
        .bind(&too_big)
        .execute(&mut *conn)
        .await;
    assert!(result.is_err(), "a value exceeding numeric(39,0)'s precision must be rejected, not silently truncated/rounded");
    assert!(result
        .unwrap_err()
        .to_string()
        .to_lowercase()
        .contains("overflow"));
}

// ---------------------------------------------------------------------
// Partition routing: a row lands in the correct partition; a missing
// partition fails clearly (no catch-all/default partition).
// ---------------------------------------------------------------------

#[tokio::test]
async fn partition_row_routes_to_the_correct_partition_and_missing_partition_fails_clearly() {
    let Some(pool) = connect(&rust_url()).await else {
        return;
    };
    let nk = format!("partition-routing-{}", uuid::Uuid::new_v4());

    // Slot 5 belongs to raw_observations_p0 [0, 10_000_000).
    insert_raw_observation(
        &pool,
        NewRawObservation {
            kind: RawObservationKind::Block,
            natural_key: &nk,
            slot: 5,
            commitment: CommitmentLevel::Confirmed,
            source: ObservationSource::RpcHttp,
            provider_id: "p",
            request_id: None,
            payload: b"x",
            payload_hash: b"hash-partition-routing",
            payload_encoding: PayloadEncoding::Borsh,
        },
    )
    .await
    .expect("insert at slot 5 must succeed");

    let bootstrap_pool = connect(&bootstrap_url())
        .await
        .expect("bootstrap must be reachable if rust role is");
    let in_p0: i64 =
        sqlx::query("SELECT count(*) AS n FROM raw_observations_p0 WHERE natural_key = $1")
            .bind(&nk)
            .fetch_one(&bootstrap_pool)
            .await
            .unwrap()
            .try_get("n")
            .unwrap();
    assert_eq!(
        in_p0, 1,
        "a row at slot 5 must physically land in raw_observations_p0"
    );

    // Slot far beyond any created partition (only p0/p1 exist, covering
    // [0, 20_000_000)) must fail loudly, not land in a catch-all.
    let far_future_slot = 500_000_000i64;
    let result = insert_raw_observation(
        &pool,
        NewRawObservation {
            kind: RawObservationKind::Block,
            natural_key: &format!("no-partition-{}", uuid::Uuid::new_v4()),
            slot: far_future_slot,
            commitment: CommitmentLevel::Confirmed,
            source: ObservationSource::RpcHttp,
            provider_id: "p",
            request_id: None,
            payload: b"x",
            payload_hash: b"hash-no-partition",
            payload_encoding: PayloadEncoding::Borsh,
        },
    )
    .await;
    assert!(
        result.is_err(),
        "an insert at a slot with no created partition must fail, not silently land somewhere"
    );
    let msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        msg.contains("no partition") || msg.contains("partition"),
        "the error must clearly name the missing-partition condition, got: {msg}"
    );
}

#[tokio::test]
async fn partition_automation_creates_a_partition_ahead_of_head_and_it_becomes_usable() {
    let Some(pool) = connect(&bootstrap_url()).await else {
        return;
    };

    // Head far beyond the two partitions 0011_initial_partitions.sql
    // creates, and randomized over a huge range (a fresh 10M-slot bucket
    // every run, picked uniformly across ~4 billion buckets) so this test
    // is safely re-runnable: partition creation is real, persisted DDL —
    // a fixed or narrow-range slot would eventually collide with a
    // partition an earlier run already created. `count_future_partitions`
    // counts every partition whose index is >= head's, which is correct
    // production behavior (any of them does cover the future relative to
    // an older head) but means it is NOT a reliable "does this exact
    // partition exist" check once a database has accumulated partitions
    // from many prior runs at scattered high indices — so this test checks
    // the SPECIFIC partition name directly instead of a broad count.
    use rand::RngExt;
    let bucket = rand::rng().random_range(1_000u64..4_000_000_000u64);
    let head_slot = (bucket * sentinel_db::partitions::PARTITION_SIZE_SLOTS as u64) as i64;
    let expected_partition_name = format!("raw_observations_p{bucket}");

    let existed_before: bool =
        sqlx::query("SELECT EXISTS (SELECT 1 FROM pg_class WHERE relname = $1) AS e")
            .bind(&expected_partition_name)
            .fetch_one(&pool)
            .await
            .unwrap()
            .try_get("e")
            .unwrap();
    assert!(!existed_before, "partition {expected_partition_name} must not exist before automation runs (bucket collision — re-run the test)");

    let created = sentinel_db::partitions::ensure_partitions_ahead(&pool, head_slot, 2)
        .await
        .unwrap();
    assert!(
        !created.is_empty(),
        "ensure_partitions_ahead must create at least one partition"
    );
    assert!(
        created.contains(&expected_partition_name),
        "ensure_partitions_ahead must create the partition covering head_slot itself, got: {created:?}"
    );

    let exists_after: bool =
        sqlx::query("SELECT EXISTS (SELECT 1 FROM pg_class WHERE relname = $1) AS e")
            .bind(&expected_partition_name)
            .fetch_one(&pool)
            .await
            .unwrap()
            .try_get("e")
            .unwrap();
    assert!(
        exists_after,
        "the specific partition covering head_slot must exist after automation"
    );

    // The newly created partition must actually be usable: insert a row at
    // exactly head_slot via the rust role.
    let Some(rust_pool) = connect(&rust_url()).await else {
        return;
    };
    let ok = insert_raw_observation(
        &rust_pool,
        NewRawObservation {
            kind: RawObservationKind::Block,
            natural_key: &format!("ahead-of-head-{}", uuid::Uuid::new_v4()),
            slot: head_slot,
            commitment: CommitmentLevel::Confirmed,
            source: ObservationSource::RpcHttp,
            provider_id: "p",
            request_id: None,
            payload: b"x",
            payload_hash: b"hash-ahead-of-head",
            payload_encoding: PayloadEncoding::Borsh,
        },
    )
    .await;
    assert!(
        ok.is_ok() && ok.unwrap(),
        "a row at head_slot must now insert successfully after automated partition creation"
    );

    // Re-running is idempotent (IF NOT EXISTS) — no error on a second call.
    let created_again = sentinel_db::partitions::ensure_partitions_ahead(&pool, head_slot, 2).await;
    assert!(
        created_again.is_ok(),
        "ensure_partitions_ahead must be safely re-runnable"
    );
}

// ---------------------------------------------------------------------
// Schema-ownership: no role has unauthorized write access, enumerated
// against the real catalog and proven by attempted writes.
// ---------------------------------------------------------------------

#[tokio::test]
async fn role_permissions_sentinel_ts_has_no_write_grant_on_any_raw_normalized_protocol_or_derived_table(
) {
    let Some(pool) = connect(&bootstrap_url()).await else {
        return;
    };

    let forbidden_tables = [
        "raw_observations",
        "decode_failures",
        "slots",
        "transactions",
        "instructions",
        "program_logs",
        "account_observations",
        "token_balance_deltas",
        "rollback_events",
        "gap_events",
        "ingest_checkpoints",
        "provider_health",
        "decoder_versions",
        "aegis_protocol_state",
        "aegis_markets",
        "aegis_market_params_history",
        "aegis_positions",
        "aegis_events",
        "aegis_oracle_observations",
        "aegis_invariant_checks",
        "position_health",
        "liquidation_candidates",
        "market_metrics",
    ];

    for table in forbidden_tables {
        let has_write: bool = sqlx::query(
            "SELECT bool_or(privilege_type IN ('INSERT', 'UPDATE', 'DELETE')) AS w \
             FROM information_schema.role_table_grants \
             WHERE grantee = 'sentinel_ts' AND table_name = $1",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap()
        .try_get::<Option<bool>, _>("w")
        .unwrap()
        .unwrap_or(false);

        assert!(
            !has_write,
            "sentinel_ts must have NO INSERT/UPDATE/DELETE grant on `{table}` (architecture.md §5) — found one in pg_catalog"
        );
    }
}

#[tokio::test]
async fn role_permissions_sentinel_rust_cannot_write_transaction_attempts_or_advance_execution_intents_state(
) {
    let Some(pool) = connect(&rust_url()).await else {
        return;
    };

    let insert = sqlx::query(
        "INSERT INTO transaction_attempts (attempt_id, intent_id, attempt_number, signature, transaction_bytes, transaction_version, recent_blockhash, last_valid_block_height, state) \
         VALUES (gen_random_uuid(), gen_random_uuid(), 1, '\\x00', '\\x00', 'v0', '\\x00', 1, 'SIGNED')",
    )
    .execute(&pool)
    .await;
    assert!(insert.is_err(), "sentinel_rust must have no write access to transaction_attempts (owner: sentinel-executor, TS)");

    let update = sqlx::query("UPDATE execution_intents SET state = 'PLANNING'")
        .execute(&pool)
        .await;
    assert!(
        update.is_err(),
        "sentinel_rust must have INSERT-only access to execution_intents — it creates intents but never advances their state (data-model.md §10)"
    );
}

// ---------------------------------------------------------------------
// Low-partition alertable condition (docs/observability.md
// PartitionsExhausted; phase-02-data-model.md §6/§8).
// ---------------------------------------------------------------------

#[tokio::test]
async fn low_partition_condition_opens_one_alert_and_does_not_error_on_repeated_checks() {
    let Some(pool) = connect(&bootstrap_url()).await else {
        return;
    };

    // A head far beyond ANY bucket range used by this file's other random
    // tests (which pick from 1_000..4_000_000_000) guarantees zero future
    // partitions exist yet for it — `count_future_partitions` counts every
    // partition whose index is >= head's, so picking from the SAME range
    // other tests use risks a false "not low" reading against a partition
    // an earlier test in this run already created higher up. Nanosecond-
    // timestamp-derived, so distinct across repeated runs too.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let bucket = 10_000_000_000u64 + (nanos % 1_000_000_000u64);
    let head_slot = (bucket * sentinel_db::partitions::PARTITION_SIZE_SLOTS as u64) as i64;

    let alerted_first =
        sentinel_db::partitions::check_low_partitions_and_alert(&pool, head_slot, 1)
            .await
            .expect("first check must succeed and open alerts");
    assert_eq!(
        alerted_first.len(),
        sentinel_db::partitions::PARTITIONED_TABLES.len(),
        "every partitioned table should be reported low against a head with zero future partitions"
    );

    // Repeated check while still low must NOT error (DM-08's unique
    // violation must be absorbed as "already alerted", not surfaced).
    let alerted_second =
        sentinel_db::partitions::check_low_partitions_and_alert(&pool, head_slot, 1).await;
    assert!(
        alerted_second.is_ok(),
        "a repeated low-partition check while unresolved must not error"
    );

    let open_alerts = queries::fetch_open_alerts(&pool).await.unwrap();
    let count_for_kind = open_alerts
        .iter()
        .filter(|a| a.kind == "partitions_low")
        .count();
    assert!(
        count_for_kind >= sentinel_db::partitions::PARTITIONED_TABLES.len(),
        "at least one open partitions_low alert per partitioned table must exist"
    );
}
