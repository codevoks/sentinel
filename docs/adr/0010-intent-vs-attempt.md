# ADR-0010 — Business intent is separate from transaction attempt

**Status:** Accepted · **Date:** 2026-09-05 · **Phase:** 0

## Context

The naive execution model is one database row per transaction. It breaks the moment anything real
happens: a blockhash expires, an RPC times out ambiguously, a provider fails over, a reorg abandons an
observed transaction, or a fee needs escalating. Every one of those produces a **new transaction** for
the **same business action**, and a one-row model has no way to say that.

The consequence of conflating them is the worst failure this system can have: **an economically
sensitive operation executing twice.**

## Decision

Two objects, two identities, two lifetimes.

| | **Execution intent** | **Transaction attempt** |
|---|---|---|
| Answers | "Should this business action happen, and did it?" | "What happened to these signed bytes?" |
| Identity | `idempotency_key` (business) | `signature` (chain) |
| Cardinality | 1 | 0..N per intent |
| Terminates on | Success, failure, expiry, cancellation, operator | Observation or `lastValidBlockHeight` expiry |

Supporting decisions:

1. **The idempotency key uses a coarse time bucket, not the trigger slot.**
   `liquidate:{program}:{market}:{position}:{trigger_epoch_bucket}`. Using the slot would permit a new
   intent every slot for the same unhealthy position — which is exactly the duplicate hazard. A bucket
   allows at most one intent per position per bucket, and legitimately allows a new one if the position
   is still unhealthy in the next bucket.
2. **At most one non-terminal attempt per intent**, enforced by a partial unique index, not by code
   discipline.
3. **Three independent budgets** — `max_attempts`, `cumulative_fee_lamports`, `expires_at`. Exhausting
   any one terminates the intent.
4. **`NEEDS_OPERATOR` is a real state.** When the automated resolver cannot prove what happened, a human
   looks. Creating another attempt under genuine ambiguity is the one thing this design exists to
   prevent.
5. **A retry re-plans; it never re-signs stale sizing.** Only a transport error resubmits the *same
   stored bytes*.

## The crash-safety corollary

The separation only pays off with the right write ordering:

```
build → simulate → policy → sign → PERSIST+COMMIT(signature, bytes) → submit
```

Because the signature and the exact bytes are durable **before** broadcast, the dangerous state
— "something was submitted and Sentinel does not know what" — is unreachable. On restart, a `SIGNED`
attempt is resolved by **resubmitting the stored bytes**, which produces an identical signature and is
therefore a no-op on-chain if it already landed.

Storing the *bytes*, not just the signature, is what makes that work: re-signing would produce a new
signature and a genuine duplicate.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **One row per transaction** | Cannot express "the same business action, retried". Every retry becomes indistinguishable from a new action. |
| **Idempotency keyed on the trigger slot** | A new intent every slot for a persistently unhealthy position — a duplicate-execution generator. |
| **Idempotency keyed on the position alone, forever** | A position liquidated today could never be liquidated again, which is wrong: positions become unhealthy repeatedly. |
| **Rely on the chain's duplicate-signature rejection alone** | Only works for *identical* bytes. A re-planned retry has a different blockhash and different sizing, so the chain sees two distinct transactions and would happily execute both. |
| **Submit first, persist after** | Creates exactly the unattributable-in-flight-transaction state that FR-17 exists to eliminate. |
| **Use a durable nonce to avoid expiry** | Removes `lastValidBlockHeight` as the termination oracle, converting a bounded ambiguity into an unbounded one around a value-moving transaction. Analyzed in `transaction-engine.md` §4.6, with the conditions under which it *would* be right. |
| **Distributed lock instead of a uniqueness constraint** | A lock is a hope with a TTL; a unique index is a guarantee. |

## Consequences

**Positive**
- Duplicate execution is prevented by a database constraint plus a write ordering, not by discipline.
- Retry, failover, fee escalation, and reorg handling all become attempt-level concerns that cannot
  affect business identity.
- `/v1/intents/{id}` gives a complete, auditable timeline — the most distinctive screen in the product.
- API-level `Idempotency-Key` maps onto the same mechanism, so the two cannot disagree.

**Negative**
- Two state machines to specify and test. Both are fully enumerated in `transaction-engine.md` §2–3 and
  property-tested for legality and termination.
- The time-bucket parameter is a real tuning decision: too coarse delays a legitimate re-attempt, too
  fine approaches per-slot behavior. Configured, documented, and measured.
- Storing full transaction bytes costs space. Trivial next to what it guarantees.

**Enforcement**
- TX-01..TX-12; FI-03, FI-04, FI-05 (crash at each boundary) run in the **required** CI tier, not
  nightly — this is the guarantee that most needs continuous proof.
