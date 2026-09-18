//! Exact `numeric(39,0)`/`numeric(20,0)` <-> `u128`/`u64` conversion.
//!
//! **Load-bearing (phase-02-data-model.md §"Implementation requirements"):**
//! `numeric(39,0)` holds a `u128` exactly; `numeric(20,0)` holds a `u64`
//! exactly. No floating-point type is used anywhere, and no large integer
//! is ever routed through a Rust float.
//!
//! ## Why this is plain string round-tripping, not a decimal crate
//!
//! `sqlx` has no built-in Rust type for `NUMERIC` unless the `bigdecimal` or
//! `decimal` (`rust_decimal`) cargo feature is enabled. `rust_decimal`
//! cannot represent the full `u128` range exactly (its mantissa is 96 bits,
//! roughly 28-29 significant decimal digits; `u128::MAX` has 39). Adding
//! `bigdecimal` was evaluated and rejected for this phase: the crate is not
//! already present in `Cargo.lock`, and `crates.io` is unreachable from this
//! session (`curl https://crates.io` returns HTTP 403), so it cannot be
//! added without an unverifiable version pin — directly against AGENTS.md
//! §15 ("verify, do not remember").
//!
//! Instead, every `u128`/`u64` column is bound as a `String` (its exact
//! decimal digits) against an explicit `$N::numeric` cast in the SQL text,
//! and read back via an explicit `::text` cast to a `String`, which is then
//! parsed with `u128::from_str`/`u64::from_str` — exact, allocation-only,
//! and never touches a floating-point representation at any point. This
//! exact pattern was verified against a live PostgreSQL 18 connection in
//! this phase before being adopted (see the `numeric_exactness` integration
//! test in this crate): `u128::MAX` round-trips exactly, and a value
//! exceeding `numeric(39,0)`'s precision is rejected by PostgreSQL itself
//! ("numeric field overflow"), not silently truncated.

use std::fmt;
use std::str::FromStr;

#[derive(Debug, thiserror::Error)]
pub enum NumericError {
    #[error("value {0:?} is not a valid decimal integer")]
    InvalidDecimal(String),
}

/// Encodes a `u128` as the exact decimal text PostgreSQL expects when bound
/// against a `$N::numeric` cast.
pub fn encode_u128(value: u128) -> String {
    value.to_string()
}

/// Encodes a `u64` as exact decimal text for a `$N::numeric` cast.
pub fn encode_u64(value: u64) -> String {
    value.to_string()
}

/// Decodes the `::text` representation of a `numeric(39,0)` column back into
/// a `u128`, exactly. Fails on anything that is not a plain non-negative
/// decimal integer (a fractional part, for instance, would mean the schema
/// itself is wrong — `numeric(39,0)` has zero fractional digits by
/// definition — so this is a defensive check, not an expected path).
pub fn decode_u128(text: &str) -> Result<u128, NumericError> {
    u128::from_str(text).map_err(|_| NumericError::InvalidDecimal(text.to_string()))
}

/// Decodes the `::text` representation of a `numeric(20,0)` column back into
/// a `u64`, exactly.
pub fn decode_u64(text: &str) -> Result<u64, NumericError> {
    u64::from_str(text).map_err(|_| NumericError::InvalidDecimal(text.to_string()))
}

/// A `u128` value ready to bind against a `$N::numeric` placeholder.
/// Newtype so call sites cannot accidentally bind a raw `String` meant for
/// a genuine text column against a numeric cast, or vice versa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NumericU128(pub u128);

impl NumericU128 {
    pub fn to_bind(self) -> String {
        encode_u128(self.0)
    }

    pub fn from_text(text: &str) -> Result<Self, NumericError> {
        decode_u128(text).map(NumericU128)
    }
}

impl fmt::Display for NumericU128 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A `u64` value ready to bind against a `$N::numeric` placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NumericU64(pub u64);

impl NumericU64 {
    pub fn to_bind(self) -> String {
        encode_u64(self.0)
    }

    pub fn from_text(text: &str) -> Result<Self, NumericError> {
        decode_u64(text).map(NumericU64)
    }
}

impl fmt::Display for NumericU64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u128_max_round_trips_through_text_encoding() {
        let encoded = encode_u128(u128::MAX);
        assert_eq!(encoded, "340282366920938463463374607431768211455");
        assert_eq!(decode_u128(&encoded).unwrap(), u128::MAX);
    }

    #[test]
    fn u64_max_round_trips_through_text_encoding() {
        let encoded = encode_u64(u64::MAX);
        assert_eq!(decode_u64(&encoded).unwrap(), u64::MAX);
    }

    #[test]
    fn zero_round_trips() {
        assert_eq!(decode_u128(&encode_u128(0)).unwrap(), 0u128);
        assert_eq!(decode_u64(&encode_u64(0)).unwrap(), 0u64);
    }

    #[test]
    fn garbage_text_is_rejected_not_coerced() {
        assert!(decode_u128("not_a_number").is_err());
        assert!(decode_u128("1.5").is_err());
        assert!(decode_u128("-1").is_err());
    }
}
