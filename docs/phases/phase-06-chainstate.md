# Phase 6 — Chain State: Commitment, Forks and Rollback

**Status: NOT STARTED.** **Prerequisite: Phase 5 complete and tagged.**
**Blocking research gate: SR-2 (Alpenglow commitment semantics).**

> **The hardest correctness phase.** It is placed before any protocol decoding because decoding a slot
> whose place in the chain is unknown is meaningless — and because a fork model retrofitted after
> materialization exists is a rewrite.

## 1. Scope

1. The canonical chain: `slots` keyed `(slot, blockhash)`, with parent links and a computed `canonical`
   flag.
2. **Commitment promotion**: evidence-driven, monotonic, idempotent, with derived rows promoted in the
   same transaction as their slot.
3. **Fork detection**: a recorded block not on the finalized chain.
4. **Rollback**: a single transaction that marks abandoned, records the event, invalidates derived
   rows, and enqueues scoped recompute.
5. **Recompute**: rebuild forward from the last **finalized** anchor, using the same materialization
   code as normal operation.
6. `materialization_state` watermarks per entity.
7. The chain-state advisory lock serializing promotion against rollback.
8. `rollback_events` and the depth metric.

## 2. Explicit non-scope

No protocol decoding, no Aegis, no risk, no execution. The entities recompute operates on are the
normalized ones; the protocol-entity recompute hook is defined here and **used** in Phase 7.

## 3. Evidence objective

- **A fork of any tested depth produces the same final state as if the abandoned blocks had never been
  observed** (RP-06). This is the single strongest statement Sentinel can make about reorg handling.
- Commitment is monotonic under every arrival order.
- **Nothing is ever deleted** by a rollback.

## 4. Files

`crates/sentinel-chainstate/src/{chain,promotion,fork,rollback,recompute,watermark}.rs`

## 5. Dependencies

Phases 1–5. **SR-2 is blocking**: whether `confirmed` retains its meaning under Votor and whether RPC
exposes any new finality surface must be answered from a primary source before the promotion logic is
finalized. If the answer is not obtainable, that is a **finding to report**, not something to assume.

## 6. Implementation requirements — do not deviate

- **`PRIMARY KEY (slot, blockhash)`**. Never collapse to `slot` — that would force a lossy choice at
  insert time and destroy the evidence fork resolution needs.
- **Promotion is evidence-driven, never timer-driven** (P-2). A slot stuck at `confirmed` past a
  configured deadline is an **alert**, not an automatic promotion.
- **Promotion is monotonic.** The only non-monotonic transition is to `abandoned`.
- **Rollback is one transaction.** Partial rollback state is the worst possible state.
- **Rebuild forward from a finalized anchor. Never subtract, never invert events** (RB-3). Inverting
  deltas is how sign errors get into a ledger.
- **Nothing is deleted.** Raw is immutable; normalized rows are marked.
- Affected entities are visibly `RECOMPUTING` with their last known-good slot. Never served silently
  stale.
- **A block whose parent is unknown is a missing ancestor, not a fork.** Backfill the ancestor first,
  then re-evaluate. Concluding "fork" from a gap is a classic and expensive mistake.
- Promotion and rollback are serialized by the same advisory lock (RB-6).
- **`CI-NOSLOTTIME`**: no duration anywhere is computed from a slot count.

## 7. Tests

**Unit:** parent-link chain walking; canonical computation; promotion transition table including every
illegal transition; abandonment set computation.

**Property:** `P-MONO-1` (commitment monotonicity under random promotion orders); `CHN-02` (every
canonical slot reachable from the finalized head).

**Integration:** promotion through `observed → confirmed → finalized` on a live local cluster;
watermark advance; recompute of a normalized entity.

## 8. Adversarial / failure cases

| ID | Case | Asserted |
|---|---|---|
| FI-16 | Injected forks of depth **1, 5, and 30** | Rollback fires; scoped recompute runs; final state matches a corpus where the abandoned blocks were never observed |
| RP-06 | Replay over a fork-containing corpus | Digest matches the no-fork-observed corpus |
| CHN-08 | Promotion and rollback attempted concurrently | Serialized by the lock; no interleaving; both complete correctly |
| — | Kill the process **mid-rollback** | The transaction either committed or did not; no partial state; recompute jobs are idempotent |
| — | Two blockhashes observed for one slot | Both stored; finality resolves; the loser is abandoned |
| — | A block arrives whose parent is unknown | Ancestor backfill is enqueued; **no fork is declared** |
| — | A slot marked skipped later receives a block | Rejected without an explicit correction event (CHN-09) |
| — | Finality stalls | `StalledFinalization` alert fires; nothing is promoted on a timer |
| — | An abandoned slot's transaction is later observed in a canonical slot | Handled as a normal late observation; the entity re-materializes from its anchor |

## 9. Acceptance criteria

- [ ] **SR-2 closed** — or, if it cannot be closed from a primary source, explicitly reported as
      unresolved with the conservative behavior chosen and why
- [ ] `CHN-01..CHN-10` all proven by test
- [ ] FI-16 passes at depths 1, 5, and 30
- [ ] RP-06 passes: fork replay equals never-observed replay
- [ ] A mid-rollback kill leaves no partial state
- [ ] No slot-derived duration anywhere (`CI-NOSLOTTIME`)
- [ ] `RECOMPUTING` entities are visibly labelled with their last known-good slot
- [ ] Universal checklist satisfied. Tag `phase-06-chainstate`.

## 10. Demo

Drive a fork on the local cluster (or via the fixture harness); watch the rollback event, the
abandonment marks, the recompute, and the restored state — with rollback depth visible in Grafana.
Then show that the raw rows for the abandoned blocks are still present and queryable.

## 11. Documentation & status updates

`finality-and-forks.md` updated only via ADR if implementation revealed a genuine problem. If SR-2's
answer changes the model, that is an **ADR**, not an edit. `project-status.md`: chain state
IMPLEMENTED + TESTED + DEMOED; fork-injection results with real output; SR-2 status.

## 12. Stop condition

**STOP after this phase.** Phase 7 has not been started.
