# Sentinel — Project Status

**Last updated: 2026-09-05**
**Current phase: Phase 0 — Planning & Architecture — COMPLETE**
**Next phase: Phase 1 — Foundation & Local Infrastructure — NOT STARTED**

> This file is the first thing any contributor or model reads after `AGENTS.md`. It must always reflect
> reality. **"Implemented" never means "verified."** The five states below are tracked separately and
> independently, on purpose.

---

## State definitions

| State | Means |
|---|---|
| **IMPLEMENTED** | The code exists and compiles. |
| **TESTED** | Tests exist, were **actually run**, and passed — and the failure-mode tests fail when their mechanism is removed. |
| **DEMOED** | Exercised end-to-end in the runnable demo. |
| **DOCUMENTED** | Reflected accurately in `docs/`. |
| **COMMITTED** | Merged and tagged. |

A row may be IMPLEMENTED without being TESTED. That is normal and must be recorded honestly, never
rounded up.

---

## Phase status

| Phase | Name | Status | Tag |
|---|---|---|---|
| 0 | Planning & architecture | ✅ **COMPLETE** | `phase-00-planning` |
| 1 | Foundation & local infrastructure | ⬜ NOT STARTED | — |
| 2 | Canonical data model & migrations | ⬜ NOT STARTED | — |
| 3 | RPC abstraction & resilient client | ⬜ NOT STARTED | — |
| 4 | Raw observation boundary & ingestion | ⬜ NOT STARTED | — |
| 5 | Normalization, backfill & replay | ⬜ NOT STARTED | — |
| 6 | Chain state: commitment & forks | ⬜ NOT STARTED | — |
| 7 | Aegis protocol adapter | ⬜ NOT STARTED — **upstream-blocked** | — |
| 8 | Derived risk state | ⬜ NOT STARTED — partially blocked | — |
| 9 | REST + realtime API | ⬜ NOT STARTED | — |
| 10 | Transaction execution engine | ⬜ NOT STARTED | — |
| 11 | Aegis liquidation keeper | ⬜ NOT STARTED — **upstream-blocked** | — |
| 12 | Observability & failure injection | ⬜ NOT STARTED | — |
| 13 | Optional Geyser adapter | ⬜ NOT STARTED — optional, trigger-gated | — |
| 14 | Load & performance campaign | ⬜ NOT STARTED | — |
| 15 | UI, demo, security review & release | ⬜ NOT STARTED | — |

## Component status

| Component | IMPL | TEST | DEMO | DOC | COMMIT |
|---|:--:|:--:|:--:|:--:|:--:|
| Rust workspace & toolchain | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| TypeScript workspace | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| Compose stack (Postgres, Surfpool, telemetry) | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| Database schema & migrations | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-rpc` — provider pool, breaker, failover | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-ingest` — raw boundary, checkpoints, gaps | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-normalize` — Solana primitives | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-chainstate` — commitment, forks, rollback | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-replay` — determinism harness | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-aegis` — decoder registry & materialization | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-risk` — health, sizing, candidates | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-jobs` — Postgres job queue | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-telemetry` — OTel | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-geyser` — optional source | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-api` — REST + WebSocket | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| `executor` — intent/attempt engine | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| `policy` — signing policy engine | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| `keeper` — Aegis liquidation loop | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| Web UI | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| Failure-injection campaign | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| Benchmark harness | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| Fixture corpus (normal / fork / corrupt) | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |

**Everything is DOCUMENTED and nothing is IMPLEMENTED. That is the correct and expected state at the
end of Phase 0**, and it is the single most important thing for the next session to understand.

## Test & evidence status

| Category | Defined | Implemented | Passing |
|---|---:|---:|---:|
| Threats (`S-01..S-25`) | 25 | 0 | 0 |
| Failure-injection entries (`FI-01..FI-28`) | 28 | 0 | 0 |
| Named races (`T-RACE-01..12`) | 12 | 0 | 0 |
| Replay criteria (`RP-01..RP-12`) | 12 | 0 | 0 |
| Keeper criteria (`KP-01..KP-14`) | 14 | 0 | 0 |
| Aegis conformance vectors (`AEGIS-CONF-01..06`) | 6 | 0 | 0 |
| Off-chain Aegis invariants (`AEGIS-INV-01..08`) | 8 | 0 | 0 |
| CI grep guards | 9 | 0 | 0 |
| Benchmarks | 0 measured | 0 | — |

**No performance number has been produced or claimed.** Phase 14 is the first phase permitted to state
one.

---

## Environment

**Not independently measured for Sentinel.** The values below are inherited from the Aegis measurement
of 2026-09-04 and **must be re-measured in Phase 1** (`ecosystem-research.md` §12).

| Tool | Version | Status |
|---|---|---|
| `solana` (Agave CLI) | 2.2.21 | ❌ **STALE — Phase 1 must upgrade** |
| `rustc` / `cargo` | 1.88.0 | ⚠️ Re-check against pinned crates |
| `node` | v22.12.0 | ⚠️ Re-verify |
| `surfpool` | not installed | ❌ Phase 1 — **and SR-7 is blocking** |
| `docker` / `docker compose` | not measured | ❌ Phase 1 |
| PostgreSQL | not measured | ❌ Phase 1 |
| Git repository | **not initialized** | ❌ Phase 1 |

---

## Open research gates

| ID | Question | Gate phase | Status |
|---|---|---|---|
| SR-1 | Transaction v1 mainnet activation; whether pinned Rust/Kit versions encode and decode v1 | 4, 10 | OPEN |
| SR-2 | Whether `confirmed` retains its meaning under Alpenglow/Votor; any new finality surface | **6 (blocking)** | OPEN |
| SR-3 | Yellowstone plugin/client/proto versions; stock-Agave compatibility; resume-from-slot | **13 (blocking)** | OPEN |
| SR-4 | Priority-fee distribution (sources conflict: 100%-to-validator vs 50/50 burn) | 14 | OPEN |
| SR-5 | Exact Rust client crate names/versions/MSRV supporting v1 decoding | 1 | OPEN |
| SR-6 | `@solana/kit` 8.x subscription-resume primitive; v0 build/serialize surface | 3, 9 | OPEN |
| SR-7 | **True Surfpool version and whether it exposes every RPC method Sentinel requires** | **1 (blocking)** | OPEN |
| SR-8 | Aegis program ID, deployed IDL, account discriminators | 7 | OPEN — **blocked upstream** |
| SR-9 | Whether Aegis uses `emit!` (program logs) or `emit_cpi!` | 7 | OPEN — **blocked upstream** |
| SR-10 | Pyth receiver program ID and `PriceUpdateV2` layout post-2026-08-26 (Aegis RV-3/RV-4) | 7 | OPEN — inherited from Aegis |
| SR-11 | Postgres version to pin; `LISTEN/NOTIFY` throughput at target load | 2, 14 | OPEN |

**SR-7 is the most important one.** If the local cluster does not expose a method Sentinel's
architecture requires, that is an architectural finding to surface — not a problem to work around.

## Upstream (Aegis) dependency status

**Aegis is at Phase 0: planning complete, no code written** (`aegis/docs/project-status.md`,
2026-09-04). Recorded rather than assumed away.

| Sentinel phase | Requires | Aegis phase that provides it | Status |
|---|---|---|---|
| 1–6 | **nothing** | — | ✅ unblocked |
| 7 | Program ID, IDL, discriminators, event layouts | Aegis 2–6 | ❌ blocked |
| 8 | `aegis-math` (preferred path) | Aegis 4–6 | ⚠️ interim path available via `AEGIS-CONF-01..06` |
| 11 | `@aegis/sdk` `ix.ts` builders | Aegis 9 | ❌ blocked — **no acceptable workaround** |
| 15 | A deployed Aegis for the live demo | Aegis 2+ | ❌ blocked for the live variant only |

## Known issues

None — no code exists yet.

## Current architectural decisions

| ADR | Decision | Status |
|---|---|---|
| 0001 | Rust owns ingestion/decode/risk; TypeScript owns API/execution/UI | Accepted |
| 0002 | PostgreSQL is the only canonical store | Accepted |
| 0003 | Redis is optional and never canonical | Accepted |
| 0004 | No message broker; a Postgres-backed job table | Accepted |
| 0005 | RPC + WebSocket baseline; HTTP is the completeness authority | Accepted |
| 0006 | Geyser/Yellowstone is an optional adapter behind the same interface | Accepted |
| 0007 | At-least-once observation, effect-once processing | Accepted |
| 0008 | An immutable raw observation boundary before any decoding | Accepted |
| 0009 | Explicit commitment model; `processed` is never persisted | Accepted |
| 0010 | Business intent is separate from transaction attempt | Accepted |
| 0011 | The backend signs only self-constructed, policy-checked transactions | Accepted |
| 0012 | One protocol, deeply, via a version-aware adapter consuming Aegis's artifacts | Accepted |
| 0013 | Zero-cost, local-first architecture | Accepted |
| 0014 | Docker Compose; no Kubernetes initially | Accepted |

---

## Phase 0 self-attack summary

The full record is in [`phase-0-self-attack.md`](phase-0-self-attack.md). It found **six material
defects**, all fixed before completion:

| # | Defect | Fix |
|---|---|---|
| A | The roadmap would have built a throwaway ingestion sink, then replaced it | Raw boundary merged into the first ingestion phase |
| B | A mis-tuned lookahead would have auto-paused the keeper for a non-bug | `LOOKAHEAD_OVERSHOOT` split from `MODEL_DIVERGENCE` by recomputing health at the observed state |
| C | The idempotency key would have suppressed a legitimate follow-up after a **partial** liquidation | A second key form keyed on the previous **finalized** signature |
| D | Summation invariants would have false-paged on a missed position | Gated on a verified-complete position set; set divergence alerts instead |
| E | A live measurement (`expected_landing_latency`) leaked into the deterministic replay path | `lookahead_ms` and `risk_params_hash` persisted on the row; replay reads them back |
| F | The Geyser equivalence criterion was unsatisfiable by a correct implementation | Compare final decoded account state, not raw observation row counts |

Ten residual risks are stated in §3 of that document, including the upstream block, the single-provider
divergence blind spot, the hot keeper key, and the honest answer to whether the platform is justified at
Aegis's current scale.

---

## Next action

**Hand Phase 1 to the implementation model. Phase 1 has NOT been started.**
