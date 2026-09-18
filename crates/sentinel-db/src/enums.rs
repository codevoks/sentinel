//! Rust mirrors of every native PostgreSQL enum type created in
//! `infra/migrations/0002_enums.sql`. Each derives `sqlx::Type` mapped to
//! its exact Postgres type name, so binding/fetching these columns is typed
//! at the Rust/Postgres boundary rather than passed as raw text.

use serde::{Deserialize, Serialize};

macro_rules! pg_enum {
    ($rust_name:ident, $pg_name:literal, { $($variant:ident => $pg_value:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
        #[sqlx(type_name = $pg_name, rename_all = "snake_case")]
        pub enum $rust_name {
            $(#[sqlx(rename = $pg_value)] $variant),+
        }
    };
}

pg_enum!(SlotStatus, "slot_status", {
    Skipped => "skipped", Observed => "observed", Confirmed => "confirmed",
    Finalized => "finalized", Abandoned => "abandoned",
});

pg_enum!(CommitmentLevel, "commitment_level", {
    Processed => "processed", Confirmed => "confirmed", Finalized => "finalized",
});

pg_enum!(TxVersion, "tx_version", {
    Legacy => "legacy", V0 => "v0", V1 => "v1",
});

pg_enum!(PriorityFeeSource, "priority_fee_source", {
    ConfigMask => "config_mask", ComputeBudgetIx => "compute_budget_ix", Absent => "absent",
});

pg_enum!(LogKind, "log_kind", {
    Invoke => "invoke", Success => "success", Failure => "failure",
    Data => "data", Log => "log", Consumed => "consumed", Unknown => "unknown",
});

pg_enum!(ObservationSource, "observation_source", {
    RpcHttp => "rpc_http", RpcWs => "rpc_ws", Geyser => "geyser", Fixture => "fixture",
});

pg_enum!(AccountObservationSource, "account_observation_source", {
    Rpc => "rpc", Geyser => "geyser",
});

pg_enum!(RawObservationKind, "raw_observation_kind", {
    Block => "block", Transaction => "transaction", Account => "account",
    LogBatch => "log_batch", SlotStatus => "slot_status",
    ProgramAccountsPage => "program_accounts_page", SignatureStatus => "signature_status",
    OracleUpdate => "oracle_update",
});

pg_enum!(PayloadEncoding, "payload_encoding", {
    JsonZstd => "json_zstd", Borsh => "borsh", Base64Raw => "base64_raw",
});

pg_enum!(MaterializationStatus, "materialization_status", {
    Current => "current", Recomputing => "recomputing", Stale => "stale", UnknownSchema => "unknown_schema",
});

pg_enum!(MaterializedVia, "materialized_via", {
    Event => "event", Snapshot => "snapshot",
});

pg_enum!(OracleValidationResult, "oracle_validation_result", {
    Valid => "valid", Failed => "failed",
});

pg_enum!(HealthState, "health_state", {
    Healthy => "healthy", Liquidatable => "liquidatable", NoDebt => "no_debt",
    UnknownOracle => "unknown_oracle", Stale => "stale",
});

pg_enum!(CandidateStatus, "candidate_status", {
    Open => "open", Claimed => "claimed", Executed => "executed",
    Expired => "expired", Invalidated => "invalidated",
});

pg_enum!(IntentKind, "intent_kind", {
    Liquidate => "liquidate", AbsorbBadDebt => "absorb_bad_debt",
    AccrueInterest => "accrue_interest", Custom => "custom",
});

pg_enum!(IntentState, "intent_state", {
    Created => "CREATED", Planning => "PLANNING", Planned => "PLANNED",
    AwaitingAttempt => "AWAITING_ATTEMPT", InFlight => "IN_FLIGHT",
    Succeeded => "SUCCEEDED", Failed => "FAILED", Expired => "EXPIRED",
    Cancelled => "CANCELLED", NeedsOperator => "NEEDS_OPERATOR",
});

pg_enum!(AttemptState, "attempt_state", {
    Signed => "SIGNED", Submitted => "SUBMITTED", Observed => "OBSERVED",
    Confirmed => "CONFIRMED", Finalized => "FINALIZED", FailedOnchain => "FAILED_ONCHAIN",
    Expired => "EXPIRED", AbandonedPreSubmit => "ABANDONED_PRE_SUBMIT", Unknown => "UNKNOWN",
});

pg_enum!(MismatchClass, "mismatch_class", {
    RaceHealed => "RACE_HEALED", RaceLost => "RACE_LOST", OracleClosed => "ORACLE_CLOSED",
    Paused => "PAUSED", SizeRejected => "SIZE_REJECTED", ModelDivergence => "MODEL_DIVERGENCE",
    AccountRejected => "ACCOUNT_REJECTED", ProjectionDivergence => "PROJECTION_DIVERGENCE",
    Unknown => "UNKNOWN",
});

pg_enum!(JobKind, "job_kind", {
    BackfillRange => "backfill_range", ReplayRange => "replay_range",
    RematerializeEntity => "rematerialize_entity", SnapshotAccounts => "snapshot_accounts",
    ReconcileProvider => "reconcile_provider", RecomputeAfterRollback => "recompute_after_rollback",
    ScanProgramAccounts => "scan_program_accounts",
});

pg_enum!(JobState, "job_state", {
    Queued => "queued", Leased => "leased", Done => "done", Failed => "failed", Quarantined => "quarantined",
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_variants_construct() {
        let _ = SlotStatus::Skipped;
        let _ = JobState::Queued;
        let _ = IntentState::Created;
    }
}
