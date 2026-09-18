-- Phase 2 — raw layer: raw_observations, decode_failures
-- docs/data-model.md §2, docs/ingestion-model.md §5 (frozen; ingestion-model.md
-- is the authoritative full-column source, data-model.md repeats only the
-- key contract).

-- raw_observations — IMMUTABLE — owner: sentinel-ingest (sentinel_rust role).
-- Slot-range partitioned, 10,000,000 slots per partition
-- (docs/data-model.md §2: "Range on slot, 10M-slot partitions").
--
-- observation_id is declared `bigint GENERATED ALWAYS AS IDENTITY` rather
-- than a partition-unaware `bigserial` sequence default, and is NOT the
-- partition key (slot is) — Postgres requires the partition key to be part
-- of every unique/primary constraint, and observation_id alone cannot be a
-- PRIMARY KEY on a partitioned table for that reason. It is unique **within
-- a table declaration** via a composite key including slot; global identity
-- is preserved because the identity sequence is shared across partitions.
CREATE TABLE raw_observations (
  observation_id  bigint GENERATED ALWAYS AS IDENTITY,
  kind            raw_observation_kind NOT NULL,
  natural_key     text NOT NULL,
  slot            bigint NOT NULL, -- partition key; slot_status rows use a sentinel slot of 0 when no slot is known (see note below)
  commitment      commitment_level NOT NULL,
  source          observation_source NOT NULL,
  provider_id     text NOT NULL,
  request_id      uuid,
  observed_at     timestamptz NOT NULL DEFAULT now(),
  payload         bytea NOT NULL,
  payload_hash    bytea NOT NULL,
  payload_encoding payload_encoding NOT NULL,

  -- ingestion-model.md §5: "UNIQUE (kind, natural_key, payload_hash)". slot
  -- must be included because it is the partition key (a Postgres
  -- requirement for any unique index on a partitioned table), but it is
  -- already embedded in every kind's natural_key (§5.1), so this adds no
  -- real width to the natural key's semantics.
  CONSTRAINT raw_observations_pk PRIMARY KEY (observation_id, slot),
  CONSTRAINT raw_observations_natural_key UNIQUE (kind, natural_key, payload_hash, slot)
) PARTITION BY RANGE (slot);

COMMENT ON TABLE raw_observations IS
  'IMMUTABLE, append-only raw observation boundary (ingestion-model.md §5, ADR-0008). '
  'DM-02: no application role may UPDATE or DELETE this table — enforced by GRANT, not by convention.';

COMMENT ON COLUMN raw_observations.slot IS
  'Best-known slot association. slot_status rows for a not-yet-assigned slot use the '
  'observed slot itself (slot_status observations always carry a concrete slot per '
  'ingestion-model.md §5.1 natural key "{slot}:{status}"), so this column is NOT NULL '
  'for every kind in practice; NULL is not used.';

-- (slot) — every partition-pruned range scan and the partition-routing
-- check itself use this implicitly via the partition key; an explicit
-- b-tree is still useful for cross-partition slot lookups that do not
-- know the partition ahead of time.
CREATE INDEX raw_observations_slot_idx ON raw_observations (slot);
COMMENT ON INDEX raw_observations_slot_idx IS
  'Supports: range scans and gap-repair queries over a slot window (ingestion-model.md §6/§9).';

-- (kind, slot) — per-kind backfill/replay range queries (e.g. "every
-- transaction observation between slot A and B").
CREATE INDEX raw_observations_kind_slot_idx ON raw_observations (kind, slot);
COMMENT ON INDEX raw_observations_kind_slot_idx IS
  'Supports: per-kind replay/backfill range reads (docs/replay-and-backfill.md).';

-- (provider_id, observed_at) — provider health / divergence reporting
-- (ingestion-model.md §10).
CREATE INDEX raw_observations_provider_observed_idx ON raw_observations (provider_id, observed_at);
COMMENT ON INDEX raw_observations_provider_observed_idx IS
  'Supports: per-provider health and divergence-rate queries (ingestion-model.md §10).';

-- (natural_key) — divergence detection joins observations sharing a
-- natural key across providers/payload hashes (ingestion-model.md §10).
CREATE INDEX raw_observations_natural_key_idx ON raw_observations (natural_key);
COMMENT ON INDEX raw_observations_natural_key_idx IS
  'Supports: divergence detection — find every observation sharing a natural key across payload hashes (ingestion-model.md §10).';

-- decode_failures — IMMUTABLE — owner: any normalizer/decoder (sentinel_rust role).
CREATE TABLE decode_failures (
  failure_id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  observation_id      bigint NOT NULL,
  observation_slot    bigint NOT NULL, -- part of the FK to raw_observations' composite PK
  stage               text NOT NULL,
  error_code          text NOT NULL,
  error_detail        text,
  decoder_version_id  bigint,
  failed_at           timestamptz NOT NULL DEFAULT now(),
  last_failed_at      timestamptz NOT NULL DEFAULT now(),
  retry_count         int NOT NULL DEFAULT 0,

  CONSTRAINT decode_failures_observation_fk
    FOREIGN KEY (observation_id, observation_slot) REFERENCES raw_observations (observation_id, slot),
  -- data-model.md §2: natural key (observation_id, stage, decoder_version_id) UNIQUE.
  -- decoder_version_id is nullable, and NULL <> NULL in a plain UNIQUE
  -- constraint would let duplicate (observation_id, stage, NULL) rows
  -- through, so the NULL case is normalized to a fixed sentinel via a
  -- unique index expression instead of a bare column constraint.
  CONSTRAINT decode_failures_retry_count_nonneg CHECK (retry_count >= 0)
);

COMMENT ON TABLE decode_failures IS
  'IMMUTABLE — owner: any Rust decoder/normalizer. Keeps a pointer to the exact failing bytes '
  '(data-model.md §2) so a decoder fix can replay precisely the inputs that failed.';

-- Natural key (DM-03): (observation_id, stage, decoder_version_id), with
-- decoder_version_id's NULL treated as a single fixed value (-1, which is
-- not a legal decoder_versions id — see 0006_protocol_layer.sql, IDs start
-- at 1) so two failures for the same observation/stage with no decoder
-- version collide exactly like two failures that share a real version id.
CREATE UNIQUE INDEX decode_failures_natural_key
  ON decode_failures (observation_id, stage, COALESCE(decoder_version_id, -1));
COMMENT ON INDEX decode_failures_natural_key IS
  'DM-03 natural key: (observation_id, stage, decoder_version_id) — data-model.md §2.';

-- (stage, failed_at) — decode-failure-rate alerting per stage (ingestion-model.md §11: "rate is a metric with an alert").
CREATE INDEX decode_failures_stage_failed_at_idx ON decode_failures (stage, failed_at);
COMMENT ON INDEX decode_failures_stage_failed_at_idx IS
  'Supports: decode-failure-rate alerting per pipeline stage (ingestion-model.md §11).';

-- (error_code) — "which SEN-* code is spiking" queries.
CREATE INDEX decode_failures_error_code_idx ON decode_failures (error_code);
COMMENT ON INDEX decode_failures_error_code_idx IS
  'Supports: error-code-specific triage queries (architecture.md error taxonomy).';
