# Phase 1 — Foundation & Local Infrastructure

**Status: NOT STARTED.** **Prerequisite: Phase 0 complete.**
**Blocking research gate: SR-7. Also closes SR-5, SR-11.**

> This phase's most important output is not code — it is **verified facts**. If Surfpool does not
> expose a method Sentinel's architecture requires, that is an architectural finding to surface, not a
> problem to work around.

## 1. Scope

1. **Version verification first**, before pinning anything. Run every command in
   `ecosystem-research.md` §12; paste real output into `project-status.md`; update the research
   document where reality differs.
2. **SR-7 capability probe (blocking).** Bring up Surfpool and verify it exposes every method Sentinel
   requires: `getVersion`, `getSlot`, `getBlockHeight`, `getBlocks`, `getBlock` (with
   `maxSupportedTransactionVersion`), `getTransaction`, `getSignaturesForAddress`,
   `getMultipleAccounts`, `getProgramAccounts`, `getLatestBlockhash`, `getSignatureStatuses`,
   `getRecentPrioritizationFees`, `simulateTransaction`, `sendTransaction`, and WebSocket
   `slotSubscribe` / `accountSubscribe` / `logsSubscribe`. Record which are present, which are absent,
   and which behave differently from documentation.
3. **Rust workspace** with the crate skeletons from `architecture.md` §4 — `sentinel-core`,
   `sentinel-config`, `sentinel-db`, `sentinel-telemetry` implemented; the rest as empty crates with
   their dependency edges declared so the rules are enforceable from day one.
4. **TypeScript workspace** with package skeletons and shared tsconfig/lint.
5. **Compose stack**: Postgres, Surfpool, OTel collector, Prometheus, Grafana, optional Redis.
6. **Makefile**: `up`, `down`, `migrate`, `test`, `lint`, `fmt`, `verify-versions`.
7. **CI**: build, lint, format, unit tests, the **no-network job**, the secret scan, the dependency
   advisory scan, and every CI grep guard (initially over an almost-empty tree — they must exist and
   pass from the first commit, not be added later).
8. `.gitignore`, `LICENSE`, and a README stating the true current state.

## 2. Explicit non-scope

No RPC client. No ingestion. No schema beyond the migrations runner itself. No API routes. No decoding.
No Aegis anything. **No placeholder implementations** — an empty crate with declared dependencies is
correct; a crate with a fake `todo!()` pipeline is not.

## 3. Evidence objective

- Every pinned version is **verified, not remembered**, with pasted output.
- The zero-cost path is **real from the first commit**: the no-network CI job passes.
- The dependency rules and CI guards exist before there is any code to violate them — which is the only
  time it is cheap to add them.

## 4. Files

`Cargo.toml` (workspace) · `crates/sentinel-{core,config,db,telemetry}/` · empty crates for the rest ·
`ts/package.json`, `ts/packages/*`, `ts/apps/*` · `infra/compose/*` · `infra/migrations/0001_init.sql`
(migrations table only) · `Makefile` · `.github/workflows/ci.yml` · `.gitignore` · `README.md`

## 5. Dependencies

None internal. **SR-7 is blocking** — the phase cannot be marked complete with the capability probe
unrun or its results unrecorded.

## 6. Implementation requirements

- **Verify, do not remember.** Anything contradicting `ecosystem-research.md` is a finding: update the
  document and note the delta in the status file.
- **No default configuration value points at a non-loopback host.** A missing configuration fails
  startup; it never silently falls back.
- Secrets are a **wrapper type** whose `Debug`/`Display` prints a placeholder, from the first line of
  `sentinel-config`. Retrofitting redaction is how secrets leak.
- Distinct least-privilege database roles are created by the first migration, **including locally**.
- `overflow-checks = true` in the release profile.
- Clippy denies `unwrap_used`/`expect_used`/`panic` in the crates that will process external input.
- CI runs with **no secrets configured**.

## 7. Tests

- Config loading: valid, missing-required, and a **redaction test** that serializes a full config and
  asserts nothing sensitive appears.
- Migration runner: up-from-empty; idempotent re-run.
- Telemetry: metrics register and export; a log line carries the expected fields.
- Compose smoke: `make up && make migrate` succeeds; Surfpool answers `getVersion`.
- The capability probe itself is a **committed test**, so a Surfpool upgrade that removes a method
  fails the build rather than surprising Phase 4.

## 8. Adversarial / failure cases

- Missing required config → startup fails with a clear message, not a default.
- A config value pointing at a non-loopback host in the local profile → startup error.
- A secret in a config value → never appears in any log or serialized output.
- Postgres unavailable → the migration command fails clearly and retries are bounded.

## 9. Acceptance criteria

- [ ] Every command in `ecosystem-research.md` §12 run, with **real output** in `project-status.md`
- [ ] `ecosystem-research.md` updated wherever reality differed; every delta noted
- [ ] **SR-7 closed**: the capability probe run, its results recorded, and any missing method escalated
      as an architectural finding
- [ ] SR-5 closed: resolved Rust client crate versions recorded from `Cargo.lock`
- [ ] SR-11 partially closed: Postgres version pinned; `LISTEN/NOTIFY` deferred to Phase 2 measurement
- [ ] `make up`, `make migrate`, `make test`, `make lint`, `make fmt` all work on a clean clone
- [ ] **The no-network CI job passes**
- [ ] Every CI grep guard exists and passes
- [ ] Secret scan and dependency advisory scan pass
- [ ] Database roles created with least privilege, locally too
- [ ] Universal checklist satisfied. Tag `phase-01-foundation`.

## 10. Demo

`git clone && make up && make test` on a clean machine, with the network interface down except
loopback. Show `make verify-versions` printing the real, verified toolchain.

## 11. Documentation & status updates

`ecosystem-research.md` updated with verified versions and the SR-7 results. `project-status.md`:
environment table with real output, research-gate status, component table (foundation IMPLEMENTED +
TESTED, everything else unchanged). README states the true state.

## 12. Stop condition

**STOP after this phase.** Phase 2 has not been started.
