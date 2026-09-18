-- Phase 2 — Canonical Data Model & Migrations
--
-- Native PostgreSQL ENUM types for every closed-set column named `enum:` in
-- `docs/data-model.md`. Using real enum types (rather than a text column
-- plus a CHECK) makes the domain visible in \d output and to any typed
-- client generator, and is what `docs/data-model.md`'s "enum:" notation
-- maps to 1:1.
--
-- All values are taken verbatim from the frozen documents cited on each
-- type. No value is invented.

-- docs/finality-and-forks.md §2 ("status") and the promotion state machine
-- in §3 (observed -> confirmed -> finalized, or -> abandoned; skipped is a
-- terminal non-promotable state for slots nobody produced a block for).
CREATE TYPE slot_status AS ENUM ('skipped', 'observed', 'confirmed', 'finalized', 'abandoned');

-- docs/finality-and-forks.md §1/§2: the three commitment levels Sentinel
-- persists. 'processed' is deliberately included here (rows/tables that
-- track raw provider-observed commitment, e.g. as evidence), even though
-- §1.1 forbids ever presenting 'processed' as the *label* of settled
-- output; that is an application-layer rule, not a schema-domain rule.
CREATE TYPE commitment_level AS ENUM ('processed', 'confirmed', 'finalized');

-- docs/data-model.md §3 `transactions.version` / `transaction_attempts.transaction_version`.
-- v1 is real (CLAUDE.md §11 table) and has no ALTs.
CREATE TYPE tx_version AS ENUM ('legacy', 'v0', 'v1');

-- docs/data-model.md §3 `transactions.priority_fee_source`: the load-bearing
-- discriminator between the pre-v1 ComputeBudget-instruction convention and
-- v1's message-level transactionConfig absolute-lamport convention.
CREATE TYPE priority_fee_source AS ENUM ('config_mask', 'compute_budget_ix', 'absent');

-- docs/data-model.md §3 `program_logs.kind`.
CREATE TYPE log_kind AS ENUM ('invoke', 'success', 'failure', 'data', 'log', 'consumed', 'unknown');

-- docs/data-model.md §3 `account_observations.source`. The frozen doc does
-- not enumerate this one explicitly, but §3's own note on `write_version`
-- ("NULL from RPC; present from Geyser") draws exactly a two-way
-- distinction, so that is the domain implemented here — not a three- or
-- four-way set copied from the unrelated `raw_observations.source` domain
-- (`ingestion-model.md` §5), which tracks the raw *transport*, not the
-- normalized account observation's provenance.
CREATE TYPE account_observation_source AS ENUM ('rpc', 'geyser');

-- docs/data-model.md §5 `aegis_markets.status` / `aegis_positions.status`.
CREATE TYPE materialization_status AS ENUM ('current', 'recomputing', 'stale', 'unknown_schema');

-- docs/data-model.md §5 `aegis_markets.materialized_via` / `aegis_positions.materialized_via`.
CREATE TYPE materialized_via AS ENUM ('event', 'snapshot');

-- docs/data-model.md §5 `aegis_oracle_observations.validation_result`.
CREATE TYPE oracle_validation_result AS ENUM ('valid', 'failed');

-- docs/data-model.md §6 `position_health.state`.
CREATE TYPE health_state AS ENUM ('healthy', 'liquidatable', 'no_debt', 'unknown_oracle', 'stale');

-- docs/data-model.md §6 `liquidation_candidates.status`.
CREATE TYPE candidate_status AS ENUM ('open', 'claimed', 'executed', 'expired', 'invalidated');

-- docs/data-model.md §7 `execution_intents.kind`.
CREATE TYPE intent_kind AS ENUM ('liquidate', 'absorb_bad_debt', 'accrue_interest', 'custom');

-- docs/data-model.md §7 `execution_intents.state`.
CREATE TYPE intent_state AS ENUM (
  'CREATED', 'PLANNING', 'PLANNED', 'AWAITING_ATTEMPT', 'IN_FLIGHT',
  'SUCCEEDED', 'FAILED', 'EXPIRED', 'CANCELLED', 'NEEDS_OPERATOR'
);

-- docs/data-model.md §7 `transaction_attempts.state`.
CREATE TYPE attempt_state AS ENUM (
  'SIGNED', 'SUBMITTED', 'OBSERVED', 'CONFIRMED', 'FINALIZED',
  'FAILED_ONCHAIN', 'EXPIRED', 'ABANDONED_PRE_SUBMIT', 'UNKNOWN'
);

-- docs/data-model.md §7 `reconciliation_mismatches.class`, values from
-- docs/aegis-integration.md §12 (cited, not re-derived).
CREATE TYPE mismatch_class AS ENUM (
  'RACE_HEALED', 'RACE_LOST', 'ORACLE_CLOSED', 'PAUSED', 'SIZE_REJECTED',
  'MODEL_DIVERGENCE', 'ACCOUNT_REJECTED', 'PROJECTION_DIVERGENCE', 'UNKNOWN'
);

-- docs/data-model.md §8 `jobs.kind`.
CREATE TYPE job_kind AS ENUM (
  'backfill_range', 'replay_range', 'rematerialize_entity', 'snapshot_accounts',
  'reconcile_provider', 'recompute_after_rollback', 'scan_program_accounts'
);

-- docs/data-model.md §8 `jobs.state`.
CREATE TYPE job_state AS ENUM ('queued', 'leased', 'done', 'failed', 'quarantined');

-- docs/ingestion-model.md §5 `raw_observations.kind` — the closed set of raw
-- observation kinds Sentinel ever persists at the raw boundary. Verbatim
-- from the frozen document; ingestion-model.md is FROZEN per AGENTS.md §4.
CREATE TYPE raw_observation_kind AS ENUM (
  'block', 'transaction', 'account', 'log_batch', 'slot_status',
  'program_accounts_page', 'signature_status', 'oracle_update'
);

-- docs/ingestion-model.md §5 `raw_observations.source`.
CREATE TYPE observation_source AS ENUM ('rpc_http', 'rpc_ws', 'geyser', 'fixture');

-- docs/ingestion-model.md §5 `raw_observations.payload_encoding`.
CREATE TYPE payload_encoding AS ENUM ('json_zstd', 'borsh', 'base64_raw');

-- NOTE: `decode_failures.stage` and `decode_failures.error_code` are
-- deliberately `text`, not a native enum, here. No frozen document
-- enumerates a closed set of pipeline-stage names for `stage`, and
-- `docs/architecture.md` §"error taxonomy" states error codes are the
-- stable, growable format `SEN-<AREA>-<NNN>` (free-form beyond the prefix
-- convention) — inventing a closed enum for either would silently narrow a
-- set the frozen documents leave open.
