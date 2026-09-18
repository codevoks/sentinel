-- Phase 2 — derived layer. docs/data-model.md §6.

-- position_health — MATERIALIZED — owner: sentinel-risk.
CREATE TABLE position_health (
  position_pubkey          bytea NOT NULL REFERENCES aegis_positions (position_pubkey),
  computed_at_slot         bigint NOT NULL,
  t_eval                   timestamptz NOT NULL,
  collateral_value_wad     numeric(39,0) NOT NULL,
  debt_value_wad           numeric(39,0) NOT NULL,
  debt_assets              numeric(20,0) NOT NULL,
  health_factor_wad        numeric(39,0),
  state                    health_state NOT NULL,
  liquidation_price_wad    numeric(39,0),
  borrow_capacity_wad      numeric(39,0),
  collateral_obs_id        bigint,
  collateral_obs_slot      bigint, -- part of the composite FK to aegis_oracle_observations' natural key is by (feed_id, publish_time, slot), not an id; see note below
  loan_obs_id               bigint,
  loan_obs_slot              bigint,
  market_params_from_slot  bigint NOT NULL,
  commitment                commitment_level NOT NULL,

  CONSTRAINT position_health_pk PRIMARY KEY (position_pubkey, computed_at_slot)
);

COMMENT ON TABLE position_health IS
  'MATERIALIZED — owner: sentinel-risk. data-model.md §6. Conflict: DO NOTHING — a health value for a '
  'given slot is a fact about that slot, never revised in place. Recording the oracle observation and '
  'market-params-version pointers is mandatory: without them a health value cannot be explained, '
  'reproduced, or defended (data-model.md §6, DM-12). '
  'NOTE on collateral_obs_id/loan_obs_id: aegis_oracle_observations has no synthetic id column (its '
  'PK is the natural key (feed_id, publish_time, slot) per data-model.md §5) — these columns are '
  'therefore advisory pointers (populated by application code, not FK-enforced) rather than a '
  'PostgreSQL foreign key, since there is no single-column id to reference. This is recorded here '
  'rather than silently narrowed to an unenforceable FK.';

CREATE INDEX position_health_state_computed_idx ON position_health (state, computed_at_slot DESC);
COMMENT ON INDEX position_health_state_computed_idx IS
  'Supports: "every liquidatable position right now" — the keeper''s primary read (data-model.md §6).';

CREATE INDEX position_health_position_computed_idx ON position_health (position_pubkey, computed_at_slot DESC);
COMMENT ON INDEX position_health_position_computed_idx IS
  'Supports: health history for one position, most recent first (data-model.md §6).';

-- liquidation_candidates — MATERIALIZED — owner: sentinel-risk.
CREATE TABLE liquidation_candidates (
  candidate_id              bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  position_pubkey           bytea NOT NULL REFERENCES aegis_positions (position_pubkey),
  market_pubkey             bytea NOT NULL REFERENCES aegis_markets (market_pubkey),
  detected_at_slot          bigint NOT NULL,
  t_eval                    timestamptz NOT NULL,
  lookahead_ms              int NOT NULL,
  risk_params_hash          text NOT NULL,
  health_factor_wad         numeric(39,0),
  max_repay_assets          numeric(20,0) NOT NULL,
  expected_seize            numeric(20,0) NOT NULL,
  expected_bonus            numeric(20,0) NOT NULL,
  expected_protocol_cut     numeric(20,0) NOT NULL,
  estimated_profit_wad      numeric(39,0) NOT NULL,
  profitable                bool NOT NULL,
  reason_unprofitable       text,
  full_liquidation          bool NOT NULL,
  dust_rule_applied         bool NOT NULL,
  expires_at                timestamptz NOT NULL,
  status                    candidate_status NOT NULL,
  invalidated_reason        text,

  CONSTRAINT liquidation_candidates_natural_key UNIQUE (position_pubkey, detected_at_slot)
);

COMMENT ON TABLE liquidation_candidates IS
  'MATERIALIZED — owner: sentinel-risk. data-model.md §6. Natural key (position_pubkey, '
  'detected_at_slot) UNIQUE, conflict DO NOTHING: re-detecting the same position at the same slot is '
  'a no-op. lookahead_ms and risk_params_hash are mandatory replay-determinism fields (D-3): replay '
  'reads them back, it never re-measures.';

CREATE INDEX liquidation_candidates_open_by_profit_idx
  ON liquidation_candidates (status, estimated_profit_wad DESC) WHERE status = 'open';
COMMENT ON INDEX liquidation_candidates_open_by_profit_idx IS
  'Supports: the keeper''s execution-priority queue — open candidates ranked by profit (data-model.md §6).';

CREATE INDEX liquidation_candidates_position_idx ON liquidation_candidates (position_pubkey);
COMMENT ON INDEX liquidation_candidates_position_idx IS
  'Supports: candidate history for one position (data-model.md §6).';

-- market_metrics — MATERIALIZED — owner: sentinel-risk.
CREATE TABLE market_metrics (
  market_pubkey             bytea NOT NULL REFERENCES aegis_markets (market_pubkey),
  bucket_start              timestamptz NOT NULL,
  utilization_wad           numeric(39,0) NOT NULL,
  borrow_rate_ps            numeric(39,0) NOT NULL,
  supply_rate_ps            numeric(39,0) NOT NULL,
  total_supply_assets       numeric(20,0) NOT NULL,
  total_borrow_assets       numeric(20,0) NOT NULL,
  total_supply_shares       numeric(39,0) NOT NULL,
  total_borrow_shares       numeric(39,0) NOT NULL,
  free_liquidity            numeric(20,0) NOT NULL,
  accrual_staleness_secs    bigint NOT NULL,
  open_positions             bigint NOT NULL,
  positions_with_debt        bigint NOT NULL,
  aggregate_bad_debt          numeric(20,0) NOT NULL,

  CONSTRAINT market_metrics_pk PRIMARY KEY (market_pubkey, bucket_start),
  CONSTRAINT market_metrics_counts_nonneg CHECK (open_positions >= 0 AND positions_with_debt >= 0)
);

COMMENT ON TABLE market_metrics IS
  'MATERIALIZED — owner: sentinel-risk. data-model.md §6. Per market per slot bucket. Freely '
  'rewritten by its owning worker (MATERIALIZED mutability class); the query layer upserts with '
  'DO UPDATE, unguarded by an as_of_slot column because this table has none — bucket_start plus the '
  'owning worker being the sole writer is the idempotency mechanism here.';

-- alerts — OPERATIONAL — owner: any.
CREATE TABLE alerts (
  alert_id      bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  kind          text NOT NULL,
  severity      text NOT NULL,
  entity_kind   text NOT NULL,
  entity_key    text NOT NULL,
  opened_at     timestamptz NOT NULL DEFAULT now(),
  resolved_at   timestamptz,
  detail        jsonb NOT NULL DEFAULT '{}'::jsonb,
  runbook_id    text
);

COMMENT ON TABLE alerts IS
  'OPERATIONAL — owner: any (Rust and TypeScript services both open/resolve alerts). data-model.md §6. '
  'DM-08: natural key (kind, entity_kind, entity_key), partial-unique WHERE resolved_at IS NULL — one '
  'open alert per condition, not one per evaluation.';

-- DM-08 / phase-02-data-model.md §6: one open alert per (kind, entity).
-- "entity" is (entity_kind, entity_key) per data-model.md §6's own natural key statement.
CREATE UNIQUE INDEX alerts_one_open_per_kind_entity
  ON alerts (kind, entity_kind, entity_key) WHERE resolved_at IS NULL;
COMMENT ON INDEX alerts_one_open_per_kind_entity IS
  'DM-08: exactly one open alert per (kind, entity_kind, entity_key) — the pager-DoS guard (data-model.md §6).';

CREATE INDEX alerts_open_by_severity_idx ON alerts (severity, opened_at) WHERE resolved_at IS NULL;
COMMENT ON INDEX alerts_open_by_severity_idx IS
  'Supports: the operator dashboard''s open-alerts-by-severity view (data-model.md §6, docs/observability.md).';
