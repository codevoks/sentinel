-- Phase 2 — execution layer. docs/data-model.md §7. OPERATIONAL. Not rebuildable, not chain-derived.

-- execution_intents — owner: sentinel-risk (create, Rust), sentinel-executor/sentinel-api (advance, TS).
CREATE TABLE execution_intents (
  intent_id                 uuid PRIMARY KEY,
  idempotency_key            text NOT NULL,
  kind                       intent_kind NOT NULL,
  market_pubkey               bytea NOT NULL,
  position_pubkey              bytea,
  params                        jsonb NOT NULL,
  constraints                     jsonb NOT NULL,
  state                            intent_state NOT NULL,
  created_at                        timestamptz NOT NULL DEFAULT now(),
  updated_at                         timestamptz NOT NULL DEFAULT now(),
  expires_at                          timestamptz NOT NULL,
  lease_holder                         text,
  lease_expires_at                      timestamptz,
  attempt_count                          int NOT NULL DEFAULT 0,
  max_attempts                            int NOT NULL,
  cumulative_fee_lamports                  bigint NOT NULL DEFAULT 0,
  terminal_reason                           text,
  trigger_slot                               bigint NOT NULL,
  trigger_commitment                          commitment_level NOT NULL,

  CONSTRAINT execution_intents_attempt_count_nonneg CHECK (attempt_count >= 0),
  CONSTRAINT execution_intents_cumulative_fee_nonneg CHECK (cumulative_fee_lamports >= 0)
);

COMMENT ON TABLE execution_intents IS
  'OPERATIONAL — owner: sentinel-risk (INSERT only), sentinel-executor/sentinel-api (state advance). '
  'data-model.md §7. idempotency_key is THE mechanism preventing a duplicate liquidation (DM-10).';

-- DM-10 / TX-02 groundwork: idempotency_key is UNIQUE GLOBALLY, not scoped to
-- kind or any other column — data-model.md §7 states this explicitly.
CREATE UNIQUE INDEX execution_intents_idempotency_key_global ON execution_intents (idempotency_key);
COMMENT ON INDEX execution_intents_idempotency_key_global IS
  'DM-10: execution_intents.idempotency_key is UNIQUE globally, not per-kind (data-model.md §7).';

CREATE INDEX execution_intents_market_idx ON execution_intents (market_pubkey);
COMMENT ON INDEX execution_intents_market_idx IS
  'Supports: per-market intent history and operator views (data-model.md §7).';

CREATE INDEX execution_intents_active_lease_idx
  ON execution_intents (lease_expires_at) WHERE lease_holder IS NOT NULL;
COMMENT ON INDEX execution_intents_active_lease_idx IS
  'Supports: lease-expiry sweeps for stuck intents (docs/distributed-correctness.md §3).';

-- transaction_attempts — owner: sentinel-executor (TS).
CREATE TABLE transaction_attempts (
  attempt_id                 uuid PRIMARY KEY,
  intent_id                   uuid NOT NULL REFERENCES execution_intents (intent_id),
  attempt_number                int NOT NULL,
  signature                       bytea NOT NULL,
  transaction_bytes                 bytea NOT NULL,
  transaction_version                 tx_version NOT NULL,
  recent_blockhash                      bytea NOT NULL,
  last_valid_block_height                 bigint NOT NULL,
  compute_unit_limit                        int,
  priority_fee_lamports                       bigint,
  simulation_result                             jsonb,
  simulated_units                                 int,
  state                                             attempt_state NOT NULL,
  signed_at                                          timestamptz NOT NULL DEFAULT now(),
  submitted_at                                        timestamptz,
  observed_at                                          timestamptz,
  resolved_at                                           timestamptz,
  observed_slot                                          bigint,
  onchain_error_code                                       text,
  onchain_error_band                                        text,

  CONSTRAINT transaction_attempts_signature_unique UNIQUE (signature),
  CONSTRAINT transaction_attempts_intent_attempt_number_unique UNIQUE (intent_id, attempt_number)
);

COMMENT ON TABLE transaction_attempts IS
  'OPERATIONAL — owner: sentinel-executor. data-model.md §7. Conflict: never — inserts only, with '
  'signature UNIQUE as the safety net. signature and transaction_bytes are persisted and committed '
  'BEFORE sendTransaction is called (FR-17, transaction-engine.md §6, DM-11).';

-- TX-02: one non-terminal attempt per intent_id. Terminal states are those
-- not in the "still in flight" set (transaction-engine.md's own state
-- machine; the non-terminal set per data-model.md §7's own index spec is
-- exactly SUBMITTED and OBSERVED — the same set the frozen doc indexes for
-- "in flight" work).
CREATE UNIQUE INDEX transaction_attempts_one_nonterminal_per_intent
  ON transaction_attempts (intent_id) WHERE state IN ('SUBMITTED', 'OBSERVED');
COMMENT ON INDEX transaction_attempts_one_nonterminal_per_intent IS
  'TX-02: one non-terminal attempt per intent_id (data-model.md §7, phase-02-data-model.md §6).';

CREATE INDEX transaction_attempts_inflight_idx ON transaction_attempts (state) WHERE state IN ('SUBMITTED', 'OBSERVED');
COMMENT ON INDEX transaction_attempts_inflight_idx IS
  'Supports: the confirmation-watcher''s working set (data-model.md §7).';

CREATE INDEX transaction_attempts_intent_idx ON transaction_attempts (intent_id);
COMMENT ON INDEX transaction_attempts_intent_idx IS
  'Supports: "every attempt for this intent" (data-model.md §7).';

CREATE INDEX transaction_attempts_expiry_watch_idx
  ON transaction_attempts (last_valid_block_height) WHERE state = 'SUBMITTED';
COMMENT ON INDEX transaction_attempts_expiry_watch_idx IS
  'Supports: the expiry sweep — submitted attempts past their last_valid_block_height (data-model.md §7).';

-- reconciliation_mismatches — IMMUTABLE — owner: sentinel-risk, sentinel-executor.
CREATE TABLE reconciliation_mismatches (
  mismatch_id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  class                mismatch_class NOT NULL,
  intent_id            uuid REFERENCES execution_intents (intent_id),
  attempt_id           uuid REFERENCES transaction_attempts (attempt_id),
  entity_kind          text NOT NULL,
  entity_key           text NOT NULL,
  slot                 bigint NOT NULL,
  predicted             jsonb NOT NULL,
  actual                 jsonb NOT NULL,
  onchain_error_code       text,
  detected_at                timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE reconciliation_mismatches IS
  'IMMUTABLE — owner: sentinel-risk, sentinel-executor. data-model.md §7. Classes from '
  'aegis-integration.md §12. The first four classes (RACE_HEALED | RACE_LOST | ORACLE_CLOSED | PAUSED) '
  'are normal operation of a permissionless market and are counted, not alerted; the rest mean '
  'Sentinel is wrong about something.';

CREATE INDEX reconciliation_mismatches_class_slot_idx ON reconciliation_mismatches (class, slot);
COMMENT ON INDEX reconciliation_mismatches_class_slot_idx IS
  'Supports: per-class rate monitoring (data-model.md §7, aegis-integration.md §12).';

CREATE INDEX reconciliation_mismatches_entity_idx ON reconciliation_mismatches (entity_kind, entity_key);
COMMENT ON INDEX reconciliation_mismatches_entity_idx IS
  'Supports: "every mismatch for this entity" (data-model.md §7).';
