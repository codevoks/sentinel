-- Phase 2 — least-privilege GRANTs. docs/data-model.md §10 (frozen ownership table).
--
-- Role-granularity note (documented here because it resolves an apparent
-- tension in the frozen documents, not because it is a redesign of them):
--
-- data-model.md §10 lists ownership by *logical Rust service* name
-- (sentinel-ingest, sentinel-normalize, sentinel-chainstate, sentinel-aegis,
-- sentinel-risk, sentinel-jobs) and by *logical TypeScript service* name
-- (sentinel-executor, sentinel-api). Phase 1's migration
-- (infra/migrations/0001_init.sql) already created exactly two application
-- login roles — `sentinel_rust` and `sentinel_ts` — one per LANGUAGE, not
-- one per logical service, and its own comment says Phase 2's job is to
-- "add the per-table GRANTs that make **these roles'** least privilege
-- real" (singular reference to the two already-created roles). data-model.md
-- §10's own closing sentence states the property this is actually for:
-- "The TypeScript service**s** hold **a role** [singular] with no write
-- grant on raw, normalized, chain-state, protocol, or derived tables. That
-- is what makes architecture.md §5's language boundary structural." That is
-- the Rust/TypeScript structural boundary ADR-0001 draws — not a
-- micro-service-per-role model, which nothing in Phase 1 provisioned and
-- which docs/architecture.md §6's six-process topology does not name
-- distinct Postgres roles for either.
--
-- This migration therefore implements data-model.md §10's ownership table
-- as GRANTs on the union of privileges each language-side role needs across
-- every logical service it hosts, and the schema-ownership test in Phase 2
-- (crates/sentinel-db/tests/role_permissions.rs) asserts the one property
-- that *is* structurally enforced: `sentinel_ts` cannot write raw,
-- normalized, chain-state, protocol, or derived tables, and neither role
-- can write a table it does not own (e.g. sentinel_rust cannot write
-- transaction_attempts; sentinel_ts cannot UPDATE-advance execution_intents
-- beyond its granted columns... it can, that is its job; but it cannot
-- INSERT-create on sentinel_rust's exclusive insert path being separately
-- privileged is not expressible in GRANT alone, see the execution_intents
-- grants below for the precise split that Postgres CAN express).
--
-- Should finer-grained per-logical-service roles ever be required (e.g. to
-- stop a compromised sentinel-normalize process from writing
-- aegis_markets), that is a real architectural change to Phase 1's role
-- model and needs its own ADR plus a new migration — it is out of Phase 2
-- scope to invent it silently.

-- ---------------------------------------------------------------------
-- sentinel_rust — union of sentinel-ingest, sentinel-normalize,
-- sentinel-chainstate, sentinel-aegis, sentinel-risk, sentinel-jobs,
-- sentinel-rpc (data-model.md §10 rows 1-5, 8, plus sentinel-rpc for
-- provider_health per data-model.md §4).
-- ---------------------------------------------------------------------

-- sentinel-ingest
GRANT INSERT, SELECT ON raw_observations TO sentinel_rust; -- DM-02: no UPDATE, no DELETE, ever.
GRANT INSERT, UPDATE, SELECT ON ingest_checkpoints TO sentinel_rust;
GRANT INSERT, UPDATE, SELECT ON gap_events TO sentinel_rust;

-- sentinel-normalize / any decoder (decode_failures)
GRANT INSERT, UPDATE, SELECT ON decode_failures TO sentinel_rust; -- DO UPDATE SET retry_count...
GRANT INSERT, UPDATE, SELECT ON transactions TO sentinel_rust; -- create (normalize) + commitment/canonical (chainstate)
GRANT INSERT, SELECT ON instructions TO sentinel_rust;
GRANT INSERT, SELECT ON program_logs TO sentinel_rust;
GRANT INSERT, SELECT ON account_observations TO sentinel_rust;
GRANT INSERT, SELECT ON token_balance_deltas TO sentinel_rust;

-- sentinel-chainstate
GRANT INSERT, UPDATE, SELECT ON slots TO sentinel_rust;
GRANT INSERT, SELECT ON rollback_events TO sentinel_rust;

-- sentinel-rpc
GRANT INSERT, UPDATE, SELECT ON provider_health TO sentinel_rust;

-- sentinel-aegis (decoder_versions: migrations + sentinel-aegis)
GRANT INSERT, SELECT ON decoder_versions TO sentinel_rust;
GRANT INSERT, UPDATE, SELECT ON aegis_protocol_state TO sentinel_rust;
GRANT INSERT, UPDATE, SELECT ON aegis_markets TO sentinel_rust;
GRANT INSERT, SELECT ON aegis_market_params_history TO sentinel_rust;
GRANT INSERT, UPDATE, SELECT ON aegis_positions TO sentinel_rust;
GRANT INSERT, SELECT ON aegis_events TO sentinel_rust;
GRANT INSERT, SELECT ON aegis_oracle_observations TO sentinel_rust;

-- sentinel-risk
GRANT INSERT, SELECT ON aegis_invariant_checks TO sentinel_rust;
GRANT INSERT, SELECT ON position_health TO sentinel_rust;
GRANT INSERT, UPDATE, SELECT ON liquidation_candidates TO sentinel_rust;
GRANT INSERT, UPDATE, SELECT ON market_metrics TO sentinel_rust;
-- execution_intents: sentinel-risk creates ONLY (data-model.md §10: "(insert only)").
-- No UPDATE for sentinel_rust — state advance is exclusively TypeScript's job.
GRANT INSERT, SELECT ON execution_intents TO sentinel_rust;
GRANT INSERT, SELECT ON reconciliation_mismatches TO sentinel_rust;

-- sentinel-jobs
GRANT INSERT, UPDATE, SELECT ON jobs TO sentinel_rust;

-- alerts: owner "any" (data-model.md §6) — both language roles may open/resolve.
GRANT INSERT, UPDATE, SELECT ON alerts TO sentinel_rust;

-- Read access sentinel_rust needs but does not own (its own raw/normalized
-- writes above cover the rest; execution_intents is already covered).
-- No cross-owner SELECT grants beyond what is listed above are required by
-- any stated Rust-side read path in Phase 2's explicit scope; broader read
-- grants for ingestion/normalization/risk cross-reads are added as later
-- phases need them, per AGENTS.md §6 ("do not add ... for later").

-- Sequences backing every `bigint GENERATED ALWAYS AS IDENTITY` column on a
-- table sentinel_rust can INSERT into. Unlike a plain `serial`, an identity
-- column's underlying sequence is owned internally by the table, but a
-- non-owner role executing a bare INSERT still needs USAGE on it —
-- verified against live PostgreSQL 18 in this phase (a role with only
-- INSERT on the table and no sequence USAGE is rejected with
-- "permission denied for sequence").
DO $$
DECLARE
  seq record;
BEGIN
  FOR seq IN
    SELECT n.nspname AS schema_name, c.relname AS seq_name
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE c.relkind = 'S' AND n.nspname = 'public'
  LOOP
    EXECUTE format('GRANT USAGE, SELECT ON SEQUENCE %I.%I TO sentinel_rust', seq.schema_name, seq.seq_name);
    EXECUTE format('GRANT USAGE, SELECT ON SEQUENCE %I.%I TO sentinel_ts', seq.schema_name, seq.seq_name);
  END LOOP;
END
$$;

-- ---------------------------------------------------------------------
-- sentinel_ts — union of sentinel-executor and sentinel-api.
-- "TypeScript never writes raw, normalized, protocol, or derived tables"
-- (architecture.md §5, data-model.md §10 closing sentence) — enforced by
-- simply never granting INSERT/UPDATE/DELETE on any table in those four
-- layers to sentinel_ts, anywhere in this file.
-- ---------------------------------------------------------------------

-- sentinel-executor
GRANT INSERT, UPDATE, SELECT ON transaction_attempts TO sentinel_ts;
GRANT INSERT, SELECT ON reconciliation_mismatches TO sentinel_ts;

-- execution_intents: sentinel-executor advances state; sentinel-api inserts
-- operator-initiated intents (data-model.md §7, §10). No DELETE, ever.
GRANT INSERT, UPDATE, SELECT ON execution_intents TO sentinel_ts;

-- alerts: owner "any".
GRANT INSERT, UPDATE, SELECT ON alerts TO sentinel_ts;

-- sentinel-api: "nothing [to write] — read-only role" (data-model.md §10).
-- Read access across every layer so the API can actually serve data.
GRANT SELECT ON
  raw_observations, decode_failures,
  slots, transactions, instructions, program_logs, account_observations, token_balance_deltas,
  rollback_events, gap_events, ingest_checkpoints, provider_health,
  decoder_versions, aegis_protocol_state, aegis_markets, aegis_market_params_history,
  aegis_positions, aegis_events, aegis_oracle_observations, aegis_invariant_checks,
  position_health, liquidation_candidates, market_metrics,
  jobs
  TO sentinel_ts;

-- Every table just listed must NEVER receive INSERT/UPDATE/DELETE for
-- sentinel_ts. This is asserted by crates/sentinel-db/tests/role_permissions.rs
-- against the live catalog and by attempted-write tests against a live
-- connection as sentinel_ts, not by this comment.

-- Partition ACLs — verified against a real, live PostgreSQL 18 connection
-- as sentinel_rust in this phase, not assumed:
--   * A GRANT on the PARENT partitioned table (e.g. `raw_observations`) is
--     sufficient for every normal access path: SELECT/INSERT addressed to
--     the parent name is what all application code uses, Postgres routes
--     the row to the correct partition internally, and the permission
--     check happens against the parent relation named in the statement.
--   * A GRANT on the parent does NOT propagate to a query that names a
--     CHILD partition directly (e.g. `SELECT * FROM raw_observations_p0`)
--     — that was tried and rejected with "permission denied for table
--     raw_observations_p0" even though sentinel_rust holds SELECT on the
--     parent. This is expected and not a gap: no application code in this
--     system ever addresses a partition by its physical child name: only
--     the ongoing, ahead-of-head partition CREATION in
--     crates/sentinel-db/src/partitions.rs runs as the bootstrap/superuser
--     migration role, which owns every table it creates and needs no grant.

