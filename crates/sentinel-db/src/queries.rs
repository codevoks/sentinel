//! Query helpers implementing the exact conflict/monotonicity semantics
//! `docs/data-model.md` and `docs/distributed-correctness.md` require.
//! Every query here is a bound, parameterized statement (`CI-NOSQLFMT`,
//! `scripts/ci-guards.sh`) — no `format!` ever builds a `SELECT`/`INSERT`/
//! `UPDATE`/`DELETE` string with interpolated data.

use chrono::{DateTime, Utc};
use sqlx::{PgExecutor, Row};
use uuid::Uuid;

use crate::enums::{
    AccountObservationSource, AttemptState, CandidateStatus, CommitmentLevel, HealthState,
    IntentKind, JobKind, LogKind, MaterializationStatus, MaterializedVia, MismatchClass,
    ObservationSource, OracleValidationResult, PayloadEncoding, RawObservationKind, SlotStatus,
};
use crate::numeric::{encode_u128, encode_u64};
use crate::tables::{
    AccountObservationRow, AegisEventRow, AegisInvariantCheckRow, AegisMarketParamsHistoryRow,
    AegisOracleObservationRow, AegisPositionRow, AegisProtocolStateRow, AlertRow,
    DecoderVersionRow, ExecutionIntentRow, GapEventRow, IngestCheckpointRow, InstructionRow,
    JobRow, LiquidationCandidateRow, MarketMetricRow, PositionHealthRow, ProgramLogRow,
    ProviderHealthRow, RawObservationRow, ReconciliationMismatchRow, RollbackEventRow, SlotRow,
    TokenBalanceDeltaRow,
};

#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

// ---------------------------------------------------------------------
// raw_observations — DM-02 (append-only), natural key DO NOTHING.
// ---------------------------------------------------------------------

pub struct NewRawObservation<'a> {
    pub kind: RawObservationKind,
    pub natural_key: &'a str,
    pub slot: i64,
    pub commitment: CommitmentLevel,
    pub source: ObservationSource,
    pub provider_id: &'a str,
    pub request_id: Option<Uuid>,
    pub payload: &'a [u8],
    pub payload_hash: &'a [u8],
    pub payload_encoding: PayloadEncoding,
}

/// `ingestion-model.md` §5 rule R-2: `ON CONFLICT (kind, natural_key,
/// payload_hash[, slot]) DO NOTHING`. Returns `true` if a new row was
/// inserted, `false` if the natural key already existed (the duplicate-
/// delivery-is-free path).
pub async fn insert_raw_observation<'e, E: PgExecutor<'e>>(
    exec: E,
    row: NewRawObservation<'_>,
) -> Result<bool, QueryError> {
    let result = sqlx::query(
        "INSERT INTO raw_observations \
           (kind, natural_key, slot, commitment, source, provider_id, request_id, payload, payload_hash, payload_encoding) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
         ON CONFLICT (kind, natural_key, payload_hash, slot) DO NOTHING",
    )
    .bind(row.kind)
    .bind(row.natural_key)
    .bind(row.slot)
    .bind(row.commitment)
    .bind(row.source)
    .bind(row.provider_id)
    .bind(row.request_id)
    .bind(row.payload)
    .bind(row.payload_hash)
    .bind(row.payload_encoding)
    .execute(exec)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn count_raw_observations<'e, E: PgExecutor<'e>>(exec: E) -> Result<i64, QueryError> {
    let row = sqlx::query("SELECT count(*) AS n FROM raw_observations")
        .fetch_one(exec)
        .await?;
    Ok(row.try_get::<i64, _>("n")?)
}

pub async fn fetch_raw_observations_by_natural_key<'e, E: PgExecutor<'e>>(
    exec: E,
    natural_key: &str,
) -> Result<Vec<RawObservationRow>, QueryError> {
    let rows = sqlx::query_as::<_, RawObservationRow>(
        "SELECT observation_id, kind, natural_key, slot, commitment, source, provider_id, \
                request_id, observed_at, payload, payload_hash, payload_encoding \
         FROM raw_observations WHERE natural_key = $1 ORDER BY observation_id",
    )
    .bind(natural_key)
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

// ---------------------------------------------------------------------
// slots — monotonic promotion (observed -> confirmed -> finalized, or
// -> abandoned). No trigger exists (Phase 2 explicit non-scope), so the
// rank comparison lives in this query's WHERE clause, atomically, inside
// the INSERT ... ON CONFLICT statement itself.
// ---------------------------------------------------------------------

/// Numeric promotion rank matching finality-and-forks.md §3's state
/// diagram: observed(1) -> confirmed(2) -> finalized(3), and abandoned(0)
/// is reachable from observed or confirmed but never itself promotable
/// further and never regressed from once finalized (finalized -> abandoned
/// is not a legal transition per the diagram, so it is excluded by the
/// CASE below evaluating to false for that pair).
fn status_rank_case(column: &str) -> String {
    format!(
        "CASE {column} \
           WHEN 'skipped' THEN -1 \
           WHEN 'observed' THEN 1 \
           WHEN 'confirmed' THEN 2 \
           WHEN 'finalized' THEN 3 \
           WHEN 'abandoned' THEN 0 \
         END"
    )
}

pub struct SlotPromotion<'a> {
    pub slot: i64,
    pub blockhash: Option<&'a [u8]>,
    pub parent_slot: Option<i64>,
    pub parent_blockhash: Option<&'a [u8]>,
    pub status: SlotStatus,
    pub commitment: CommitmentLevel,
    pub canonical: bool,
}

/// Inserts or promotes a slot row. `ON CONFLICT (slot, blockhash) DO
/// UPDATE` only proceeds when the new status is a legal forward move: a
/// strictly higher rank (observed->confirmed->finalized), or a move to
/// `abandoned` from `observed`/`confirmed` (never from `finalized`, and
/// never a regression). Returns `true` if the row was inserted or promoted,
/// `false` if the write was rejected as stale/illegal (affected zero rows —
/// DM-04's mechanism, generalized from `as_of_slot` to a status rank here).
pub async fn upsert_slot<'e, E: PgExecutor<'e>>(
    exec: E,
    p: SlotPromotion<'_>,
) -> Result<bool, QueryError> {
    let rank_new = status_rank_case("$5");
    let rank_current = status_rank_case("slots.status");
    let sql = format!(
        "INSERT INTO slots (slot, blockhash, parent_slot, parent_blockhash, status, commitment, canonical) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         ON CONFLICT (slot, blockhash) DO UPDATE SET \
           status = excluded.status, commitment = excluded.commitment, canonical = excluded.canonical, \
           finalized_at = CASE WHEN excluded.status = 'finalized' THEN now() ELSE slots.finalized_at END \
         WHERE ({rank_new}) > ({rank_current}) \
            OR (excluded.status = 'abandoned' AND slots.status IN ('observed', 'confirmed'))"
    );
    // AssertSqlSafe: `sql` is built entirely from the two fixed CASE
    // expressions above (no interpolated data, no user input — the actual
    // row values are all bound parameters below).
    let result = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(p.slot)
        .bind(p.blockhash)
        .bind(p.parent_slot)
        .bind(p.parent_blockhash)
        .bind(p.status)
        .bind(p.commitment)
        .bind(p.canonical)
        .execute(exec)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn fetch_slot<'e, E: PgExecutor<'e>>(
    exec: E,
    slot: i64,
    blockhash: Option<&[u8]>,
) -> Result<Option<SlotRow>, QueryError> {
    let row = sqlx::query_as::<_, SlotRow>(
        "SELECT slot, blockhash, parent_slot, parent_blockhash, block_time, block_height, \
                status, commitment, canonical, first_seen_at, finalized_at \
         FROM slots WHERE slot = $1 AND blockhash IS NOT DISTINCT FROM $2",
    )
    .bind(slot)
    .bind(blockhash)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

// ---------------------------------------------------------------------
// decoder_versions — needed as an FK target for the DM-04 aegis_markets test.
// ---------------------------------------------------------------------

pub struct NewDecoderVersion<'a> {
    pub protocol: &'a str,
    pub program_id: &'a str,
    pub account_kind: &'a str,
    pub schema_version: i32,
    pub discriminator: &'a [u8],
    pub layout_hash: &'a str,
    pub effective_from_slot: i64,
    pub source: &'a str,
}

pub async fn insert_decoder_version<'e, E: PgExecutor<'e>>(
    exec: E,
    v: NewDecoderVersion<'_>,
) -> Result<i64, QueryError> {
    let row = sqlx::query(
        "INSERT INTO decoder_versions \
           (protocol, program_id, account_kind, schema_version, discriminator, layout_hash, effective_from_slot, source) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         RETURNING id",
    )
    .bind(v.protocol)
    .bind(v.program_id)
    .bind(v.account_kind)
    .bind(v.schema_version)
    .bind(v.discriminator)
    .bind(v.layout_hash)
    .bind(v.effective_from_slot)
    .bind(v.source)
    .fetch_one(exec)
    .await?;
    Ok(row.try_get::<i64, _>("id")?)
}

pub async fn fetch_decoder_version<'e, E: PgExecutor<'e>>(
    exec: E,
    id: i64,
) -> Result<Option<DecoderVersionRow>, QueryError> {
    let row = sqlx::query_as::<_, DecoderVersionRow>(
        "SELECT id, protocol, program_id, account_kind, schema_version, discriminator, \
                layout_hash, effective_from_slot, effective_to_slot, source \
         FROM decoder_versions WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

// ---------------------------------------------------------------------
// aegis_markets — DM-04 monotonic materialization:
// WHERE excluded.as_of_slot > current.as_of_slot.
// Only the subset of columns this phase's tests exercise is written;
// every other NOT NULL column in the frozen schema is given a caller-
// supplied placeholder value via `AegisMarketFixture` so the insert is
// well-formed without inventing real Aegis semantics (explicit Phase 2
// non-scope: "no aegis_* table content logic").
// ---------------------------------------------------------------------

pub struct AegisMarketFixture<'a> {
    pub market_pubkey: &'a [u8],
    pub program_id: &'a [u8],
    pub as_of_slot: i64,
    pub as_of_commitment: CommitmentLevel,
    pub decoder_version_id: i64,
    pub status: crate::enums::MaterializationStatus,
    pub materialized_via: crate::enums::MaterializedVia,
}

pub async fn upsert_aegis_market<'e, E: PgExecutor<'e>>(
    exec: E,
    f: AegisMarketFixture<'_>,
) -> Result<bool, QueryError> {
    let zero_u128 = encode_u128(0);
    let zero_u64 = encode_u64(0);
    let result = sqlx::query(
        "INSERT INTO aegis_markets ( \
           market_pubkey, program_id, \
           collateral_mint, loan_mint, collateral_token_program, loan_token_program, \
           collateral_vault, loan_vault, fee_recipient, config_id, collateral_decimals, loan_decimals, \
           oracle_kind, collateral_feed_id, loan_feed_id, max_price_age_secs, max_conf_bps, \
           max_ltv, liq_threshold, liq_bonus, close_factor, full_liq_hf, liq_protocol_fee, fee, min_debt, \
           base_rate_ps, slope1_ps, slope2_ps, u_kink, max_rate_ps, \
           total_supply_assets, total_supply_shares, total_borrow_assets, total_borrow_shares, \
           collateral_fee_accrued, last_accrual_ts, paused, flags, \
           as_of_slot, as_of_commitment, materialized_via, decoder_version_id, status \
         ) VALUES ( \
           $1, $2, \
           $2, $2, $2, $2, \
           $2, $2, $2, 0, 0, 0, \
           0, $2, $2, 0, 0, \
           $3::numeric, $3::numeric, $3::numeric, $3::numeric, $3::numeric, $3::numeric, $3::numeric, $4::numeric, \
           $3::numeric, $3::numeric, $3::numeric, $3::numeric, $3::numeric, \
           $4::numeric, $3::numeric, $4::numeric, $3::numeric, \
           $4::numeric, 0, 0, 0, \
           $5, $6, $7, $8, $9 \
         ) \
         ON CONFLICT (market_pubkey) DO UPDATE SET \
           as_of_slot = excluded.as_of_slot, as_of_commitment = excluded.as_of_commitment, \
           status = excluded.status, materialized_via = excluded.materialized_via, \
           decoder_version_id = excluded.decoder_version_id \
         WHERE excluded.as_of_slot > aegis_markets.as_of_slot",
    )
    .bind(f.market_pubkey)
    .bind(f.program_id)
    .bind(&zero_u128)
    .bind(&zero_u64)
    .bind(f.as_of_slot)
    .bind(f.as_of_commitment)
    .bind(f.materialized_via)
    .bind(f.decoder_version_id)
    .bind(f.status)
    .execute(exec)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn fetch_aegis_market_as_of_slot<'e, E: PgExecutor<'e>>(
    exec: E,
    market_pubkey: &[u8],
) -> Result<Option<i64>, QueryError> {
    let row = sqlx::query("SELECT as_of_slot FROM aegis_markets WHERE market_pubkey = $1")
        .bind(market_pubkey)
        .fetch_optional(exec)
        .await?;
    Ok(row.map(|r| r.try_get::<i64, _>("as_of_slot")).transpose()?)
}

// ---------------------------------------------------------------------
// alerts — DM-08: one open alert per (kind, entity_kind, entity_key).
// ---------------------------------------------------------------------

pub async fn open_alert<'e, E: PgExecutor<'e>>(
    exec: E,
    kind: &str,
    severity: &str,
    entity_kind: &str,
    entity_key: &str,
    detail: serde_json::Value,
    runbook_id: Option<&str>,
) -> Result<i64, QueryError> {
    let row = sqlx::query(
        "INSERT INTO alerts (kind, severity, entity_kind, entity_key, detail, runbook_id) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING alert_id",
    )
    .bind(kind)
    .bind(severity)
    .bind(entity_kind)
    .bind(entity_key)
    .bind(detail)
    .bind(runbook_id)
    .fetch_one(exec)
    .await?;
    Ok(row.try_get::<i64, _>("alert_id")?)
}

pub async fn resolve_alert<'e, E: PgExecutor<'e>>(
    exec: E,
    alert_id: i64,
) -> Result<bool, QueryError> {
    let result = sqlx::query(
        "UPDATE alerts SET resolved_at = now() WHERE alert_id = $1 AND resolved_at IS NULL",
    )
    .bind(alert_id)
    .execute(exec)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn fetch_open_alerts<'e, E: PgExecutor<'e>>(
    exec: E,
) -> Result<Vec<AlertRow>, QueryError> {
    let rows = sqlx::query_as::<_, AlertRow>(
        "SELECT alert_id, kind, severity, entity_kind, entity_key, opened_at, resolved_at, detail, runbook_id \
         FROM alerts WHERE resolved_at IS NULL ORDER BY alert_id",
    )
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

// ---------------------------------------------------------------------
// execution_intents — DM-10: idempotency_key UNIQUE globally.
// ---------------------------------------------------------------------

pub struct NewExecutionIntent<'a> {
    pub intent_id: Uuid,
    pub idempotency_key: &'a str,
    pub kind: IntentKind,
    pub market_pubkey: &'a [u8],
    pub position_pubkey: Option<&'a [u8]>,
    pub params: serde_json::Value,
    pub constraints: serde_json::Value,
    pub expires_at: DateTime<Utc>,
    pub max_attempts: i32,
    pub trigger_slot: i64,
    pub trigger_commitment: CommitmentLevel,
}

/// Inserts a new intent. No `ON CONFLICT` clause: a duplicate
/// `idempotency_key` MUST surface as a real constraint violation to the
/// caller (DM-10's adversarial test asserts exactly this error, not a
/// silently-absorbed no-op — creating an intent is a business decision,
/// not an idempotent observation).
pub async fn create_execution_intent<'e, E: PgExecutor<'e>>(
    exec: E,
    intent: NewExecutionIntent<'_>,
) -> Result<(), QueryError> {
    sqlx::query(
        "INSERT INTO execution_intents \
           (intent_id, idempotency_key, kind, market_pubkey, position_pubkey, params, constraints, \
            state, expires_at, max_attempts, trigger_slot, trigger_commitment) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'CREATED', $8, $9, $10, $11)",
    )
    .bind(intent.intent_id)
    .bind(intent.idempotency_key)
    .bind(intent.kind)
    .bind(intent.market_pubkey)
    .bind(intent.position_pubkey)
    .bind(intent.params)
    .bind(intent.constraints)
    .bind(intent.expires_at)
    .bind(intent.max_attempts)
    .bind(intent.trigger_slot)
    .bind(intent.trigger_commitment)
    .execute(exec)
    .await?;
    Ok(())
}

pub async fn fetch_execution_intent<'e, E: PgExecutor<'e>>(
    exec: E,
    intent_id: Uuid,
) -> Result<Option<ExecutionIntentRow>, QueryError> {
    let row = sqlx::query_as::<_, ExecutionIntentRow>(
        "SELECT intent_id, idempotency_key, kind, market_pubkey, position_pubkey, params, constraints, \
                state, created_at, updated_at, expires_at, lease_holder, lease_expires_at, \
                attempt_count, max_attempts, cumulative_fee_lamports, terminal_reason, \
                trigger_slot, trigger_commitment \
         FROM execution_intents WHERE intent_id = $1",
    )
    .bind(intent_id)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

// ---------------------------------------------------------------------
// transaction_attempts — TX-02: one non-terminal attempt per intent_id.
// ---------------------------------------------------------------------

pub struct NewTransactionAttempt<'a> {
    pub attempt_id: Uuid,
    pub intent_id: Uuid,
    pub attempt_number: i32,
    pub signature: &'a [u8],
    pub transaction_bytes: &'a [u8],
    pub transaction_version: crate::enums::TxVersion,
    pub recent_blockhash: &'a [u8],
    pub last_valid_block_height: i64,
    pub state: AttemptState,
}

/// FR-17 / DM-11: signature and transaction_bytes are committed BEFORE
/// submission. This function's only job is the insert itself; callers are
/// responsible for calling it before invoking `sendTransaction`.
pub async fn insert_transaction_attempt<'e, E: PgExecutor<'e>>(
    exec: E,
    a: NewTransactionAttempt<'_>,
) -> Result<(), QueryError> {
    sqlx::query(
        "INSERT INTO transaction_attempts \
           (attempt_id, intent_id, attempt_number, signature, transaction_bytes, transaction_version, \
            recent_blockhash, last_valid_block_height, state) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(a.attempt_id)
    .bind(a.intent_id)
    .bind(a.attempt_number)
    .bind(a.signature)
    .bind(a.transaction_bytes)
    .bind(a.transaction_version)
    .bind(a.recent_blockhash)
    .bind(a.last_valid_block_height)
    .bind(a.state)
    .execute(exec)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------
// jobs — see crate::jobs for the full claim/renew/release/quarantine API
// (kept in its own module because sentinel-jobs wraps it, not because the
// SQL differs by crate).
// ---------------------------------------------------------------------

pub async fn enqueue_job<'e, E: PgExecutor<'e>>(
    exec: E,
    kind: JobKind,
    dedupe_key: &str,
    payload: serde_json::Value,
    priority: i16,
    max_attempts: i32,
) -> Result<Option<i64>, QueryError> {
    // DM-09: one outstanding job per dedupe_key. A duplicate enqueue while
    // one is outstanding is a documented no-op (data-model.md §8), so this
    // uses DO NOTHING against the partial unique index rather than letting
    // the constraint violation surface as an error.
    let row = sqlx::query(
        "INSERT INTO jobs (kind, dedupe_key, payload, priority, max_attempts) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (dedupe_key) WHERE state IN ('queued', 'leased') DO NOTHING \
         RETURNING job_id",
    )
    .bind(kind)
    .bind(dedupe_key)
    .bind(payload)
    .bind(priority)
    .bind(max_attempts)
    .fetch_optional(exec)
    .await?;
    Ok(row.map(|r| r.try_get::<i64, _>("job_id")).transpose()?)
}

pub async fn fetch_job<'e, E: PgExecutor<'e>>(
    exec: E,
    job_id: i64,
) -> Result<Option<JobRow>, QueryError> {
    let row = sqlx::query_as::<_, JobRow>(
        "SELECT job_id, kind, dedupe_key, payload, priority, state, lease_holder, lease_expires_at, \
                attempts, max_attempts, last_error, available_at, created_at, updated_at \
         FROM jobs WHERE job_id = $1",
    )
    .bind(job_id)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

// =====================================================================
// Closure-fix additions: query functions for every remaining canonical
// table, matching `sentinel_rust`'s exact grants in
// `infra/migrations/0010_grants.sql` — never a writer for a table
// sentinel_rust does not own.
// =====================================================================

// ---------------------------------------------------------------------
// instructions — IMMUTABLE, owner sentinel-normalize. DO NOTHING.
// ---------------------------------------------------------------------

pub struct NewInstruction<'a> {
    pub signature: &'a [u8],
    pub slot: i64,
    pub blockhash: &'a [u8],
    pub ix_index: i16,
    pub inner_index: i16,
    pub stack_height: i16,
    pub program_id: &'a [u8],
    pub accounts: &'a [Vec<u8>],
    pub data: &'a [u8],
}

pub async fn insert_instruction<'e, E: PgExecutor<'e>>(
    exec: E,
    ix: NewInstruction<'_>,
) -> Result<bool, QueryError> {
    let result = sqlx::query(
        "INSERT INTO instructions \
           (signature, slot, blockhash, ix_index, inner_index, stack_height, program_id, accounts, data) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         ON CONFLICT (signature, slot, blockhash, ix_index, inner_index) DO NOTHING",
    )
    .bind(ix.signature)
    .bind(ix.slot)
    .bind(ix.blockhash)
    .bind(ix.ix_index)
    .bind(ix.inner_index)
    .bind(ix.stack_height)
    .bind(ix.program_id)
    .bind(ix.accounts)
    .bind(ix.data)
    .execute(exec)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn fetch_instructions_for_transaction<'e, E: PgExecutor<'e>>(
    exec: E,
    signature: &[u8],
    slot: i64,
    blockhash: &[u8],
) -> Result<Vec<InstructionRow>, QueryError> {
    let rows = sqlx::query_as::<_, InstructionRow>(
        "SELECT signature, slot, blockhash, ix_index, inner_index, stack_height, program_id, accounts, data \
         FROM instructions WHERE signature = $1 AND slot = $2 AND blockhash = $3 \
         ORDER BY ix_index, inner_index",
    )
    .bind(signature)
    .bind(slot)
    .bind(blockhash)
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

// ---------------------------------------------------------------------
// program_logs — IMMUTABLE, owner sentinel-normalize. DO NOTHING.
// ---------------------------------------------------------------------

pub struct NewProgramLog<'a> {
    pub signature: &'a [u8],
    pub slot: i64,
    pub blockhash: &'a [u8],
    pub log_index: i32,
    pub program_id: Option<&'a [u8]>,
    pub raw_line: &'a str,
    pub kind: LogKind,
}

pub async fn insert_program_log<'e, E: PgExecutor<'e>>(
    exec: E,
    log: NewProgramLog<'_>,
) -> Result<bool, QueryError> {
    let result = sqlx::query(
        "INSERT INTO program_logs (signature, slot, blockhash, log_index, program_id, raw_line, kind) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         ON CONFLICT (signature, slot, blockhash, log_index) DO NOTHING",
    )
    .bind(log.signature)
    .bind(log.slot)
    .bind(log.blockhash)
    .bind(log.log_index)
    .bind(log.program_id)
    .bind(log.raw_line)
    .bind(log.kind)
    .execute(exec)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn fetch_program_logs_for_transaction<'e, E: PgExecutor<'e>>(
    exec: E,
    signature: &[u8],
    slot: i64,
    blockhash: &[u8],
) -> Result<Vec<ProgramLogRow>, QueryError> {
    let rows = sqlx::query_as::<_, ProgramLogRow>(
        "SELECT signature, slot, blockhash, log_index, program_id, raw_line, kind \
         FROM program_logs WHERE signature = $1 AND slot = $2 AND blockhash = $3 \
         ORDER BY log_index",
    )
    .bind(signature)
    .bind(slot)
    .bind(blockhash)
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

// ---------------------------------------------------------------------
// account_observations — IMMUTABLE, owner sentinel-normalize. DO NOTHING.
// ---------------------------------------------------------------------

pub struct NewAccountObservation<'a> {
    pub pubkey: &'a [u8],
    pub slot: i64,
    pub content_hash: &'a [u8],
    pub owner_program: &'a [u8],
    pub lamports: u64,
    pub data: &'a [u8],
    pub executable: bool,
    pub rent_epoch: u64,
    pub write_version: Option<i64>,
    pub source: AccountObservationSource,
    pub commitment: CommitmentLevel,
}

pub async fn insert_account_observation<'e, E: PgExecutor<'e>>(
    exec: E,
    a: NewAccountObservation<'_>,
) -> Result<bool, QueryError> {
    let lamports = encode_u64(a.lamports);
    let rent_epoch = encode_u64(a.rent_epoch);
    let result = sqlx::query(
        "INSERT INTO account_observations \
           (pubkey, slot, content_hash, owner_program, lamports, data, executable, rent_epoch, write_version, source, commitment) \
         VALUES ($1, $2, $3, $4, $5::numeric, $6, $7, $8::numeric, $9, $10, $11) \
         ON CONFLICT (pubkey, slot, content_hash) DO NOTHING",
    )
    .bind(a.pubkey)
    .bind(a.slot)
    .bind(a.content_hash)
    .bind(a.owner_program)
    .bind(&lamports)
    .bind(a.data)
    .bind(a.executable)
    .bind(&rent_epoch)
    .bind(a.write_version)
    .bind(a.source)
    .bind(a.commitment)
    .execute(exec)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn fetch_latest_account_observation<'e, E: PgExecutor<'e>>(
    exec: E,
    pubkey: &[u8],
) -> Result<Option<AccountObservationRow>, QueryError> {
    let row = sqlx::query_as::<_, AccountObservationRow>(
        "SELECT pubkey, slot, content_hash, owner_program, lamports::text AS lamports, data, \
                executable, rent_epoch::text AS rent_epoch, write_version, source, commitment \
         FROM account_observations WHERE pubkey = $1 ORDER BY slot DESC LIMIT 1",
    )
    .bind(pubkey)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

// ---------------------------------------------------------------------
// token_balance_deltas — IMMUTABLE, owner sentinel-normalize. DO NOTHING.
// ---------------------------------------------------------------------

pub struct NewTokenBalanceDelta<'a> {
    pub signature: &'a [u8],
    pub slot: i64,
    pub blockhash: &'a [u8],
    pub account_index: i16,
    pub token_account: &'a [u8],
    pub mint: &'a [u8],
    pub owner: &'a [u8],
    pub pre_amount: u128,
    pub post_amount: u128,
    pub decimals: i16,
    pub program_id: &'a [u8],
}

pub async fn insert_token_balance_delta<'e, E: PgExecutor<'e>>(
    exec: E,
    d: NewTokenBalanceDelta<'_>,
) -> Result<bool, QueryError> {
    let pre = encode_u128(d.pre_amount);
    let post = encode_u128(d.post_amount);
    let result = sqlx::query(
        "INSERT INTO token_balance_deltas \
           (signature, slot, blockhash, account_index, token_account, mint, owner, pre_amount, post_amount, decimals, program_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8::numeric, $9::numeric, $10, $11) \
         ON CONFLICT (signature, slot, blockhash, account_index) DO NOTHING",
    )
    .bind(d.signature)
    .bind(d.slot)
    .bind(d.blockhash)
    .bind(d.account_index)
    .bind(d.token_account)
    .bind(d.mint)
    .bind(d.owner)
    .bind(&pre)
    .bind(&post)
    .bind(d.decimals)
    .bind(d.program_id)
    .execute(exec)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn fetch_token_balance_deltas_for_transaction<'e, E: PgExecutor<'e>>(
    exec: E,
    signature: &[u8],
    slot: i64,
    blockhash: &[u8],
) -> Result<Vec<TokenBalanceDeltaRow>, QueryError> {
    let rows = sqlx::query_as::<_, TokenBalanceDeltaRow>(
        "SELECT signature, slot, blockhash, account_index, token_account, mint, owner, \
                pre_amount::text AS pre_amount, post_amount::text AS post_amount, decimals, program_id \
         FROM token_balance_deltas WHERE signature = $1 AND slot = $2 AND blockhash = $3 \
         ORDER BY account_index",
    )
    .bind(signature)
    .bind(slot)
    .bind(blockhash)
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

// ---------------------------------------------------------------------
// rollback_events — IMMUTABLE, owner sentinel-chainstate. No natural key
// (data-model.md §4: a reorg is a point-in-time event, not deduplicated).
// ---------------------------------------------------------------------

pub struct NewRollbackEvent<'a> {
    pub slot_low: i64,
    pub slot_high: i64,
    pub abandoned_block_count: i32,
    pub depth_slots: i64,
    pub cause: &'a str,
}

pub async fn insert_rollback_event<'e, E: PgExecutor<'e>>(
    exec: E,
    r: NewRollbackEvent<'_>,
) -> Result<i64, QueryError> {
    let row = sqlx::query(
        "INSERT INTO rollback_events (slot_low, slot_high, abandoned_block_count, depth_slots, cause) \
         VALUES ($1, $2, $3, $4, $5) RETURNING rollback_id",
    )
    .bind(r.slot_low)
    .bind(r.slot_high)
    .bind(r.abandoned_block_count)
    .bind(r.depth_slots)
    .bind(r.cause)
    .fetch_one(exec)
    .await?;
    Ok(row.try_get::<i64, _>("rollback_id")?)
}

pub async fn fetch_rollback_events_in_slot_range<'e, E: PgExecutor<'e>>(
    exec: E,
    slot_low: i64,
    slot_high: i64,
) -> Result<Vec<RollbackEventRow>, QueryError> {
    let rows = sqlx::query_as::<_, RollbackEventRow>(
        "SELECT rollback_id, detected_at, slot_low, slot_high, abandoned_block_count, depth_slots, cause, resolved_at \
         FROM rollback_events WHERE slot_low <= $2 AND slot_high >= $1 ORDER BY rollback_id",
    )
    .bind(slot_low)
    .bind(slot_high)
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

// ---------------------------------------------------------------------
// gap_events — PROMOTABLE, owner sentinel-ingest. Natural key
// (slot_start, slot_end) UNIQUE, DO UPDATE on attempts.
// ---------------------------------------------------------------------

pub async fn record_gap_event<'e, E: PgExecutor<'e>>(
    exec: E,
    slot_start: i64,
    slot_end: i64,
    cause: &str,
) -> Result<(), QueryError> {
    sqlx::query(
        "INSERT INTO gap_events (slot_start, slot_end, cause) VALUES ($1, $2, $3) \
         ON CONFLICT (slot_start, slot_end) DO UPDATE SET attempts = gap_events.attempts + 1",
    )
    .bind(slot_start)
    .bind(slot_end)
    .bind(cause)
    .execute(exec)
    .await?;
    Ok(())
}

pub async fn mark_gap_event_repaired<'e, E: PgExecutor<'e>>(
    exec: E,
    slot_start: i64,
    slot_end: i64,
) -> Result<bool, QueryError> {
    let result = sqlx::query(
        "UPDATE gap_events SET repaired_at = now() WHERE slot_start = $1 AND slot_end = $2 AND repaired_at IS NULL",
    )
    .bind(slot_start)
    .bind(slot_end)
    .execute(exec)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn fetch_unrepaired_gap_events<'e, E: PgExecutor<'e>>(
    exec: E,
) -> Result<Vec<GapEventRow>, QueryError> {
    let rows = sqlx::query_as::<_, GapEventRow>(
        "SELECT gap_id, slot_start, slot_end, detected_at, repaired_at, cause, attempts \
         FROM gap_events WHERE repaired_at IS NULL ORDER BY slot_start",
    )
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

// ---------------------------------------------------------------------
// ingest_checkpoints — PROMOTABLE, owner sentinel-ingest. PK stream_name
// is the natural key.
// ---------------------------------------------------------------------

pub async fn upsert_ingest_checkpoint<'e, E: PgExecutor<'e>>(
    exec: E,
    stream_name: &str,
    last_contiguous_slot: i64,
    head_slot: i64,
    commitment: CommitmentLevel,
    holder: Option<&str>,
) -> Result<(), QueryError> {
    sqlx::query(
        "INSERT INTO ingest_checkpoints (stream_name, last_contiguous_slot, head_slot, commitment, holder) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (stream_name) DO UPDATE SET \
           last_contiguous_slot = excluded.last_contiguous_slot, head_slot = excluded.head_slot, \
           commitment = excluded.commitment, holder = excluded.holder, updated_at = now()",
    )
    .bind(stream_name)
    .bind(last_contiguous_slot)
    .bind(head_slot)
    .bind(commitment)
    .bind(holder)
    .execute(exec)
    .await?;
    Ok(())
}

pub async fn fetch_ingest_checkpoint<'e, E: PgExecutor<'e>>(
    exec: E,
    stream_name: &str,
) -> Result<Option<IngestCheckpointRow>, QueryError> {
    let row = sqlx::query_as::<_, IngestCheckpointRow>(
        "SELECT stream_name, last_contiguous_slot, head_slot, commitment, updated_at, holder \
         FROM ingest_checkpoints WHERE stream_name = $1",
    )
    .bind(stream_name)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

// ---------------------------------------------------------------------
// provider_health — MATERIALIZED, owner sentinel-rpc. PK
// (provider_id, window_start).
// ---------------------------------------------------------------------

pub struct ProviderHealthUpdate<'a> {
    pub provider_id: &'a str,
    pub window_start: DateTime<Utc>,
    pub requests: i64,
    pub errors: i64,
    pub timeouts: i64,
    pub rate_limited: i64,
    pub p50_ms: Option<i64>,
    pub p95_ms: Option<i64>,
    pub breaker_state: &'a str,
    pub last_error_code: Option<&'a str>,
}

pub async fn upsert_provider_health<'e, E: PgExecutor<'e>>(
    exec: E,
    h: ProviderHealthUpdate<'_>,
) -> Result<(), QueryError> {
    sqlx::query(
        "INSERT INTO provider_health \
           (provider_id, window_start, requests, errors, timeouts, rate_limited, p50_ms, p95_ms, breaker_state, last_error_code) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
         ON CONFLICT (provider_id, window_start) DO UPDATE SET \
           requests = excluded.requests, errors = excluded.errors, timeouts = excluded.timeouts, \
           rate_limited = excluded.rate_limited, p50_ms = excluded.p50_ms, p95_ms = excluded.p95_ms, \
           breaker_state = excluded.breaker_state, last_error_code = excluded.last_error_code",
    )
    .bind(h.provider_id)
    .bind(h.window_start)
    .bind(h.requests)
    .bind(h.errors)
    .bind(h.timeouts)
    .bind(h.rate_limited)
    .bind(h.p50_ms)
    .bind(h.p95_ms)
    .bind(h.breaker_state)
    .bind(h.last_error_code)
    .execute(exec)
    .await?;
    Ok(())
}

pub async fn fetch_provider_health<'e, E: PgExecutor<'e>>(
    exec: E,
    provider_id: &str,
    window_start: DateTime<Utc>,
) -> Result<Option<ProviderHealthRow>, QueryError> {
    let row = sqlx::query_as::<_, ProviderHealthRow>(
        "SELECT provider_id, window_start, requests, errors, timeouts, rate_limited, p50_ms, p95_ms, breaker_state, last_error_code \
         FROM provider_health WHERE provider_id = $1 AND window_start = $2",
    )
    .bind(provider_id)
    .bind(window_start)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

// ---------------------------------------------------------------------
// aegis_protocol_state — MATERIALIZED singleton, owner sentinel-aegis.
// DM-04: WHERE excluded.as_of_slot > current.as_of_slot.
// ---------------------------------------------------------------------

pub struct AegisProtocolStateFixture<'a> {
    pub program_id: &'a [u8],
    pub admin: &'a [u8],
    pub guardian: &'a [u8],
    pub fee_recipient: &'a [u8],
    pub paused: i16,
    pub as_of_slot: i64,
    pub as_of_commitment: CommitmentLevel,
    pub decoder_version_id: i64,
    pub materialized_via: MaterializedVia,
}

pub async fn upsert_aegis_protocol_state<'e, E: PgExecutor<'e>>(
    exec: E,
    f: AegisProtocolStateFixture<'_>,
) -> Result<bool, QueryError> {
    let result = sqlx::query(
        "INSERT INTO aegis_protocol_state \
           (program_id, admin, guardian, fee_recipient, paused, as_of_slot, as_of_commitment, decoder_version_id, materialized_via) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         ON CONFLICT (program_id) DO UPDATE SET \
           admin = excluded.admin, guardian = excluded.guardian, fee_recipient = excluded.fee_recipient, \
           paused = excluded.paused, as_of_slot = excluded.as_of_slot, as_of_commitment = excluded.as_of_commitment, \
           decoder_version_id = excluded.decoder_version_id, materialized_via = excluded.materialized_via \
         WHERE excluded.as_of_slot > aegis_protocol_state.as_of_slot",
    )
    .bind(f.program_id)
    .bind(f.admin)
    .bind(f.guardian)
    .bind(f.fee_recipient)
    .bind(f.paused)
    .bind(f.as_of_slot)
    .bind(f.as_of_commitment)
    .bind(f.decoder_version_id)
    .bind(f.materialized_via)
    .execute(exec)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn fetch_aegis_protocol_state<'e, E: PgExecutor<'e>>(
    exec: E,
    program_id: &[u8],
) -> Result<Option<AegisProtocolStateRow>, QueryError> {
    let row = sqlx::query_as::<_, AegisProtocolStateRow>(
        "SELECT program_id, admin, pending_admin, guardian, fee_recipient, paused, \
                as_of_slot, as_of_commitment, decoder_version_id, materialized_via \
         FROM aegis_protocol_state WHERE program_id = $1",
    )
    .bind(program_id)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

// ---------------------------------------------------------------------
// aegis_market_params_history — IMMUTABLE, owner sentinel-aegis.
// PK (market_pubkey, effective_from_slot), DO NOTHING.
// ---------------------------------------------------------------------

pub struct AegisMarketParamsHistoryFixture<'a> {
    pub market_pubkey: &'a [u8],
    pub effective_from_slot: i64,
    pub decoder_version_id: i64,
    pub collateral_feed_id: &'a [u8],
    pub loan_feed_id: &'a [u8],
}

/// Every other NOT NULL risk/IRM/oracle column in the frozen schema is
/// given a caller-supplied zero placeholder, exactly mirroring
/// `upsert_aegis_market`'s existing established pattern — no real Aegis
/// parameter semantics are invented (explicit Phase 2 non-scope).
pub async fn insert_aegis_market_params_history<'e, E: PgExecutor<'e>>(
    exec: E,
    f: AegisMarketParamsHistoryFixture<'_>,
) -> Result<bool, QueryError> {
    let zero_u128 = encode_u128(0);
    let zero_u64 = encode_u64(0);
    let result = sqlx::query(
        "INSERT INTO aegis_market_params_history ( \
           market_pubkey, effective_from_slot, \
           max_ltv, liq_threshold, liq_bonus, close_factor, full_liq_hf, liq_protocol_fee, fee, min_debt, \
           base_rate_ps, slope1_ps, slope2_ps, u_kink, max_rate_ps, \
           oracle_kind, collateral_feed_id, loan_feed_id, max_price_age_secs, max_conf_bps, decoder_version_id \
         ) VALUES ( \
           $1, $2, \
           $3::numeric, $3::numeric, $3::numeric, $3::numeric, $3::numeric, $3::numeric, $3::numeric, $4::numeric, \
           $3::numeric, $3::numeric, $3::numeric, $3::numeric, $3::numeric, \
           0, $5, $6, $4::numeric, 0, $7 \
         ) ON CONFLICT (market_pubkey, effective_from_slot) DO NOTHING",
    )
    .bind(f.market_pubkey)
    .bind(f.effective_from_slot)
    .bind(&zero_u128)
    .bind(&zero_u64)
    .bind(f.collateral_feed_id)
    .bind(f.loan_feed_id)
    .bind(f.decoder_version_id)
    .execute(exec)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn fetch_aegis_market_params_history<'e, E: PgExecutor<'e>>(
    exec: E,
    market_pubkey: &[u8],
) -> Result<Vec<AegisMarketParamsHistoryRow>, QueryError> {
    let rows = sqlx::query_as::<_, AegisMarketParamsHistoryRow>(
        "SELECT market_pubkey, effective_from_slot, effective_to_slot, decoder_version_id \
         FROM aegis_market_params_history WHERE market_pubkey = $1 ORDER BY effective_from_slot",
    )
    .bind(market_pubkey)
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

// ---------------------------------------------------------------------
// aegis_positions — MATERIALIZED, owner sentinel-aegis. DM-04:
// WHERE excluded.as_of_slot > current.as_of_slot.
// ---------------------------------------------------------------------

pub struct AegisPositionFixture<'a> {
    pub position_pubkey: &'a [u8],
    pub market_pubkey: &'a [u8],
    pub owner: &'a [u8],
    pub supply_shares: u128,
    pub borrow_shares: u128,
    pub collateral_amount: u64,
    pub is_open: bool,
    pub as_of_slot: i64,
    pub as_of_commitment: CommitmentLevel,
    pub decoder_version_id: i64,
    pub materialized_via: MaterializedVia,
    pub status: MaterializationStatus,
}

pub async fn upsert_aegis_position<'e, E: PgExecutor<'e>>(
    exec: E,
    f: AegisPositionFixture<'_>,
) -> Result<bool, QueryError> {
    let supply_shares = encode_u128(f.supply_shares);
    let borrow_shares = encode_u128(f.borrow_shares);
    let collateral_amount = encode_u64(f.collateral_amount);
    let result = sqlx::query(
        "INSERT INTO aegis_positions \
           (position_pubkey, market_pubkey, owner, supply_shares, borrow_shares, collateral_amount, \
            is_open, as_of_slot, as_of_commitment, decoder_version_id, materialized_via, status) \
         VALUES ($1, $2, $3, $4::numeric, $5::numeric, $6::numeric, $7, $8, $9, $10, $11, $12) \
         ON CONFLICT (position_pubkey) DO UPDATE SET \
           supply_shares = excluded.supply_shares, borrow_shares = excluded.borrow_shares, \
           collateral_amount = excluded.collateral_amount, is_open = excluded.is_open, \
           as_of_slot = excluded.as_of_slot, as_of_commitment = excluded.as_of_commitment, \
           decoder_version_id = excluded.decoder_version_id, materialized_via = excluded.materialized_via, \
           status = excluded.status \
         WHERE excluded.as_of_slot > aegis_positions.as_of_slot",
    )
    .bind(f.position_pubkey)
    .bind(f.market_pubkey)
    .bind(f.owner)
    .bind(&supply_shares)
    .bind(&borrow_shares)
    .bind(&collateral_amount)
    .bind(f.is_open)
    .bind(f.as_of_slot)
    .bind(f.as_of_commitment)
    .bind(f.decoder_version_id)
    .bind(f.materialized_via)
    .bind(f.status)
    .execute(exec)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn fetch_aegis_position<'e, E: PgExecutor<'e>>(
    exec: E,
    position_pubkey: &[u8],
) -> Result<Option<AegisPositionRow>, QueryError> {
    let row = sqlx::query_as::<_, AegisPositionRow>(
        "SELECT position_pubkey, market_pubkey, owner, supply_shares::text AS supply_shares, \
                borrow_shares::text AS borrow_shares, collateral_amount::text AS collateral_amount, \
                is_open, as_of_slot, as_of_commitment, last_snapshot_slot, materialized_via, decoder_version_id, status \
         FROM aegis_positions WHERE position_pubkey = $1",
    )
    .bind(position_pubkey)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

// ---------------------------------------------------------------------
// aegis_events — IMMUTABLE, owner sentinel-aegis. DO NOTHING.
// ---------------------------------------------------------------------

pub struct NewAegisEvent<'a> {
    pub signature: &'a [u8],
    pub slot: i64,
    pub blockhash: &'a [u8],
    pub ix_index: i16,
    pub inner_index: i16,
    pub log_index: i32,
    pub event_name: &'a str,
    pub market_pubkey: Option<&'a [u8]>,
    pub position_pubkey: Option<&'a [u8]>,
    pub payload: serde_json::Value,
    pub decoder_version_id: i64,
    pub as_of_commitment: CommitmentLevel,
}

pub async fn insert_aegis_event<'e, E: PgExecutor<'e>>(
    exec: E,
    e: NewAegisEvent<'_>,
) -> Result<bool, QueryError> {
    let result = sqlx::query(
        "INSERT INTO aegis_events \
           (signature, slot, blockhash, ix_index, inner_index, log_index, event_name, market_pubkey, position_pubkey, payload, decoder_version_id, as_of_commitment) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
         ON CONFLICT (signature, slot, blockhash, log_index) DO NOTHING",
    )
    .bind(e.signature)
    .bind(e.slot)
    .bind(e.blockhash)
    .bind(e.ix_index)
    .bind(e.inner_index)
    .bind(e.log_index)
    .bind(e.event_name)
    .bind(e.market_pubkey)
    .bind(e.position_pubkey)
    .bind(e.payload)
    .bind(e.decoder_version_id)
    .bind(e.as_of_commitment)
    .execute(exec)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn fetch_aegis_events_for_position<'e, E: PgExecutor<'e>>(
    exec: E,
    position_pubkey: &[u8],
) -> Result<Vec<AegisEventRow>, QueryError> {
    let rows = sqlx::query_as::<_, AegisEventRow>(
        "SELECT signature, slot, blockhash, ix_index, inner_index, log_index, event_name, \
                market_pubkey, position_pubkey, payload, decoder_version_id, as_of_commitment \
         FROM aegis_events WHERE position_pubkey = $1 ORDER BY slot, ix_index, inner_index",
    )
    .bind(position_pubkey)
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

// ---------------------------------------------------------------------
// aegis_oracle_observations — IMMUTABLE, owner sentinel-aegis. DO NOTHING.
// ---------------------------------------------------------------------

pub struct NewAegisOracleObservation<'a> {
    pub feed_id: &'a [u8],
    pub publish_time: DateTime<Utc>,
    pub slot: i64,
    pub price: u128,
    pub conf: u128,
    pub expo: i32,
    pub verification_level: &'a str,
    pub price_account: &'a [u8],
    pub price_lo_wad: u128,
    pub price_hi_wad: u128,
    pub validation_result: OracleValidationResult,
    pub failed_check: Option<&'a str>,
}

pub async fn insert_aegis_oracle_observation<'e, E: PgExecutor<'e>>(
    exec: E,
    o: NewAegisOracleObservation<'_>,
) -> Result<bool, QueryError> {
    let price = encode_u128(o.price);
    let conf = encode_u128(o.conf);
    let lo = encode_u128(o.price_lo_wad);
    let hi = encode_u128(o.price_hi_wad);
    let result = sqlx::query(
        "INSERT INTO aegis_oracle_observations \
           (feed_id, publish_time, slot, price, conf, expo, verification_level, price_account, price_lo_wad, price_hi_wad, validation_result, failed_check) \
         VALUES ($1, $2, $3, $4::numeric, $5::numeric, $6, $7, $8, $9::numeric, $10::numeric, $11, $12) \
         ON CONFLICT (feed_id, publish_time, slot) DO NOTHING",
    )
    .bind(o.feed_id)
    .bind(o.publish_time)
    .bind(o.slot)
    .bind(&price)
    .bind(&conf)
    .bind(o.expo)
    .bind(o.verification_level)
    .bind(o.price_account)
    .bind(&lo)
    .bind(&hi)
    .bind(o.validation_result)
    .bind(o.failed_check)
    .execute(exec)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn fetch_latest_valid_oracle_observation<'e, E: PgExecutor<'e>>(
    exec: E,
    feed_id: &[u8],
) -> Result<Option<AegisOracleObservationRow>, QueryError> {
    let row = sqlx::query_as::<_, AegisOracleObservationRow>(
        "SELECT feed_id, publish_time, slot, price::text AS price, conf::text AS conf, expo, \
                verification_level, price_account, price_lo_wad::text AS price_lo_wad, \
                price_hi_wad::text AS price_hi_wad, validation_result, failed_check \
         FROM aegis_oracle_observations \
         WHERE feed_id = $1 AND validation_result = 'valid' ORDER BY slot DESC LIMIT 1",
    )
    .bind(feed_id)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

// ---------------------------------------------------------------------
// aegis_invariant_checks — IMMUTABLE, owner sentinel-risk. No declared
// natural key (data-model.md §5): each check run is its own fact.
// ---------------------------------------------------------------------

pub struct NewAegisInvariantCheck<'a> {
    pub invariant_id: &'a str,
    pub market_pubkey: Option<&'a [u8]>,
    pub slot: i64,
    pub expected: u128,
    pub actual: u128,
    pub holds: bool,
}

pub async fn insert_aegis_invariant_check<'e, E: PgExecutor<'e>>(
    exec: E,
    c: NewAegisInvariantCheck<'_>,
) -> Result<i64, QueryError> {
    let expected = encode_u128(c.expected);
    let actual = encode_u128(c.actual);
    let row = sqlx::query(
        "INSERT INTO aegis_invariant_checks (invariant_id, market_pubkey, slot, expected, actual, holds) \
         VALUES ($1, $2, $3, $4::numeric, $5::numeric, $6) RETURNING check_id",
    )
    .bind(c.invariant_id)
    .bind(c.market_pubkey)
    .bind(c.slot)
    .bind(&expected)
    .bind(&actual)
    .bind(c.holds)
    .fetch_one(exec)
    .await?;
    Ok(row.try_get::<i64, _>("check_id")?)
}

pub async fn fetch_failed_invariant_checks<'e, E: PgExecutor<'e>>(
    exec: E,
) -> Result<Vec<AegisInvariantCheckRow>, QueryError> {
    let rows = sqlx::query_as::<_, AegisInvariantCheckRow>(
        "SELECT check_id, invariant_id, market_pubkey, slot, expected::text AS expected, \
                actual::text AS actual, holds, checked_at \
         FROM aegis_invariant_checks WHERE NOT holds ORDER BY slot DESC",
    )
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

// ---------------------------------------------------------------------
// position_health — MATERIALIZED, owner sentinel-risk. DO NOTHING (a
// health value for a given slot is a fact about that slot).
// ---------------------------------------------------------------------

pub struct NewPositionHealth<'a> {
    pub position_pubkey: &'a [u8],
    pub computed_at_slot: i64,
    pub t_eval: DateTime<Utc>,
    pub collateral_value_wad: u128,
    pub debt_value_wad: u128,
    pub debt_assets: u64,
    pub health_factor_wad: Option<u128>,
    pub state: HealthState,
    pub market_params_from_slot: i64,
    pub commitment: CommitmentLevel,
}

pub async fn insert_position_health<'e, E: PgExecutor<'e>>(
    exec: E,
    h: NewPositionHealth<'_>,
) -> Result<bool, QueryError> {
    let collateral_value_wad = encode_u128(h.collateral_value_wad);
    let debt_value_wad = encode_u128(h.debt_value_wad);
    let debt_assets = encode_u64(h.debt_assets);
    let health_factor_wad = h.health_factor_wad.map(encode_u128);
    let result = sqlx::query(
        "INSERT INTO position_health \
           (position_pubkey, computed_at_slot, t_eval, collateral_value_wad, debt_value_wad, debt_assets, \
            health_factor_wad, state, market_params_from_slot, commitment) \
         VALUES ($1, $2, $3, $4::numeric, $5::numeric, $6::numeric, $7::numeric, $8, $9, $10) \
         ON CONFLICT (position_pubkey, computed_at_slot) DO NOTHING",
    )
    .bind(h.position_pubkey)
    .bind(h.computed_at_slot)
    .bind(h.t_eval)
    .bind(&collateral_value_wad)
    .bind(&debt_value_wad)
    .bind(&debt_assets)
    .bind(health_factor_wad.as_deref())
    .bind(h.state)
    .bind(h.market_params_from_slot)
    .bind(h.commitment)
    .execute(exec)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn fetch_latest_position_health<'e, E: PgExecutor<'e>>(
    exec: E,
    position_pubkey: &[u8],
) -> Result<Option<PositionHealthRow>, QueryError> {
    let row = sqlx::query_as::<_, PositionHealthRow>(
        "SELECT position_pubkey, computed_at_slot, t_eval, collateral_value_wad::text AS collateral_value_wad, \
                debt_value_wad::text AS debt_value_wad, debt_assets::text AS debt_assets, \
                health_factor_wad::text AS health_factor_wad, state, \
                liquidation_price_wad::text AS liquidation_price_wad, borrow_capacity_wad::text AS borrow_capacity_wad, \
                collateral_obs_id, collateral_obs_slot, loan_obs_id, loan_obs_slot, \
                market_params_from_slot, commitment \
         FROM position_health WHERE position_pubkey = $1 ORDER BY computed_at_slot DESC LIMIT 1",
    )
    .bind(position_pubkey)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

// ---------------------------------------------------------------------
// liquidation_candidates — MATERIALIZED, owner sentinel-risk. Natural key
// (position_pubkey, detected_at_slot) UNIQUE, DO NOTHING on detection;
// status is advanced by a separate UPDATE (open -> claimed/executed/
// expired/invalidated), never re-inserted.
// ---------------------------------------------------------------------

pub struct NewLiquidationCandidate<'a> {
    pub position_pubkey: &'a [u8],
    pub market_pubkey: &'a [u8],
    pub detected_at_slot: i64,
    pub t_eval: DateTime<Utc>,
    pub lookahead_ms: i32,
    pub risk_params_hash: &'a str,
    pub max_repay_assets: u64,
    pub expected_seize: u64,
    pub expected_bonus: u64,
    pub expected_protocol_cut: u64,
    pub estimated_profit_wad: u128,
    pub profitable: bool,
    pub full_liquidation: bool,
    pub dust_rule_applied: bool,
    pub expires_at: DateTime<Utc>,
}

pub async fn insert_liquidation_candidate<'e, E: PgExecutor<'e>>(
    exec: E,
    c: NewLiquidationCandidate<'_>,
) -> Result<Option<i64>, QueryError> {
    let max_repay_assets = encode_u64(c.max_repay_assets);
    let expected_seize = encode_u64(c.expected_seize);
    let expected_bonus = encode_u64(c.expected_bonus);
    let expected_protocol_cut = encode_u64(c.expected_protocol_cut);
    let estimated_profit_wad = encode_u128(c.estimated_profit_wad);
    let row = sqlx::query(
        "INSERT INTO liquidation_candidates ( \
           position_pubkey, market_pubkey, detected_at_slot, t_eval, lookahead_ms, risk_params_hash, \
           max_repay_assets, expected_seize, expected_bonus, expected_protocol_cut, estimated_profit_wad, \
           profitable, full_liquidation, dust_rule_applied, expires_at, status \
         ) VALUES ($1, $2, $3, $4, $5, $6, $7::numeric, $8::numeric, $9::numeric, $10::numeric, $11::numeric, $12, $13, $14, $15, 'open') \
         ON CONFLICT (position_pubkey, detected_at_slot) DO NOTHING \
         RETURNING candidate_id",
    )
    .bind(c.position_pubkey)
    .bind(c.market_pubkey)
    .bind(c.detected_at_slot)
    .bind(c.t_eval)
    .bind(c.lookahead_ms)
    .bind(c.risk_params_hash)
    .bind(&max_repay_assets)
    .bind(&expected_seize)
    .bind(&expected_bonus)
    .bind(&expected_protocol_cut)
    .bind(&estimated_profit_wad)
    .bind(c.profitable)
    .bind(c.full_liquidation)
    .bind(c.dust_rule_applied)
    .bind(c.expires_at)
    .fetch_optional(exec)
    .await?;
    Ok(row
        .map(|r| r.try_get::<i64, _>("candidate_id"))
        .transpose()?)
}

pub async fn update_liquidation_candidate_status<'e, E: PgExecutor<'e>>(
    exec: E,
    candidate_id: i64,
    status: CandidateStatus,
    invalidated_reason: Option<&str>,
) -> Result<bool, QueryError> {
    let result = sqlx::query(
        "UPDATE liquidation_candidates SET status = $2, invalidated_reason = $3 WHERE candidate_id = $1",
    )
    .bind(candidate_id)
    .bind(status)
    .bind(invalidated_reason)
    .execute(exec)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn fetch_open_liquidation_candidates<'e, E: PgExecutor<'e>>(
    exec: E,
) -> Result<Vec<LiquidationCandidateRow>, QueryError> {
    let rows = sqlx::query_as::<_, LiquidationCandidateRow>(
        "SELECT candidate_id, position_pubkey, market_pubkey, detected_at_slot, t_eval, lookahead_ms, \
                risk_params_hash, health_factor_wad::text AS health_factor_wad, \
                max_repay_assets::text AS max_repay_assets, expected_seize::text AS expected_seize, \
                expected_bonus::text AS expected_bonus, expected_protocol_cut::text AS expected_protocol_cut, \
                estimated_profit_wad::text AS estimated_profit_wad, profitable, reason_unprofitable, \
                full_liquidation, dust_rule_applied, expires_at, status, invalidated_reason \
         FROM liquidation_candidates WHERE status = 'open' ORDER BY estimated_profit_wad DESC",
    )
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

// ---------------------------------------------------------------------
// market_metrics — MATERIALIZED, owner sentinel-risk. Freely rewritten;
// bucket_start plus single-writer ownership is the idempotency mechanism
// (infra/migrations/0007_derived_layer.sql's own comment).
// ---------------------------------------------------------------------

pub struct MarketMetricUpsert<'a> {
    pub market_pubkey: &'a [u8],
    pub bucket_start: DateTime<Utc>,
    pub utilization_wad: u128,
    pub borrow_rate_ps: u128,
    pub supply_rate_ps: u128,
    pub total_supply_assets: u64,
    pub total_borrow_assets: u64,
    pub total_supply_shares: u128,
    pub total_borrow_shares: u128,
    pub free_liquidity: u64,
    pub accrual_staleness_secs: i64,
    pub open_positions: i64,
    pub positions_with_debt: i64,
    pub aggregate_bad_debt: u64,
}

pub async fn upsert_market_metrics<'e, E: PgExecutor<'e>>(
    exec: E,
    m: MarketMetricUpsert<'_>,
) -> Result<(), QueryError> {
    let utilization_wad = encode_u128(m.utilization_wad);
    let borrow_rate_ps = encode_u128(m.borrow_rate_ps);
    let supply_rate_ps = encode_u128(m.supply_rate_ps);
    let total_supply_assets = encode_u64(m.total_supply_assets);
    let total_borrow_assets = encode_u64(m.total_borrow_assets);
    let total_supply_shares = encode_u128(m.total_supply_shares);
    let total_borrow_shares = encode_u128(m.total_borrow_shares);
    let free_liquidity = encode_u64(m.free_liquidity);
    let aggregate_bad_debt = encode_u64(m.aggregate_bad_debt);
    sqlx::query(
        "INSERT INTO market_metrics ( \
           market_pubkey, bucket_start, utilization_wad, borrow_rate_ps, supply_rate_ps, \
           total_supply_assets, total_borrow_assets, total_supply_shares, total_borrow_shares, \
           free_liquidity, accrual_staleness_secs, open_positions, positions_with_debt, aggregate_bad_debt \
         ) VALUES ($1, $2, $3::numeric, $4::numeric, $5::numeric, $6::numeric, $7::numeric, $8::numeric, $9::numeric, $10::numeric, $11, $12, $13, $14::numeric) \
         ON CONFLICT (market_pubkey, bucket_start) DO UPDATE SET \
           utilization_wad = excluded.utilization_wad, borrow_rate_ps = excluded.borrow_rate_ps, \
           supply_rate_ps = excluded.supply_rate_ps, total_supply_assets = excluded.total_supply_assets, \
           total_borrow_assets = excluded.total_borrow_assets, total_supply_shares = excluded.total_supply_shares, \
           total_borrow_shares = excluded.total_borrow_shares, free_liquidity = excluded.free_liquidity, \
           accrual_staleness_secs = excluded.accrual_staleness_secs, open_positions = excluded.open_positions, \
           positions_with_debt = excluded.positions_with_debt, aggregate_bad_debt = excluded.aggregate_bad_debt",
    )
    .bind(m.market_pubkey)
    .bind(m.bucket_start)
    .bind(&utilization_wad)
    .bind(&borrow_rate_ps)
    .bind(&supply_rate_ps)
    .bind(&total_supply_assets)
    .bind(&total_borrow_assets)
    .bind(&total_supply_shares)
    .bind(&total_borrow_shares)
    .bind(&free_liquidity)
    .bind(m.accrual_staleness_secs)
    .bind(m.open_positions)
    .bind(m.positions_with_debt)
    .bind(&aggregate_bad_debt)
    .execute(exec)
    .await?;
    Ok(())
}

pub async fn fetch_market_metrics<'e, E: PgExecutor<'e>>(
    exec: E,
    market_pubkey: &[u8],
    bucket_start: DateTime<Utc>,
) -> Result<Option<MarketMetricRow>, QueryError> {
    let row = sqlx::query_as::<_, MarketMetricRow>(
        "SELECT market_pubkey, bucket_start, utilization_wad::text AS utilization_wad, \
                borrow_rate_ps::text AS borrow_rate_ps, supply_rate_ps::text AS supply_rate_ps, \
                total_supply_assets::text AS total_supply_assets, total_borrow_assets::text AS total_borrow_assets, \
                total_supply_shares::text AS total_supply_shares, total_borrow_shares::text AS total_borrow_shares, \
                free_liquidity::text AS free_liquidity, accrual_staleness_secs, open_positions, positions_with_debt, \
                aggregate_bad_debt::text AS aggregate_bad_debt \
         FROM market_metrics WHERE market_pubkey = $1 AND bucket_start = $2",
    )
    .bind(market_pubkey)
    .bind(bucket_start)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

// ---------------------------------------------------------------------
// reconciliation_mismatches — IMMUTABLE, owner sentinel-risk +
// sentinel-executor. No declared natural key: each mismatch is its own
// fact about a specific detection event.
// ---------------------------------------------------------------------

pub struct NewReconciliationMismatch<'a> {
    pub class: MismatchClass,
    pub intent_id: Option<Uuid>,
    pub attempt_id: Option<Uuid>,
    pub entity_kind: &'a str,
    pub entity_key: &'a str,
    pub slot: i64,
    pub predicted: serde_json::Value,
    pub actual: serde_json::Value,
    pub onchain_error_code: Option<&'a str>,
}

pub async fn insert_reconciliation_mismatch<'e, E: PgExecutor<'e>>(
    exec: E,
    m: NewReconciliationMismatch<'_>,
) -> Result<i64, QueryError> {
    let row = sqlx::query(
        "INSERT INTO reconciliation_mismatches \
           (class, intent_id, attempt_id, entity_kind, entity_key, slot, predicted, actual, onchain_error_code) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING mismatch_id",
    )
    .bind(m.class)
    .bind(m.intent_id)
    .bind(m.attempt_id)
    .bind(m.entity_kind)
    .bind(m.entity_key)
    .bind(m.slot)
    .bind(m.predicted)
    .bind(m.actual)
    .bind(m.onchain_error_code)
    .fetch_one(exec)
    .await?;
    Ok(row.try_get::<i64, _>("mismatch_id")?)
}

pub async fn fetch_reconciliation_mismatches_for_entity<'e, E: PgExecutor<'e>>(
    exec: E,
    entity_kind: &str,
    entity_key: &str,
) -> Result<Vec<ReconciliationMismatchRow>, QueryError> {
    let rows = sqlx::query_as::<_, ReconciliationMismatchRow>(
        "SELECT mismatch_id, class, intent_id, attempt_id, entity_kind, entity_key, slot, \
                predicted, actual, onchain_error_code, detected_at \
         FROM reconciliation_mismatches WHERE entity_kind = $1 AND entity_key = $2 ORDER BY detected_at DESC",
    )
    .bind(entity_kind)
    .bind(entity_key)
    .fetch_all(exec)
    .await?;
    Ok(rows)
}
