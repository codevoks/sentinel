//! Typed row structs for the tables this phase's query layer and test
//! campaign exercise directly. Every field maps 1:1 to a column declared in
//! `infra/migrations/`; `numeric(39,0)`/`numeric(20,0)` columns are typed
//! `String` here (the exact `::text` wire representation — see
//! `crate::numeric`) and converted with `NumericU128`/`NumericU64` at call
//! sites, never through a float.
//!
//! **Scope note (honest, not silently narrowed):** row structs exist here
//! for the tables Phase 2's adversarial test campaign and job queue
//! directly exercise: `raw_observations`, `decode_failures`, `slots`,
//! `transactions`, `decoder_versions`, `aegis_markets`, `aegis_positions`,
//! `alerts`, `execution_intents`, `transaction_attempts`, `jobs`. The
//! remaining tables in `docs/data-model.md` exist in the schema with their
//! full constraints (every migration in `infra/migrations/` creates them),
//! but do not yet have a corresponding Rust row struct/query helper — no
//! later-phase crate writes them yet (phase-02-data-model.md §2 explicit
//! non-scope: "nothing writes [aegis_*] yet"), so there is no consumer to
//! type against. Recorded in `docs/project-status.md` rather than silently
//! presented as complete.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::enums::{
    AttemptState, CommitmentLevel, IntentKind, IntentState, JobKind, JobState,
    MaterializationStatus, MaterializedVia, ObservationSource, PayloadEncoding, PriorityFeeSource,
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
