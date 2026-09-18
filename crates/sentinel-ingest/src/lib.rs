//! Slot/tx/account/log ingestors, subscription manager, raw writer.
//! **Empty skeleton — implemented in Phase 4**
//! (`docs/phases/phase-01-foundation.md` §2, explicit non-scope: "No
//! ingestion").
//!
//! Processes untrusted RPC responses once implemented, hence the lint gate
//! from day one (`docs/architecture.md` §5).

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
