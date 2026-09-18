-- Phase 2 — control plane: jobs. docs/data-model.md §8, docs/distributed-correctness.md §3/§7.

CREATE TABLE jobs (
  job_id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  kind              job_kind NOT NULL,
  dedupe_key        text NOT NULL,
  payload           jsonb NOT NULL,
  priority          smallint NOT NULL DEFAULT 0,
  state             job_state NOT NULL DEFAULT 'queued',
  lease_holder      text,
  lease_expires_at  timestamptz,
  attempts          int NOT NULL DEFAULT 0,
  max_attempts      int NOT NULL,
  last_error        text,
  available_at      timestamptz NOT NULL DEFAULT now(),
  created_at        timestamptz NOT NULL DEFAULT now(),
  updated_at        timestamptz NOT NULL DEFAULT now(),

  CONSTRAINT jobs_attempts_nonneg CHECK (attempts >= 0),
  CONSTRAINT jobs_max_attempts_positive CHECK (max_attempts > 0)
);

COMMENT ON TABLE jobs IS
  'OPERATIONAL — owner: sentinel-jobs. data-model.md §8. A queue is necessary; a broker is not '
  '(ADR-0004). Transactional enqueue in the same commit as the state change that caused it.';

-- DM-09: one outstanding job per dedupe_key. "Outstanding" = queued or
-- leased (data-model.md §8's own definition of the partial index predicate).
CREATE UNIQUE INDEX jobs_one_outstanding_per_dedupe_key
  ON jobs (dedupe_key) WHERE state IN ('queued', 'leased');
COMMENT ON INDEX jobs_one_outstanding_per_dedupe_key IS
  'DM-09: enqueueing the same work twice is a no-op while it is outstanding (data-model.md §8).';

-- The claim query's exact shape (data-model.md §8):
--   SELECT ... WHERE state='queued' AND available_at <= now()
--   ORDER BY priority, job_id FOR UPDATE SKIP LOCKED LIMIT n
CREATE INDEX jobs_claimable_idx ON jobs (priority, job_id) WHERE state = 'queued';
COMMENT ON INDEX jobs_claimable_idx IS
  'Supports the exact claim query in data-model.md §8: WHERE state=queued AND available_at<=now() '
  'ORDER BY priority, job_id FOR UPDATE SKIP LOCKED LIMIT n. available_at is checked at query time '
  '(not indexed) because it is a small, highly-selective range scan over an already-narrow partial '
  'index; a composite (priority, job_id) index lets the ORDER BY be satisfied without a sort.';

CREATE INDEX jobs_quarantined_idx ON jobs (kind) WHERE state = 'quarantined';
COMMENT ON INDEX jobs_quarantined_idx IS
  'Supports: quarantine-depth-by-kind alerting (data-model.md §8, distributed-correctness.md §7, Q-4).';

CREATE INDEX jobs_lease_expiry_idx ON jobs (lease_expires_at) WHERE state = 'leased';
COMMENT ON INDEX jobs_lease_expiry_idx IS
  'Supports: the lease-reclaim sweep for leased jobs whose holder went silent (distributed-correctness.md §3).';

-- LISTEN/NOTIFY wake (data-model.md §8: "LISTEN/NOTIFY for latency, plus a
-- polling floor so a missed notification costs latency and never
-- correctness"). This is deliberately NOT a trigger (Phase 2 explicit
-- non-scope: "No triggers. No stored procedures.") — the enqueue query
-- itself issues `NOTIFY sentinel_jobs` in the same statement/transaction as
-- the INSERT, from application code (crates/sentinel-jobs), so there is no
-- function or trigger object in the schema. Workers always also poll on a
-- floor interval regardless of notifications, so a dropped NOTIFY (Postgres
-- does not queue notifications for a channel with no active LISTENer, and a
-- notification sent to a connection that misses it is simply gone) costs
-- latency, never correctness (docs/architecture.md, ADR-0004).
