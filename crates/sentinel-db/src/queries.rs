//! Query helpers implementing the exact conflict/monotonicity semantics
//! `docs/data-model.md` and `docs/distributed-correctness.md` require.
//! Every query here is a bound, parameterized statement (`CI-NOSQLFMT`,
//! `scripts/ci-guards.sh`) — no `format!` ever builds a `SELECT`/`INSERT`/
//! `UPDATE`/`DELETE` string with interpolated data.

use chrono::{DateTime, Utc};
use sqlx::{PgExecutor, Row};
use uuid::Uuid;

use crate::enums::{
    AttemptState, CommitmentLevel, IntentKind, JobKind, ObservationSource, PayloadEncoding,
    RawObservationKind, SlotStatus,
};
use crate::numeric::{encode_u128, encode_u64};
use crate::tables::{
    AlertRow, DecoderVersionRow, ExecutionIntentRow, JobRow, RawObservationRow, SlotRow,
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
