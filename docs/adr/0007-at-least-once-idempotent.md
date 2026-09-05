# ADR-0007 — At-least-once observation, effect-once processing

**Status:** Accepted · **Date:** 2026-09-05 · **Phase:** 0

## Context

Sentinel receives the same observation more than once as a matter of routine: reconnects re-deliver,
backfill overlaps realtime, multiple providers return the same block, and a reorg re-observes a
transaction in a later slot. It also *loses* observations: the native PubSub drops under load.

A design that assumes exactly-once delivery is not merely optimistic — it is wrong on the first
reconnect, and its failure is silent.

## Decision

**Delivery is assumed at-least-once and lossy. Processing is required to be effect-once.**

Three layers of idempotency, deliberately redundant:

| Layer | Guards against | Mechanism |
|---|---|---|
| **Row** | Duplicate delivery | `UNIQUE (natural key)` + `ON CONFLICT DO NOTHING` |
| **State** | A stale writer overwriting newer state | `ON CONFLICT DO UPDATE ... WHERE excluded.as_of_slot > current.as_of_slot` |
| **Effect** | A business action happening twice | `execution_intents.idempotency_key UNIQUE` + sign→persist→submit ordering |

Supporting rules:
- **Every write states its natural key and its conflict policy.** A consumer that cannot is not
  finished (`ingestion-model.md` §9).
- **Materialization is a fold over an idempotent event set at a pinned anchor**, never a sequence of
  increments. Increment-based materialization is duplicate-sensitive by construction and is banned.
- Completeness is achieved by **slot-range reconciliation**, not by trusting delivery.
- A `CONFLICT` is a **success**, not an error — it is the idempotency mechanism working
  (`architecture.md` §9).

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **Assume exactly-once delivery** | Wrong on the first reconnect, and wrong silently. There is no exactly-once delivery over a reconnecting socket to a node that drops messages. |
| **Deduplicate with an in-memory seen-set** | Lost on restart, unbounded in memory, and useless across processes. The database already has the constraint. |
| **A dedup service / bloom filter tier** | Approximate deduplication for a problem where a unique index is exact and free. |
| **Sequence numbers from the provider** | Providers do not offer a globally consistent sequence, and a Geyser `write_version` is per-account and per-source. Natural keys derived from chain data work across every source. |
| **Application-level "have I seen this?" checks before insert** | A read-then-write race. `ON CONFLICT` is atomic; a pre-check is not. |
| **Increment-based materialization with duplicate detection** | Puts the correctness burden on never mis-detecting a duplicate. The fold formulation removes the burden entirely. |

## Consequences

**Positive**
- Reconnects, provider fan-out, backfill overlap, and reorg re-observation are all **free**. They cost
  work, never correctness.
- Backfill and realtime can share one code path (`replay-and-backfill.md` §9, BF-1).
- Replay determinism follows directly: replaying the same rows produces the same result.
- The system can prefer a **recorded, repairable gap over unbounded memory** under backpressure, which
  is only a safe preference because the reconciliation path exists.

**Negative**
- Every table needs a genuine natural key, which is real design work per table
  (`data-model.md`). That is a feature — a table without one has an undefined duplicate policy.
- Uniqueness indexes cost write throughput. Measured in Phase 14 (hypothesis B-3).
- Duplicate work is done and discarded — an efficiency cost paid for a correctness guarantee.

**Enforcement**
- ING-04 (duplicate delivery → no second row, no second effect), ING-08 (backfill/realtime overlap →
  zero duplicates), P-IDEM-1..3, FI-07, DM-03 (a test asserts a unique index exists per declared
  natural key).
