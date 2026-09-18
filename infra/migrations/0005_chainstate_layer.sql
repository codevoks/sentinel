-- Phase 2 — chain-state layer.
-- docs/data-model.md §4.

-- rollback_events — IMMUTABLE — owner: sentinel-chainstate.
CREATE TABLE rollback_events (
  rollback_id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  detected_at            timestamptz NOT NULL DEFAULT now(),
  slot_low               bigint NOT NULL,
  slot_high              bigint NOT NULL,
  abandoned_block_count  int NOT NULL,
  depth_slots            bigint NOT NULL,
  cause                  text NOT NULL,
  resolved_at            timestamptz,

  CONSTRAINT rollback_events_range_valid CHECK (slot_high >= slot_low),
  CONSTRAINT rollback_events_counts_nonneg CHECK (abandoned_block_count >= 0 AND depth_slots >= 0)
);

COMMENT ON TABLE rollback_events IS
  'IMMUTABLE — owner: sentinel-chainstate. data-model.md §4: the permanent record of every reorg '
  'Sentinel saw. No declared natural key in the frozen doc — a reorg is a point-in-time event, not an '
  'idempotent upsert target; DM-03''s "declared natural key" requirement applies to tables that state '
  'one, and this table intentionally does not (rollback detection append-only inserts one row per '
  'detection event, not deduplicated by design).';

CREATE INDEX rollback_events_slot_range_idx ON rollback_events (slot_low, slot_high);
COMMENT ON INDEX rollback_events_slot_range_idx IS
  'Supports: "was there a reorg affecting slot N" queries (data-model.md §4).';

-- gap_events — PROMOTABLE — owner: sentinel-ingest.
CREATE TABLE gap_events (
  gap_id        bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  slot_start    bigint NOT NULL,
  slot_end      bigint NOT NULL,
  detected_at   timestamptz NOT NULL DEFAULT now(),
  repaired_at   timestamptz,
  cause         text NOT NULL,
  attempts      int NOT NULL DEFAULT 0,

  CONSTRAINT gap_events_range_valid CHECK (slot_end >= slot_start),
  CONSTRAINT gap_events_attempts_nonneg CHECK (attempts >= 0),
  CONSTRAINT gap_events_natural_key UNIQUE (slot_start, slot_end)
);

COMMENT ON TABLE gap_events IS
  'PROMOTABLE — owner: sentinel-ingest. data-model.md §4. Natural key (slot_start, slot_end) UNIQUE, '
  'DO UPDATE on attempts.';

CREATE INDEX gap_events_unrepaired_idx ON gap_events (slot_start) WHERE repaired_at IS NULL;
COMMENT ON INDEX gap_events_unrepaired_idx IS
  'Supports: the gap-repair worker''s claim query — outstanding (unrepaired) gaps (ingestion-model.md §9).';

-- ingest_checkpoints — PROMOTABLE — owner: sentinel-ingest.
-- docs/ingestion-model.md §6 (frozen; full column set).
CREATE TABLE ingest_checkpoints (
  stream_name           text PRIMARY KEY,
  last_contiguous_slot  bigint NOT NULL,
  head_slot             bigint NOT NULL,
  commitment            commitment_level NOT NULL,
  updated_at            timestamptz NOT NULL DEFAULT now(),
  holder                text,

  CONSTRAINT ingest_checkpoints_head_ge_contiguous CHECK (head_slot >= last_contiguous_slot)
);

COMMENT ON TABLE ingest_checkpoints IS
  'PROMOTABLE — owner: sentinel-ingest. ingestion-model.md §6. PK stream_name is the natural key. '
  'Advanced in the same transaction as the data it covers, and after it (ingestion-model.md §6).';

-- provider_health — MATERIALIZED — owner: sentinel-rpc.
CREATE TABLE provider_health (
  provider_id     text NOT NULL,
  window_start    timestamptz NOT NULL,
  requests        bigint NOT NULL DEFAULT 0,
  errors          bigint NOT NULL DEFAULT 0,
  timeouts        bigint NOT NULL DEFAULT 0,
  rate_limited    bigint NOT NULL DEFAULT 0,
  p50_ms          bigint,
  p95_ms          bigint,
  breaker_state   text NOT NULL,
  last_error_code text,

  CONSTRAINT provider_health_pk PRIMARY KEY (provider_id, window_start),
  CONSTRAINT provider_health_counts_nonneg
    CHECK (requests >= 0 AND errors >= 0 AND timeouts >= 0 AND rate_limited >= 0)
);

COMMENT ON TABLE provider_health IS
  'MATERIALIZED — owner: sentinel-rpc. data-model.md §4. p50_ms/p95_ms are integer millisecond '
  'latencies (bigint), never float — DM-07.';
