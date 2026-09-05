# Sentinel — Phase Roadmap

**Status: FROZEN (Phase 0). Phase order and boundaries may change only via ADR.**

> **The implementation model MUST STOP after completing exactly one phase.**
> Starting the next phase without an explicit human instruction is a process violation, regardless of
> how much time or context remains.

---

## 1. Roadmap overview

| Phase | Name | Ships | Gate |
|---|---|---|---|
| 0 | Planning & architecture | This `docs/` tree, `AGENTS.md`, `CLAUDE.md`, 14 ADRs | **COMPLETE** |
| 1 | Foundation & local infrastructure | Workspaces, pinned toolchain, Compose stack, CI, verified versions | **SR-7 blocking**; SR-5, SR-11 |
| 2 | Canonical data model & migrations | Every table, key, index, partition, role — no ingestion | SR-11 |
| 3 | RPC abstraction & resilient client | Provider pool, capabilities, breaker, failover, fault injection | SR-6 |
| 4 | Raw observation boundary & ingestion | Raw layer, slot/block/tx/account/log ingestion, checkpoints, gap detection | SR-1 (decode) |
| 5 | Normalization, backfill & replay | Normalized layer, replay determinism harness, backfill | — |
| 6 | Chain state: commitment & forks | Canonical chain, promotion, fork detection, rollback, recompute | **SR-2 blocking** |
| 7 | Aegis protocol adapter | Version-aware decoder, event + snapshot materialization, reconciliation | **SR-8, SR-9, SR-10; Aegis ≥ Phase 6** |
| 8 | Derived risk state | Health, candidates, market metrics, Aegis conformance vectors, invariant checks | — |
| 9 | REST + realtime API | Versioned REST, resumable WebSocket, auth, rate limits | — |
| 10 | Transaction execution engine | Intent/attempt state machines, signer boundary, policy engine | SR-1 (build) |
| 11 | Aegis liquidation keeper | The closed loop, end to end, with reconciliation | **Aegis ≥ Phase 9 (`@aegis/sdk`)** |
| 12 | Observability, failure injection & recovery | OTel, dashboards, alerts+runbooks, the 28-entry FI campaign, adversarial suite | SR-2 re-check |
| 13 | Optional Geyser/Yellowstone adapter | Second `ObservationSource`, fallback, measured comparison | **SR-3 blocking**; ADR-0006 trigger |
| 14 | Load & performance campaign | Benchmark harness, targets, measured results, selected optimizations | SR-4 |
| 15 | UI, demo, security review & release | Next.js app, full demo, self-review, README | — |

---

## 2. Why this order, and what changed from the provisional sequence

Three deliberate changes from the sequence proposed in the brief. Each is a correctness argument, not a
preference.

### 2.1 The raw observation boundary moved *into* the first ingestion phase

The provisional order had "basic ingestion" (Phase 4) before "raw durable observation boundary +
checkpoints" (Phase 5). That would mean Phase 4's ingestion writes somewhere else and Phase 5 rewrites
it — building a throwaway sink and then replacing the most load-bearing component in the system.

**Ingestion writes to the raw boundary from its first line of code.** Phases 4 and 5 of the provisional
sequence are merged into Phase 4, and Phase 5 becomes normalization + replay.

### 2.2 Fork and finality handling was promoted to its own phase

The provisional sequence never gave commitment/fork handling a phase; it was implicitly folded into
ingestion or decoding. It is the **hardest correctness property in the system** and the one most likely
to be quietly skipped, so it gets Phase 6, before any protocol decoding — because decoding a slot whose
place in the chain is unknown is meaningless.

**It sits after normalization** because the chain-state engine needs `parent_slot`/`parent_blockhash`
links, which normalization produces.

### 2.3 The transaction engine is separated from the keeper

Phase 10 builds the generic durable execution machinery and proves its crash-safety with a **trivial,
non-economic instruction** (`accrue_interest`, which is permissionless and idempotent — a no-op when
`dt == 0`). Phase 11 then adds Aegis liquidation on top.

This is deliberate: the crash-safety, idempotency, and ambiguity-resolution guarantees are tested
against something harmless **before** they guard something that spends money. Building both at once
would mean debugging the state machine and the economics simultaneously, using real value.

### 2.4 The ordering property that makes the whole sequence work

**Phases 1–6 have zero upstream dependency on Aegis.** They are pure Solana infrastructure. Since Aegis
is itself at Phase 0, this means roughly half of Sentinel can be built to completion regardless of
Aegis's schedule — and the Aegis-dependent phases are all late and clearly gated.

---

## 3. Cross-project dependencies on Aegis

**As of 2026-09-04, Aegis is at Phase 0: planning complete, no code written.** Stated plainly rather
than assumed away.

| Sentinel phase | Requires from Aegis | Status | Interim path |
|---|---|---|---|
| 1–6 | **Nothing** | — | — |
| 7 | Deployed program ID, IDL, discriminators, event layouts (Aegis Phases 2–6) | **BLOCKED** | Decoder written against the frozen spec, `source='spec'`; fixtures from a locally-built Aegis; SR-8/SR-9 must close before the phase can complete |
| 8 | `aegis-math` crate (Aegis Phase 4–6) | **BLOCKED for the preferred path** | Implement per `economic-model.md` and prove against `AEGIS-CONF-01..06`, which are **frozen and available today** |
| 11 | `@aegis/sdk` with `ix.ts` builders (Aegis Phase 9) | **BLOCKED** | None acceptable. Hand-building the `liquidate` instruction would create a second source of truth for the most dangerous instruction in the protocol (ADR-0001). **Phase 11 waits.** |
| 15 | A running Aegis deployment for the demo | Follows from the above | Fixtures for everything except the live demo |

**Rule:** a blocked phase is **not** started with a substitute for the blocking artifact. It is
reported as blocked, and the unblocked phases are done instead. Phase 11 in particular has no
acceptable workaround, and inventing one would be exactly the drift `AGENTS.md` §3 forbids.

---

## 4. Universal phase completion checklist

Applies to **every** phase. A phase is not complete until all are true:

- [ ] All acceptance criteria in the phase spec are met.
- [ ] `make test` passes **offline, with no secrets**, on a clean clone.
- [ ] The no-network CI job passes.
- [ ] Every invariant assigned to this phase has a test that **fails when its mechanism is removed**.
- [ ] `cargo clippy -- -D warnings`, `cargo fmt --check`, TypeScript lint and type-check all pass.
- [ ] All CI grep guards pass (`CI-NOFLOAT`, `CI-NOSLOTTIME`, `CI-NOPANIC`, `CI-NOSQLFMT`,
      `CI-NORAWCLIENT`, `CI-NOAEGISLEAK`, `CI-NOMATHDUP`, `CI-NOMAXVER`, `CI-NOSECRET`).
- [ ] Traceability passes: every test ID referenced in a frozen document exists.
- [ ] Replay determinism (RP-01..RP-03) passes, from Phase 5 onward.
- [ ] Crash-boundary failure injection (FI-03..FI-05) passes, from Phase 10 onward.
- [ ] `docs/project-status.md` updated with IMPLEMENTED / TESTED / DEMOED / DOCUMENTED / COMMITTED per
      item, and every research gate touched is updated.
- [ ] Any architectural deviation is recorded as an ADR — **not** silently absorbed.
- [ ] Exact validation commands and their **real** output are pasted into the status file.
- [ ] Git tag `phase-NN-<name>` created.
- [ ] **STOP.** Report completion and await explicit instruction.

---

## 5. Git milestone discipline

| Phase | Tag | Branch |
|---|---|---|
| 1 | `phase-01-foundation` | `phase/01-foundation` |
| 2 | `phase-02-data-model` | `phase/02-data-model` |
| 3 | `phase-03-rpc` | `phase/03-rpc` |
| 4 | `phase-04-ingestion` | `phase/04-ingestion` |
| 5 | `phase-05-normalize-replay` | `phase/05-normalize-replay` |
| 6 | `phase-06-chainstate` | `phase/06-chainstate` |
| 7 | `phase-07-aegis-adapter` | `phase/07-aegis-adapter` |
| 8 | `phase-08-risk` | `phase/08-risk` |
| 9 | `phase-09-api` | `phase/09-api` |
| 10 | `phase-10-execution` | `phase/10-execution` |
| 11 | `phase-11-keeper` | `phase/11-keeper` |
| 12 | `phase-12-observability` | `phase/12-observability` |
| 13 | `phase-13-geyser` | `phase/13-geyser` |
| 14 | `phase-14-performance` | `phase/14-performance` |
| 15 | `phase-15-release` | `phase/15-release` |

Rules: conventional commits; no secrets ever; no force-push to `main`; every phase merges as a
reviewable unit; the tag is created only after the completion checklist passes.

---

## 6. Estimated relative effort

Not calendar time — relative weight, so effort is not accidentally spent in the wrong place.

| Phase | Weight | Note |
|---|---|---|
| 1 | ▓▓ | Mostly verification and configuration; SR-7 could turn it into a finding |
| 2 | ▓▓▓ | The schema is the contract; getting keys and partitions right is most of it |
| 3 | ▓▓▓ | Pool, capabilities, breaker, and the fault-injection harness |
| 4 | ▓▓▓▓ | The raw boundary, checkpointing, and gap detection are load-bearing |
| 5 | ▓▓▓▓ | Normalization plus the **replay determinism harness**, which everything later depends on |
| 6 | ▓▓▓▓▓ | **The hardest correctness phase.** Forks, promotion, rollback, recompute |
| 7 | ▓▓▓▓ | Version-aware decoding and dual reconstruction |
| 8 | ▓▓▓ | Risk derivation; mostly calling Aegis's math correctly and proving it |
| 9 | ▓▓▓ | API surface and resumable WebSocket semantics |
| 10 | ▓▓▓▓▓ | **The other hardest phase.** Durable execution, crash safety, policy engine |
| 11 | ▓▓▓▓ | The closed loop and its adversarial cases |
| 12 | ▓▓▓▓▓ | **The phase that makes the repository credible** |
| 13 | ▓▓ | Optional; bounded |
| 14 | ▓▓▓ | Measurement, not speculation |
| 15 | ▓▓▓ | UI, demo, self-review |

Phases 6, 10 and 12 carry the most weight and the most risk. **If time is constrained, cut Phase 13 and
Phase 15 scope before cutting Phase 12.** A platform with a UI and no failure-injection campaign is
worth less than a platform with a failure-injection campaign and a CLI demo — and this instruction
exists precisely because the opposite temptation is strong.
