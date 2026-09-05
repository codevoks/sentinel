# ADR-0003 — Redis is optional and never canonical

**Status:** Accepted · **Date:** 2026-09-05 · **Phase:** 0

## Context

Redis is the reflexive addition to any pipeline, and it is almost always justified after the fact. The
honest question for Sentinel is: **is it necessary at all?**

Answered directly: **for correctness, no.** Not one correctness property in this repository depends on
it.

## Decision

Redis is **optional**. It is permitted in exactly three roles, all ephemeral:

| Role | Why Redis | What happens without it |
|---|---|---|
| **WebSocket fanout across API instances** | A message committed by a worker must reach clients connected to any instance. Pub/sub is the right primitive. | In-process bus; fanout works within a single API instance. Degraded, not broken. |
| **Rate limiting** | Shared counters across instances | Per-instance in-memory limits. Weaker, never absent. |
| **`processed`-commitment hints** | Short-TTL "market M may have changed" signals that are explicitly **not facts** (`finality-and-forks.md` §1.1) | Hints are skipped; the keeper re-evaluates on `confirmed` state, which it does anyway. Slightly higher latency, identical correctness. |

**Hard requirement: Sentinel must run correctly with Redis absent**, in a documented degraded mode.
This is verified continuously by FI-13, which removes Redis entirely and asserts no correctness
behavior changes.

## Rules

| # | Rule |
|---|---|
| R-1 | **No durable state in Redis.** Ever. No queue, no checkpoint, no materialized value, no lock that guards a money-moving action. |
| R-2 | Every Redis value has a **TTL**. A value without one is a bug. |
| R-3 | Redis is behind a trait with a **working in-process implementation**. Both are exercised in CI. |
| R-4 | Losing Redis is a **degradation, never an outage**. `sentinel_redis_available` is a gauge; the alert is a ticket, not a page. |
| R-5 | Nothing authorization-dependent is cached in Redis (S-22). |

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **No Redis at all** | Genuinely tempting, and correct for a single-instance deployment. Rejected only because cross-instance WebSocket fanout is a real requirement for a horizontally-scaled API and Postgres `LISTEN/NOTIFY` is the wrong tool for high-frequency fanout (payload limits, connection-per-listener cost). The three roles are narrow and the fallback is real. |
| **Redis as the job queue** | Loses transactional enqueue — the single most valuable property of the Postgres job table (ADR-0004). A job could be enqueued for a state change that then rolls back, or vice versa. |
| **Redis as a materialized-state cache** | A second source of truth for protocol state, with invalidation bugs, in a system whose entire subject is not being wrong about state. |
| **Redis as a distributed lock for the indexer singleton** | Postgres advisory locks are released automatically when the connection dies. A Redis lock needs TTL tuning and has a well-known correctness debate. And correctness rests on idempotency regardless (`distributed-correctness.md` §3.1). |
| **Redis Streams instead of the job table** | Same transactional-enqueue loss, plus a second durability model to reason about. |
| **NATS / a lighter broker for fanout** | Another dependency for the same three roles, with no advantage. |

## Consequences

**Positive**
- Redis can be removed at any time; the system keeps working. That is testable and tested.
- No cache-invalidation class of bug, because nothing authoritative is cached.
- Local development and CI can run without it.

**Negative**
- Two fanout implementations to maintain (Redis and in-process). Accepted: the in-process one is small
  and is what keeps the "optional" claim honest.
- Single-instance rate limiting is weaker without Redis. Documented, and acceptable for the deployment
  shape Sentinel targets.

**Enforcement**
- FI-13 removes Redis and asserts unchanged correctness behavior.
- `A-CACHE-02` asserts removing Redis changes no response **body**.
- Adding a fourth Redis role requires an ADR arguing against this one.
