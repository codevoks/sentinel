//! Closure-fix round-trip campaign: proves the newly-added typed access
//! layer (crates/sentinel-db/src/{tables,queries}.rs) actually works against
//! real PostgreSQL — one representative insert/read round-trip per newly
//! covered table, across every schema layer (normalized, chain-state,
//! protocol, derived, execution), plus a dedicated `u128::MAX` exactness
//! regression on two of the newly-covered `numeric(39,0)` columns
//! (`token_balance_deltas.pre_amount`, `aegis_positions.supply_shares`) and
//! a permission proof that the new append-only tables have no UPDATE grant
//! for `sentinel_rust`, matching `infra/migrations/0010_grants.sql` exactly.
//!
//! Skips (never fails) when Postgres is unreachable — same pattern as
//! `tests/adversarial.rs`.

use chrono::Utc;
use rand::RngExt;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

use sentinel_db::enums::{
    AccountObservationSource, CommitmentLevel, HealthState, LogKind, MaterializationStatus,
    MaterializedVia, MismatchClass, OracleValidationResult, SlotStatus,
};
use sentinel_db::numeric::decode_u128;
use sentinel_db::queries::{
    self, insert_account_observation, insert_aegis_event, insert_aegis_invariant_check,
    insert_aegis_market_params_history, insert_aegis_oracle_observation, insert_decoder_version,
    insert_instruction, insert_liquidation_candidate, insert_position_health, insert_program_log,
    insert_reconciliation_mismatch, insert_rollback_event, insert_token_balance_delta,
    record_gap_event, upsert_aegis_market, upsert_aegis_position, upsert_aegis_protocol_state,
    upsert_ingest_checkpoint, upsert_market_metrics, upsert_provider_health, upsert_slot,
    AegisMarketFixture, AegisMarketParamsHistoryFixture, AegisPositionFixture,
    AegisProtocolStateFixture, MarketMetricUpsert, NewAccountObservation, NewAegisEvent,
    NewAegisInvariantCheck, NewAegisOracleObservation, NewInstruction, NewLiquidationCandidate,
    NewPositionHealth, NewProgramLog, NewReconciliationMismatch, NewTokenBalanceDelta,
    ProviderHealthUpdate, SlotPromotion,
};

fn rust_url() -> String {
    std::env::var("SENTINEL_TEST_RUST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://sentinel_rust:sentinel_local_dev_only_rust@127.0.0.1:5432/sentinel".to_string()
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

fn rand_bytes(n: usize) -> Vec<u8> {
    let mut rng = rand::rng();
    (0..n).map(|_| rng.random::<u8>()).collect()
}

/// Builds one full raw_observations -> slots -> transactions fixture chain
/// (the FK prerequisites every normalized-layer table below needs), and
/// returns (signature, slot, blockhash) for reuse.
async fn seed_transaction_fixture(pool: &PgPool, slot: i64) -> (Vec<u8>, i64, Vec<u8>) {
    let blockhash = rand_bytes(32);
    let signature = rand_bytes(64);

    upsert_slot(
        pool,
        SlotPromotion {
            slot,
            blockhash: Some(&blockhash),
            parent_slot: Some(slot - 1),
            parent_blockhash: None,
            status: SlotStatus::Finalized,
            commitment: CommitmentLevel::Finalized,
            canonical: true,
        },
    )
    .await
    .expect("seed slot must insert");

    sqlx::query(
        "INSERT INTO transactions \
           (signature, slot, blockhash, transaction_index, version, num_required_signatures, \
            recent_blockhash, success, fee_lamports, priority_fee_source, \
            loaded_addresses_from_alt, is_vote, commitment, canonical) \
         VALUES ($1, $2, $3, 0, 'v0', 1, $3, true, 5000, 'absent', false, false, 'finalized', true)",
    )
    .bind(&signature)
    .bind(slot)
    .bind(&blockhash)
    .execute(pool)
    .await
    .expect("seed transaction must insert");

    (signature, slot, blockhash)
}

// ---------------------------------------------------------------------
// Normalized layer additions
// ---------------------------------------------------------------------

#[tokio::test]
async fn instructions_program_logs_round_trip() {
    let Some(pool) = connect(&rust_url()).await else {
        return;
    };
    let slot = 5_000_000 + (rand_bytes(2)[0] as i64);
    let (signature, slot, blockhash) = seed_transaction_fixture(&pool, slot).await;

    let program_id = rand_bytes(32);
    let inserted = insert_instruction(
        &pool,
        NewInstruction {
            signature: &signature,
            slot,
            blockhash: &blockhash,
            ix_index: 0,
            inner_index: -1,
            stack_height: 1,
            program_id: &program_id,
            accounts: &[rand_bytes(32)],
            data: &[1, 2, 3],
        },
    )
    .await
    .expect("insert_instruction must succeed");
    assert!(inserted, "first insert of a new instruction must apply");

    // DO NOTHING re-insert of the exact same natural key changes nothing.
    let reinserted = insert_instruction(
        &pool,
        NewInstruction {
            signature: &signature,
            slot,
            blockhash: &blockhash,
            ix_index: 0,
            inner_index: -1,
            stack_height: 1,
            program_id: &program_id,
            accounts: &[rand_bytes(32)],
            data: &[9, 9, 9],
        },
    )
    .await
    .expect("duplicate insert must not error, per DO NOTHING");
    assert!(
        !reinserted,
        "duplicate (signature,slot,blockhash,ix_index,inner_index) must be a no-op"
    );

    let rows = queries::fetch_instructions_for_transaction(&pool, &signature, slot, &blockhash)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].data,
        vec![1, 2, 3],
        "DO NOTHING must have kept the original row"
    );

    let log_inserted = insert_program_log(
        &pool,
        NewProgramLog {
            signature: &signature,
            slot,
            blockhash: &blockhash,
            log_index: 0,
            program_id: Some(&program_id),
            raw_line: "Program log: test",
            kind: LogKind::Log,
        },
    )
    .await
    .unwrap();
    assert!(log_inserted);
    let logs = queries::fetch_program_logs_for_transaction(&pool, &signature, slot, &blockhash)
        .await
        .unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].raw_line, "Program log: test");
}

#[tokio::test]
async fn account_observations_round_trip() {
    let Some(pool) = connect(&rust_url()).await else {
        return;
    };
    let pubkey = rand_bytes(32);
    let content_hash = rand_bytes(32);
    let owner_program = rand_bytes(32);
    let slot = 5_100_000 + (rand_bytes(2)[0] as i64);

    let inserted = insert_account_observation(
        &pool,
        NewAccountObservation {
            pubkey: &pubkey,
            slot,
            content_hash: &content_hash,
            owner_program: &owner_program,
            lamports: 1_000_000_000,
            data: &[0xAB; 16],
            executable: false,
            rent_epoch: 500,
            write_version: None,
            source: AccountObservationSource::Rpc,
            commitment: CommitmentLevel::Confirmed,
        },
    )
    .await
    .unwrap();
    assert!(inserted);

    let fetched = queries::fetch_latest_account_observation(&pool, &pubkey)
        .await
        .unwrap()
        .expect("must read back the row just inserted");
    assert_eq!(
        sentinel_db::numeric::decode_u64(&fetched.lamports).unwrap(),
        1_000_000_000
    );
}

/// token_balance_deltas.pre_amount/post_amount are numeric(39,0) — the
/// dedicated u128::MAX exactness regression the closure-fix task requires
/// for a newly-covered table with a u128 column.
#[tokio::test]
async fn token_balance_deltas_round_trip_and_u128_max_is_exact() {
    let Some(pool) = connect(&rust_url()).await else {
        return;
    };
    let slot = 5_200_000 + (rand_bytes(2)[0] as i64);
    let (signature, slot, blockhash) = seed_transaction_fixture(&pool, slot).await;
    let token_account = rand_bytes(32);
    let mint = rand_bytes(32);
    let owner = rand_bytes(32);
    let program_id = rand_bytes(32);

    let inserted = insert_token_balance_delta(
        &pool,
        NewTokenBalanceDelta {
            signature: &signature,
            slot,
            blockhash: &blockhash,
            account_index: 0,
            token_account: &token_account,
            mint: &mint,
            owner: &owner,
            pre_amount: 0,
            post_amount: u128::MAX,
            decimals: 9,
            program_id: &program_id,
        },
    )
    .await
    .unwrap();
    assert!(inserted);

    let rows =
        queries::fetch_token_balance_deltas_for_transaction(&pool, &signature, slot, &blockhash)
            .await
            .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        decode_u128(&rows[0].post_amount).unwrap(),
        u128::MAX,
        "numeric(39,0) must round-trip u128::MAX exactly through the newly-added typed layer"
    );
}

// ---------------------------------------------------------------------
// Chain-state layer additions
// ---------------------------------------------------------------------

#[tokio::test]
async fn rollback_events_gap_events_ingest_checkpoints_provider_health_round_trip() {
    let Some(pool) = connect(&rust_url()).await else {
        return;
    };

    let rollback_id = insert_rollback_event(
        &pool,
        queries::NewRollbackEvent {
            slot_low: 100,
            slot_high: 105,
            abandoned_block_count: 2,
            depth_slots: 5,
            cause: "test reorg",
        },
    )
    .await
    .unwrap();
    assert!(rollback_id > 0);
    let found = queries::fetch_rollback_events_in_slot_range(&pool, 100, 105)
        .await
        .unwrap();
    assert!(!found.is_empty());

    // Wide, timestamp-derived entropy so this natural key never collides
    // with another concurrently-running test or a previous run's leftover
    // row against the same real, persisted database (gap_events is not
    // partitioned, so any i64 slot range is valid) — same fix class already
    // applied to crates/sentinel-db/tests/adversarial.rs (see
    // docs/project-status.md DEVIATIONS, finding 3).
    let slot_start = 900_002_000_000
        + (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64
            % 100_000_000);
    let slot_end = slot_start + 10;
    record_gap_event(&pool, slot_start, slot_end, "provider timeout")
        .await
        .unwrap();
    // DO UPDATE SET attempts = attempts + 1 on a repeat.
    record_gap_event(&pool, slot_start, slot_end, "provider timeout again")
        .await
        .unwrap();
    let unrepaired = queries::fetch_unrepaired_gap_events(&pool).await.unwrap();
    let this_gap = unrepaired
        .iter()
        .find(|g| g.slot_start == slot_start && g.slot_end == slot_end)
        .expect("gap must be present");
    assert_eq!(
        this_gap.attempts, 1,
        "second enqueue must increment attempts via DO UPDATE"
    );
    let repaired = queries::mark_gap_event_repaired(&pool, slot_start, slot_end)
        .await
        .unwrap();
    assert!(repaired);

    let stream = format!(
        "test-stream-{}",
        rand_bytes(4)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    upsert_ingest_checkpoint(
        &pool,
        &stream,
        1000,
        1010,
        CommitmentLevel::Confirmed,
        Some("worker-1"),
    )
    .await
    .unwrap();
    let cp = queries::fetch_ingest_checkpoint(&pool, &stream)
        .await
        .unwrap()
        .expect("checkpoint must be readable");
    assert_eq!(cp.head_slot, 1010);
    upsert_ingest_checkpoint(
        &pool,
        &stream,
        1005,
        1020,
        CommitmentLevel::Confirmed,
        Some("worker-1"),
    )
    .await
    .unwrap();
    let cp2 = queries::fetch_ingest_checkpoint(&pool, &stream)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cp2.head_slot, 1020, "upsert must advance the checkpoint");

    let provider_id = format!(
        "provider-{}",
        rand_bytes(4)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let window_start = Utc::now();
    upsert_provider_health(
        &pool,
        ProviderHealthUpdate {
            provider_id: &provider_id,
            window_start,
            requests: 100,
            errors: 1,
            timeouts: 0,
            rate_limited: 0,
            p50_ms: Some(12),
            p95_ms: Some(40),
            breaker_state: "closed",
            last_error_code: None,
        },
    )
    .await
    .unwrap();
    let health = queries::fetch_provider_health(&pool, &provider_id, window_start)
        .await
        .unwrap()
        .expect("provider_health row must be readable");
    assert_eq!(health.requests, 100);
}

// ---------------------------------------------------------------------
// Protocol layer additions
// ---------------------------------------------------------------------

#[tokio::test]
async fn protocol_layer_additions_round_trip() {
    let Some(pool) = connect(&rust_url()).await else {
        return;
    };

    let program_id_str = format!(
        "AeGisProgramMarket-{}",
        rand_bytes(8)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let decoder_version_id = insert_decoder_version(
        &pool,
        queries::NewDecoderVersion {
            protocol: "aegis",
            program_id: &program_id_str,
            account_kind: "market",
            schema_version: 1,
            discriminator: &[1, 2, 3, 4, 5, 6, 7, 8],
            layout_hash: "closurefixtesthash",
            effective_from_slot: 0,
            source: "spec",
        },
    )
    .await
    .unwrap();

    let program_id = rand_bytes(32);
    let admin = rand_bytes(32);
    let guardian = rand_bytes(32);
    let fee_recipient = rand_bytes(32);
    let applied = upsert_aegis_protocol_state(
        &pool,
        AegisProtocolStateFixture {
            program_id: &program_id,
            admin: &admin,
            guardian: &guardian,
            fee_recipient: &fee_recipient,
            paused: 0,
            as_of_slot: 100,
            as_of_commitment: CommitmentLevel::Finalized,
            decoder_version_id,
            materialized_via: MaterializedVia::Snapshot,
        },
    )
    .await
    .unwrap();
    assert!(applied);
    // DM-04: a stale (lower as_of_slot) write must not overwrite the newer one.
    let stale = upsert_aegis_protocol_state(
        &pool,
        AegisProtocolStateFixture {
            program_id: &program_id,
            admin: &admin,
            guardian: &guardian,
            fee_recipient: &fee_recipient,
            paused: 1,
            as_of_slot: 50,
            as_of_commitment: CommitmentLevel::Finalized,
            decoder_version_id,
            materialized_via: MaterializedVia::Snapshot,
        },
    )
    .await
    .unwrap();
    assert!(!stale, "DM-04: stale as_of_slot write must be rejected");
    let state = queries::fetch_aegis_protocol_state(&pool, &program_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(state.paused, 0, "the stale write must not have applied");

    // aegis_markets fixture (already-covered helper) is the FK target for
    // aegis_positions/aegis_market_params_history below.
    let market_pubkey = rand_bytes(32);
    upsert_aegis_market(
        &pool,
        AegisMarketFixture {
            market_pubkey: &market_pubkey,
            program_id: &program_id,
            as_of_slot: 100,
            as_of_commitment: CommitmentLevel::Finalized,
            decoder_version_id,
            status: MaterializationStatus::Current,
            materialized_via: MaterializedVia::Snapshot,
        },
    )
    .await
    .unwrap();

    let feed = rand_bytes(32);
    let applied = insert_aegis_market_params_history(
        &pool,
        AegisMarketParamsHistoryFixture {
            market_pubkey: &market_pubkey,
            effective_from_slot: 100,
            decoder_version_id,
            collateral_feed_id: &feed,
            loan_feed_id: &feed,
        },
    )
    .await
    .unwrap();
    assert!(applied);
    let history = queries::fetch_aegis_market_params_history(&pool, &market_pubkey)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);

    let position_pubkey = rand_bytes(32);
    let owner = rand_bytes(32);
    let applied = upsert_aegis_position(
        &pool,
        AegisPositionFixture {
            position_pubkey: &position_pubkey,
            market_pubkey: &market_pubkey,
            owner: &owner,
            supply_shares: u128::MAX,
            borrow_shares: 12345,
            collateral_amount: 999,
            is_open: true,
            as_of_slot: 100,
            as_of_commitment: CommitmentLevel::Finalized,
            decoder_version_id,
            materialized_via: MaterializedVia::Snapshot,
            status: MaterializationStatus::Current,
        },
    )
    .await
    .unwrap();
    assert!(applied);
    let pos = queries::fetch_aegis_position(&pool, &position_pubkey)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        decode_u128(&pos.supply_shares).unwrap(),
        u128::MAX,
        "aegis_positions.supply_shares (numeric(39,0)) must round-trip u128::MAX exactly"
    );

    let event_applied = insert_aegis_event(
        &pool,
        NewAegisEvent {
            signature: &rand_bytes(64),
            slot: 100,
            blockhash: &rand_bytes(32),
            ix_index: 0,
            inner_index: -1,
            log_index: 0,
            event_name: "PositionOpened",
            market_pubkey: Some(&market_pubkey),
            position_pubkey: Some(&position_pubkey),
            payload: serde_json::json!({"test": true}),
            decoder_version_id,
            as_of_commitment: CommitmentLevel::Finalized,
        },
    )
    .await
    .unwrap();
    assert!(event_applied);
    let events = queries::fetch_aegis_events_for_position(&pool, &position_pubkey)
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_name, "PositionOpened");

    let oracle_applied = insert_aegis_oracle_observation(
        &pool,
        NewAegisOracleObservation {
            feed_id: &feed,
            publish_time: Utc::now(),
            slot: 100,
            price: 50_000_000_000,
            conf: 10_000,
            expo: -8,
            verification_level: "full",
            price_account: &rand_bytes(32),
            price_lo_wad: 49_999_000_000,
            price_hi_wad: 50_001_000_000,
            validation_result: OracleValidationResult::Valid,
            failed_check: None,
        },
    )
    .await
    .unwrap();
    assert!(oracle_applied);
    let oracle = queries::fetch_latest_valid_oracle_observation(&pool, &feed)
        .await
        .unwrap()
        .expect("valid observation must be readable");
    assert_eq!(oracle.validation_result, OracleValidationResult::Valid);

    let check_id = insert_aegis_invariant_check(
        &pool,
        NewAegisInvariantCheck {
            invariant_id: "INV-TEST-1",
            market_pubkey: Some(&market_pubkey),
            slot: 100,
            expected: 100,
            actual: 99,
            holds: false,
        },
    )
    .await
    .unwrap();
    assert!(check_id > 0);
    let failed = queries::fetch_failed_invariant_checks(&pool).await.unwrap();
    assert!(failed.iter().any(|c| c.check_id == check_id));
}

// ---------------------------------------------------------------------
// Derived layer additions
// ---------------------------------------------------------------------

#[tokio::test]
async fn derived_layer_additions_round_trip() {
    let Some(pool) = connect(&rust_url()).await else {
        return;
    };

    let program_id_str = format!(
        "AeGisProgramPosition-{}",
        rand_bytes(8)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let decoder_version_id = insert_decoder_version(
        &pool,
        queries::NewDecoderVersion {
            protocol: "aegis",
            program_id: &program_id_str,
            account_kind: "position",
            schema_version: 1,
            discriminator: &[9, 9, 9, 9, 9, 9, 9, 9],
            layout_hash: "closurefixtesthash2",
            effective_from_slot: 0,
            source: "spec",
        },
    )
    .await
    .unwrap();
    let market_pubkey = rand_bytes(32);
    upsert_aegis_market(
        &pool,
        AegisMarketFixture {
            market_pubkey: &market_pubkey,
            program_id: &rand_bytes(32),
            as_of_slot: 200,
            as_of_commitment: CommitmentLevel::Finalized,
            decoder_version_id,
            status: MaterializationStatus::Current,
            materialized_via: MaterializedVia::Snapshot,
        },
    )
    .await
    .unwrap();
    let position_pubkey = rand_bytes(32);
    upsert_aegis_position(
        &pool,
        AegisPositionFixture {
            position_pubkey: &position_pubkey,
            market_pubkey: &market_pubkey,
            owner: &rand_bytes(32),
            supply_shares: 1,
            borrow_shares: 1,
            collateral_amount: 1,
            is_open: true,
            as_of_slot: 200,
            as_of_commitment: CommitmentLevel::Finalized,
            decoder_version_id,
            materialized_via: MaterializedVia::Snapshot,
            status: MaterializationStatus::Current,
        },
    )
    .await
    .unwrap();

    let applied = insert_position_health(
        &pool,
        NewPositionHealth {
            position_pubkey: &position_pubkey,
            computed_at_slot: 200,
            t_eval: Utc::now(),
            collateral_value_wad: 1_000_000,
            debt_value_wad: 500_000,
            debt_assets: 500,
            health_factor_wad: Some(2_000_000_000_000_000_000),
            state: HealthState::Healthy,
            market_params_from_slot: 200,
            commitment: CommitmentLevel::Finalized,
        },
    )
    .await
    .unwrap();
    assert!(applied);
    let health = queries::fetch_latest_position_health(&pool, &position_pubkey)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(health.state, HealthState::Healthy);

    // DO NOTHING: re-detecting the same slot must be a no-op, not an error.
    let repeat = insert_position_health(
        &pool,
        NewPositionHealth {
            position_pubkey: &position_pubkey,
            computed_at_slot: 200,
            t_eval: Utc::now(),
            collateral_value_wad: 1,
            debt_value_wad: 1,
            debt_assets: 1,
            health_factor_wad: None,
            state: HealthState::Stale,
            market_params_from_slot: 200,
            commitment: CommitmentLevel::Finalized,
        },
    )
    .await
    .unwrap();
    assert!(
        !repeat,
        "position_health DO NOTHING: a slot's health value is a fact, never revised in place"
    );

    let candidate_id = insert_liquidation_candidate(
        &pool,
        NewLiquidationCandidate {
            position_pubkey: &position_pubkey,
            market_pubkey: &market_pubkey,
            detected_at_slot: 200,
            t_eval: Utc::now(),
            lookahead_ms: 500,
            risk_params_hash: "deadbeef",
            max_repay_assets: 100,
            expected_seize: 90,
            expected_bonus: 5,
            expected_protocol_cut: 5,
            estimated_profit_wad: 1_000,
            profitable: true,
            full_liquidation: false,
            dust_rule_applied: false,
            expires_at: Utc::now() + chrono::Duration::seconds(30),
        },
    )
    .await
    .unwrap()
    .expect("first detection at this slot must insert");

    // Natural key (position_pubkey, detected_at_slot) DO NOTHING re-detect.
    let repeat = insert_liquidation_candidate(
        &pool,
        NewLiquidationCandidate {
            position_pubkey: &position_pubkey,
            market_pubkey: &market_pubkey,
            detected_at_slot: 200,
            t_eval: Utc::now(),
            lookahead_ms: 999,
            risk_params_hash: "different",
            max_repay_assets: 1,
            expected_seize: 1,
            expected_bonus: 1,
            expected_protocol_cut: 1,
            estimated_profit_wad: 1,
            profitable: false,
            full_liquidation: false,
            dust_rule_applied: false,
            expires_at: Utc::now(),
        },
    )
    .await
    .unwrap();
    assert!(
        repeat.is_none(),
        "re-detecting the same (position,slot) must be a DO NOTHING no-op"
    );

    let open = queries::fetch_open_liquidation_candidates(&pool)
        .await
        .unwrap();
    assert!(open.iter().any(|c| c.candidate_id == candidate_id));

    let advanced = queries::update_liquidation_candidate_status(
        &pool,
        candidate_id,
        sentinel_db::enums::CandidateStatus::Claimed,
        None,
    )
    .await
    .unwrap();
    assert!(advanced);

    upsert_market_metrics(
        &pool,
        MarketMetricUpsert {
            market_pubkey: &market_pubkey,
            bucket_start: Utc::now(),
            utilization_wad: 500_000_000_000_000_000,
            borrow_rate_ps: 1,
            supply_rate_ps: 1,
            total_supply_assets: 1_000_000,
            total_borrow_assets: 500_000,
            total_supply_shares: 1_000_000,
            total_borrow_shares: 500_000,
            free_liquidity: 500_000,
            accrual_staleness_secs: 5,
            open_positions: 1,
            positions_with_debt: 1,
            aggregate_bad_debt: 0,
        },
    )
    .await
    .unwrap();
}

// ---------------------------------------------------------------------
// Execution layer additions
// ---------------------------------------------------------------------

#[tokio::test]
async fn reconciliation_mismatches_round_trip() {
    let Some(pool) = connect(&rust_url()).await else {
        return;
    };
    let entity_key = format!(
        "pos-{}",
        rand_bytes(4)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let id = insert_reconciliation_mismatch(
        &pool,
        NewReconciliationMismatch {
            class: MismatchClass::RaceHealed,
            intent_id: None,
            attempt_id: None,
            entity_kind: "position",
            entity_key: &entity_key,
            slot: 300,
            predicted: serde_json::json!({"health": "liquidatable"}),
            actual: serde_json::json!({"health": "healthy"}),
            onchain_error_code: None,
        },
    )
    .await
    .unwrap();
    assert!(id > 0);
    let rows = queries::fetch_reconciliation_mismatches_for_entity(&pool, "position", &entity_key)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].class, MismatchClass::RaceHealed);
}

// ---------------------------------------------------------------------
// Permission proof: the newly-covered append-only tables have no UPDATE
// grant for sentinel_rust, matching infra/migrations/0010_grants.sql.
// ---------------------------------------------------------------------

/// `infra/migrations/0010_grants.sql` gives `sentinel_rust` only INSERT and
/// SELECT on every append-only table added in this closure fix — never
/// UPDATE. Each statement is a literal (`CI-NOSQLFMT` forbids building SQL
/// text from interpolated data), matched to a real column each table has.
#[tokio::test]
async fn sentinel_rust_has_no_update_grant_on_newly_covered_append_only_tables() {
    let Some(pool) = connect(&rust_url()).await else {
        return;
    };
    let statements: &[&str] = &[
        "UPDATE instructions SET stack_height = stack_height",
        "UPDATE program_logs SET raw_line = raw_line",
        "UPDATE account_observations SET executable = executable",
        "UPDATE token_balance_deltas SET decimals = decimals",
        "UPDATE rollback_events SET cause = cause",
        "UPDATE aegis_market_params_history SET oracle_kind = oracle_kind",
        "UPDATE aegis_events SET event_name = event_name",
        "UPDATE aegis_oracle_observations SET expo = expo",
        "UPDATE aegis_invariant_checks SET holds = holds",
        "UPDATE position_health SET state = state",
        "UPDATE reconciliation_mismatches SET entity_kind = entity_kind",
    ];
    for sql in statements {
        let result = sqlx::query(*sql).execute(&pool).await;
        assert!(
            result.is_err(),
            "sentinel_rust must not be able to run: {sql}"
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .to_lowercase()
                .contains("permission denied"),
            "must fail specifically with a permission error: {sql}"
        );
    }
}
