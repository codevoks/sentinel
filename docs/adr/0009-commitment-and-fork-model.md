# ADR-0009 — Explicit commitment model; `processed` is never persisted

**Status:** Accepted · **Date:** 2026-09-05 · **Phase:** 0 · **Revisit on SR-2 (Alpenglow)**

## Context

Solana exposes three commitment levels. `confirmed` blocks can be reverted; `finalized` blocks cannot.
An off-chain observer that does not model this is not wrong occasionally — it is wrong *silently*, and
it presents provisional state as fact.

Timing is also in flux: Alpenglow (Agave 4.3, activation beginning 2026-09-28) replaces TowerBFT with
Votor and targets ~150ms finality versus ~12.8s. Any design that hardcodes a finality duration is
already wrong.

## Decision

| Level | Persisted? | Drives derived state? | Exposed? | Drives execution? |
|---|---|---|---|---|
| `processed` | **No canonical row.** Ephemeral hint only | **No** | Only on an explicitly ephemeral channel | Only as a **trigger to re-evaluate** |
| `confirmed` | **Yes**, labelled, revisable | **Yes**, labelled | Yes, labelled | **Yes** |
| `finalized` | Yes, promoted in place | Yes, promotes | Yes, labelled | Yes — the only level at which historical facts are asserted |

Supporting decisions:

1. **The canonical chain is explicit**, keyed `(slot, blockhash)` — not `(slot)`. Two blocks may be
   observed for one slot from providers on different forks, or from an equivocating leader. Keying on
   slot alone forces a lossy choice at insert time; keying on both records the evidence and lets
   finality decide.
2. **Promotion is monotonic and evidence-driven.** Never a timer. Never lowered.
3. **Rollback marks, never deletes.** Raw is immutable; normalized rows are marked abandoned.
4. **Recompute rebuilds forward from a finalized anchor.** It never subtracts or inverts events —
   inverting deltas is how sign errors get into a ledger.
5. **Every row and every API field carries its commitment.** A value without one is a bug.
6. **Finality *timing* is configuration**, measured, never a constant.

## Why `processed` is not persisted

Persisting it would place rows in the canonical store corresponding to blocks that may never have
existed on the winning chain, and would run the rollback machinery constantly for no benefit. The only
thing `processed` genuinely buys is latency, and latency matters only to the keeper — which
re-evaluates against `confirmed` state before acting anyway.

**`processed` is a wake-up signal, not a fact.**

## Why the keeper acts on `confirmed`

Waiting for finality before creating a candidate would lose every liquidation race, making the keeper
pointless. Acting on `confirmed` is safe because:

- **The chain re-validates everything.** Aegis checks `HF < WAD` itself; a candidate based on a reverted
  block simply fails on-chain as a race.
- The intent's idempotency key prevents a duplicate after a revert.
- The cost of being wrong is one failed transaction's fees, accounted for in the budget.

**The trade is stated explicitly and measured** (`fork_wasted_attempts`), rather than assumed.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **Only ingest `finalized`** | Simplest and safest, and it makes the keeper useless — detection would lag finality by the whole finality window. Also loses the ability to show users near-real-time state. |
| **Persist `processed` too** | Constant rollback churn, canonical rows for blocks that never existed, no benefit the ephemeral hint does not already provide. |
| **Key `slots` on `slot` alone** | Forces a lossy choice when two blocks are observed for one slot, destroying the evidence the fork resolution needs. |
| **Delete rows on rollback** | Loses the forensic record of what Sentinel believed and when — which is precisely what is needed to debug a fork-handling bug. |
| **Unwind by inverting events** | Sign errors, ordering dependencies, and no way to verify the result. Rebuilding forward from a finalized anchor is verifiable by digest comparison. |
| **A fixed "N slots = finalized" heuristic** | Wrong before Alpenglow, more wrong after, and slot durations are themselves changing (SIMD-0525). |
| **Serve `confirmed` data without a label** | The core dishonesty this ADR exists to prevent. |

## Consequences

**Positive**
- Provisional state is never presented as final, anywhere, by construction.
- Reorgs are handled deterministically and are testable (FI-16 at depths 1, 5, 30).
- The keeper is competitive without being unsafe.
- Alpenglow's timing change is a configuration change, not a redesign.

**Negative**
- Every derived row carries commitment metadata and may be revised. Real complexity, and it is the
  price of not lying.
- The keeper occasionally acts on state that is later reverted, wasting fees. Bounded, measured, and
  budgeted.
- Two-phase promotion means every derived table needs a promotion path. Uniform and testable.

**Open**
- **SR-2:** whether `confirmed` retains its meaning under Votor, and whether RPC exposes any new
  finality surface. Blocking for Phase 6; re-checked in Phase 12.
