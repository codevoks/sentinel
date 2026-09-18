-- Phase 2 — protocol layer (Aegis). Explicit non-scope (phase-02-data-model.md §2):
-- "No aegis_* table content logic — the tables exist with their constraints;
-- nothing writes them yet." Columns are taken from data-model.md §5 and,
-- where it says "every Market/Position field", from aegis-integration.md §4
-- (frozen, cited per field group below). No triggers, no stored procedures.

-- decoder_versions — IMMUTABLE — owner: migrations + sentinel-aegis.
-- aegis-integration.md §8.1 (frozen; full column set).
CREATE TABLE decoder_versions (
  id                    bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  protocol              text NOT NULL,
  program_id            text NOT NULL,
  account_kind          text NOT NULL CHECK (account_kind IN ('protocol', 'market', 'position')),
  schema_version        int NOT NULL,
  discriminator         bytea NOT NULL,
  layout_hash           text NOT NULL,
  effective_from_slot   bigint NOT NULL,
  effective_to_slot     bigint, -- exclusive; NULL = current
  source                text NOT NULL CHECK (source IN ('idl', 'spec')),

  CONSTRAINT decoder_versions_natural_key UNIQUE (protocol, program_id, account_kind, schema_version),
  CONSTRAINT decoder_versions_range_valid CHECK (effective_to_slot IS NULL OR effective_to_slot > effective_from_slot)
);

COMMENT ON TABLE decoder_versions IS
  'IMMUTABLE — owner: migrations + sentinel-aegis. aegis-integration.md §8.1. '
  'Every decoded protocol row names the decoder_version_id that produced it (DM-05).';

-- aegis_protocol_state — MATERIALIZED — owner: sentinel-aegis. Singleton config.
CREATE TABLE aegis_protocol_state (
  program_id          bytea PRIMARY KEY,
  admin               bytea NOT NULL,
  pending_admin       bytea,
  guardian            bytea NOT NULL,
  fee_recipient       bytea NOT NULL,
  paused              smallint NOT NULL, -- bitmask, u8 (aegis-integration.md §4.6 shape)
  as_of_slot          bigint NOT NULL,
  as_of_commitment    commitment_level NOT NULL,
  decoder_version_id  bigint NOT NULL REFERENCES decoder_versions (id),
  materialized_via    materialized_via NOT NULL
);

COMMENT ON TABLE aegis_protocol_state IS
  'MATERIALIZED — owner: sentinel-aegis. data-model.md §5. Singleton config, PK program_id. '
  'DM-04 monotonicity is enforced by the upsert query (WHERE excluded.as_of_slot > current.as_of_slot), '
  'not by this migration, which creates no rows (explicit Phase 2 non-scope).';

-- aegis_markets — MATERIALIZED — owner: sentinel-aegis.
CREATE TABLE aegis_markets (
  market_pubkey             bytea PRIMARY KEY,
  program_id                bytea NOT NULL,

  -- §4.1 identity (immutable after creation)
  collateral_mint            bytea NOT NULL,
  loan_mint                  bytea NOT NULL,
  collateral_token_program   bytea NOT NULL,
  loan_token_program         bytea NOT NULL,
  collateral_vault           bytea NOT NULL,
  loan_vault                 bytea NOT NULL,
  fee_recipient               bytea NOT NULL,
  config_id                  bigint NOT NULL,
  collateral_decimals        smallint NOT NULL,
  loan_decimals               smallint NOT NULL,

  -- §4.2 oracle config (admin-mutable)
  oracle_kind                 smallint NOT NULL,
  collateral_feed_id          bytea NOT NULL, -- [u8;32]
  loan_feed_id                 bytea NOT NULL, -- [u8;32]
  max_price_age_secs          bigint NOT NULL, -- u32
  max_conf_bps                 int NOT NULL,    -- u16

  -- §4.3 risk parameters (admin-mutable, bounds-checked) — all u128 WAD except min_debt (u64)
  max_ltv               numeric(39,0) NOT NULL,
  liq_threshold          numeric(39,0) NOT NULL,
  liq_bonus               numeric(39,0) NOT NULL,
  close_factor             numeric(39,0) NOT NULL,
  full_liq_hf               numeric(39,0) NOT NULL,
  liq_protocol_fee           numeric(39,0) NOT NULL,
  fee                         numeric(39,0) NOT NULL,
  min_debt                     numeric(20,0) NOT NULL,

  -- §4.4 IRM parameters (stateless, per-second WAD, u128)
  base_rate_ps    numeric(39,0) NOT NULL,
  slope1_ps        numeric(39,0) NOT NULL,
  slope2_ps         numeric(39,0) NOT NULL,
  u_kink             numeric(39,0) NOT NULL,
  max_rate_ps         numeric(39,0) NOT NULL,

  -- §4.5 hot accounting (lazily accrued as of last_accrual_ts)
  total_supply_assets       numeric(20,0) NOT NULL, -- u64
  total_supply_shares        numeric(39,0) NOT NULL, -- u128
  total_borrow_assets         numeric(20,0) NOT NULL, -- u64
  total_borrow_shares          numeric(39,0) NOT NULL, -- u128
  collateral_fee_accrued        numeric(20,0) NOT NULL, -- u64
  last_accrual_ts                 bigint NOT NULL, -- i64

  -- §4.6 flags
  paused    smallint NOT NULL,
  flags      smallint NOT NULL,

  -- materialization bookkeeping (data-model.md §5)
  as_of_slot           bigint NOT NULL,
  as_of_commitment     commitment_level NOT NULL,
  last_snapshot_slot   bigint,
  materialized_via     materialized_via NOT NULL,
  decoder_version_id   bigint NOT NULL REFERENCES decoder_versions (id),
  status               materialization_status NOT NULL
);

COMMENT ON TABLE aegis_markets IS
  'MATERIALIZED — owner: sentinel-aegis. data-model.md §5, aegis-integration.md §4.1-§4.6. '
  'Natural key: market_pubkey (itself derived from (collateral_mint, loan_mint, config_id) per PDA '
  'derivation — not re-derived here, out of Phase 2 scope). '
  'DM-04: upsert query guards WHERE excluded.as_of_slot > aegis_markets.as_of_slot — no stale write '
  'ever overwrites a newer one.';

CREATE INDEX aegis_markets_mints_idx ON aegis_markets (collateral_mint, loan_mint);
COMMENT ON INDEX aegis_markets_mints_idx IS
  'Supports: "which markets exist for this collateral/loan pair" (data-model.md §5).';

CREATE INDEX aegis_markets_status_idx ON aegis_markets (status);
COMMENT ON INDEX aegis_markets_status_idx IS
  'Supports: operator/alerting queries for markets not in status=current (data-model.md §5).';

-- aegis_market_params_history — IMMUTABLE — owner: sentinel-aegis.
CREATE TABLE aegis_market_params_history (
  market_pubkey         bytea NOT NULL,
  effective_from_slot   bigint NOT NULL,
  effective_to_slot     bigint, -- exclusive; NULL = current
  max_ltv               numeric(39,0) NOT NULL,
  liq_threshold          numeric(39,0) NOT NULL,
  liq_bonus               numeric(39,0) NOT NULL,
  close_factor             numeric(39,0) NOT NULL,
  full_liq_hf               numeric(39,0) NOT NULL,
  liq_protocol_fee           numeric(39,0) NOT NULL,
  fee                         numeric(39,0) NOT NULL,
  min_debt                     numeric(20,0) NOT NULL,
  base_rate_ps    numeric(39,0) NOT NULL,
  slope1_ps        numeric(39,0) NOT NULL,
  slope2_ps         numeric(39,0) NOT NULL,
  u_kink             numeric(39,0) NOT NULL,
  max_rate_ps         numeric(39,0) NOT NULL,
  oracle_kind smallint NOT NULL,
  collateral_feed_id bytea NOT NULL,
  loan_feed_id bytea NOT NULL,
  max_price_age_secs bigint NOT NULL,
  max_conf_bps int NOT NULL,
  decoder_version_id bigint NOT NULL REFERENCES decoder_versions (id),

  CONSTRAINT aegis_market_params_history_pk PRIMARY KEY (market_pubkey, effective_from_slot),
  CONSTRAINT aegis_market_params_history_range_valid
    CHECK (effective_to_slot IS NULL OR effective_to_slot > effective_from_slot)
);

COMMENT ON TABLE aegis_market_params_history IS
  'IMMUTABLE — owner: sentinel-aegis. data-model.md §5. The versioned parameter set required by '
  'aegis-integration.md §4.3: historical health must use the parameters in force at that slot, never '
  'today''s (DM-12).';

-- aegis_positions — MATERIALIZED — owner: sentinel-aegis.
CREATE TABLE aegis_positions (
  position_pubkey     bytea PRIMARY KEY,
  market_pubkey       bytea NOT NULL REFERENCES aegis_markets (market_pubkey),
  owner               bytea NOT NULL,
  supply_shares       numeric(39,0) NOT NULL, -- u128 (aegis-integration.md §4.7)
  borrow_shares       numeric(39,0) NOT NULL, -- u128
  collateral_amount   numeric(20,0) NOT NULL, -- u64
  is_open             bool NOT NULL,
  as_of_slot          bigint NOT NULL,
  as_of_commitment    commitment_level NOT NULL,
  last_snapshot_slot  bigint,
  materialized_via    materialized_via NOT NULL,
  decoder_version_id  bigint NOT NULL REFERENCES decoder_versions (id),
  status              materialization_status NOT NULL
);

COMMENT ON TABLE aegis_positions IS
  'MATERIALIZED — owner: sentinel-aegis. data-model.md §5, aegis-integration.md §4.7. '
  'is_open rather than deletion: Aegis positions are closable and re-creatable at the same address. '
  'DM-04: upsert guarded by WHERE excluded.as_of_slot > current.as_of_slot.';

CREATE INDEX aegis_positions_open_by_market_idx ON aegis_positions (market_pubkey) WHERE is_open;
COMMENT ON INDEX aegis_positions_open_by_market_idx IS
  'Supports: enumerate open positions in a market (data-model.md §5).';

CREATE INDEX aegis_positions_owner_idx ON aegis_positions (owner);
COMMENT ON INDEX aegis_positions_owner_idx IS 'Supports: "my positions" queries (data-model.md §5).';

CREATE INDEX aegis_positions_debt_idx ON aegis_positions (market_pubkey, borrow_shares) WHERE borrow_shares > 0;
COMMENT ON INDEX aegis_positions_debt_idx IS
  'Supports: the keeper''s working set — only positions with debt can be liquidated (data-model.md §5).';

-- aegis_events — IMMUTABLE — owner: sentinel-aegis.
CREATE TABLE aegis_events (
  signature           bytea NOT NULL,
  slot                bigint NOT NULL,
  blockhash           bytea NOT NULL,
  ix_index             smallint NOT NULL,
  inner_index           smallint NOT NULL,
  log_index              int NOT NULL,
  event_name           text NOT NULL,
  market_pubkey        bytea,
  position_pubkey        bytea,
  payload                 jsonb NOT NULL,
  decoder_version_id     bigint NOT NULL REFERENCES decoder_versions (id),
  as_of_commitment       commitment_level NOT NULL,

  CONSTRAINT aegis_events_pk PRIMARY KEY (signature, slot, blockhash, log_index)
);

COMMENT ON TABLE aegis_events IS
  'IMMUTABLE — owner: sentinel-aegis. data-model.md §5. The event projection''s input and the '
  'position-history API''s source. Ordered by (slot, transaction_index, ix_index, inner_index) — never '
  'by arrival (transaction_index lives on the joined transactions row).';

CREATE INDEX aegis_events_market_slot_ix_idx ON aegis_events (market_pubkey, slot, ix_index);
COMMENT ON INDEX aegis_events_market_slot_ix_idx IS
  'Supports: replay-ordered event fold per market (data-model.md §5).';

CREATE INDEX aegis_events_position_slot_idx ON aegis_events (position_pubkey, slot);
COMMENT ON INDEX aegis_events_position_slot_idx IS
  'Supports: position-history API reads (data-model.md §5).';

CREATE INDEX aegis_events_name_slot_idx ON aegis_events (event_name, slot);
COMMENT ON INDEX aegis_events_name_slot_idx IS
  'Supports: per-event-type scans, e.g. every Liquidated event in a range (data-model.md §5).';

-- aegis_oracle_observations — IMMUTABLE — owner: sentinel-aegis.
CREATE TABLE aegis_oracle_observations (
  feed_id             bytea NOT NULL, -- [u8;32]
  publish_time        timestamptz NOT NULL,
  slot                bigint NOT NULL,
  price               numeric(39,0) NOT NULL,
  conf                numeric(39,0) NOT NULL,
  expo                int NOT NULL,
  verification_level  text NOT NULL,
  price_account       bytea NOT NULL, -- provenance only, never identity
  price_lo_wad        numeric(39,0) NOT NULL,
  price_hi_wad        numeric(39,0) NOT NULL,
  validation_result   oracle_validation_result NOT NULL,
  failed_check        text, -- 'O-4' | 'O-5' | 'O-8' | ... — free-form per architecture.md error taxonomy, not a closed enum

  CONSTRAINT aegis_oracle_observations_pk PRIMARY KEY (feed_id, publish_time, slot)
);

COMMENT ON TABLE aegis_oracle_observations IS
  'IMMUTABLE — owner: sentinel-aegis. data-model.md §5. Indexed by feed ID, not account address: the '
  'account is ephemeral and permissionlessly posted (oracle-design.md §2, O-3).';

CREATE INDEX aegis_oracle_observations_feed_publish_idx ON aegis_oracle_observations (feed_id, publish_time DESC);
COMMENT ON INDEX aegis_oracle_observations_feed_publish_idx IS
  'Supports: "latest observations for this feed" (data-model.md §5).';

CREATE INDEX aegis_oracle_observations_feed_slot_valid_idx
  ON aegis_oracle_observations (feed_id, slot DESC) WHERE validation_result = 'valid';
COMMENT ON INDEX aegis_oracle_observations_feed_slot_valid_idx IS
  'Supports: "the latest VALID price for this feed" — the health-computation read path (data-model.md §5).';

-- aegis_invariant_checks — IMMUTABLE — owner: sentinel-risk.
CREATE TABLE aegis_invariant_checks (
  check_id       bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  invariant_id   text NOT NULL,
  market_pubkey  bytea,
  slot           bigint NOT NULL,
  expected       numeric(39,0) NOT NULL,
  actual         numeric(39,0) NOT NULL,
  holds          bool NOT NULL,
  checked_at     timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE aegis_invariant_checks IS
  'IMMUTABLE — owner: sentinel-risk. data-model.md §5. The off-chain checkable subset of Aegis''s '
  'invariants (aegis-integration.md §7.1). Run only on finalized state.';

CREATE INDEX aegis_invariant_checks_invariant_slot_idx ON aegis_invariant_checks (invariant_id, slot);
COMMENT ON INDEX aegis_invariant_checks_invariant_slot_idx IS
  'Supports: per-invariant history and "did INV-X hold at slot N" queries (data-model.md §5).';

CREATE INDEX aegis_invariant_checks_failed_idx ON aegis_invariant_checks (slot) WHERE NOT holds;
COMMENT ON INDEX aegis_invariant_checks_failed_idx IS
  'Supports: the alerting query — invariant violations only (data-model.md §5, aegis-integration.md §7.1).';
