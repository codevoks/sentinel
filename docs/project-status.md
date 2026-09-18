# Sentinel — Project Status

**Last updated: 2026-09-18**
**Current phase: Phase 1 — Foundation & Local Infrastructure — COMPLETE**
**Next phase: Phase 2 — Canonical Data Model & Migrations — NOT STARTED**

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
| 2 | Canonical data model & migrations | ⬜ NOT STARTED | — |
| 3 | RPC abstraction & resilient client | ⬜ NOT STARTED | — |
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
| `sentinel-rpc` — provider pool, breaker, failover | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-ingest` — raw boundary, checkpoints, gaps | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-normalize` — Solana primitives | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-chainstate` — commitment, forks, rollback | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-decode` — protocol adapter trait + registry | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-replay` — determinism harness | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-aegis` — decoder registry & materialization | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-risk` — health, sizing, candidates | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-jobs` — Postgres job queue | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `sentinel-geyser` — optional source | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `bins/sentinel-indexer`, `bins/sentinel-backfill` | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
| `@sentinel/db`, `@sentinel/aegis`, `@sentinel/executor`, `@sentinel/policy` | ⬜ empty skeleton | ⬜ | ⬜ | ✅ | ⬜ |
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
| Failure-injection entries (`FI-01..FI-28`) | 28 | 0 | 0 |
| Named races (`T-RACE-01..12`) | 12 | 0 | 0 |
| Replay criteria (`RP-01..RP-12`) | 12 | 0 | 0 |
| Keeper criteria (`KP-01..KP-14`) | 14 | 0 | 0 |
| Aegis conformance vectors (`AEGIS-CONF-01..06`) | 6 | 0 | 0 |
| Off-chain Aegis invariants (`AEGIS-INV-01..08`) | 8 | 0 | 0 |
| CI grep guards | 9 | **9** | **9** |
| Phase 1 Rust unit/integration tests | — | **15** | **15** |
| Phase 1 TypeScript tests | — | **1** (harness proof; rest are empty-skeleton packages) | **1** |
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
| SR-11 | Postgres version to pin; `LISTEN/NOTIFY` throughput at target load | 2, 14 | ⚠️ **PARTIALLY CLOSED** — version pinned to `postgres:18-alpine` (18.6), verified by `docker run --rm postgres:18-alpine postgres --version`. `LISTEN/NOTIFY` throughput measurement remains OPEN, deferred to Phase 2/14 exactly as the Phase 1 spec requires. |

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
(all other crates: 0 tests — empty skeletons)

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
`docs/ecosystem-research.md` §12a.

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
| `test` | ✅ | `cargo test --workspace` (15 tests) + `npm test` (1 real + 6 honest no-op) all pass against live Postgres |
| `no-network` | ✅ (defined; see below) | Not executed inside GitHub Actions from this session — see "NOT DONE" |
| `secret-scan` | ✅ | `scripts/ci-guards.sh`'s `CI-NOSECRET` check, run standalone |
| `dependency-advisory-scan` | ✅ (defined) | `npm audit` run locally: **0 vulnerabilities**. `cargo audit` was not completed inside this session (see "NOT DONE") |

**Important limitation, stated plainly:** `.github/workflows/ci.yml` was authored and its YAML was
validated to parse correctly, and every job's commands were run **locally** with real, passing output
(pasted above). **The workflow itself has not been executed by GitHub Actions**, because this session
has no push access to trigger it and no sandboxed GitHub Actions runner. This is the one piece of
Phase 1 evidence that is design-plus-local-equivalent rather than "seen passing in CI," and it is
recorded here rather than glossed over (`AGENTS.md` §9).

---

## Next action

**Phase 1 is complete.** Hand Phase 2 (Canonical Data Model & Migrations) to the next session.
`LISTEN/NOTIFY` throughput (SR-11's remaining half) is explicitly Phase 2's to measure.
