# ADR-0015 — `raw_observations` primary key becomes `(observation_id, slot)`

**Status:** Accepted · **Date:** 2026-09-18 · **Phase:** 2

## Context

`docs/data-model.md` §2 declares, for `raw_observations`, both:

- **PK:** `observation_id bigserial`
- **Partitioning:** `Range on slot, 10M-slot partitions`

in the same table specification. PostgreSQL requires that **every unique constraint on a partitioned
table — including the primary key — include the partition key as one of its columns**. This is not a
configuration option; it is enforced at `CREATE TABLE ... PARTITION BY RANGE` time and cannot be
disabled. A bare `PRIMARY KEY (observation_id)` on a table partitioned by `slot` is rejected by
PostgreSQL outright (`ERROR: unique constraint on partitioned table must include all partitioning
columns`).

So the two clauses of the frozen table spec, read together, describe a table PostgreSQL cannot create.
This was only discovered while implementing the migration, not while reading the document in isolation.

## Decision

`raw_observations`' primary key is `(observation_id, slot)`, not `observation_id` alone.

This is a **mechanical accommodation, not a semantic change**:

- `observation_id` is still `GENERATED ALWAYS AS IDENTITY`, globally unique across every partition (the
  identity sequence is shared, not per-partition), and monotonically increasing. Every property a caller
  relies on when treating `observation_id` as "the row's stable identifier" still holds.
- The natural key stays exactly `(kind, natural_key, payload_hash)` per `ingestion-model.md` §5 — `slot`
  is folded into the unique index backing it only because Postgres requires the partition column in that
  index too, and `slot` is already embedded in `natural_key` for every observation kind (`ingestion-model.md`
  §5.1), so no real key width is added.
- No query changes shape: any lookup by `observation_id` alone still returns at most one row (Postgres
  cannot enforce that as a *constraint* without `slot`, but it remains true in practice because the
  identity sequence never repeats).

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Keep `PRIMARY KEY (observation_id)`, drop partitioning | Directly contradicts `data-model.md` §2's explicit partitioning requirement and the phase-02 spec's mandatory partition list, and loses the archival/detach story ADR-0002 depends on. |
| Make `observation_id` itself the partition key instead of `slot` | Defeats the entire purpose of slot-range partitioning: cheap detach-and-archive of old *chain history*, and partition pruning on the query pattern every consumer actually uses (`WHERE slot BETWEEN ...`). An identity-based partition would need to be range-partitioned by an ever-increasing surrogate that has no relationship to retention policy. |
| A separate non-partitioned "index" table mapping `observation_id -> slot` | A second source of truth for something derivable for free from the row itself; adds a join to every by-id lookup for no benefit. |

## Consequences

**Positive**
- The table builds and partitions correctly under real PostgreSQL 18, verified in this phase's migration
  run.
- `observation_id` keeps every property downstream code needs from it.

**Negative**
- Any future code doing `SELECT ... WHERE observation_id = $1` without `slot` still works (b-tree index
  on the identity value is efficient via the local per-partition indexes plus a lookup), but a
  `REFERENCES raw_observations (observation_id)` foreign key from another table is not possible — a
  referencing table must carry `slot` too and reference the composite key. `decode_failures` does this
  (see `0003_raw_layer.sql`).

## Note on scope

This does not touch `docs/data-model.md`'s primary contract — the natural key, the conflict policy, and
the partitioning strategy are all implemented exactly as specified. Only the PostgreSQL-mechanical
expression of the primary key changes, and only for the one table (`raw_observations`) whose declared
single-column PK is not jointly satisfiable with its own partitioning clause. The other five partitioned
tables (`transactions`, `instructions`, `program_logs`, `account_observations`, `token_balance_deltas`)
already declare `slot` as part of their primary key in `data-model.md`, so no equivalent ADR is needed
for them.
