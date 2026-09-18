-- Phase 2 — normalized layer.
-- docs/data-model.md §3, docs/finality-and-forks.md §2 (slots is chain-state
-- but its columns are defined there; data-model.md places its ownership in
-- the normalized-layer table per §10... actually chainstate layer owns
-- `slots` per data-model.md §10 ownership table. It is created here because
-- transactions/instructions/program_logs/account_observations all carry a
-- (slot, blockhash) FK to it and migrations are forward-only.)

-- slots — PROMOTABLE — owner: sentinel-chainstate (sentinel_rust role).
-- docs/finality-and-forks.md §2.
CREATE TABLE slots (
  slot              bigint NOT NULL,
  blockhash         bytea, -- NULL for skipped slots (§2)
  parent_slot       bigint,
  parent_blockhash  bytea,
  block_time        timestamptz,
  block_height      bigint,
  status            slot_status NOT NULL,
  commitment        commitment_level NOT NULL,
  canonical         bool NOT NULL DEFAULT false,
  first_seen_at     timestamptz NOT NULL DEFAULT now(),
  finalized_at      timestamptz,

  CONSTRAINT slots_pk PRIMARY KEY (slot, blockhash)
);

COMMENT ON TABLE slots IS
  'PROMOTABLE — owner: sentinel-chainstate. finality-and-forks.md §2. '
  'PRIMARY KEY (slot, blockhash), not (slot): Sentinel observes forks and possible equivocation.';

-- blockhash is nullable and part of the PK; a plain PRIMARY KEY treats NULL
-- as always-distinct, which already gives "one row per (slot, NULL)"
-- naturally for however many skipped-slot rows are inserted — but
-- finality-and-forks.md §2 requires exactly ONE skipped row per slot. A
-- partial unique index on slot where blockhash IS NULL enforces that.
CREATE UNIQUE INDEX slots_one_skipped_per_slot ON slots (slot) WHERE blockhash IS NULL;
COMMENT ON INDEX slots_one_skipped_per_slot IS
  'finality-and-forks.md §2 note: exactly one skipped-slot row per slot (blockhash IS NULL).';

CREATE INDEX slots_canonical_idx ON slots (slot) WHERE canonical;
COMMENT ON INDEX slots_canonical_idx IS
  'Supports: "what is the canonical block at slot N" — the query every consumer of chain state issues (data-model.md §3).';

CREATE INDEX slots_commitment_slot_idx ON slots (commitment, slot);
COMMENT ON INDEX slots_commitment_slot_idx IS
  'Supports: promotion sweeps and "slots stuck below finalized" queries (finality-and-forks.md §3, rule P-5).';

CREATE INDEX slots_status_idx ON slots (status);
COMMENT ON INDEX slots_status_idx IS
  'Supports: contiguity/gap scanning by status (ingestion-model.md §9).';

-- transactions — PROMOTABLE — owner: sentinel-normalize (create), sentinel-chainstate (promote).
-- docs/data-model.md §3.
CREATE TABLE transactions (
  signature                  bytea NOT NULL,
  slot                       bigint NOT NULL,
  blockhash                  bytea NOT NULL,
  transaction_index          int NOT NULL,
  version                    tx_version NOT NULL,
  num_required_signatures    smallint NOT NULL,
  recent_blockhash           bytea NOT NULL,
  success                    bool NOT NULL,
  error_code                 text,
  fee_lamports               numeric(20,0) NOT NULL,
  compute_units_consumed     numeric(20,0),
  compute_unit_limit         numeric(20,0),
  priority_fee_lamports      numeric(20,0),
  priority_fee_source        priority_fee_source NOT NULL,
  loaded_addresses_from_alt  bool NOT NULL,
  is_vote                    bool NOT NULL,
  commitment                 commitment_level NOT NULL,
  canonical                  bool NOT NULL DEFAULT false,

  CONSTRAINT transactions_pk PRIMARY KEY (signature, slot, blockhash),
  CONSTRAINT transactions_slot_fk FOREIGN KEY (slot, blockhash) REFERENCES slots (slot, blockhash),
  -- v1 has no address lookup tables (CLAUDE.md §11 table) — loaded_addresses_from_alt
  -- must be false whenever version = 'v1'.
  CONSTRAINT transactions_v1_no_alt CHECK (version <> 'v1' OR loaded_addresses_from_alt = false)
) PARTITION BY RANGE (slot);

COMMENT ON TABLE transactions IS
  'PROMOTABLE — owner: sentinel-normalize (create), sentinel-chainstate (commitment/canonical only). data-model.md §3. '
  'PRIMARY KEY (signature, slot, blockhash), not signature alone: Sentinel observes forks.';

CREATE INDEX transactions_signature_idx ON transactions (signature);
COMMENT ON INDEX transactions_signature_idx IS
  'Supports: "find every observed instance of this signature across forks" (data-model.md §3).';

CREATE INDEX transactions_slot_tx_index_idx ON transactions (slot, transaction_index);
COMMENT ON INDEX transactions_slot_tx_index_idx IS
  'Supports: block-ordered replay — transaction_index is the ordering authority within a slot (data-model.md §3).';

CREATE INDEX transactions_canonical_nonvote_idx ON transactions (slot) WHERE canonical AND NOT is_vote;
COMMENT ON INDEX transactions_canonical_nonvote_idx IS
  'Supports: the API/keeper working set — canonical, non-vote transactions by slot (data-model.md §3).';

-- instructions — IMMUTABLE — owner: sentinel-normalize.
CREATE TABLE instructions (
  signature     bytea NOT NULL,
  slot          bigint NOT NULL,
  blockhash     bytea NOT NULL,
  ix_index      smallint NOT NULL,
  inner_index   smallint NOT NULL, -- -1 for top-level, else inner instruction index
  stack_height  smallint NOT NULL,
  program_id    bytea NOT NULL,
  accounts      bytea[] NOT NULL,
  data          bytea NOT NULL,

  CONSTRAINT instructions_pk PRIMARY KEY (signature, slot, blockhash, ix_index, inner_index),
  CONSTRAINT instructions_tx_fk FOREIGN KEY (signature, slot, blockhash) REFERENCES transactions (signature, slot, blockhash)
) PARTITION BY RANGE (slot);

COMMENT ON TABLE instructions IS 'IMMUTABLE — owner: sentinel-normalize. data-model.md §3.';

CREATE INDEX instructions_program_id_slot_idx ON instructions (program_id, slot);
COMMENT ON INDEX instructions_program_id_slot_idx IS
  'Supports: per-program instruction scans (the Aegis decoder''s primary read pattern) (data-model.md §3).';

CREATE INDEX instructions_slot_idx ON instructions (slot);
COMMENT ON INDEX instructions_slot_idx IS 'Supports: slot-range replay reads (data-model.md §3).';

-- program_logs — IMMUTABLE — owner: sentinel-normalize.
CREATE TABLE program_logs (
  signature   bytea NOT NULL,
  slot        bigint NOT NULL,
  blockhash   bytea NOT NULL,
  log_index   int NOT NULL,
  program_id  bytea,
  raw_line    text NOT NULL,
  kind        log_kind NOT NULL,

  CONSTRAINT program_logs_pk PRIMARY KEY (signature, slot, blockhash, log_index),
  CONSTRAINT program_logs_tx_fk FOREIGN KEY (signature, slot, blockhash) REFERENCES transactions (signature, slot, blockhash)
) PARTITION BY RANGE (slot);

COMMENT ON TABLE program_logs IS
  'IMMUTABLE — owner: sentinel-normalize. data-model.md §3. Stored verbatim and in order: '
  'Anchor emit! events are base64 payloads on "Program data:" lines whose attribution depends '
  'on invoke/success framing.';

CREATE INDEX program_logs_slot_idx ON program_logs (slot);
COMMENT ON INDEX program_logs_slot_idx IS 'Supports: slot-range replay reads (data-model.md §3).';

CREATE INDEX program_logs_kind_slot_idx ON program_logs (kind, slot);
COMMENT ON INDEX program_logs_kind_slot_idx IS
  'Supports: event-projection scans filtered by log kind (e.g. only "data" lines) (data-model.md §3).';

-- account_observations — IMMUTABLE — owner: sentinel-normalize.
CREATE TABLE account_observations (
  pubkey          bytea NOT NULL,
  slot            bigint NOT NULL,
  content_hash    bytea NOT NULL,
  owner_program   bytea NOT NULL,
  lamports        numeric(20,0) NOT NULL,
  data            bytea NOT NULL,
  executable      bool NOT NULL,
  rent_epoch      numeric(20,0) NOT NULL,
  write_version   bigint, -- NULL from RPC; present from Geyser (data-model.md §3)
  source          account_observation_source NOT NULL,
  commitment      commitment_level NOT NULL,

  CONSTRAINT account_observations_pk PRIMARY KEY (pubkey, slot, content_hash)
) PARTITION BY RANGE (slot);

COMMENT ON TABLE account_observations IS
  'IMMUTABLE — owner: sentinel-normalize. data-model.md §3. Sparse by construction: absence of a row '
  'means "not observed", never "unchanged" and never "missing".';

CREATE INDEX account_observations_pubkey_slot_idx ON account_observations (pubkey, slot DESC);
COMMENT ON INDEX account_observations_pubkey_slot_idx IS
  'Supports: "the most recent observation of this account" (data-model.md §3).';

CREATE INDEX account_observations_owner_program_slot_idx ON account_observations (owner_program, slot);
COMMENT ON INDEX account_observations_owner_program_slot_idx IS
  'Supports: per-program account scans (data-model.md §3).';

-- token_balance_deltas — IMMUTABLE — owner: sentinel-normalize.
CREATE TABLE token_balance_deltas (
  signature       bytea NOT NULL,
  slot            bigint NOT NULL,
  blockhash       bytea NOT NULL,
  account_index   smallint NOT NULL,
  token_account   bytea NOT NULL,
  mint            bytea NOT NULL,
  owner           bytea NOT NULL,
  pre_amount      numeric(39,0) NOT NULL,
  post_amount     numeric(39,0) NOT NULL,
  decimals        smallint NOT NULL,
  program_id      bytea NOT NULL,

  CONSTRAINT token_balance_deltas_pk PRIMARY KEY (signature, slot, blockhash, account_index),
  CONSTRAINT token_balance_deltas_tx_fk FOREIGN KEY (signature, slot, blockhash) REFERENCES transactions (signature, slot, blockhash)
) PARTITION BY RANGE (slot);

COMMENT ON TABLE token_balance_deltas IS
  'IMMUTABLE — owner: sentinel-normalize. data-model.md §3. Derived from meta.pre/postTokenBalances, '
  'never from parsed instruction JSON (Agave 4.2 shape changes, ecosystem-research.md §1.4). '
  'numeric(39,0), not bigint: holds a full u128 exactly.';

CREATE INDEX token_balance_deltas_token_account_slot_idx ON token_balance_deltas (token_account, slot);
COMMENT ON INDEX token_balance_deltas_token_account_slot_idx IS
  'Supports: per-token-account balance history (data-model.md §3).';

CREATE INDEX token_balance_deltas_mint_slot_idx ON token_balance_deltas (mint, slot);
COMMENT ON INDEX token_balance_deltas_mint_slot_idx IS
  'Supports: per-mint flow queries (data-model.md §3).';
