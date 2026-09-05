# Phase 2 — Canonical Data Model & Migrations

**Status: NOT STARTED.** **Prerequisite: Phase 1 complete and tagged.**

> The schema **is** the contract (`architecture.md` §8). Every idempotency guarantee in this system is
> a database constraint, so this phase is where most of the correctness is actually decided.

## 1. Scope

1. Every table in `data-model.md`, with its exact primary key, natural key, uniqueness constraints,
   indexes, and check constraints.
2. **Slot-range partitioning** for `raw_observations`, `transactions`, `instructions`, `program_logs`,
   `account_observations`, `token_balance_deltas`, with automated ahead-of-head partition creation.
3. Least-privilege **database roles per service**, with the TypeScript role having **no write grant**
   on raw, normalized, chain-state, protocol, or derived tables.
4. Typed row structs and a `sqlx` access layer in `sentinel-db` for every table.
5. The `jobs` table with claim/renew/release/quarantine queries and `LISTEN/NOTIFY` wake plus a polling
   floor.
6. A **schema-ownership test**: each table declares its owning writer, and a test asserts no other role
   has write permission.
7. A **natural-key audit test**: every table declared to have a natural key has a matching unique index.

## 2. Explicit non-scope

No ingestion, no RPC, no decoding, no API. **No `aegis_*` table content logic** — the tables exist with
their constraints; nothing writes them yet. No triggers. No stored procedures. No ORM.

## 3. Evidence objective

That idempotency and monotonicity are **enforced by the database**, not by application discipline —
demonstrated by tests that attempt the violation and are rejected by a constraint.

## 4. Files

`infra/migrations/*.sql` · `crates/sentinel-db/src/{tables,queries,partitions}.rs` ·
`crates/sentinel-jobs/src/*` · `ts/packages/db/*`

## 5. Dependencies

Phase 1. **SR-11** (LISTEN/NOTIFY throughput) is measured here at a smoke level and fully in Phase 14.

## 6. Implementation requirements — do not deviate

- **`numeric(39,0)` for `u128`, `numeric(20,0)` for `u64`.** No floating-point type in any column, ever.
- `raw_observations` is **append-only**: the writer role has `INSERT` and `SELECT` only. **No role used
  by application code has `UPDATE` or `DELETE` on it.**
- Monotonicity guards are **in the schema and in the query**: a `CHECK` where expressible, plus the
  `WHERE excluded.as_of_slot > current.as_of_slot` clause on every materialization upsert.
- Partial unique indexes exactly as specified: one open alert per `(kind, entity)`; one outstanding job
  per `dedupe_key`; one non-terminal attempt per `intent_id`.
- `execution_intents.idempotency_key` is `UNIQUE` **globally**, not per-kind.
- Migrations are **forward-only** and numbered. No down-migrations.
- Partition creation is automated with a stated lead time, and running low is an alertable condition.
- Every index is justified in a comment naming the query it serves. An unjustified index is deleted.

## 7. Tests

**Unit / integration (real Postgres):**
- Every table: insert, conflict behavior, and constraint violation each produce the documented outcome.
- `ON CONFLICT DO NOTHING` on every immutable table: a second identical insert changes nothing.
- Monotonicity: an out-of-order `as_of_slot` upsert is **rejected**, not applied.
- Partial unique indexes: a second open alert / outstanding job / non-terminal attempt is rejected.
- `idempotency_key` collision is rejected.
- Job queue: concurrent claim by N workers yields **disjoint** job sets; lease expiry allows re-claim; a
  lease-conditioned update by a stale holder affects **zero** rows.
- Partition routing: a row lands in the correct partition; a missing partition produces a clear error.
- Migration: up-from-empty; re-run is a no-op.
- **Role permissions**: each service role can write only its own tables — asserted, not assumed.
- `numeric(39,0)` round-trips a full `u128` maximum without loss.

**Property:** `P-KEY-1` (distinct logical rows never collide on a natural key); `P-MONO-2`.

## 8. Adversarial / failure cases

- Attempt `UPDATE`/`DELETE` on `raw_observations` as every application role → **permission denied**.
- Attempt a write from the TypeScript role to a normalized table → **permission denied**.
- Concurrent job claim by 16 workers → no job claimed twice.
- Stale lease holder attempts to complete a job → zero rows affected, detected.
- Insert a `u128` maximum → exact.
- Insert a value exceeding `numeric(39,0)` → rejected, not truncated.

## 9. Acceptance criteria

- [ ] Every table from `data-model.md` exists with its documented PK, natural key, and indexes
- [ ] `DM-02` proven: no application role can update or delete raw rows
- [ ] `DM-03` proven: the natural-key audit test passes for every table
- [ ] `DM-04` proven: a stale materialization write is rejected
- [ ] `DM-07` proven: no floating-point column exists (schema type audit)
- [ ] `DM-08`, `DM-09`, `DM-10`, `TX-02` partial unique indexes proven by attempted violation
- [ ] Job queue concurrency test passes with 16 workers
- [ ] Partition automation creates partitions ahead of the head and alerts when low
- [ ] Migration up-from-empty and re-run both clean
- [ ] Universal checklist satisfied. Tag `phase-02-data-model`.

## 10. Demo

`make migrate` on an empty database, then a script that attempts each forbidden operation and shows the
database refusing it — the schema defending itself.

## 11. Documentation & status updates

`data-model.md` updated **only** if implementation revealed a genuine problem, and then via an ADR.
`project-status.md`: schema IMPLEMENTED + TESTED; SR-11 partially closed with a measured
`LISTEN/NOTIFY` smoke figure.

## 12. Stop condition

**STOP after this phase.** Phase 3 has not been started.
