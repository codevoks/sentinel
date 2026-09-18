//! `ObservationSource` + `RpcProvider` traits, pool, health, breaker,
//! failover. **Empty skeleton — implemented in Phase 3**
//! (`docs/phases/phase-01-foundation.md` §2: "No RPC client" is explicit
//! non-scope for Phase 1).
//!
//! This crate exists now so its dependency edges
//! (`docs/architecture.md` §5: `sentinel-rpc -> core, config`) and its
//! external-input clippy lints are enforceable from the first commit,
//! before there is any code to violate them.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
