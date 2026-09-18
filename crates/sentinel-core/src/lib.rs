//! Shared types used across every Sentinel crate: `Slot`, `Commitment`,
//! `ObservationId`, and the base error type.
//!
//! This crate deliberately knows nothing about Postgres, RPC, or Aegis. It is
//! the one dependency every other crate in the workspace may take
//! (`docs/architecture.md` §5: `sentinel-core -> (nothing internal)`).

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A Solana slot number.
///
/// Never derive a duration from a difference of two `Slot`s — slot times are
/// not constant and are still moving (`AGENTS.md` §15; `docs/ecosystem-research.md`
/// §11). `Slot` exists to identify chain progress, not to measure time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Slot(pub u64);

impl Slot {
    pub const GENESIS: Slot = Slot(0);

    pub fn checked_add(self, rhs: u64) -> Option<Slot> {
        self.0.checked_add(rhs).map(Slot)
    }

    pub fn checked_sub(self, rhs: Slot) -> Option<u64> {
        self.0.checked_sub(rhs.0)
    }
}

impl fmt::Display for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for Slot {
    fn from(value: u64) -> Self {
        Slot(value)
    }
}

/// The commitment level under which a piece of chain state was observed.
///
/// `Processed` exists because the RPC/WebSocket surface reports it — it is
/// **never persisted** (ADR-0009). Any code path that would write a row with
/// `Commitment::Processed` is a bug; the data-model layer (Phase 2) is
/// responsible for enforcing that at the schema boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Commitment {
    Processed,
    Confirmed,
    Finalized,
}

impl Commitment {
    /// Whether this commitment level is durable enough to persist as derived
    /// state (ADR-0009). `Processed` is not.
    pub fn is_persistable(self) -> bool {
        !matches!(self, Commitment::Processed)
    }
}

impl fmt::Display for Commitment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Commitment::Processed => "processed",
            Commitment::Confirmed => "confirmed",
            Commitment::Finalized => "finalized",
        };
        write!(f, "{s}")
    }
}

/// Opaque identity for a single raw observation row.
///
/// The natural key and uniqueness semantics of the raw observation boundary
/// are owned by `docs/data-model.md` (Phase 2, frozen). This type is only an
/// identity handle so that Phase 1 crates that need to reference "an
/// observation" can compile without reaching into Phase 2's schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObservationId(pub Uuid);

impl ObservationId {
    pub fn new() -> Self {
        ObservationId(Uuid::new_v4())
    }
}

impl Default for ObservationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ObservationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Base error type shared by crates that do not yet need a richer,
/// domain-specific error enum of their own.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("internal error: {0}")]
    Internal(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_arithmetic_is_checked() {
        assert_eq!(Slot(5).checked_add(3), Some(Slot(8)));
        assert_eq!(Slot(u64::MAX).checked_add(1), None);
        assert_eq!(Slot(5).checked_sub(Slot(3)), Some(2));
        assert_eq!(Slot(3).checked_sub(Slot(5)), None);
    }

    #[test]
    fn processed_commitment_is_not_persistable() {
        assert!(!Commitment::Processed.is_persistable());
        assert!(Commitment::Confirmed.is_persistable());
        assert!(Commitment::Finalized.is_persistable());
    }

    #[test]
    fn commitment_serializes_snake_case() {
        let json = serde_json::to_string(&Commitment::Confirmed).unwrap();
        assert_eq!(json, "\"confirmed\"");
    }

    #[test]
    fn observation_id_round_trips_through_json() {
        let id = ObservationId::new();
        let json = serde_json::to_string(&id).unwrap();
        let back: ObservationId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }
}
