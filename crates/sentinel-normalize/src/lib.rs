//! Raw -> normalized decoders (Solana primitives only, no protocol
//! concepts). **Empty skeleton — implemented in Phase 5.**
//!
//! `docs/architecture.md` §5: "`sentinel-normalize` must not reference any
//! Aegis or protocol concept" — this is the seam that makes a second
//! protocol adapter additive, enforced by CI grep guard `CI-NOAEGISLEAK`
//! from the first commit even though the tree is empty.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
