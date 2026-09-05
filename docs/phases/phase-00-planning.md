# Phase 0 — Planning & Architecture

**Status: COMPLETE (2026-09-05).**

## Scope

The frozen architecture corpus: product thesis, system architecture, the Aegis integration contract,
the ingestion model, commitment/fork semantics, the data model, replay guarantees, the transaction
execution engine, the keeper design, RPC strategy, the optional Geyser path, the signer boundary,
distributed-system correctness, the threat model, testing strategy, performance methodology,
observability, API design, external integrations, the frontend scope, the zero-cost local path, the
coverage matrix, 14 ADRs, 16 phase specifications, the engineering constitution, the Claude operating
guide, the implementation handoff, and this status model.

## Explicit non-scope

**No runtime production code.** No workspace, no crate, no package, no migration, no Dockerfile, no CI
configuration, no scaffolding. Phase 0 produces documents and nothing else, so that a later reader
cannot mistake a stub for progress.

## Evidence objective

That an implementation model can execute one bounded phase at a time **without inventing a core design
decision**, and that every claim the repository will eventually make is traceable to a specific future
artifact.

## Dependencies

The real Aegis Phase 0 repository, read at a pinned revision. Aegis is at Phase 0: planning complete,
no code written — which is recorded rather than assumed away.

## Acceptance criteria

- [x] Product thesis coherent, with the rejected alternatives argued
- [x] Aegis integration grounded in the **actual** Aegis Phase 0 artifacts, not an imagined interface
- [x] Ingestion, commitment/fork, data, and replay semantics explicit
- [x] Transaction execution semantics explicit, including crash-boundary ordering
- [x] Distributed-system failure modes named individually, not summarized as "use retries"
- [x] Threat model with trust boundaries, 25 threats, and stated residual risks
- [x] Testing strategy across six tiers with a traceability rule
- [x] Performance methodology with **no invented results**
- [x] Observability plan where every alert has an operator action
- [x] Coverage matrix mapping every topic to a future artifact
- [x] 14 ADRs, each stating what was rejected and why
- [x] `AGENTS.md`, `CLAUDE.md`, `docs/project-status.md`, `docs/implementation-handoff.md`
- [x] 16 phase specifications
- [x] Unresolved ecosystem facts recorded as research gates SR-1..SR-11
- [x] Self-attack performed and material findings fixed (`phase-0-self-attack.md`)
- [x] **No Phase 1 work started**

## Documentation & status updates

The whole `docs/` tree is the deliverable. `docs/project-status.md` records that everything is
DOCUMENTED and nothing is IMPLEMENTED — **which is the correct and expected state at the end of Phase
0**, and the single most important thing for the next session to understand.

## Stop condition

Phase 0 ends with documents complete and runtime implementation not started. **Phase 1 has NOT been
started.**
