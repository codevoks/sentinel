# Sentinel — Phase Specifications

One file per phase. **Implement exactly one phase per session, then STOP and report.**

Every specification contains the same twelve sections:

1. **Scope** — what ships.
2. **Explicit non-scope** — what must not be built, so "while I'm in here" cannot happen.
3. **Evidence objective** — what this phase makes *provable*, not just what it makes exist.
4. **Files** — where the code goes.
5. **Dependencies** — including any blocking research gate or upstream Aegis dependency.
6. **Implementation requirements** — the decisions already made; do not re-derive them.
7. **Tests** — by tier and by ID.
8. **Adversarial / failure cases** — the things that must be deliberately produced.
9. **Acceptance criteria** — the checklist that gates completion.
10. **Demo** — what a human can watch.
11. **Documentation & status updates** — what must be true in `docs/` when the phase ends.
12. **Stop condition.**

Read in order: `AGENTS.md` → `docs/project-status.md` → this phase's file → the relevant ADRs → the
relevant Aegis documents (if the phase touches protocol decoding, risk, or liquidation).

| Phase | File | Upstream-blocked? |
|---|---|---|
| 0 | [phase-00-planning.md](phase-00-planning.md) | — (COMPLETE) |
| 1 | [phase-01-foundation.md](phase-01-foundation.md) | no |
| 2 | [phase-02-data-model.md](phase-02-data-model.md) | no |
| 3 | [phase-03-rpc.md](phase-03-rpc.md) | no |
| 4 | [phase-04-ingestion.md](phase-04-ingestion.md) | no |
| 5 | [phase-05-normalize-replay.md](phase-05-normalize-replay.md) | no |
| 6 | [phase-06-chainstate.md](phase-06-chainstate.md) | no (SR-2 blocking) |
| 7 | [phase-07-aegis-adapter.md](phase-07-aegis-adapter.md) | **yes — Aegis ≥ Phase 6** |
| 8 | [phase-08-risk.md](phase-08-risk.md) | partially |
| 9 | [phase-09-api.md](phase-09-api.md) | no |
| 10 | [phase-10-execution.md](phase-10-execution.md) | no |
| 11 | [phase-11-keeper.md](phase-11-keeper.md) | **yes — Aegis ≥ Phase 9** |
| 12 | [phase-12-observability.md](phase-12-observability.md) | no |
| 13 | [phase-13-geyser.md](phase-13-geyser.md) | no (SR-3 blocking; ADR-0006 trigger) |
| 14 | [phase-14-performance.md](phase-14-performance.md) | no |
| 15 | [phase-15-release.md](phase-15-release.md) | yes for the live demo |
