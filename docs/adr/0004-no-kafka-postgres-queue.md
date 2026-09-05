# ADR-0004 — No message broker; a PostgreSQL-backed job table

**Status:** Accepted · **Date:** 2026-09-05 · **Phase:** 0

## Context

Sentinel has genuine queueing needs: backfill ranges, replay ranges, rematerialization jobs, scheduled
scans, and the handoff from risk detection to transaction execution. The reflexive answer is Kafka, or
at least a broker.

The precise question is not "do we need a queue?" — we do — but **"do we need a broker?"**

## Decision

**No message broker in v1.** Queueing is a PostgreSQL table:

```sql
SELECT ... FROM jobs
 WHERE state='queued' AND available_at <= now()
 ORDER BY priority, job_id
 FOR UPDATE SKIP LOCKED LIMIT $n;
```

with `LISTEN/NOTIFY` for wake latency, a **polling floor** so a missed notification costs latency and
never correctness, leases with expiry, bounded attempts, and a `quarantined` terminal state.

The risk→execution handoff is the same mechanism: `execution_intents` claimed with
`FOR UPDATE SKIP LOCKED` (`architecture.md` §3.4).

## Why this is better here, not merely cheaper

1. **Transactional enqueue.** A liquidation candidate and its execution intent are created in **one
   transaction**. With a broker, the enqueue and the state change cannot be atomic — you get the
   outbox pattern, which is a job table with extra steps, or you accept a lost/duplicated message class
   that this system specifically cannot afford.
2. **Uniqueness is enforceable.** A partial unique index on `dedupe_key WHERE state IN
   ('queued','leased')` makes duplicate enqueue a no-op. Broker-side deduplication is either absent or
   best-effort.
3. **The queue is queryable.** "Why is this range not processed?" is a `SELECT`. Debugging a stuck
   broker partition is not.
4. **Quarantine beats a DLQ system.** A `quarantined` state gives isolation, retention, and replay —
   every property a dead-letter queue provides — with none of the operational surface. The *semantics*
   matter; the infrastructure does not.
5. **Ordering is not a broker's job here.** Sentinel's ordering comes from `(slot, transaction_index,
   ix_index)` **in the data**. A partitioned log's ordering guarantee would be redundant with what the
   data already provides.
6. **The volume does not justify it.** Sentinel processes one program's activity. Kafka's operational
   cost is real: partition planning, retention, consumer-group management, rebalancing, and a whole
   second durability model to reason about.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **Kafka / Redpanda** | Loses transactional enqueue; adds a second durability model, partition/retention/rebalance operations, and a second replay story. Real threshold for adoption stated below. |
| **NATS JetStream** | Lighter than Kafka, same fundamental loss of transactional enqueue with the state change. |
| **Redis Streams / lists** | ADR-0003 — nothing durable in Redis, and the same enqueue-atomicity loss. |
| **RabbitMQ** | Broker operations for at-most-a-few-thousand jobs an hour. |
| **The outbox pattern with a broker** | Requires a job table anyway, then adds a relay process and a broker. Strictly more moving parts for the same guarantee. |
| **In-process channels only** | No durability. A crash loses work, which contradicts DC-2. |

## Consequences

**Positive**
- Enqueue is atomic with the state change that caused it. This is the property everything else rests on.
- One durability model, one backup, one operational story.
- The queue is inspectable with SQL, including in production incidents.
- Local development and CI need nothing extra.

**Negative**
- **Queue throughput is bounded by Postgres.** `SKIP LOCKED` scales to thousands of jobs per second on
  modest hardware, which is far above Sentinel's need — but it is a real ceiling, and it is the thing
  Phase 14 measures (hypothesis B-4).
- Job-table churn produces dead tuples. Mitigated by pruning terminal jobs and by autovacuum tuning;
  `db_bloat_ratio` is a monitored metric.
- `LISTEN/NOTIFY` has payload limits and connection costs. Mitigated by using it purely as a **wake
  signal**, never as a data channel, with a polling floor beneath it. SR-11 verifies throughput.
- No cross-datacenter fan-out. Not required.

## Adoption threshold for a broker

Stated as a measurement, so this decision can be revisited honestly rather than by preference. Adopt a
durable log when **any** of these is demonstrated in a Phase 14 measurement:

1. Sustained job dispatch rate at which Postgres-backed claiming is **measurably the bottleneck**, with
   `lock_wait_p95` and dispatch throughput as evidence;
2. **≥3 independent consumer groups** each needing independent replay of the same firehose — the case a
   log genuinely serves and a job table does not;
3. A cross-datacenter or multi-region fan-out requirement.

None of these is present today, and adopting a broker before one is would be exactly the CV-driven
architecture `AGENTS.md` §14 forbids.
