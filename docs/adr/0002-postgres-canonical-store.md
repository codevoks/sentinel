# ADR-0002 — PostgreSQL as the only canonical store

**Status:** Accepted · **Date:** 2026-09-05 · **Phase:** 0

## Context

Sentinel holds append-heavy observational data, mutable materialized state, and an execution audit
trail with strict uniqueness requirements. The temptation is to reach for a specialized store per
workload — a time-series database for observations, a key-value store for current state, an object
store for raw payloads.

## Decision

**PostgreSQL is the only canonical store.** Every layer — raw, normalized, chain state, protocol,
derived, execution, and the job queue — lives in it.

Supporting decisions:
- **Range partitioning on `slot`** for the append-heavy tables, so archival is a partition detach
  rather than a mass delete.
- **`numeric(39,0)` for `u128` and `numeric(20,0)` for `u64`.** Exact. No floating-point type appears
  in any schema.
- **One writer per table**, enforced by distinct least-privilege database roles in every environment
  including local (`data-model.md` §10).
- Migrations are forward-only, numbered, and additive by default.

## Why one store

1. **Transactional enqueue.** A candidate and its execution intent are created in **one transaction**.
   No message can be lost between them, and none can be delivered twice. Every alternative topology
   reintroduces that partial-failure class.
2. **Uniqueness is the correctness mechanism.** `execution_intents.idempotency_key UNIQUE` is what
   prevents a duplicate liquidation. A database that enforces it is not a convenience — it is the
   safety property.
3. **Materialization needs a monotonicity guard** (`WHERE excluded.as_of_slot > current.as_of_slot`).
   That is an atomic conditional upsert, which key-value stores emulate awkwardly and usually racily.
4. **Replay needs `TRUNCATE` + rebuild with referential structure intact.** Cross-store replay means
   coordinating truncation across systems that cannot participate in one transaction.
5. **The volume does not require anything else.** Sentinel indexes one program's activity, not the
   whole chain. Choosing a distributed store for a single-node workload is architecture theatre.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **ClickHouse / TimescaleDB for observations, Postgres for state** | Two stores, no cross-store transaction, and replay must coordinate truncation across both. The analytical queries Sentinel actually needs are served by partitioned Postgres with correct indexes. Threshold for revisiting is stated in `data-model.md` §9. |
| **S3/object storage for raw payloads** | Would break the "insert raw and advance the checkpoint in one transaction" property (ING-03), which is the entire crash-safety argument for ingestion. Object storage is the right *archive* target and is exactly what partition detach feeds. |
| **Redis or another KV for current materialized state** | A second source of truth for protocol state, with its own invalidation bugs, and no conditional-upsert guarantee. ADR-0003. |
| **An embedded store (RocksDB/SQLite) in the indexer** | Would make the TypeScript half unable to read chain state without a service boundary, and would lose Postgres's concurrency and query surface. |
| **A distributed SQL database (CockroachDB/Yugabyte)** | Solves multi-region availability, which Sentinel does not have. Adds latency and operational surface for nothing. |
| **Event-sourcing into an append-only log with projections in a separate store** | Sentinel *is* event-sourced — the raw layer is the log. Putting the log in a different system adds a coordination boundary without adding a property. |

## Consequences

**Positive**
- Transactional handoffs everywhere; no lost or duplicated messages between components.
- Uniqueness constraints do the safety work, rather than application discipline.
- One backup, one restore, one operational story.
- Replay is a `TRUNCATE` plus a rebuild inside one system.

**Negative**
- **Postgres is a single point of failure.** Accepted: it is also a single point of *durability*, and
  every worker resumes from durable state when it returns. No data is lost, only availability.
- Write amplification from the raw boundary lands on one system. Mitigated by partitioning, batching,
  and a stated retention policy; measured in Phase 14 (hypothesis B-1).
- Analytical queries over long ranges will eventually need care. Mitigated by partition pruning and
  by a stated adoption threshold for a columnar store.
- Vertical scaling is the only scaling story for the primary. Accepted at this scale; a read replica
  has a stated threshold.

## Adoption thresholds for revisiting

`performance-strategy.md` §6. Specifically: a columnar store only when the raw hot window exceeds the
disk budget at minimum useful retention, **or** a required analytical query misses its target with
correct indexes and partition pruning already in place. Both must be shown with numbers.
