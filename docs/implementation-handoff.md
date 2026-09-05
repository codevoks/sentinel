# Sentinel — Implementation Handoff Instructions

**For the implementation model (e.g. Claude Sonnet) taking over from Phase 0.**

---

## 1. How to start a phase session

Give the implementation model a prompt of this shape:

```
Implement Sentinel Phase N exactly according to the frozen Phase 0 specification
and the current repository state.

Read, in order:
  1. AGENTS.md
  2. docs/project-status.md
  3. docs/phases/phase-NN-<name>.md
  4. Any ADR relevant to what you are building
  5. If the phase touches protocol decoding, risk, or liquidation:
     the Aegis documents named in docs/aegis-integration.md §1,
     read from the pinned Aegis revision — not from memory.

Implement only Phase N. Stop when it is complete and report.
```

Nothing else should be required. **If the model asks a design question the specification already
answers, the specification needs a fix — record that as a finding.**

---

## 2. What the implementation model must NOT need to decide

These are fully specified. **If a session starts redesigning any of them, stop it** — it means either
the prompt did not point at the specification, or the specification has a gap worth fixing.

| Already decided | Where |
|---|---|
| Which language owns which responsibility, and why | `architecture.md` §3, ADR-0001 |
| Every table: primary key, natural key, conflict policy, indexes, mutability class, owning writer | `data-model.md` |
| Which layers are rebuildable and which are not | `architecture.md` §2, `data-model.md` §1 |
| Ingestion sources, lifecycle, dedup, gaps, checkpoints, backpressure | `ingestion-model.md` |
| What each commitment level means and what may be persisted or exposed at it | `finality-and-forks.md`, ADR-0009 |
| Fork detection, rollback, and recompute semantics | `finality-and-forks.md` §4–5 |
| Determinism rules and the twelve replay acceptance criteria | `replay-and-backfill.md` |
| Both execution state machines, every transition, and the retry table | `transaction-engine.md` §2–3, §9 |
| **The sign → persist → submit ordering** | `transaction-engine.md` §6 |
| The twelve signing policy checks | `signer-and-key-management.md` §4 |
| The Aegis contract: PDAs, fields, health sequence, conformance vectors, version handling | `aegis-integration.md` |
| The keeper loop and its eighteen adversarial cases | `keeper-design.md` |
| RPC classes, budgets, breaker semantics, freshness checking | `rpc-strategy.md` |
| The API envelope, pagination, error model, WebSocket semantics | `api-design.md` |
| Threats, mitigations, and their test IDs | `threat-model.md` |
| Test tiers and what belongs in each | `testing-strategy.md` |
| Metrics, alerts, and the operator action for each alert | `observability.md` |
| Phase scope and non-scope | `docs/phases/` |

---

## 3. Where flexibility is explicitly allowed

The implementation model **may** choose freely, without an ADR:

1. **Internal function decomposition** within a module — helper functions, parameter ordering, naming
   of private items.
2. **Concrete library choices within a stated role**, provided the dependency policy (`AGENTS.md` §14)
   is satisfied: the HTTP framework, the WebSocket library, the metrics exporter, the property-testing
   crate.
3. **Test file organization**, as long as every required test ID exists and is discoverable.
4. **Query formulation and index tuning**, provided the declared natural keys and uniqueness constraints
   are unchanged and every index is justified by a named query.
5. **Batch sizes, channel capacities, lease TTLs, and backoff constants** — these are configuration with
   documented defaults, not architecture. They must be *configurable*, not hardcoded.
6. **Property-test generator weights and biasing**, provided the dangerous regions named in
   `testing-strategy.md` §3 are covered.
7. **UI layout, styling, and component structure** (Phase 15), provided `UI-1..UI-10` hold.
8. **CI job granularity**, provided every listed check runs and blocks.
9. **Comment wording and doc-comment style**, matching surrounding code.

Anything not on this list follows the specification. **When unsure, ask rather than choose.**

---

## 4. Non-negotiables (restated because these are the ones most likely to slip)

1. **One phase per session. Stop at the end. Report. Wait.**
2. **Never weaken a check, a constraint, or a test to make progress.** Specifically: do not relax a
   `UNIQUE` index, do not turn an `ON CONFLICT DO NOTHING` into a blind insert, do not remove a
   simulation step, do not widen a signer policy, do not delete a failure-injection test.
3. **Never claim a test passed without running it and reading the output.**
4. **Never silently drop scope.** Say what you could not do and why.
5. **Verify versions; do not remember them.** Run `ecosystem-research.md` §12.
6. **Every persisted observation and every API field carries explicit commitment semantics.**
7. **Every externally visible effect is idempotent**, with a stated natural or business key.
8. **Derived state stays replayable.** If a change would break RP-01, the change is wrong.
9. **Never reimplement Aegis economics from memory.** Read the Aegis repository; consume `aegis-math`
   and `@aegis/sdk` where they exist.
10. **No number without a measurement.** Not "fast", not "optimized", not a throughput figure.
11. **Record deviations as ADRs**, in the same commit as the document and test updates.
12. **Update `docs/project-status.md`** with real command output before declaring a phase complete.

---

## 5. The eight mistakes most likely to happen

Named specifically so they can be watched for.

1. **Calling `getBlock` without `maxSupportedTransactionVersion`.** It works until the first v1
   transaction and then presents as an inexplicable ingestion stall. `CI-NOMAXVER` exists for this.
2. **Reading a priority fee by scanning for `ComputeBudget` instructions.** Returns **zero, silently**,
   for every v1 transaction. Read `transactionConfig` first and record `priority_fee_source`.
3. **Submitting before persisting the signature and bytes.** It appears to work in every happy-path
   test and produces a duplicate liquidation exactly once, in production, after a crash. FI-04 exists
   for this, in the required tier.
4. **Computing health without accruing to `t_eval`.** Systematically understates debt, so candidates
   are missed and predictions are biased. The mutation test in Phase 8 exists for this.
5. **Treating a missing account notification as a missed event.** Since Agave 4.2, an update is emitted
   only when the account actually changes. Absence means "no change", never "lost".
6. **Materializing by incrementing rather than folding.** Works until the first duplicate delivery, then
   double-counts silently. The fold formulation is mandatory.
7. **Inverting events to unwind a reorg.** Produces sign errors that are almost impossible to find.
   Rebuild forward from the finalized anchor; never subtract.
8. **Deriving a duration from a slot count.** Slot times are changing and Alpenglow changes finality
   timing. `CI-NOSLOTTIME` exists for this.

---

## 6. Reporting format

Use the format in `CLAUDE.md` § "Reporting format at the end of a phase". Every phase report includes
real command output, failure-injection results, the invariant and property IDs tested, evidence,
research-gate status, deviations (or "none"), what was not done (or "none"), and an explicit statement
that the next phase has not been started.

---

## 7. Escalate to the human when

- A frozen document appears wrong or unimplementable → **stop, explain, propose**.
- Two documents contradict each other → **stop, quote both, recommend**.
- **An Aegis document has changed since the pin** → **stop and report the delta.** Never absorb it
  silently.
- A verified version contradicts `ecosystem-research.md` in a way that invalidates a *decision* (not
  merely a version number) → **stop and report**.
- **A blocking research gate cannot be closed from a primary source** → report it as unresolved with
  the conservative behavior chosen and why. Do not guess and proceed.
- **An upstream artifact you need does not exist yet** (Phases 7, 8, 11) → complete everything
  unblocked, report exactly what is missing, and **do not substitute a hand-rolled equivalent** for
  `aegis-math` or `@aegis/sdk`.
- You are about to weaken any check, constraint, or test → **stop. This is never the right answer.**
