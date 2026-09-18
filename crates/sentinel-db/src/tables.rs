//! Typed row structs for **every** canonical table declared in
//! `docs/data-model.md` and created by `infra/migrations/`. Every field maps
//! 1:1 to a column; `numeric(39,0)`/`numeric(20,0)` columns are typed
//! `String` here (the exact `::text` wire representation — see
//! `crate::numeric`) and converted with `NumericU128`/`NumericU64` at call
//! sites, never through a float.
//!
//! **Coverage (closure fix, `docs/phases/phase-02-data-model.md` §1.4):**
//! every one of the 28 canonical tables (verified against
//! `information_schema`/`pg_catalog` on a live Postgres — see
//! `crates/sentinel-db/tests/coverage_audit.rs`) has a row struct here and
//! `sentinel_rust`-role-appropriate query functions in `queries.rs`. Tables
//! `sentinel_rust` does not own (per `docs/data-model.md` §10 and the actual
//! GRANTs in `infra/migrations/0010_grants.sql`) get read-only typed access
//! here, never an invented writer.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::enums::{
    AccountObservationSource, AttemptState, CandidateStatus, CommitmentLevel, HealthState,
    IntentKind, IntentState, JobKind, JobState, LogKind, MaterializationStatus, MaterializedVia,
    MismatchClass, ObservationSource, OracleValidationResult, PayloadEncoding, PriorityFeeSource,
    RawObservationKind, SlotStatus, TxVersion,
};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RawObservationRow {
    pub observation_id: i64,
    pub kind: RawObservationKind,
    pub natural_key: String,
    pub slot: i64,
    pub commitment: CommitmentLevel,
    pub source: ObservationSource,
    pub provider_id: String,
    pub request_id: Option<Uuid>,
    pub observed_at: DateTime<Utc>,
    pub payload: Vec<u8>,
    pub payload_hash: Vec<u8>,
    pub payload_encoding: PayloadEncoding,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DecodeFailureRow {
    pub failure_id: i64,
    pub observation_id: i64,
    pub observation_slot: i64,
    pub stage: String,
    pub error_code: String,
    pub error_detail: Option<String>,
    pub decoder_version_id: Option<i64>,
    pub failed_at: DateTime<Utc>,
    pub last_failed_at: DateTime<Utc>,
    pub retry_count: i32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SlotRow {
    pub slot: i64,
    pub blockhash: Option<Vec<u8>>,
    pub parent_slot: Option<i64>,
    pub parent_blockhash: Option<Vec<u8>>,
    pub block_time: Option<DateTime<Utc>>,
    pub block_height: Option<i64>,
    pub status: SlotStatus,
    pub commitment: CommitmentLevel,
    pub canonical: bool,
    pub first_seen_at: DateTime<Utc>,
    pub finalized_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TransactionRow {
    pub signature: Vec<u8>,
    pub slot: i64,
    pub blockhash: Vec<u8>,
    pub transaction_index: i32,
    pub version: TxVersion,
    pub num_required_signatures: i16,
    pub recent_blockhash: Vec<u8>,
    pub success: bool,
    pub error_code: Option<String>,
    /// numeric(20,0) as exact decimal text — see `crate::numeric`.
    pub fee_lamports: String,
    pub compute_units_consumed: Option<String>,
    pub compute_unit_limit: Option<String>,
    pub priority_fee_lamports: Option<String>,
    pub priority_fee_source: PriorityFeeSource,
    pub loaded_addresses_from_alt: bool,
    pub is_vote: bool,
    pub commitment: CommitmentLevel,
    pub canonical: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DecoderVersionRow {
    pub id: i64,
    pub protocol: String,
    pub program_id: String,
    pub account_kind: String,
    pub schema_version: i32,
    pub discriminator: Vec<u8>,
    pub layout_hash: String,
    pub effective_from_slot: i64,
    pub effective_to_slot: Option<i64>,
    pub source: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AegisMarketRow {
    pub market_pubkey: Vec<u8>,
    pub program_id: Vec<u8>,
    pub as_of_slot: i64,
    pub as_of_commitment: CommitmentLevel,
    pub last_snapshot_slot: Option<i64>,
    pub materialized_via: MaterializedVia,
    pub decoder_version_id: i64,
    pub status: MaterializationStatus,
    // Every other column exists in the schema (infra/migrations/0006_protocol_layer.sql)
    // but is not read back through this row type yet — no Phase 2 consumer
    // needs the full risk-parameter set; the monotonicity/DM-04 test only
    // needs the columns above plus the ones it writes directly.
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AlertRow {
    pub alert_id: i64,
    pub kind: String,
    pub severity: String,
    pub entity_kind: String,
    pub entity_key: String,
    pub opened_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub detail: serde_json::Value,
    pub runbook_id: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExecutionIntentRow {
    pub intent_id: Uuid,
    pub idempotency_key: String,
    pub kind: IntentKind,
    pub market_pubkey: Vec<u8>,
    pub position_pubkey: Option<Vec<u8>>,
    pub params: serde_json::Value,
    pub constraints: serde_json::Value,
    pub state: IntentState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub lease_holder: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub cumulative_fee_lamports: i64,
    pub terminal_reason: Option<String>,
    pub trigger_slot: i64,
    pub trigger_commitment: CommitmentLevel,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TransactionAttemptRow {
    pub attempt_id: Uuid,
    pub intent_id: Uuid,
    pub attempt_number: i32,
    pub signature: Vec<u8>,
    pub transaction_bytes: Vec<u8>,
    pub transaction_version: TxVersion,
    pub recent_blockhash: Vec<u8>,
    pub last_valid_block_height: i64,
    pub compute_unit_limit: Option<i32>,
    pub priority_fee_lamports: Option<i64>,
    pub simulation_result: Option<serde_json::Value>,
    pub simulated_units: Option<i32>,
    pub state: AttemptState,
    pub signed_at: DateTime<Utc>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub observed_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub observed_slot: Option<i64>,
    pub onchain_error_code: Option<String>,
    pub onchain_error_band: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct JobRow {
    pub job_id: i64,
    pub kind: JobKind,
    pub dedupe_key: String,
    pub payload: serde_json::Value,
    pub priority: i16,
    pub state: JobState,
    pub lease_holder: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub attempts: i32,
    pub max_attempts: i32,
    pub last_error: Option<String>,
    pub available_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// =====================================================================
// Closure-fix additions: every remaining canonical table from
// docs/data-model.md. See infra/migrations/0004-0009 for the exact
// column definitions these mirror 1:1.
// =====================================================================

// --- normalized layer (0004_normalized_layer.sql) ---------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InstructionRow {
    pub signature: Vec<u8>,
    pub slot: i64,
    pub blockhash: Vec<u8>,
    pub ix_index: i16,
    pub inner_index: i16,
    pub stack_height: i16,
    pub program_id: Vec<u8>,
    pub accounts: Vec<Vec<u8>>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProgramLogRow {
    pub signature: Vec<u8>,
    pub slot: i64,
    pub blockhash: Vec<u8>,
    pub log_index: i32,
    pub program_id: Option<Vec<u8>>,
    pub raw_line: String,
    pub kind: LogKind,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AccountObservationRow {
    pub pubkey: Vec<u8>,
    pub slot: i64,
    pub content_hash: Vec<u8>,
    pub owner_program: Vec<u8>,
    /// numeric(20,0) as exact decimal text — see `crate::numeric`.
    pub lamports: String,
    pub data: Vec<u8>,
    pub executable: bool,
    /// numeric(20,0) as exact decimal text.
    pub rent_epoch: String,
    pub write_version: Option<i64>,
    pub source: AccountObservationSource,
    pub commitment: CommitmentLevel,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TokenBalanceDeltaRow {
    pub signature: Vec<u8>,
    pub slot: i64,
    pub blockhash: Vec<u8>,
    pub account_index: i16,
    pub token_account: Vec<u8>,
    pub mint: Vec<u8>,
    pub owner: Vec<u8>,
    /// numeric(39,0) as exact decimal text.
    pub pre_amount: String,
    /// numeric(39,0) as exact decimal text.
    pub post_amount: String,
    pub decimals: i16,
    pub program_id: Vec<u8>,
}

// --- chain-state layer (0005_chainstate_layer.sql) --------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RollbackEventRow {
    pub rollback_id: i64,
    pub detected_at: DateTime<Utc>,
    pub slot_low: i64,
    pub slot_high: i64,
    pub abandoned_block_count: i32,
    pub depth_slots: i64,
    pub cause: String,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GapEventRow {
    pub gap_id: i64,
    pub slot_start: i64,
    pub slot_end: i64,
    pub detected_at: DateTime<Utc>,
    pub repaired_at: Option<DateTime<Utc>>,
    pub cause: String,
    pub attempts: i32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IngestCheckpointRow {
    pub stream_name: String,
    pub last_contiguous_slot: i64,
    pub head_slot: i64,
    pub commitment: CommitmentLevel,
    pub updated_at: DateTime<Utc>,
    pub holder: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProviderHealthRow {
    pub provider_id: String,
    pub window_start: DateTime<Utc>,
    pub requests: i64,
    pub errors: i64,
    pub timeouts: i64,
    pub rate_limited: i64,
    pub p50_ms: Option<i64>,
    pub p95_ms: Option<i64>,
    pub breaker_state: String,
    pub last_error_code: Option<String>,
}

// --- protocol layer (0006_protocol_layer.sql) --------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AegisProtocolStateRow {
    pub program_id: Vec<u8>,
    pub admin: Vec<u8>,
    pub pending_admin: Option<Vec<u8>>,
    pub guardian: Vec<u8>,
    pub fee_recipient: Vec<u8>,
    pub paused: i16,
    pub as_of_slot: i64,
    pub as_of_commitment: CommitmentLevel,
    pub decoder_version_id: i64,
    pub materialized_via: MaterializedVia,
}

/// The versioned parameter set (`aegis-integration.md` §4.3). Only the
/// bookkeeping/range columns are typed here (mirroring the same,
/// already-established pattern `AegisMarketRow` uses for `aegis_markets`) —
/// every other NOT NULL risk/IRM/oracle column exists in the schema with
/// its full constraints and is written via the fixture-style insert helper
/// in `queries.rs`, exactly as `aegis_markets` already does.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AegisMarketParamsHistoryRow {
    pub market_pubkey: Vec<u8>,
    pub effective_from_slot: i64,
    pub effective_to_slot: Option<i64>,
    pub decoder_version_id: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AegisPositionRow {
    pub position_pubkey: Vec<u8>,
    pub market_pubkey: Vec<u8>,
    pub owner: Vec<u8>,
    /// numeric(39,0) as exact decimal text.
    pub supply_shares: String,
    /// numeric(39,0) as exact decimal text.
    pub borrow_shares: String,
    /// numeric(20,0) as exact decimal text.
    pub collateral_amount: String,
    pub is_open: bool,
    pub as_of_slot: i64,
    pub as_of_commitment: CommitmentLevel,
    pub last_snapshot_slot: Option<i64>,
    pub materialized_via: MaterializedVia,
    pub decoder_version_id: i64,
    pub status: MaterializationStatus,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AegisEventRow {
    pub signature: Vec<u8>,
    pub slot: i64,
    pub blockhash: Vec<u8>,
    pub ix_index: i16,
    pub inner_index: i16,
    pub log_index: i32,
    pub event_name: String,
    pub market_pubkey: Option<Vec<u8>>,
    pub position_pubkey: Option<Vec<u8>>,
    pub payload: serde_json::Value,
    pub decoder_version_id: i64,
    pub as_of_commitment: CommitmentLevel,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AegisOracleObservationRow {
    pub feed_id: Vec<u8>,
    pub publish_time: DateTime<Utc>,
    pub slot: i64,
    /// numeric(39,0) as exact decimal text.
    pub price: String,
    /// numeric(39,0) as exact decimal text.
    pub conf: String,
    pub expo: i32,
    pub verification_level: String,
    pub price_account: Vec<u8>,
    /// numeric(39,0) as exact decimal text.
    pub price_lo_wad: String,
    /// numeric(39,0) as exact decimal text.
    pub price_hi_wad: String,
    pub validation_result: OracleValidationResult,
    pub failed_check: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AegisInvariantCheckRow {
    pub check_id: i64,
    pub invariant_id: String,
    pub market_pubkey: Option<Vec<u8>>,
    pub slot: i64,
    /// numeric(39,0) as exact decimal text.
    pub expected: String,
    /// numeric(39,0) as exact decimal text.
    pub actual: String,
    pub holds: bool,
    pub checked_at: DateTime<Utc>,
}

// --- derived layer (0007_derived_layer.sql) -----------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PositionHealthRow {
    pub position_pubkey: Vec<u8>,
    pub computed_at_slot: i64,
    pub t_eval: DateTime<Utc>,
    /// numeric(39,0) as exact decimal text.
    pub collateral_value_wad: String,
    /// numeric(39,0) as exact decimal text.
    pub debt_value_wad: String,
    /// numeric(20,0) as exact decimal text.
    pub debt_assets: String,
    /// numeric(39,0) as exact decimal text.
    pub health_factor_wad: Option<String>,
    pub state: HealthState,
    /// numeric(39,0) as exact decimal text.
    pub liquidation_price_wad: Option<String>,
    /// numeric(39,0) as exact decimal text.
    pub borrow_capacity_wad: Option<String>,
    pub collateral_obs_id: Option<i64>,
    pub collateral_obs_slot: Option<i64>,
    pub loan_obs_id: Option<i64>,
    pub loan_obs_slot: Option<i64>,
    pub market_params_from_slot: i64,
    pub commitment: CommitmentLevel,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LiquidationCandidateRow {
    pub candidate_id: i64,
    pub position_pubkey: Vec<u8>,
    pub market_pubkey: Vec<u8>,
    pub detected_at_slot: i64,
    pub t_eval: DateTime<Utc>,
    pub lookahead_ms: i32,
    pub risk_params_hash: String,
    /// numeric(39,0) as exact decimal text.
    pub health_factor_wad: Option<String>,
    /// numeric(20,0) as exact decimal text.
    pub max_repay_assets: String,
    /// numeric(20,0) as exact decimal text.
    pub expected_seize: String,
    /// numeric(20,0) as exact decimal text.
    pub expected_bonus: String,
    /// numeric(20,0) as exact decimal text.
    pub expected_protocol_cut: String,
    /// numeric(39,0) as exact decimal text.
    pub estimated_profit_wad: String,
    pub profitable: bool,
    pub reason_unprofitable: Option<String>,
    pub full_liquidation: bool,
    pub dust_rule_applied: bool,
    pub expires_at: DateTime<Utc>,
    pub status: CandidateStatus,
    pub invalidated_reason: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MarketMetricRow {
    pub market_pubkey: Vec<u8>,
    pub bucket_start: DateTime<Utc>,
    /// numeric(39,0) as exact decimal text.
    pub utilization_wad: String,
    /// numeric(39,0) as exact decimal text.
    pub borrow_rate_ps: String,
    /// numeric(39,0) as exact decimal text.
    pub supply_rate_ps: String,
    /// numeric(20,0) as exact decimal text.
    pub total_supply_assets: String,
    /// numeric(20,0) as exact decimal text.
    pub total_borrow_assets: String,
    /// numeric(39,0) as exact decimal text.
    pub total_supply_shares: String,
    /// numeric(39,0) as exact decimal text.
    pub total_borrow_shares: String,
    /// numeric(20,0) as exact decimal text.
    pub free_liquidity: String,
    pub accrual_staleness_secs: i64,
    pub open_positions: i64,
    pub positions_with_debt: i64,
    /// numeric(20,0) as exact decimal text.
    pub aggregate_bad_debt: String,
}

// --- execution layer (0008_execution_layer.sql) -------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReconciliationMismatchRow {
    pub mismatch_id: i64,
    pub class: MismatchClass,
    pub intent_id: Option<Uuid>,
    pub attempt_id: Option<Uuid>,
    pub entity_kind: String,
    pub entity_key: String,
    pub slot: i64,
    pub predicted: serde_json::Value,
    pub actual: serde_json::Value,
    pub onchain_error_code: Option<String>,
    pub detected_at: DateTime<Utc>,
}
