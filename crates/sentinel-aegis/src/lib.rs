//! The Aegis adapter: account layouts, event decoding, materialization.
//! **Empty skeleton — implemented in Phase 7.** Not begun in Phase 1 even
//! though Aegis's upstream artifacts now exist (`docs/project-status.md`
//! upstream reconciliation, 2026-09-18) — `AGENTS.md` §5 still requires
//! exactly one phase per session.
//!
//! Will consume `aegis-math` when Phase 7 begins; that dependency edge is
//! deliberately **not** declared yet (`docs/architecture.md` §5 lists it
//! parenthetically as a Phase 7/8 addition, not a Phase 1 one).

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
