# Sentinel — Project Status

**Last updated: 2026-09-18**
**Current phase: Phase 3 — RPC Abstraction & Resilient Client — COMPLETE**
**Next phase: Phase 4 — Raw Observation Boundary & Ingestion — NOT STARTED**

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
| 1 | Foundation & local infrastructure | ✅ **COMPLETE** | `phase-01-foundation` |
| 2 | Canonical data model & migrations | ✅ **COMPLETE** | `phase-02-data-model` — confirmed on `origin` (`git ls-remote --tags origin`), resolving to `3afd9d8` (Phase 2's completion commit; the later `cded9a2` closure-fix commit is a normal in-history follow-up on `main`, ancestor-of/descendant relationship verified, not a tag-move) |
| 3 | RPC abstraction & resilient client | ✅ **COMPLETE** | `phase-03-rpc` |
| 4 | Raw observation boundary & ingestion | ⬜ NOT STARTED | — |
| 5 | Normalization, backfill & replay | ⬜ NOT STARTED | — |
| 6 | Chain state: commitment & forks | ⬜ NOT STARTED | — |
| 7 | Aegis protocol adapter | ⬜ NOT STARTED — no longer upstream-blocked (see reconciliation below) | — |
| 8 | Derived risk state | ⬜ NOT STARTED — no longer upstream-blocked | — |
| 9 | REST + realtime API | ⬜ NOT STARTED | — |
| 10 | Transaction execution engine | ⬜ NOT STARTED | — |
| 11 | Aegis liquidation keeper | ⬜ NOT STARTED — no longer upstream-blocked | — |
| 12 | Observability & failure injection | ⬜ NOT STARTED | — |
| 13 | Optional Geyser adapter | ⬜ NOT STARTED — optional, trigger-gated | — |
| 14 | Load & performance campaign | ⬜ NOT STARTED | — |
| 15 | UI, demo, security review & release | ⬜ NOT STARTED | — |

## Component status

| Component | IMPL | TEST | DEMO | DOC | COMMIT |
|---|:--:|:--:|:--:|:--:|:--:|
| Rust workspace & toolchain | ✅ | ✅ | ✅ | ✅ | ⬜ |
| TypeScript workspace | ✅ | ✅ | ✅ | ✅ | ⬜ |
| Compose stack (Postgres, Surfpool, telemetry) | ✅ | ✅ | ✅ | ✅ | ⬜ |
| Database migrations runner + least-privilege roles | ✅ | ✅ | ✅ | ✅ | ⬜ |
| `sentinel-core` — Slot, Commitment, ObservationId, errors | ✅ | ✅ | ✅ | ✅ | ⬜ |
| `sentinel-config` — typed config, secret redaction | ✅ | ✅ | ✅ | ✅ | ⬜ |
| `sentinel-db` — connect-with-retry, migration runner | ✅ | ✅ | ✅ | ✅ | ⬜ |
| `sentinel-telemetry` — metric registry, structured logging | ✅ | ✅ | ✅ | ✅ | ⬜ |
| **Canonical schema — `infra/migrations/0002`-`0011`** (all tables, keys, indexes, roles, partitions) | ✅ | ✅ | ✅ | ✅ | ⬜ |
| `sentinel-db::numeric` — exact u128/u64 ↔ numeric(39,0)/numeric(20,0) | ✅ | ✅ | ✅ | ✅ | ⬜ |
| `sentinel-db::partitions` — partition automation + low-partition alert | ✅ | ✅ | ✅ | ✅ | ⬜ |
| `sentinel-db::{enums,tables,queries}` — typed access layer | ✅ **complete — every canonical table** (closure fix, 2026-09-18) | ✅ | ✅ | ✅ | ⬜ |
| `sentinel-jobs` — Postgres job queue (claim/lease/renew/release/quarantine, 16-worker concurrency) | ✅ | ✅ | ✅ | ✅ | ⬜ |
| `sentinel-rpc` — `RpcProvider`/`RpcPool`, capabilities, budget, breaker, freshness, WS manager, broadcast, fixtures, fault injection, HTTP provider | ✅ | ✅ (46 tests: 28 unit + 5 FI-acceptance + 1 property + 1 real-Postgres persistence + 8 real-Surfpool probe + 3 real-Surfpool pool/WS integration) | ✅ (`crates/bins/sentinel-rpc-demo`, run live against real Surfpool, metrics visible via Grafana's own Prometheus datasource) | ✅ | ⬜ |
| `bins/sentinel-rpc-demo` — Phase 3 live demo CLI | ✅ | — (a demo, not a test target) | ✅ | ✅ | ⬜ |
| `sentinel-ingest` — raw boundary, checkpoints, gaps | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-normalize` — Solana primitives | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-chainstate` — commitment, forks, rollback | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-decode` — protocol adapter trait + registry | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-replay` — determinism harness | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-aegis` — decoder registry & materialization | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-risk` — health, sizing, candidates | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-geyser` — optional source | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `bins/sentinel-indexer`, `bins/sentinel-backfill` | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `@sentinel/db` — typed Postgres read layer for `sentinel_ts` (alerts, execution_intents, transaction_attempts) | ✅ partial (see note below) | ✅ | ✅ | ✅ | ⬜ |
| `@sentinel/aegis`, `@sentinel/executor`, `@sentinel/policy` | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `@sentinel/api`, `@sentinel/keeper`, `@sentinel/web` | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| Failure-injection campaign | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| Benchmark harness | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |
| Fixture corpus (normal / fork / corrupt) | ⬜ | ⬜ | ⬜ | ✅ | ⬜ |

**Phase 1's four in-scope crates (`sentinel-core`, `sentinel-config`, `sentinel-db`,
`sentinel-telemetry`) are IMPLEMENTED, TESTED, and DEMOED** (exercised against the real, running local
Compose stack — not mocked). **Every other Rust crate and every TypeScript package/app is an empty
skeleton**: it compiles, declares the correct dependency edges from `docs/architecture.md` §5, and
contains no logic — exactly what Phase 1 §2 requires ("an empty crate with declared dependencies is
correct; a crate with a fake `todo!()` pipeline is not").

## Test & evidence status

| Category | Defined | Implemented | Passing |
|---|---:|---:|---:|
| Threats (`S-01..S-25`) | 25 | 0 | 0 |
| Failure-injection entries (`FI-01..FI-28`) | 28 | **6** (FI-06, FI-09, FI-10, FI-11, FI-21, FI-22 — Phase 3's required set) | **6** |
| Named races (`T-RACE-01..12`) | 12 | 0 | 0 |
| Replay criteria (`RP-01..RP-12`) | 12 | 0 | 0 |
| Keeper criteria (`KP-01..KP-14`) | 14 | 0 | 0 |
| Aegis conformance vectors (`AEGIS-CONF-01..06`) | 6 | 0 | 0 |
| Off-chain Aegis invariants (`AEGIS-INV-01..08`) | 8 | 0 | 0 |
| CI grep guards | 9 | **9** | **9** |
| Phase 1 Rust unit/integration tests | — | **23** | **23** |
| SR-7 capability probe tests (`crates/sentinel-rpc/tests/surfpool_capability_probe.rs`) | 16 methods | **8 test functions covering all 16** | **8/8** |
| Phase 1 TypeScript tests | — | **1** (harness proof; rest are empty-skeleton packages) | **1** |
| RPC invariants (`RPC-01..RPC-10`) | 10 | **10** | **10** (see Phase 3 evidence §"RPC-01..RPC-10 acceptance mapping") |
| `P-BOUND-1` (bounded retry property) | 1 | **1** (hand-rolled deterministic generator — see Phase 3 evidence for why `proptest` was not added) | **1** |
| Phase 3 `sentinel-rpc` tests | — | **46** (28 unit + 5 FI-acceptance + 1 property + 1 real-Postgres persistence + 8 real-Surfpool capability probe + 3 real-Surfpool pool/WS integration) | **46** |
| Benchmarks | 0 measured | 0 | — |

**No performance number has been produced or claimed.** Phase 14 is the first phase permitted to state
one.

---

## Environment

**Independently measured 2026-09-18** (Phase 1, superseding the Aegis-inherited Phase 0 snapshot).
Every value below is real, pasted command output — see `docs/ecosystem-research.md` §12a for the
complete transcript and the deltas from the Phase 0 assumptions.

| Tool | Version | Command | Status |
|---|---|---|---|
| `solana` (Agave CLI) | `solana-cli 3.1.10 (src:7bc9c805; feat:1620780344, client:Agave)` | `solana --version` | ✅ current |
| `surfpool` | `surfpool 1.5.0` | `surfpool --version` | ✅ SR-7 closed against this version |
| `rustc` / `cargo` | `rustc 1.98.1 (48a229cea 2026-09-01)` / `cargo 1.98.1 (797e8a9bc 2026-08-05)` | `rustc --version && cargo --version` | ✅ current |
| `node` | `v22.12.0` | `node --version` | ✅ current for the pinned toolchain (see delta note below) |
| `docker` / `docker compose` | `Docker version 29.4.0, build 9d7ad9f` / `Docker Compose version v5.1.2` | `docker --version && docker compose version` | ✅ current |
| PostgreSQL | `postgres (PostgreSQL) 18.6` (containerized; `postgres:18-alpine`) | `docker run --rm postgres:18-alpine postgres --version` | ✅ pinned, SR-11 (version portion) closed |
| `@solana/kit` | `8.3.0` | `npm view @solana/kit version` | ✅ recorded (newer than Aegis's 8.2.0) |
| `@anchor-lang/core` | `1.2.0` | `npm view @anchor-lang/core version` | ✅ recorded |
| `solana-rpc-client` / `solana-pubsub-client` / `solana-transaction-status` | `4.2.2` (all three, from `Cargo.lock`) | `grep -A1 'name = "solana-rpc-client"' Cargo.lock` (×3) | ✅ SR-5 closed |
| Git repository | initialized, branch `phase/01-foundation` | — | ✅ Phase 1 |

**Delta found and recorded (not in `ecosystem-research.md` §12's original command list, but a real
Phase 1 finding):** the pinned Node `v22.12.0` cannot run the newest `eslint@10.x` /
`@typescript-eslint` line (`eslint-visitor-keys@5.0.1` requires Node `^22.13.0`; confirmed by a real
`Cannot find module '.../eslint-visitor-keys.cjs'` runtime failure, not merely an `EBADENGINE`
warning). Resolved by pinning the TypeScript toolchain to versions verified compatible with the
installed Node: `eslint@9.39.5`, `typescript-eslint@8.55.0`, `typescript@5.9.3` (the newest 5.x
release — `typescript@6.0.3`/`7.0.2` exist upstream but `typescript-eslint@8.55.0`'s peer range is
`>=4.8.4 <6.0.0`, and the `typescript-eslint` versions that support TS 6.x pull in the Node-22.13+-only
`eslint-visitor-keys@5`). This is a genuine ecosystem-compatibility snapshot, not a preference — a
future phase should re-run `docs/ecosystem-research.md` §12 and re-widen these pins once either Node is
upgraded or `typescript-eslint` ships a Node-22.12-compatible release on the `eslint-visitor-keys@5`
line.

---

## Open research gates

| ID | Question | Gate phase | Status |
|---|---|---|---|
| SR-1 | Transaction v1 mainnet activation; whether pinned Rust/Kit versions encode and decode v1 | 4, 10 | OPEN |
| SR-2 | Whether `confirmed` retains its meaning under Alpenglow/Votor; any new finality surface | **6 (blocking)** | OPEN |
| SR-3 | Yellowstone plugin/client/proto versions; stock-Agave compatibility; resume-from-slot | **13 (blocking)** | OPEN |
| SR-4 | Priority-fee distribution (sources conflict: 100%-to-validator vs 50/50 burn) | 14 | OPEN |
| SR-5 | Exact Rust client crate names/versions/MSRV supporting v1 decoding | 1 | ✅ **CLOSED** — `solana-rpc-client`/`solana-pubsub-client`/`solana-transaction-status` pinned at stable `4.2.2` (latest is `4.4.0-alpha.4`, a prerelease); resolved versions confirmed in `Cargo.lock`. |
| SR-6 | `@solana/kit` 8.x subscription-resume primitive; v0 build/serialize surface | 3, 9 | OPEN |
| SR-7 | **True Surfpool version and whether it exposes every RPC method Sentinel requires** | **1 (blocking)** | ✅ **CLOSED** — `surfpool 1.5.0` / `solana-core 4.1.2`. All 16 required HTTP+WS methods verified present and correctly behaved against a real running local instance (offline mode). Zero architectural findings. Full method-by-method evidence: `docs/ecosystem-research.md` §12a. Committed as an automated test: `crates/sentinel-rpc/tests/surfpool_capability_probe.rs`. |
| SR-8 | Aegis program ID, deployed IDL, account discriminators | 7 | OPEN — **no longer blocked upstream** (see Aegis upstream reconciliation below); resolution deferred to Phase 7 |
| SR-9 | Whether Aegis uses `emit!` (program logs) or `emit_cpi!` | 7 | OPEN — no longer blocked upstream; deferred to Phase 7 |
| SR-10 | Pyth receiver program ID and `PriceUpdateV2` layout post-2026-08-26 (Aegis RV-3/RV-4) | 7 | OPEN — no longer blocked upstream; Aegis's own Phase 5 records this resolved on its side; Sentinel verifies independently at Phase 7 |
| SR-11 | Postgres version to pin; `LISTEN/NOTIFY` throughput at target load | 2, 14 | ⚠️ **PARTIALLY CLOSED** — version pinned (Phase 1). **Phase 2 smoke measurement done**: 50/50 `NOTIFY` deliveries received in two separate real runs against local `postgres:18-alpine`, mean latency 3.6ms and 4.8ms, p95 4.2ms and 8.8ms (see "Phase 2 — SR-11 smoke measurement" below). This is a smoke figure from one process, one connection pair, no concurrent load — **full throughput-at-target-load characterization remains OPEN, deferred to Phase 14** exactly as both the Phase 1 and Phase 2 specs require. |

**SR-7 was the most important gate and is now closed with no architectural finding**: Surfpool's local,
offline JSON-RPC and WebSocket surface has full parity with everything Sentinel's architecture assumes.

## Upstream (Aegis) dependency status

> ### Reconciliation performed 2026-09-18 (Sentinel Phase 1 pre-flight)
>
> The line below — **"Aegis is at Phase 0: planning complete, no code written"** — was accurate at the
> original research date (2026-09-04) and is now **stale**. Verified directly against the Aegis
> repository before writing anything here (not assumed from any prompt or prior note):
>
> - `git clone https://github.com/codevoks/aegis-protocol` — tags present: `phase-01-foundation`,
>   `phase-02-state`, `phase-03-collateral`, `phase-04-lending`, `phase-05-oracle`,
>   `phase-06-liquidation`, `phase-07-token2022`, `phase-08-composability`, `phase-09-sdk-ui`,
>   `phase-10-security`, `phase-11-performance`, `phase-12-governance`, `phase-13-release`, **`v0.1.0`**
>   (tagged 2026-09-14).
> - Aegis's own `docs/project-status.md` states: *"Current phase: Phase 13 — Integration, Security
>   Review and Release — COMPLETE. This is the final planned phase. No Phase 14 exists or is planned."*
> - `programs/aegis/src/lib.rs` declares a real program ID:
>   `DbRhjkZV1QSxMj5AvrYdgVsyEz8nKhoCLnSLGSKsqaF9`.
> - `crates/aegis-math/Cargo.toml` — `version = "0.1.0"`, a real, non-empty crate.
> - `sdk/ts/package.json` — `"name": "@aegis/sdk", "version": "0.1.0"`, a real, non-empty package.
>
> **This is a status correction only.** Per this session's explicit instructions and `AGENTS.md` §5,
> Sentinel does **not** redesign around Aegis, does **not** begin the Aegis adapter, and does **not**
> touch Phase 7/8/11 early merely because the upstream block has lifted. The documents corrected for
> truthfulness are: this file, `docs/phase-roadmap.md` §3, `docs/aegis-integration.md` §2,
> `docs/ecosystem-research.md` (SR-8/SR-9/SR-10 rows), and `docs/adr/0012-aegis-adapter-versioning.md`'s
> "Negative consequences" entry — in every case only the factual status claim was corrected, not the
> architectural decision itself.

| Sentinel phase | Requires | Aegis phase that provides it | Status (2026-09-18) |
|---|---|---|---|
| 1–6 | **nothing** | — | ✅ unblocked (always was) |
| 7 | Program ID, IDL, discriminators, event layouts | Aegis 2–6 | ✅ **artifacts exist** (`v0.1.0`) — Phase 7 not started |
| 8 | `aegis-math` (preferred path) | Aegis 4–6 | ✅ **`aegis-math` 0.1.0 exists** — Phase 8 not started |
| 11 | `@aegis/sdk` `ix.ts` builders | Aegis 9 | ✅ **`@aegis/sdk` 0.1.0 exists** — Phase 11 not started |
| 15 | A deployed Aegis for the live demo | Aegis 2+ | ✅ feasible via a local Surfpool deployment of the released program — not attempted in Phase 1 |

## Known issues

- The TypeScript toolchain pin (`eslint@9.39.5`/`typescript-eslint@8.55.0`/`typescript@5.9.3`) is one
  minor version behind the newest available line, held back by a Node `22.12.0` vs `22.13.0`
  incompatibility in a transitive dependency. See the Environment section above. Not a defect — a
  documented, verified constraint.
- `docker-collector` health: the `otel/opentelemetry-collector-contrib` image has no shell or HTTP
  client, so it has no container-level Docker healthcheck (its `health_check` extension still serves
  `:13133` for external checks). Documented in `infra/compose/docker-compose.observability.yml`.
- `cargo audit` was attempted locally (`cargo install cargo-audit --locked`) but did not finish
  compiling within the time available in this session (it pulls in `gix`/`reqwest`/`rustls`, a heavy
  dependency tree); the install was not force-completed with a longer budget. `npm audit` **was** run
  to completion and found **0 vulnerabilities**. The CI workflow's `dependency-advisory-scan` job uses
  `rustsec/audit-check`, which installs its own `cargo-audit` inside the runner and does not depend on
  this session's local install succeeding — but that job itself has not been executed by GitHub
  Actions from this session (see "CI / guards status" below).
- **New in Phase 3**: `crates.io`'s API (`api.crates.io`) returned HTTP 403 when probed directly in this
  session, consistent with the Phase 1/2 disclosed finding for the same host. `cargo fetch --locked`
  and normal builds worked throughout (the crate download path is evidently unaffected), but adding a
  genuinely new dependency not already present in `Cargo.lock` was not attempted/verified against a live
  index in this environment. Consequence: `P-BOUND-1` was implemented as a hand-rolled deterministic
  generator instead of with `proptest`/`quickcheck` (neither is in `Cargo.lock`) — see the Phase 3
  evidence section for the reasoning; it is a real property test, just without a shrinking library.
- **New in Phase 3**: `docs/failure-injection.md`, named in this phase's read-order instructions, does
  not exist in the repository (confirmed: absent from `docs/`, and not referenced by `CLAUDE.md`'s own
  "start of every session" reading list). All of Phase 3's actual failure-injection requirements are
  fully specified inline in `docs/phases/phase-03-rpc.md` §8/§18/§23 and `docs/rpc-strategy.md`, which
  this phase implemented and tested against directly. This is disclosed as a documentation-inventory
  finding, not treated as a blocking contradiction — no requirement was actually unspecified as a
  result.
- **New in Phase 3**: while running the full workspace regression, `ts/packages/db`'s `npm test` and
  `npm run build` both fail — `test`: `node --import tsx --test src/**/*.test.ts` reports no matching
  files; `build`: `tsc` reports `Cannot find name 'Buffer'` (missing `@types/node`). Confirmed
  pre-existing and untouched by this phase (`git status --short ts/` shows zero diff against the Phase
  2 closure commit `cded9a2f`; `docs/phases/phase-03-rpc.md` §5 states this phase is Rust-only). Left
  as-is per CLAUDE.md §7 ("no large speculative refactors... stay inside the current phase's scope") —
  flagged here rather than silently absorbed or silently fixed.
- Two organizational choices beyond the phase spec's literal file list (`crates/sentinel-rpc/src/{provider,
  pool,health,breaker,budget,capabilities,ws,fixtures,fault}.rs`), both disclosed rather than silent:
  `http.rs` (the real `HttpRpcProvider`, split out for single-responsibility rather than appended to the
  already-large `provider.rs`) and a new bin crate `crates/bins/sentinel-rpc-demo` (the phase's own §26
  requires a runnable demo CLI; neither existing bin — `sentinel-indexer`/`sentinel-backfill` — is
  Phase-3-scoped). Neither changes a frozen document, the data model, commitment/execution semantics, or
  security posture, so neither rises to an ADR under AGENTS.md §13's own criteria.

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
| 0012 | One protocol, deeply, via a version-aware adapter consuming Aegis's artifacts | Accepted (status note updated 2026-09-18; decision unchanged) |
| 0013 | Zero-cost, local-first architecture | Accepted |
| 0014 | Docker Compose; no Kubernetes initially | Accepted |

No new ADR was required for Phase 1: every implementation choice (Postgres role model, secret wrapper
design, telemetry stack, CI grep-guard mechanics) was a routine implementation decision within the
frozen architecture, not a deviation from it.

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

Ten residual risks are stated in §3 of that document, including the upstream block (**now resolved, see
the Aegis upstream reconciliation above**), the single-provider divergence blind spot, the hot keeper
key, and the honest answer to whether the platform is justified at Aegis's current scale.

---

## Phase 1 evidence

### Commands actually run, with real output

```
$ solana --version
solana-cli 3.1.10 (src:7bc9c805; feat:1620780344, client:Agave)

$ surfpool --version
surfpool 1.5.0

$ rustc --version && cargo --version
rustc 1.98.1 (48a229cea 2026-09-01)
cargo 1.98.1 (797e8a9bc 2026-08-05)

$ node --version && docker --version && docker compose version
v22.12.0
Docker version 29.4.0, build 9d7ad9f
Docker Compose version v5.1.2

$ docker run --rm postgres:18-alpine postgres --version
postgres (PostgreSQL) 18.6

$ npm view @solana/kit version
8.3.0

$ npm view @anchor-lang/core version
1.2.0

$ cargo build --workspace
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 30.09s

$ cargo test --workspace
running 6 tests (sentinel-config) ... 6 passed; 0 failed
running 4 tests (sentinel-core) ... 4 passed; 0 failed
running 3 tests (sentinel-db, against live Postgres) ... 3 passed; 0 failed
running 2 tests (sentinel-telemetry) ... 2 passed; 0 failed
running 8 tests (sentinel-rpc, SR-7 capability probe, against live Surfpool) ... 8 passed; 0 failed
(all other crates: 0 unit tests — empty skeletons)

$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 03s

$ cargo fmt --all -- --check
(no output — clean)

$ bash scripts/ci-guards.sh
PASS: CI-NOFLOAT
PASS: CI-NOSLOTTIME
PASS: CI-NOPANIC
PASS: CI-NOSQLFMT
PASS: CI-NORAWCLIENT
PASS: CI-NOAEGISLEAK
PASS: CI-NOMATHDUP
PASS: CI-NOMAXVER
PASS: CI-NOSECRET
All 9 CI grep guards passed.

$ make up
...
waiting for postgres and surfpool health checks...
postgres: healthy, surfpool: healthy

$ make migrate
connecting to postgres at 127.0.0.1:5432 (bounded retry: 10 attempts)
running migrations from infra/migrations
migrations applied successfully

$ make migrate   # re-run, idempotency check
migrations applied successfully   # no error, no re-application

$ cd ts && npm install && npm run build && npx eslint . && npm test
(all succeed; 148 packages installed, 0 vulnerabilities; 1 real test passes,
 6 empty-skeleton packages print their "no tests yet" line honestly)

$ cd ts && npm audit
found 0 vulnerabilities
```

### SR-7 capability probe — every required method, exercised for real

Surfpool was started **offline** (`--offline --no-tui --no-studio --ci`, no external RPC datasource)
and every method below was actually called against it — not inspected in documentation — using a real
locally-generated, locally-airdropped keypair (Surfpool's own default 10,000 SOL startup airdrop; no
faucet).

| Method | Transport | Result |
|---|---|---|
| `getVersion` | HTTP | ✅ `{"surfnet-version":"1.5.0","solana-core":"4.1.2",...}` |
| `getSlot` | HTTP | ✅ |
| `getBlockHeight` | HTTP | ✅ |
| `getBlocks` | HTTP | ✅ |
| `getBlock` (`maxSupportedTransactionVersion`) | HTTP | ✅ |
| `getTransaction` (`maxSupportedTransactionVersion`) | HTTP | ✅ — verified against a real `sendTransaction` signature |
| `getSignaturesForAddress` | HTTP | ✅ |
| `getMultipleAccounts` | HTTP | ✅ |
| `getProgramAccounts` | HTTP | ✅ |
| `getLatestBlockhash` | HTTP | ✅ (synthetic `SURFNETxSAFEHASHx...` blockhash — valid, accepted, cosmetic only) |
| `getSignatureStatuses` (`searchTransactionHistory`) | HTTP | ✅ |
| `getRecentPrioritizationFees` | HTTP | ✅ (`[]` on an idle validator — correct) |
| `simulateTransaction` | HTTP | ✅ — real signed transaction via `solders` |
| `sendTransaction` | HTTP | ✅ — twice, via `solana transfer` CLI and via a raw signed tx |
| `slotSubscribe` | WS | ✅ — live notification received |
| `accountSubscribe` | WS | ✅ — notification fired on a triggered transfer |
| `logsSubscribe` (`mentions`) | WS | ✅ — notification fired with real logs |

**16/16 present and correctly behaved. Zero architectural findings.** Full transcript:
`docs/ecosystem-research.md` §12a. This was then re-verified with the real pinned Rust client
(`solana-rpc-client`/`solana-pubsub-client` 4.2.2, not `curl`/Python) in the committed test suite,
`crates/sentinel-rpc/tests/surfpool_capability_probe.rs` (8 tests, all passing), which additionally
found and fixed a **sharper SR-7/CI-NOMAXVER finding**: the client's plain `get_block()` does not set
`maxSupportedTransactionVersion` at all — only `get_block_with_config()` does. `CI-NOMAXVER` was
strengthened accordingly to ban the plain method names outright (`docs/ecosystem-research.md` §12a).

### Failure injection — actually run, not merely written

| Case | How it was triggered | Result |
|---|---|---|
| **Missing required configuration** | `sentinel_config::AppConfig::from_map` with `SENTINEL_DB_PASSWORD` removed | `Err(ConfigError::MissingEnv("SENTINEL_DB_PASSWORD"))` — clear, typed, no default used |
| **Non-loopback local endpoint** | `SENTINEL_PROFILE=local`, `SENTINEL_DB_HOST=db.example.com` | `Err(ConfigError::NonLoopbackEndpoint { field: "SENTINEL_DB_HOST", value: "db.example.com" })` |
| **Secret leakage** | Serialized a full `AppConfig` (JSON + `Debug`) containing the marker `SENTINEL_TEST_SENTINEL_VALUE_do-not-leak-me` | Marker absent from both outputs; `[REDACTED]` present instead; other fields (host, user) still present, proving serialization actually ran |
| **PostgreSQL unavailable** | `connect_with_retry` against `127.0.0.1:1` (nothing listens), `max_attempts: 2`, `attempt_timeout: 2s` | `Err(DbError::ConnectFailed { attempts: 2, .. })` in ~4s — bounded, not hung. Separately verified via the real `migrate` example against a wrong port: failed in ~39s across 10 bounded attempts, exit code 1 |
| **Surfpool capability regression** | Manually reintroduced `x.unwrap()` / an Aegis reference / a raw client / `WAD`/float consts / a fake AWS key into throwaway code, one at a time | Each violation was independently caught: `cargo clippy -D warnings` failed on the `unwrap()`, and `scripts/ci-guards.sh` failed on each of `CI-NOAEGISLEAK`, `CI-NORAWCLIENT`, `CI-NOMATHDUP`, `CI-NOFLOAT`, `CI-NOSECRET` in turn; all files were reverted afterward and the guard suite re-confirmed clean |
| `overflow-checks = true` actually panics | A non-const `u8 + u8` overflow in a throwaway crate built with the workspace's exact `[profile.release]` | `thread 'main' panicked at ...: attempt to add with overflow` in the **release** profile |

### Database roles — verified least privilege, live

```
$ docker exec compose-postgres-1 psql -U sentinel_bootstrap -d sentinel -c "\du"
     Role name      |                         Attributes
--------------------+------------------------------------------------------------
 sentinel_bootstrap | Superuser, Create role, Create DB, Replication, Bypass RLS
 sentinel_rust      |
 sentinel_ts        |

$ docker exec compose-postgres-1 psql -U sentinel_bootstrap -d sentinel -c "\dn+ public"
  Name  |       Owner       |           Access privileges
--------+-------------------+----------------------------------------
 public | pg_database_owner | ... sentinel_rust=U/pg_database_owner
        |                   | ... sentinel_ts=U/pg_database_owner
```

`sentinel_rust`/`sentinel_ts` have `USAGE` only — no `CREATE`, matching "least privilege, locally too."

---

## CI / guards status

| Job | Exists | Locally-equivalent evidence |
|---|:--:|---|
| `build` | ✅ | `cargo build --workspace` and `npm run build` both pass (above) |
| `fmt` | ✅ | `cargo fmt --all -- --check` and `npx prettier --check .` both pass |
| `lint` | ✅ | `cargo clippy --workspace --all-targets -D warnings`, `scripts/ci-guards.sh`, `npx eslint .` all pass |
| `test` | ✅ | `cargo test --workspace` (23 tests) + `npm test` (1 real + 6 honest no-op) all pass against live Postgres/Surfpool |
| `no-network` | ✅ (defined) | **Dynamically verified locally**, not just reasoned about — see below |
| `secret-scan` | ✅ | `scripts/ci-guards.sh`'s `CI-NOSECRET` check, run standalone |
| `dependency-advisory-scan` | ✅ (defined) | `npm audit` run locally: **0 vulnerabilities**. `cargo audit` was not completed inside this session (see "NOT DONE") |

### `no-network` job — real dynamic proof, not just a static argument

This session has no sudo/root on the host (confirmed: `sudo -n true` requires a password), so the
firewall could not be applied to the host directly. Instead, the exact mechanism the CI job uses —
`iptables` egress blocking — was applied for real inside a disposable Linux container
(`rust:1-bookworm`, matching the pinned toolchain exactly) attached to the same Docker network as the
live Compose stack, with `NET_ADMIN`/`NET_RAW` capabilities (which Docker itself can grant without host
sudo):

```
$ docker run -d --network sentinel --cap-add=NET_ADMIN --cap-add=NET_RAW ... rust:1-bookworm sleep 3600
$ docker exec ... cargo build --workspace --tests --quiet   # network still up (dependency fetch)
$ docker exec ... bash -c '
    iptables -P OUTPUT DROP
    iptables -A OUTPUT -o lo -j ACCEPT
    iptables -A OUTPUT -d <postgres-container-ip> -j ACCEPT
    iptables -A OUTPUT -d <surfpool-container-ip> -j ACCEPT
    iptables -A OUTPUT -d 127.0.0.0/8 -j ACCEPT
    curl -s --max-time 3 -o /dev/null -w "internet:%{http_code}\n" https://1.1.1.1
  '
internet:000        # confirmed BLOCKED — the firewall is real, not asserted
$ docker exec ... cargo test -p sentinel-rpc --test surfpool_capability_probe -- --test-threads=1
running 8 tests ... test result: ok. 8 passed; 0 failed
```

With the firewall active and a public IP genuinely unreachable, the SR-7 capability probe (which talks
to Surfpool over the Docker network) still passed 8/8. This proves the actual claim ZC-1 makes — a
required test suite works with no network beyond loopback/local services — **operationally**, not just
by code review, even though the literal `.github/workflows/ci.yml` file has not been run by GitHub
Actions itself.

**Important limitation, stated plainly:** `.github/workflows/ci.yml` was authored and its YAML was
validated to parse correctly, and every job's commands were run **locally** with real, passing output
(pasted above), including a real dynamic network-isolation test as described. **The workflow file
itself has not been executed by GitHub Actions**, because this session has no push access to trigger it
and no sandboxed GitHub Actions runner. This is the one piece of Phase 1 evidence that is
local-dynamic-equivalent rather than "seen passing in GitHub's own infrastructure," and it is
recorded here rather than glossed over (`AGENTS.md` §9).

---

## Phase 2 — Canonical Data Model & Migrations — evidence

### Schema

Eleven forward-only migrations (`infra/migrations/0002_enums.sql` through `0011_initial_partitions.sql`)
implement every table in `docs/data-model.md`, with:

- Exact primary keys, natural keys, unique/partial-unique indexes, `FOREIGN KEY`s, and `CHECK`
  constraints as specified, plus a SQL comment on every index naming the query pattern it supports.
- Native PostgreSQL `ENUM` types for every closed-set column data-model.md/ingestion-model.md declare
  as `enum:`, taken verbatim from the frozen documents — never invented (`0002_enums.sql`'s own header
  documents the one case, `decode_failures.stage`/`error_code`, where no frozen document closes the set,
  and leaves those `text`).
- `numeric(39,0)` for every `u128` field, `numeric(20,0)` for every `u64` field. **Zero** floating-point
  columns anywhere — proven by `dm07_no_floating_point_column_exists_in_the_schema`, a real catalog
  query, not a source-text grep.
- Slot-range partitioning (10,000,000 slots/partition) for `raw_observations`, `transactions`,
  `instructions`, `program_logs`, `account_observations`, `token_balance_deltas`. **No `DEFAULT`
  partition on any of them** — a row outside every created partition fails loudly
  (`no partition of relation ... found for row`), proven live.
- Least-privilege GRANTs (`0010_grants.sql`) implementing `data-model.md` §10's ownership table onto
  Phase 1's two application roles (`sentinel_rust`, `sentinel_ts`) — see the role-granularity note
  inside that migration file, and DEVIATIONS below.

### Database invariants — DM-02/03/04/07/08/09/10, TX-02: proven against real PostgreSQL, not asserted

All of the following are real `#[tokio::test]`s in `crates/sentinel-db/tests/adversarial.rs` (17 tests)
and `crates/sentinel-db/tests/property_tests.rs` (2 tests), run against a live `postgres:18-alpine`
container, connecting **as each actual role** (`sentinel_bootstrap`, `sentinel_rust`, `sentinel_ts`) —
not simulated in application logic:

```
$ cargo test -p sentinel-db --test adversarial
running 17 tests
test dm02_sentinel_rust_cannot_update_or_delete_raw_observations ... ok
test dm02_sentinel_ts_cannot_update_delete_or_insert_raw_observations ... ok
test dm03_every_declared_natural_key_has_a_matching_unique_index ... ok
test dm03_duplicate_raw_observation_natural_key_is_rejected_not_duplicated ... ok
test dm04_stale_as_of_slot_write_is_rejected_on_aegis_markets ... ok
test dm04_slot_promotion_is_monotonic_observed_confirmed_finalized ... ok
test dm07_no_floating_point_column_exists_in_the_schema ... ok
test dm08_duplicate_open_alert_is_rejected_then_resolved_reopen_succeeds ... ok
test dm10_duplicate_idempotency_key_is_rejected_globally ... ok
test tx02_second_nonterminal_attempt_for_same_intent_is_rejected ... ok
test numeric_u128_max_round_trips_exactly_through_a_real_numeric_39_0_column ... ok
test numeric_value_exceeding_numeric_39_0_precision_is_rejected_not_truncated ... ok
test partition_row_routes_to_the_correct_partition_and_missing_partition_fails_clearly ... ok
test partition_automation_creates_a_partition_ahead_of_head_and_it_becomes_usable ... ok
test low_partition_condition_opens_one_alert_and_does_not_error_on_repeated_checks ... ok
test role_permissions_sentinel_ts_has_no_write_grant_on_any_raw_normalized_protocol_or_derived_table ... ok
test role_permissions_sentinel_rust_cannot_write_transaction_attempts_or_advance_execution_intents_state ... ok

test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.64s
```

DM-09 (one outstanding job per `dedupe_key`) is proven in `crates/sentinel-jobs/tests/concurrency.rs`
(the enqueue path uses the same partial-unique-index `ON CONFLICT ... DO NOTHING`; the 16-worker test
below is the same mechanism under real concurrency). Verified stable across 4 repeated full-suite runs
in this session (no flakes after fixing the two re-run-hygiene issues noted in DEVIATIONS).

### Role security

`sentinel_ts` was proven — by attempting real `INSERT`/`UPDATE` as that role, not by inspecting grants —
to have **no write access whatsoever** to every raw/normalized/chain-state/protocol/derived table
(23 tables enumerated and checked against `information_schema.role_table_grants`, plus direct write
attempts against `raw_observations`, `slots`, and `aegis_markets`). `sentinel_rust` was proven to have
no write access to `transaction_attempts` and no `UPDATE` access to `execution_intents` (insert-only,
per `data-model.md` §10). The same forbidden-write proof was independently repeated from the
**TypeScript side** in `ts/packages/db/src/index.test.ts` against a live connection as `sentinel_ts`.

A genuine ambiguity in the frozen documents' role-granularity language ("distinct database roles per
service" in `data-model.md` §10's table headers vs. the single `sentinel_rust`/`sentinel_ts` pair Phase
1 actually created) is resolved and documented explicitly — see DEVIATIONS.

### Job queue — `sentinel-jobs`

Real claim/lease/renew/complete/fail/quarantine implementation (`crates/sentinel-jobs/src/lib.rs`)
against the exact query shapes in `docs/distributed-correctness.md` §3/§7. The required 16-worker
concurrency test spawns 16 **genuine** `tokio::spawn` tasks, each with its own Postgres connection:

```
$ cargo test -p sentinel-jobs --test concurrency
running 3 tests
test poison_job_is_quarantined_after_max_attempts_and_opens_an_alert ... ok
test stale_lease_holder_update_affects_zero_rows_and_is_detected ... ok
test sixteen_workers_claim_disjoint_job_sets_with_no_duplicate_claim ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.6s
```

`sixteen_workers_...`: 80 jobs enqueued, 16 workers race `FOR UPDATE SKIP LOCKED` claims of 5 each,
asserted disjoint (`HashSet` of claimed `job_id`s has exactly 80 entries — no duplicate claim) and
exhaustive (every enqueued job claimed by exactly one worker). Verified stable across 4 repeated runs
after fixing a real cross-test-interference bug (see DEVIATIONS).
`stale_lease_holder_...`: a lease is claimed with a negative TTL (deterministically already expired),
reclaimed by a second holder, and the original holder's `complete()` call is proven to affect **zero
rows** and return `false` — never silently succeed.
`poison_job_...`: a job failed twice against `max_attempts=2` is proven `quarantined` (never
auto-retried — a third claim attempt does not return it) and proven to have opened a real
`kind='job_quarantined'` row in `alerts`.

### Partitioning

`crates/sentinel-db/src/partitions.rs`: `ensure_partitions_ahead` creates partitions ahead of a given
head slot (application code, not a trigger/stored procedure — Phase 2's explicit non-scope);
`count_future_partitions` and `check_low_partitions_and_alert` implement the observable/alertable
low-partition condition behind `docs/observability.md`'s `PartitionsExhausted` alert, opening a real
`alerts` row (kind `partitions_low`) and correctly absorbing DM-08's unique-violation on repeated checks
rather than erroring. All three proven against live Postgres in `adversarial.rs`
(`partition_row_routes_to_the_correct_partition_and_missing_partition_fails_clearly`,
`partition_automation_creates_a_partition_ahead_of_head_and_it_becomes_usable`,
`low_partition_condition_opens_one_alert_and_does_not_error_on_repeated_checks`).

### Numeric exactness

No `bigdecimal`/`rust_decimal` dependency (unreachable from this session — `curl https://crates.io`
returns HTTP 403; `npm`'s registry, by contrast, is reachable, which is why `ts/packages/db` could add
`pg`). Instead `crates/sentinel-db/src/numeric.rs` encodes/decodes `u128`/`u64` as exact decimal text
against explicit `::numeric` SQL casts — verified against real PostgreSQL:

```
$ cargo test -p sentinel-db --test adversarial numeric
test numeric_u128_max_round_trips_exactly_through_a_real_numeric_39_0_column ... ok
test numeric_value_exceeding_numeric_39_0_precision_is_rejected_not_truncated ... ok
```

`u128::MAX` (`340282366920938463463374607431768211455`) round-trips exactly; a `numeric(39,0)`-exceeding
value (`10^39`) is rejected by PostgreSQL itself with "numeric field overflow" — not truncated, not
rounded. The TypeScript side (`ts/packages/db`) uses native `bigint` end to end for the same reason,
verified with the same `u128::MAX` value in `index.test.ts`.

### Property tests

`P-KEY-1` and `P-MONO-2` (`crates/sentinel-db/tests/property_tests.rs`), fixed-seed (`0x5E17_1E02`),
300 generated cases each against real PostgreSQL, biased toward partition-boundary slots and
regressive/no-op candidate values per `testing-strategy.md` §3:

```
$ cargo test -p sentinel-db --test property_tests
running 2 tests
test p_mono_2_last_contiguous_slot_never_decreases ... ok
test p_key_1_distinct_logical_observations_never_collide_on_natural_key ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.8s
```

No `proptest` crate (same `crates.io`-unreachable reason as the numeric decision) — reproducibility
comes from the fixed seed itself (deterministic replay, not automatic shrinking), documented in the
test file's own header.

### SR-11 smoke measurement (real, not fabricated)

`crates/sentinel-jobs/examples/sr11_notify_smoke.rs`, run twice against the live local stack:

```
$ cargo run -p sentinel-jobs --example sr11_notify_smoke
SR-11 LISTEN/NOTIFY smoke measurement
rounds attempted: 50, rounds with a received notification: 50
mean: 3.602226ms
p50:  1.807542ms
p95:  4.227ms
max:  82.412042ms

$ cargo run -p sentinel-jobs --example sr11_notify_smoke   # second run
rounds attempted: 50, rounds with a received notification: 50
mean: 4.846614ms
p50:  3.025708ms
p95:  8.752042ms
max:  67.456834ms
```

This is a **smoke** measurement (one process, one `NOTIFY`/`LISTEN` connection pair, sequential
enqueues, no concurrent load) — it closes the "does the mechanism work and roughly how fast" question
Phase 2 owes, not the full throughput-at-target-load characterization, which stays explicitly deferred
to Phase 14 as `docs/testing-strategy.md`/`docs/project-status.md` already stated.

### Migrations

```
$ (drop schema + roles entirely) && make migrate
migrations applied successfully
$ make migrate   # immediately again, no changes in between
migrations applied successfully   # verified no-op: _sqlx_migrations unchanged, no DDL errors
$ psql ... -c "SELECT count(*) FROM information_schema.tables WHERE table_schema='public'"
 count
-------
    41
```

Both the from-empty apply and the immediate re-run were performed against a **freshly dropped** schema
and roles (`DROP SCHEMA public CASCADE`, `DROP ROLE sentinel_rust/sentinel_ts`) in this session, not
inferred from incremental state.

### VALIDATED — universal checklist

```
$ cargo fmt --all -- --check
(clean)

$ cargo clippy --workspace --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.85s
(zero warnings, zero errors)

$ bash scripts/ci-guards.sh
PASS: CI-NOFLOAT
PASS: CI-NOSLOTTIME
PASS: CI-NOPANIC
PASS: CI-NOSQLFMT
PASS: CI-NORAWCLIENT
PASS: CI-NOAEGISLEAK
PASS: CI-NOMATHDUP
PASS: CI-NOMAXVER
PASS: CI-NOSECRET
All 9 CI grep guards passed.

$ cargo test --workspace
(52 real tests across the workspace: 0 failed, 0 ignored — full per-crate breakdown: sentinel-core 6,
sentinel-config 4, sentinel-db lib 10, sentinel-db adversarial 17, sentinel-db property_tests 2,
sentinel-jobs concurrency 3, sentinel-rpc capability probe 8, sentinel-rpc other 2; every other crate
is an empty Phase-1 skeleton with 0 tests, unchanged from Phase 1)

$ cd ts && npx prettier --check . && npx eslint . && npm run build && npm test
All matched files use Prettier code style!
(eslint: zero errors)
(build: tsc succeeds for every workspace package)
(test: 4/4 in @sentinel/db — package skeleton marker, numeric round-trip, sentinel_ts forbidden-write
rejection, sentinel_ts read + execution-layer write — all other packages remain "no tests yet", empty
Phase 1 skeletons unrelated to Phase 2 scope)

$ cd ts && npm audit
found 0 vulnerabilities

$ bash scripts/demo-phase2-forbidden-ops.sh
PASS (rejected as required): sentinel_rust UPDATE raw_observations
PASS (rejected as required): sentinel_rust DELETE FROM raw_observations
PASS (rejected as required): sentinel_ts INSERT into raw_observations
PASS (rejected as required): sentinel_ts UPDATE slots
PASS (rejected as required): sentinel_ts INSERT into aegis_markets
PASS (rejected as required): second OPEN alert for the same (kind, entity)
PASS (rejected as required): duplicate idempotency_key, different kind
PASS (rejected as required): value exceeding numeric(39,0) precision (10^39, one digit beyond 39-digit precision)
PASS (rejected as required): insert at a slot with no created partition
Phase 2 demo: PostgreSQL refused every forbidden operation. The schema is defending itself.
```

`cargo audit` was **not run** in this session — same reason Phase 1 recorded: `crates.io` is
unreachable (HTTP 403), so `cargo install cargo-audit` cannot fetch the tool. This is a repeat of
Phase 1's own disclosed gap, not a new one introduced here.

### CLOSURE FIX — 2026-09-18 — VALIDATED (real commands, real output)

```
$ cargo build -p sentinel-db
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 11.59s

$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.98s
(zero warnings, zero errors)

$ cargo fmt --all -- --check
(no output — clean)

$ bash scripts/ci-guards.sh
PASS: CI-NOFLOAT
PASS: CI-NOSLOTTIME
PASS: CI-NOPANIC
PASS: CI-NOSQLFMT
PASS: CI-NORAWCLIENT
PASS: CI-NOAEGISLEAK
PASS: CI-NOMATHDUP
PASS: CI-NOMAXVER
PASS: CI-NOSECRET
All 9 CI grep guards passed.

$ cargo test --workspace
(same 52 pre-existing tests, unchanged, PLUS the closure fix's new tests:
 sentinel-db coverage_audit: 2, sentinel-db closure_fix_coverage: 8 — 62 total; 0 failed)
     Running tests/coverage_audit.rs
running 2 tests
test the_audit_mechanism_itself_detects_a_missing_table ... ok
test every_canonical_table_has_typed_rust_coverage ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/closure_fix_coverage.rs
running 8 tests
test reconciliation_mismatches_round_trip ... ok
test token_balance_deltas_round_trip_and_u128_max_is_exact ... ok
test account_observations_round_trip ... ok
test instructions_program_logs_round_trip ... ok
test rollback_events_gap_events_ingest_checkpoints_provider_health_round_trip ... ok
test derived_layer_additions_round_trip ... ok
test protocol_layer_additions_round_trip ... ok
test sentinel_rust_has_no_update_grant_on_newly_covered_append_only_tables ... ok
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

Full workspace test --workspace run 3 times in a row in this session: 0 failures every time.
```

**Coverage-audit mechanism proof (temporarily broken, then restored, in this session):**

```
$ # removed "jobs" from crates/sentinel-db/tests/coverage_audit.rs's rust_covered_tables()
$ cargo test -p sentinel-db --test coverage_audit every_canonical_table
thread 'every_canonical_table_has_typed_rust_coverage' panicked at ...:
the following canonical table(s) exist in the live schema but have NO typed Rust row struct /
query coverage in crates/sentinel-db — this is exactly the closure defect this test exists to
prevent: ["jobs"]
test result: FAILED. 0 passed; 1 failed

$ # restored "jobs"; re-ran
test result: ok. 2 passed; 0 failed
```

```
$ cargo test -p sentinel-jobs --test concurrency
running 3 tests
test stale_lease_holder_update_affects_zero_rows_and_is_detected ... ok
test poison_job_is_quarantined_after_max_attempts_and_opens_an_alert ... ok
test sixteen_workers_claim_disjoint_job_sets_with_no_duplicate_claim ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.79s

$ bash scripts/demo-phase2-forbidden-ops.sh
(identical PASS output to the original Phase 2 run — unchanged, reprinted above)

$ cd ts && npm install && npm run build && npx prettier --check . && npx eslint . && npm test
added 41 packages, audited 169 packages, 0 vulnerabilities
(build: tsc succeeds for every workspace package)
(prettier: all matched files use Prettier code style)
(eslint: zero errors)
(@sentinel/db test: 4/4 pass — package skeleton marker, numeric round-trip, sentinel_ts forbidden-write
 rejection, sentinel_ts read + execution-layer write; other packages remain the same honest
 empty-skeleton no-op)

$ cd ts && npm audit
found 0 vulnerabilities
```

**Note on `npm install`:** `ts/node_modules/pg` was absent at the start of this closure-fix session
(a fresh-clone-style gap unrelated to the `crates/sentinel-db` defect this session targets), which made
`npm run build`/`npm test` fail with `Cannot find module 'pg'` until `npm install` was re-run. This is
recorded because it is a real finding from this session, not silently worked around.

### DEVIATIONS

**ADR-0015** (`docs/adr/0015-raw-observations-composite-pk-for-partitioning.md`): `raw_observations`'
primary key is `(observation_id, slot)`, not `observation_id` alone as `data-model.md` §2 states in
isolation. PostgreSQL requires the partition key to be part of every unique constraint (including the
primary key) on a partitioned table — a bare `PRIMARY KEY (observation_id)` on a table partitioned by
`slot` is rejected outright. `data-model.md` §2 declares both the single-column PK and the partitioning
in the same table spec, which are not jointly satisfiable in real PostgreSQL; this was only discovered
implementing the migration, not from reading the document. `observation_id` keeps every property
(global uniqueness, monotonic identity) a caller relies on; only the constraint's physical column list
changes. The other five partitioned tables already declare `slot` as part of their PK in `data-model.md`,
so no equivalent ADR was needed for them.

**Role granularity (documented inline, no ADR — a routine implementation choice, not a frozen-document
change):** `data-model.md` §10's ownership table names ownership per *logical Rust/TypeScript service*
(`sentinel-ingest`, `sentinel-normalize`, ... `sentinel-executor`, `sentinel-api`), which could be read
as requiring one Postgres role per logical service. Phase 1's migration already created exactly **two**
application login roles — `sentinel_rust`, `sentinel_ts` — one per **language**, and its own comment
states Phase 2's job is to "add the per-table GRANTs that make **these roles'** least privilege real"
(singular reference to the two already-created roles). `data-model.md` §10's own closing sentence
confirms the structural property actually required: "The TypeScript service**s** hold **a role**
[singular] with no write grant on raw, normalized, chain-state, protocol, or derived tables. That is
what makes `architecture.md` §5's language boundary structural." This phase implements the ownership
table as GRANTs on the union of privileges each language-side role needs (full detail and per-table
rationale in `infra/migrations/0010_grants.sql`'s header comment), and the schema-ownership tests assert
exactly the property the frozen text actually states: `sentinel_ts` cannot write any raw/normalized/
chain-state/protocol/derived table, and neither role can write a table outside its declared ownership.
**If finer-grained per-logical-service Postgres roles are later required** (e.g. so a compromised
`sentinel-normalize` process cannot write `aegis_markets`, which the current two-role split does not
prevent — both are Rust-side and share one physical role), that is a real architectural change needing
its own ADR and migration; it was not silently invented here.

**Test-infrastructure fixes discovered and fixed during this phase, recorded because they are real
engineering findings, not narrative:**
1. `sqlx::migrate!`'s compile-time embedding of `infra/migrations/` did not reliably pick up newly
   *added* migration files under `cargo`'s normal incremental build — `make migrate` silently ran a
   stale, smaller migration set and reported "success" trivially (nothing new to apply from its own
   stale view). Forcing a rebuild (`touch crates/sentinel-db/src/lib.rs`) before `make migrate` is
   required after adding a new migration file; discovered when `_sqlx_migrations` showed only version 1
   applied despite nine additional migration files existing on disk and repeated "successful" `make
   migrate` runs. Worth a `make`-target fix in a later phase (out of this phase's scope to make).
2. `crates/sentinel-jobs/tests/concurrency.rs`'s three tests share one real `jobs` table with no
   test-scoping filter (`claim()`'s query is intentionally global, matching real production behavior) —
   `#[tokio::test]` functions in one binary run concurrently by default under `cargo test`, so without
   serialization one test's workers claimed another concurrently-running test's fixture rows. Fixed with
   a session-level Postgres advisory lock held for each test's body (no new crate dependency), plus
   cleanup of accumulated cross-run leftover rows inside the same locked section. Verified stable across
   4 repeated full runs after the fix.
3. Two `crates/sentinel-db/tests/adversarial.rs` tests (`dm04_stale_as_of_slot_write_is_rejected_on_aegis_markets`,
   `partition_automation_creates_a_partition_ahead_of_head_and_it_becomes_usable`) originally used fixed
   fixture identifiers/slots, which collided with the same test's own leftover state on a second run
   against the same database (real, persisted DDL and rows — not an in-memory test double). Fixed by
   randomizing (UUID-suffixed program IDs; a much larger, non-overlapping random head-slot range for
   partition automation, and later a nanosecond-timestamp-derived range for the low-partition test to
   avoid colliding with the *other* random-range tests' own leftover partitions). Verified stable across
   4+ repeated runs after each fix.

None of the three findings above required weakening a check, a constraint, or a test — every fix made
the test more correct/robust, never less strict.

### CLOSURE FIX — 2026-09-18 — `crates/sentinel-db` typed access layer completed

**Prior gap (now closed):** `crates/sentinel-db`'s typed row/query layer covered a representative subset
of tables (11 of 28), not literally every table, against Phase 2 requirement 10's "typed row structs and
a sqlx access layer in sentinel-db for every table." This has been fixed for real:

- `crates/sentinel-db/src/tables.rs` and `crates/sentinel-db/src/queries.rs` now have a row struct and
  `sentinel_rust`-role-appropriate query function(s) for all **28** canonical tables (verified against the
  live `pg_catalog`, not migration SQL text — see the table below and `coverage_audit.rs`'s own doc
  comment for the full per-table matrix).
- A new regression-proofing test, `crates/sentinel-db/tests/coverage_audit.rs`, queries the real Postgres
  catalog for the canonical table inventory and asserts it matches a hardcoded Rust coverage list — a
  canonical table added later without typed coverage makes this test fail and **names the missing
  table(s) explicitly**. The mechanism was verified by temporarily removing `"jobs"` from the coverage
  list and confirming the test failed with `... ["jobs"]`, then restoring it and confirming the test
  passed again (not merely asserted — actually done, in this session).
- A new round-trip campaign, `crates/sentinel-db/tests/closure_fix_coverage.rs` (8 tests), exercises one
  representative insert/read cycle per newly-covered table across every schema layer (normalized,
  chain-state, protocol, derived, execution), including a dedicated `u128::MAX` exactness regression on
  two newly-covered `numeric(39,0)` columns (`token_balance_deltas.post_amount`,
  `aegis_positions.supply_shares`) and a permission proof that the newly-covered append-only tables carry
  no `UPDATE` grant for `sentinel_rust`.
- No new writer was invented for any table `sentinel_rust` does not own per
  `infra/migrations/0010_grants.sql`'s actual GRANTs — `transaction_attempts` (owner: `sentinel-executor`,
  TypeScript-side) keeps only the pre-existing row struct and insert helper it already had; every other
  table's coverage matches sentinel_rust's exact INSERT/UPDATE/SELECT grant, never wider.
- No migration file changed. This was a Rust-side access-layer fix only; the schema itself was already
  complete and correct (proven by the pre-existing DM-03/DM-07 catalog audits).
- `ts/packages/db` still covers only the tables `sentinel_ts` may read/write with dedicated helper
  functions (`alerts`, `execution_intents`, `transaction_attempts`); broad read access is proven to work
  (`SELECT count(*) FROM raw_observations` / `aegis_markets` succeed as `sentinel_ts`) but no typed row
  interface exists yet for the read-only layers beyond `AlertRow`/`ExecutionIntentRow`/`TransactionAttemptRow`.
- `cargo audit` not run this session (crates.io unreachable — see VALIDATED section above); this is
  Phase 1's already-disclosed gap, unchanged.
- The from-empty migration apply/re-run drop-and-reapply proof was **not repeated** in this closure-fix
  session — the sandbox's destructive-action classifier declined a `DROP SCHEMA public CASCADE` against
  the local dev Postgres container (misclassified as a "cloud storage mass delete"). This closure fix
  changed **zero** migration files (only `crates/sentinel-db/src/{tables,queries}.rs` and new test files),
  so the schema itself is byte-identical to the already-verified original Phase 2 migration set (see
  "Migrations" above, performed against a freshly dropped schema+roles in the original Phase 2 session).
  The still-passing `migrate_from_empty_database_succeeds`/`migration_re_run_is_idempotent` tests in this
  session prove the idempotent-rerun path against the live, already-migrated database, but not a fresh
  from-empty apply in this specific session.
- The partition **lead-distance** number (how many future partitions to keep pre-created) and the
  low-partition alert **threshold** are not frozen-document constants — no frozen document states one —
  so they are left as caller-supplied parameters (`ensure_partitions_ahead`/`check_low_partitions_and_alert`
  take them as arguments) rather than invented as hardcoded defaults; a later phase's operational
  configuration is expected to set them for real deployment.
- `aegis_market_params_history`'s conflict policy (`DO NOTHING`) and its FK relationships are
  implemented in the schema but have no dedicated adversarial test in this phase (covered structurally
  by the same DM-03 audit, not by a duplicate-insert test).
- No Kafka/queue-broker, no additional Postgres role beyond the two Phase 1 established — consistent
  with ADR-0004/ADR-0002, not a gap.

---

## Phase 3 — RPC Abstraction & Resilient Client — evidence

### Implementation

`crates/sentinel-rpc/src/{provider,pool,health,breaker,budget,capabilities,ws,fixtures,fault,http}.rs`
(`http.rs` beyond the spec's literal file list — see "Known issues" above for why):

- `provider.rs` — `RpcProvider`/`RpcPool` traits, `RequestClass` (5 classes, priority-ordered),
  `CallContext` (correlation id + per-attempt request id + explicit commitment + deadline +
  `minContextSlot`), the closed `RpcMethodCall`/`RpcMethodResponse` enums (every
  `getBlock`/`getTransaction` variant requires `max_supported_transaction_version` at the type level —
  RPC-10), the five-class `RpcError` taxonomy (`Transient`/`Stale`/`Malformed`/`ContentDivergence`/`Fatal`,
  mirroring `docs/architecture.md` §9 exactly).
- `capabilities.rs` — `ProviderCapabilities`, a **whitelist** (`supported_methods`) so an unprobed
  method defaults to unsupported, never assumed; `validate_configured_classes` (CF-4 startup check).
- `budget.rs` — five independent per-class token-bucket + concurrency-gate pairs, sized by a fixed,
  strictly-decreasing weight table (`Execution` 40% .. `ScheduledScan` 8%), so pressure on one class can
  never consume another's capacity.
- `breaker.rs` — `Closed/Degraded/Open/HalfOpen`, CB-1..CB-6 implemented exactly (minimum sample size,
  Degraded excludes only `Execution`/`RealtimeCompleteness`, CB-4 immediate trip, every transition
  recorded).
- `health.rs` — per-provider rolling counters, health-weighted selection score, `provider_health`
  persistence via `sentinel-db`'s existing typed layer (no new datastore).
- `pool.rs` — the retry loop exactly as `docs/rpc-strategy.md` §5 pseudocode (bounded attempts per
  class, full-jitter exponential backoff, a retry may land on a different provider, budget enforced
  before the provider is ever called), `broadcast()` (concurrent fan-out via `join_all`, never
  sequential), `cross_check_block()` (FI-10/FI-11 divergence detection).
- `ws.rs` — the `Connecting/Establishing/Live/Degraded/Backoff/Failover` state machine from
  `docs/ingestion-model.md` §4, a declarative `SubscriptionSpec` set re-applied on every connect (W-1),
  bounded reconnect (W-4/FI-06), heartbeat derived from actual message arrival (W-3, never a hardcoded
  interval).
- `fixtures.rs` / `fault.rs` — `FixtureProvider`/`FaultInjectingProvider` implementing the same
  `RpcProvider` trait every production path uses.
- `http.rs` — `HttpRpcProvider`: a hand-rolled JSON-RPC transport over `reqwest` (not a wrapper around
  `solana-rpc-client::RpcClient`, which does not expose response headers and so cannot distinguish a 429
  with `Retry-After` from one without — see the module doc comment for the full reasoning), capability
  discovery by live probing (not name/version inference), A-SEC-01 credential redaction (`Debug` is
  hand-written to omit the URL; every constructed message interpolates the safe `id`, never the URL).

Demo: `crates/bins/sentinel-rpc-demo` (new bin crate; §26 requires a runnable demo, neither existing
empty-skeleton bin is Phase-3-scoped).

### RPC contract

Every call carries an explicit `Commitment` (`sentinel_core::Commitment`; no default-commitment code
path exists), a per-attempt `RequestId` (fresh each attempt, proven distinct across retries by
`fixtures::tests::each_attempt_gets_a_distinct_request_id`), and a `RequestClass`. No call site
anywhere in the workspace constructs a raw `solana_rpc_client`/`solana_pubsub_client` client outside
`sentinel-rpc` — enforced by `CI-NORAWCLIENT`, and the guard's actual detection was proven (not merely
assumed) by inserting a real violation into `sentinel-ingest` and confirming the guard failed, then
reverting (see VALIDATED below).

### Capabilities

Whitelist model (`ProviderCapabilities::supported_methods`): `unknown()` supports nothing until probed.
Tested against a fixture provider deliberately missing a method (`capabilities::tests`,
`fixtures::tests::missing_capability_returns_fatal_not_a_panic`) and against real local Surfpool
(`surfpool_pool_integration.rs::capability_discovery_against_real_surfpool_finds_every_required_method`).
A provider configured for a class it cannot serve fails at pool **construction**
(`capabilities::tests::provider_configured_for_unsupported_class_is_rejected`,
`PoolBuildError::Capability`), never at request time.

**Real finding from probing Surfpool** (not assumed): `getBlock(0, ...)` returns `-32602` ("before the
first local slot") on a rolling local validator that has already pruned slot 0 — probing must use a
currently-retained slot, not a hardcoded one. Also: Surfpool returns `-32602` (not `-32601`) for a
deliberately-thin `simulateTransaction`/`sendTransaction` probe payload, which is indistinguishable from
"unsupported" at a coarse error-class level; `classify_jsonrpc_error` was refined to treat only
`-32601` ("method not found") as confirmed-absent during capability discovery, while every JSON-RPC
error code keeps its normal five-class mapping (including non-retryable `Fatal`) on a real call. Both
fixed in `http.rs`, both caught by the real Surfpool integration test failing first, not assumed.

### Retry & budgets

RT-1..RT-6 implemented in `pool.rs::call`. `P-BOUND-1` (`crates/sentinel-rpc/tests/property_bound.rs`):
a fixed-seed splitmix64 generator produces 60 adversarial retryable-error sequences per request class
(300 total), each exceeding that class's retry bound by construction, run through the real
`RpcPool::call` end to end; every case's actual attempt count (read from `provider_health`) and elapsed
time are asserted bounded. `proptest`/`quickcheck` were not added — neither is in `Cargo.lock` and this
session could not verify `crates.io`'s API was reachable to add a new dependency (see Known Issues); this
is a real, passing property test, just without a shrinking/reporting library.

Budget: `budget::tests::success_rate_falls_in_strict_priority_order_under_symmetric_pressure_fi21`
proves the exact priority ordering under 1000-attempt symmetric pressure per class;
`exhausted_budget_denies_without_a_provider_call` proves an exhausted budget means the underlying
provider is never called (the `FixtureProvider`/`FaultInjectingProvider` call counter never increments
on a budget-denied attempt).

### Breaker

All CB-1..CB-6 rules unit-tested in `breaker.rs` (including the CB-1 fix found during this phase: the
ratio trip must be evaluated on every sample, success or failure — a run of failures followed by a
window-filling success was originally missed). Acceptance-level proof in
`tests/adversarial_campaign.rs`: FI-09 (staleness degrades), FI-10/FI-11 (CB-4 immediate trip regardless
of `min_sample_size=10000`), FI-22 (all-open is `RpcError::Fatal { code: "SEN-RPC-000", .. }`, the
distinct loud `NoHealthyProvider`-equivalent CB-6 requires).

### Freshness & divergence

`RpcPool::call_with_min_slot` rejects a response whose `context.slot` is below the required minimum as
`Stale`, never returning it to the caller (RPC-04), and degrades the provider's breaker
(`fi09_stale_response_is_rejected_and_degrades_health`). `RpcPool::cross_check_block` proves FI-10
(different blockhash for one slot — both surfaced, no canonical pick) and FI-11 (same blockhash,
different content — the more severe case, distinguished explicitly) independently, each tripping both
providers' breakers per CB-4.

### WebSocket

State machine unit-tested with a scripted, deterministic connector
(`ws::tests::subscribe_receive_and_reconnect_resubscribe_cycle`,
`flapping_connector_produces_bounded_reconnects_fi06`) and against **real local Surfpool**
(`surfpool_pool_integration.rs::websocket_connect_subscribe_notify_disconnect_reconnect_resubscribe_notify_against_real_surfpool`)
via a connector wrapper that force-closes the first real connection after one genuine notification —
proving a real connect → subscribe → notify → disconnect → reconnect → re-subscribe → notify cycle end
to end, not two independent runs. A genuine bug (integer overflow in the test's own force-close state
machine) was found and fixed by this real run, not merely by inspection.

### Broadcast

`RpcPool::broadcast` fans out concurrently via `futures_util::future::join_all` to every eligible
(`Closed`-breaker, `Execution`-configured) provider — never a sequential try-A-then-B. Covered
structurally by the same selection/eligibility tests as the retry path; RT-5 is implemented as a
distinct code path (no `for` loop with early return over providers).

### Security

A-SEC-01 (`http::tests::credential_bearing_url_never_appears_in_debug_or_id`,
`error_surfaced_from_a_failed_call_never_contains_the_credential`): a credential embedded in the
provider URL never appears in `Debug` output, the safe `id()`, or an error's `Display` text (checked
against a real failed call, not just a synthetic string).

### Provider health & metrics

`provider_health` (Phase 2 canonical table, owner `sentinel-rpc`) written via `sentinel-db`'s existing
`upsert_provider_health`/`fetch_provider_health` — proven against **real PostgreSQL**
(`tests/provider_health_persistence.rs`), including the upsert-on-repeat path and cleanup respecting
`sentinel_rust`'s real grant boundary (no `DELETE` grant — confirmed by a real permission-denied error
before the test was corrected to clean up via the bootstrap role).

Metrics: `crates/bins/sentinel-rpc-demo` exports `sentinel_rpc_demo_{requests_total,breaker_state,
p95_latency_ms}` (dimensions: provider, class/outcome/error_class) via `sentinel-telemetry`'s Prometheus
exporter, scraped by the real local Prometheus (`infra/compose/prometheus/prometheus.yml`, new
`sentinel-rpc-demo` job at `host.docker.internal:9464`) and queried live through **Grafana's own
datasource proxy** — see DEMO below for the actual queried values.

### Acceptance

**RPC-01..RPC-10 mapping** (audited from `docs/rpc-strategy.md` §11's actual definitions, not guessed
from names):

| ID | Invariant | Evidence |
|---|---|---|
| RPC-01 | No call site bypasses the pool | `CI-NORAWCLIENT`, proven to actually catch a violation (VALIDATED below) |
| RPC-02 | Every retry sequence bounded in count and total time | `tests/property_bound.rs` |
| RPC-03 | A non-retryable error is never retried | `fault::tests::every_required_fault_class_is_producible_and_classified_correctly` (Malformed/Stale asserted `!is_retryable()`); `RpcError::is_retryable` only matches `Transient` |
| RPC-04 | A response failing freshness is never used as canonical | `adversarial_campaign.rs::fi09_stale_response_is_rejected_and_degrades_health` |
| RPC-05 | `sendTransaction` fans out, never fails over sequentially | `pool.rs::broadcast` (concurrent `join_all`); RT-5 |
| RPC-06 | An open breaker excludes the provider from every class | `breaker::tests::open_excludes_every_class_cb2` |
| RPC-07 | All-providers-open pauses/refuses loudly | `adversarial_campaign.rs::fi22_all_providers_open_is_a_distinct_loud_failure` |
| RPC-08 | Credentials never appear in logs/metrics/errors/provider IDs | `http::tests::*` (A-SEC-01), `CI-NOSECRET` |
| RPC-09 | Every stored raw observation names the provider and request | `RequestId` threaded through `RpcOutcome`/`RpcError` on every path (Phase 4 will persist it — non-scope here per §2) |
| RPC-10 | Every `getBlock`/`getTransaction` sets `maxSupportedTransactionVersion` | Required at the Rust type level (`RpcMethodCall` variant fields) + `CI-NOMAXVER`, proven to actually catch a violation (VALIDATED below) |

FI-09/FI-10/FI-11/FI-21/FI-22 pass with the specific assertions the spec names (not merely "returned an
error") — see `tests/adversarial_campaign.rs`. FI-06 passes in `ws.rs`'s unit tests and is exercised for
real against Surfpool in the integration test above.

### Demo

Run live against real local Surfpool (`make up`), transcript below is the actual terminal output
(re-run twice in this session after an infrastructure interruption — both runs produced the same
qualitative sequence):

```
Phase 1/6: normal operation — both providers healthy.
[00] getSlot OK via provider=primary
...
>>> Phase 2/6: killing provider `secondary` (simulated provider failure) <<<
      (issuing a quick burst of calls so the breaker trip is observable promptly)
[08] getSlot OK via provider=primary
      health: provider=secondary  breaker=open      requests=5    errors=3
...
[18] getSlot OK via provider=primary
      half-open probe on secondary: failed
      health: provider=secondary  breaker=open      requests=5    errors=3
>>> Phase 5/6: restoring provider `secondary` — watch it recover via half-open probes <<<
...
[28] getSlot OK via provider=primary
      half-open probe on secondary: SUCCESS
      health: provider=secondary  breaker=half_open requests=5    errors=3
[29] getSlot OK via provider=primary
      half-open probe on secondary: SUCCESS
      health: provider=secondary  breaker=closed    requests=5    errors=3
[31] getSlot OK via provider=secondary
...
```

Every acceptance element is present and real: normal operation, provider failure, breaker opens
(step 08), selection routes away (every call steps 8–27 goes to `primary`), half-open probe occurs
(step 18, failed — fault still active; step 28, succeeded), provider recovers (`half_open` → `closed`,
step 29), selection uses it again (steps 31/35–39 route back to `secondary`).

Metrics visible in Grafana — queried through **Grafana's own datasource proxy**
(`/api/datasources/proxy/1/api/v1/query_range`, exactly what a dashboard panel issues), not just
Prometheus directly:

```
$ curl -s -u admin:admin \
  "http://127.0.0.1:3001/api/datasources/proxy/1/api/v1/query_range?query=sentinel_rpc_demo_breaker_state&start=<t-300s>&end=<t>&step=10"
{"status":"success","data":{"resultType":"matrix","result":[
  {"metric":{"provider":"primary"},  "values":[[t0,"0"],[t1,"0"],[t2,"0"]]},
  {"metric":{"provider":"secondary"},"values":[[t0,"0"],[t1,"2"],[t2,"0"]]}
]}}
```

`0`=Closed, `2`=Open — `secondary`'s breaker state visibly transitions `Closed → Open → Closed` in
Grafana's own queried data, reflecting the live fault-injection-and-recovery cycle above.

A genuine tuning bug was found and fixed while producing this demo, not hidden: with
`BreakerConfig::default()`'s window/sample settings, five pre-fault successes diluted the failure ratio
below the trip threshold for the whole 40-step run — the breaker never opened. Fixed with a
demo-tuned (smaller window/sample-size, documented as demo-only, not a production default) `BreakerConfig`
and a short burst of calls immediately after the fault starts, so the Open transition is observable
within the demo's timescale; production traffic volume does not need this.

**Environment note**: the Docker Compose stack this session originally found running was bind-mounted
from a *different* checkout of this repository (`/Users/vansh/Developer/Solana/sentinel`, not this
session's working tree at `/Users/vansh/Developer/sentinel`) — discovered via `docker inspect`'s mount
source when the demo's new Prometheus scrape config did not appear in the running container. Resolved
by `make down && make up` from this working tree so the running stack matches the code being delivered;
Postgres was re-migrated afterward. Also: mid-session, the host Docker daemon (OrbStack) itself restarted
independently of anything this session did, dropping the stack; it was detected, restarted, and the
stack was brought back up and re-verified before continuing.

### Regression

```
$ cargo fmt --all -- --check          → clean (after `cargo fmt --all`, which touched only Phase 3's own new files)
$ cargo clippy --workspace --all-targets --offline -- -D warnings   → clean, 0 warnings
$ bash scripts/ci-guards.sh           → All 9 CI grep guards passed
$ cargo test --workspace --offline (with real Postgres + Surfpool running)
  sentinel-config: 6 passed
  sentinel-core: 4 passed
  sentinel-db (unit + adversarial + closure_fix_coverage + coverage_audit + property_tests): 10+17+8+2+2 = 39 passed
  sentinel-jobs concurrency: 3 passed
  sentinel-rpc (unit + adversarial_campaign + property_bound + provider_health_persistence
    + surfpool_capability_probe + surfpool_pool_integration): 28+5+1+1+8+3 = 46 passed
  sentinel-telemetry: 2 passed
  → 0 failed across the entire workspace
```

CI-NORAWCLIENT and CI-NOMAXVER were each proven to **actually catch** a real violation, not merely
assumed to work: a `solana_rpc_client::rpc_client::RpcClient::new(...)` was temporarily inserted into
`sentinel-ingest/src/lib.rs` (guard failed with the exact expected message), and a plain
`.get_block(slot)` call was temporarily inserted into `sentinel-rpc/src/pool.rs` (guard failed with the
exact expected message); both reverted immediately after, `git status` confirmed clean, and the guards
were re-run to confirm they pass again.

TypeScript: `ts/packages/db`'s `npm test`/`npm run build` fail — confirmed pre-existing, `ts/` untouched
by this phase (see Known Issues). Every other TS package is an empty-skeleton echo, as in Phase 1/2.

### Validated

```
$ curl -sf -X POST ... getVersion → {"surfnet-version":"1.5.0","solana-core":"4.1.2",...}
$ cargo run --quiet -p sentinel-db --example migrate → migrations applied successfully
$ cargo test -p sentinel-rpc --offline → 26 unit passed (initial), then 28 after http.rs added; 5+1+1+8+3 integration/adversarial/property passed
$ cargo clippy -p sentinel-rpc --all-targets --offline -- -D warnings → clean
$ curl -s http://127.0.0.1:9464/metrics → real Prometheus text, sentinel_rpc_demo_* present
$ curl -s http://127.0.0.1:9090/api/v1/targets → [("sentinel-rpc-demo","up")] after `make up` was re-run from the correct checkout
$ curl -s -u admin:admin http://127.0.0.1:3001/api/datasources → Prometheus datasource confirmed provisioned
```

### Deviations

None requiring an ADR (AGENTS.md §13's criteria: change to a frozen document, a new dependency of
consequence, a deviation from a phase specification, a change to data model/commitment/execution/security
semantics, or a rejected alternative worth recording — none apply). The `RpcProvider` trait's generic
`call<R: RpcRequest>` from `rpc-strategy.md` §1's pseudocode was implemented as a closed, object-safe
enum (`RpcMethodCall`/`RpcMethodResponse`) instead — a routine implementation adaptation, documented
inline in `provider.rs`'s module doc comment, preserving every documented property (no raw call sites,
commitment/request-id/class always present, exact method set).

### Not done / known issues

See "Known issues" above for the full, honest list (crates.io API reachability → hand-rolled property
test instead of `proptest`; `docs/failure-injection.md` absent from the repo; pre-existing unrelated TS
failures in `ts/packages/db`; two disclosed organizational choices beyond the literal file list). Beyond
those: RPC-09's "every stored raw observation names the provider and request" is proven at the
`RequestId`/`ProviderId` plumbing level (both are present on every `RpcOutcome`/`RpcError`) but the
actual `raw_observations` write does not exist yet — that is Phase 4's raw observation boundary, explicit
non-scope here (`phase-03-rpc.md` §2).

---

## Next action

**Phase 3 is complete.** Hand Phase 4 (Raw Observation Boundary & Ingestion) to the next session.
Full `LISTEN/NOTIFY` throughput-at-load characterization (SR-11's remaining piece) stays explicitly
deferred to Phase 14, as both the Phase 1 and Phase 2 specs require.
