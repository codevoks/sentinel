# Sentinel — Data Model

**Status: FROZEN (Phase 0). Implementation in Phase 2 (schema) and thereafter.**

> Every table below states its primary key, its **natural / idempotency key**, its conflict policy, its
> mutability class, and who owns writes to it. A table without all five is not specified.

Mutability classes:
- **IMMUTABLE** — inserted once, never updated, never deleted by application code.
- **PROMOTABLE** — inserted once; a bounded set of fields may advance monotonically (commitment,
  canonical flag, status).
- **MATERIALIZED** — freely rewritten by its owning worker; fully rebuildable from upstream layers.
- **OPERATIONAL** — Sentinel's own records of what it did; not rebuildable, not chain-derived.

---

## 1. Layers and rebuildability

```
                      rebuildable from
raw            ──────────────────────────────► (nothing — the chain, by re-fetch)
normalized     ──────────────────────────────► raw
chain state    ──────────────────────────────► raw + normalized
protocol       ──────────────────────────────► normalized
derived        ──────────────────────────────► protocol
execution      ──────────────────────────────► NOTHING — this is Sentinel's own history
jobs/ops       ──────────────────────────────► NOTHING — transient control plane
```

**The replay guarantee (FR-13):** `TRUNCATE` everything in *protocol* and *derived*, replay, and get
byte-identical rows. `execution` is deliberately outside that boundary because it records actions, not
observations.

---

## 2. Raw layer

### `raw_observations` — IMMUTABLE — owner: `sentinel-ingest`

Defined in `ingestion-model.md` §5. Repeated here for the key contract only.

| | |
|---|---|
| PK | `observation_id bigserial` |
| Natural key | `(kind, natural_key, payload_hash)` **UNIQUE** |
| Conflict | `DO NOTHING` |
| Indexes | `(slot)`, `(kind, slot)`, `(provider_id, observed_at)`, `(natural_key)` |
| Partitioning | **Range on `slot`**, 10M-slot partitions. Enables cheap detach-and-archive without touching live data. |
| Retention | See §9 |

**Why the payload hash is in the uniqueness key:** two providers returning different bytes for the same
logical observation both persist, which is the evidence divergence detection needs
(`ingestion-model.md` §10). Any "last write wins" here destroys the mechanism.

### `decode_failures` — IMMUTABLE — owner: any normalizer/decoder

| | |
|---|---|
| PK | `failure_id bigserial` |
| Fields | `observation_id` FK, `stage`, `error_code` (`SEN-*`), `error_detail`, `decoder_version_id` NULL, `failed_at`, `retry_count` |
| Natural key | `(observation_id, stage, decoder_version_id)` **UNIQUE** |
| Conflict | `DO UPDATE SET retry_count = retry_count + 1, last_failed_at = now()` |
| Index | `(stage, failed_at)`, `(error_code)` |

Every decode failure keeps a pointer to the exact bytes, so a decoder fix replays precisely the inputs
that failed.

---

## 3. Normalized layer

Contains **no protocol-specific concepts**. This is the seam that makes a second protocol adapter
additive (`architecture.md` §5).

### `slots` — PROMOTABLE — owner: `sentinel-chainstate`

Defined in `finality-and-forks.md` §2.

| | |
|---|---|
| PK | `(slot, blockhash)` |
| Natural key | same |
| Conflict | `DO UPDATE` on `status`, `commitment`, `canonical`, `finalized_at` — **monotonic only** (enforced by a `CHECK` and by the promotion query's `WHERE` clause) |
| Indexes | `(slot) WHERE canonical`, `(commitment, slot)`, `(status)` |
| Notes | `blockhash` is NULL for skipped slots; a partial unique index enforces one skipped row per slot |

### `transactions` — PROMOTABLE — owner: `sentinel-normalize`

```
signature              bytea         -- 64 bytes
slot                   bigint
blockhash              bytea         -- the containing block, ties this row to a fork
transaction_index      int           -- position in the block; the ordering authority
version                enum: legacy | v0 | v1
num_required_signatures smallint
recent_blockhash       bytea
success                bool
error_code             text NULL     -- normalized program error, when present
fee_lamports           bigint
compute_units_consumed bigint NULL
compute_unit_limit     bigint NULL
priority_fee_lamports  bigint NULL
priority_fee_source    enum: config_mask | compute_budget_ix | absent
loaded_addresses_from_alt bool        -- v0 only; always false for v1 (v1 has no ALTs)
is_vote                bool
commitment             enum
canonical              bool
```

| | |
|---|---|
| PK | `(signature, slot, blockhash)` |
| Natural key | same |
| Conflict | `DO UPDATE` on `commitment`, `canonical` only |
| Indexes | `(signature)`, `(slot, transaction_index)`, `(slot) WHERE canonical AND NOT is_vote` |
| Partitioning | Range on `slot` |

**`priority_fee_source` is not optional.** Transaction v1 moves the priority fee out of `ComputeBudget`
instructions into a message-level `transactionConfig`, changes its unit from micro-lamports-per-CU to
absolute lamports, and any code that scans for ComputeBudget instructions returns **zero silently**
(`ecosystem-research.md` §2). Storing the two in one column without a discriminator corrupts every fee
metric in the system.

**`(signature, slot, blockhash)` rather than `signature` alone:** a signature is unique *on a chain*,
but Sentinel observes forks. Keying on the signature alone would force a lossy choice when the same
transaction is seen in two competing blocks. Consumers query the canonical view.

### `instructions` — IMMUTABLE — owner: `sentinel-normalize`

```
signature, slot, blockhash            -- FK to transactions
ix_index          smallint            -- top-level index
inner_index       smallint            -- -1 for top-level, else inner instruction index
stack_height      smallint
program_id        bytea
accounts          bytea[]             -- ordered account keys as passed
data              bytea               -- raw instruction data
```

| | |
|---|---|
| PK | `(signature, slot, blockhash, ix_index, inner_index)` |
| Conflict | `DO NOTHING` |
| Indexes | `(program_id, slot)`, `(slot)` |
| Partitioning | Range on `slot` |

### `program_logs` — IMMUTABLE — owner: `sentinel-normalize`

```
signature, slot, blockhash, log_index, program_id NULL, raw_line text,
kind enum: invoke | success | failure | data | log | consumed | unknown
```

| | |
|---|---|
| PK | `(signature, slot, blockhash, log_index)` |
| Conflict | `DO NOTHING` |
| Index | `(slot)`, `(kind, slot)` |

Program logs are stored **verbatim and in order**, because Anchor `emit!` events are base64 payloads on
`Program data:` lines and their decoding depends on the surrounding invoke/success framing to attribute
them to the right program.

### `account_observations` — IMMUTABLE — owner: `sentinel-normalize`

```
pubkey, slot, content_hash, owner_program, lamports, data bytea,
executable, rent_epoch, write_version bigint NULL,  -- NULL from RPC; present from Geyser
source enum, commitment enum
```

| | |
|---|---|
| PK | `(pubkey, slot, content_hash)` |
| Conflict | `DO NOTHING` |
| Indexes | `(pubkey, slot DESC)`, `(owner_program, slot)` |
| Partitioning | Range on `slot` |

**These are observations, not state.** They are sparse by construction: Agave 4.2 emits an update only
when an account actually changes, and Sentinel cannot subscribe to every position account. The absence
of a row means "not observed", never "unchanged" and never "missing".

### `token_balance_deltas` — IMMUTABLE — owner: `sentinel-normalize`

```
signature, slot, blockhash, account_index, token_account, mint, owner,
pre_amount numeric(39,0), post_amount numeric(39,0), decimals smallint, program_id
```

| | |
|---|---|
| PK | `(signature, slot, blockhash, account_index)` |
| Conflict | `DO NOTHING` |
| Indexes | `(token_account, slot)`, `(mint, slot)` |

Derived from `meta.pre/postTokenBalances`, **not** from parsed instruction JSON, whose shapes changed
in Agave 4.2 (`ecosystem-research.md` §1.4). `numeric(39,0)` holds a full `u128` exactly; `bigint`
would truncate and no floating type may appear anywhere in this system.

---

## 4. Chain-state layer

### `rollback_events` — IMMUTABLE — owner: `sentinel-chainstate`

`(rollback_id, detected_at, slot_low, slot_high, abandoned_block_count, depth_slots, cause, resolved_at)`
— the permanent record of every reorg Sentinel saw. Rollback depth is an alerting metric.

### `gap_events` — PROMOTABLE — owner: `sentinel-ingest`

`(gap_id, slot_start, slot_end, detected_at, repaired_at NULL, cause, attempts)`
UNIQUE `(slot_start, slot_end)` with `DO UPDATE` on `attempts`.

### `ingest_checkpoints` — PROMOTABLE — owner: `sentinel-ingest`

Defined in `ingestion-model.md` §6. PK `stream_name`. Advanced **in the same transaction** as the data
it covers, and **after** it.

### `provider_health` — MATERIALIZED — owner: `sentinel-rpc`

`(provider_id, window_start, requests, errors, timeouts, rate_limited, p50_ms, p95_ms, breaker_state, last_error_code)`
PK `(provider_id, window_start)`.

---

## 5. Protocol layer (Aegis)

Every row here carries `decoder_version_id` and `as_of_slot`/`as_of_commitment`.

### `decoder_versions` — IMMUTABLE — owner: migrations + `sentinel-aegis`

Defined in `aegis-integration.md` §8.1.

### `aegis_protocol_state` — MATERIALIZED

Singleton config: `admin`, `pending_admin`, `guardian`, `fee_recipient`, `paused` bits, `as_of_slot`,
`as_of_commitment`, `decoder_version_id`, `materialized_via`.
PK `program_id`.

### `aegis_markets` — MATERIALIZED — owner: `sentinel-aegis`

Every `Market` field (`aegis-integration.md` §4), plus:
`market_pubkey` PK, `program_id`, `as_of_slot`, `as_of_commitment`, `last_snapshot_slot`,
`materialized_via` (`event|snapshot`), `decoder_version_id`, `status`
(`current|recomputing|stale|unknown_schema`).

| | |
|---|---|
| PK | `market_pubkey` |
| Natural key | `market_pubkey` (which is itself derived from `(collateral_mint, loan_mint, config_id)`) |
| Conflict | `DO UPDATE` guarded by `WHERE excluded.as_of_slot > aegis_markets.as_of_slot` — **a stale write can never overwrite a newer one** |
| Index | `(collateral_mint, loan_mint)`, `(status)` |

### `aegis_market_params_history` — IMMUTABLE — owner: `sentinel-aegis`

The **versioned parameter set** required by `aegis-integration.md` §4.3: every risk, IRM, and oracle
parameter with `effective_from_slot` and `effective_to_slot`.

| | |
|---|---|
| PK | `(market_pubkey, effective_from_slot)` |
| Conflict | `DO NOTHING` |
| Why | Historical health **must** be recomputed with the parameters in force at that slot. Reading today's parameters to evaluate a past position is a silent correctness bug that only shows up in a backtest. |

### `aegis_positions` — MATERIALIZED — owner: `sentinel-aegis`

`position_pubkey` PK, `market_pubkey`, `owner`, `supply_shares numeric(39,0)`,
`borrow_shares numeric(39,0)`, `collateral_amount numeric(20,0)`, `is_open`, `as_of_slot`,
`as_of_commitment`, `last_snapshot_slot`, `materialized_via`, `decoder_version_id`, `status`.

| | |
|---|---|
| PK | `position_pubkey` |
| Conflict | `DO UPDATE ... WHERE excluded.as_of_slot > current.as_of_slot` |
| Indexes | `(market_pubkey) WHERE is_open`, `(owner)`, `(market_pubkey, borrow_shares) WHERE borrow_shares > 0` |

The last index is the keeper's working set: **only positions with debt can be liquidated.**

`is_open` rather than deletion: Aegis positions are closable and re-creatable at the same address
(`account-model.md` §10). Deleting on close would make the re-init look like corruption.

### `aegis_events` — IMMUTABLE — owner: `sentinel-aegis`

```
signature, slot, blockhash, ix_index, inner_index, log_index,
event_name text, market_pubkey NULL, position_pubkey NULL,
payload jsonb,               -- decoded fields, typed per event
decoder_version_id, as_of_commitment
```

| | |
|---|---|
| PK | `(signature, slot, blockhash, log_index)` |
| Conflict | `DO NOTHING` |
| Indexes | `(market_pubkey, slot, ix_index)`, `(position_pubkey, slot)`, `(event_name, slot)` |

**This table is the event projection's input and the position-history API's source.** It is ordered by
`(slot, transaction_index, ix_index, inner_index)` — never by arrival.

### `aegis_oracle_observations` — IMMUTABLE — owner: `sentinel-aegis`

```
feed_id bytea(32), publish_time timestamptz, slot, price numeric(39,0), conf numeric(39,0),
expo int, verification_level text, price_account pubkey,   -- provenance only, never identity
price_lo_wad numeric(39,0), price_hi_wad numeric(39,0),
validation_result enum: valid | failed,
failed_check text NULL       -- 'O-4' | 'O-5' | 'O-8' | ...
```

| | |
|---|---|
| PK | `(feed_id, publish_time, slot)` |
| Conflict | `DO NOTHING` |
| Indexes | `(feed_id, publish_time DESC)`, `(feed_id, slot DESC) WHERE validation_result='valid'` |

**Indexed by feed ID, not by account address** — the account is ephemeral and permissionlessly posted
(`aegis/oracle-design.md` §2, O-3). Storing `failed_check` is what makes "why is this market
fail-closed right now?" answerable with a specific cause.

### `aegis_invariant_checks` — IMMUTABLE — owner: `sentinel-risk`

`(check_id, invariant_id, market_pubkey, slot, expected numeric, actual numeric, holds bool, checked_at)`
— the off-chain checkable subset of Aegis's invariants (`aegis-integration.md` §7.1). Run **only on
finalized state**.

---

## 6. Derived layer

### `position_health` — MATERIALIZED — owner: `sentinel-risk`

```
position_pubkey, computed_at_slot, t_eval timestamptz,
collateral_value_wad numeric(39,0), debt_value_wad numeric(39,0),
debt_assets numeric(20,0), health_factor_wad numeric(39,0) NULL,
state enum: healthy | liquidatable | no_debt | unknown_oracle | stale,
liquidation_price_wad numeric(39,0) NULL,
borrow_capacity_wad numeric(39,0) NULL,
collateral_obs_id, loan_obs_id,       -- FK to the exact oracle observations used
market_params_from_slot,               -- FK to the parameter version used
commitment enum
```

| | |
|---|---|
| PK | `(position_pubkey, computed_at_slot)` |
| Conflict | `DO NOTHING` (a health value for a given slot is a fact about that slot) |
| Indexes | `(state, computed_at_slot DESC)`, `(position_pubkey, computed_at_slot DESC)` |
| Retention | Full history for positions with debt; sampled for the rest (§9) |

**Recording the oracle observation IDs and the parameter version is mandatory.** Without them a health
value cannot be explained, reproduced, or defended when Aegis rejects a liquidation built on it.

`state = unknown_oracle` is a first-class value (`aegis-integration.md` §5.1, H-3). A NULL health factor
with a `healthy` state would be a lie.

### `liquidation_candidates` — MATERIALIZED — owner: `sentinel-risk`

```
candidate_id, position_pubkey, market_pubkey, detected_at_slot, t_eval,
lookahead_ms int NOT NULL,              -- the expected_landing_latency used to derive t_eval
risk_params_hash text NOT NULL,         -- hash of every declared risk/profitability parameter
health_factor_wad, max_repay_assets numeric(20,0), expected_seize numeric(20,0),
expected_bonus numeric(20,0), expected_protocol_cut numeric(20,0),
estimated_profit_wad numeric(39,0), profitable bool, reason_unprofitable text NULL,
full_liquidation bool, dust_rule_applied bool,
expires_at timestamptz,
status enum: open | claimed | executed | expired | invalidated,
invalidated_reason text NULL
```

| | |
|---|---|
| PK | `candidate_id` |
| Natural key | `(position_pubkey, detected_at_slot)` **UNIQUE** |
| Conflict | `DO NOTHING` — re-detecting the same position at the same slot is a no-op |
| Indexes | `(status, estimated_profit_wad DESC) WHERE status='open'`, `(position_pubkey)` |

**`expires_at` is mandatory.** A candidate without an expiry is a stale instruction to spend money.

**`lookahead_ms` and `risk_params_hash` are mandatory, and they are what keep this table replayable.**
`t_eval` is derived from a *measured* landing latency, which is live data — so without persisting the
lookahead and the parameter hash, a replay would recompute `t_eval` from a different measurement and
produce different candidates, silently breaking determinism rule D-3. **Replay reads these values from
the row (or from the run manifest); it never re-measures.**

### `market_metrics` — MATERIALIZED — owner: `sentinel-risk`

Per market per slot bucket: utilization, borrow rate, supply rate, total supply/borrow assets and
shares, free liquidity, accrual staleness, open positions, positions with debt, aggregate bad debt.
PK `(market_pubkey, bucket_start)`.

### `alerts` — OPERATIONAL — owner: any

`(alert_id, kind, severity, entity_kind, entity_key, opened_at, resolved_at, detail jsonb, runbook_id)`
Natural key `(kind, entity_kind, entity_key)` **partial-unique WHERE resolved_at IS NULL** — so an
ongoing condition produces **one** open alert, not one per evaluation. That single constraint is the
difference between an alerting system and a pager DoS.

---

## 7. Execution layer

**OPERATIONAL. Not rebuildable. Not chain-derived. This is what Sentinel did.**

### `execution_intents` — owner: `sentinel-risk` (create), `sentinel-executor` (advance)

```
intent_id uuid PK
idempotency_key text NOT NULL UNIQUE      -- the business identity; see below
kind enum: liquidate | absorb_bad_debt | accrue_interest | custom
market_pubkey, position_pubkey NULL
params jsonb NOT NULL                     -- typed per kind; never raw transaction bytes
constraints jsonb NOT NULL                -- max_repay, min_profit, price_deadline, fee ceiling
state enum: CREATED | PLANNING | PLANNED | AWAITING_ATTEMPT | IN_FLIGHT
           | SUCCEEDED | FAILED | EXPIRED | CANCELLED | NEEDS_OPERATOR
created_at, updated_at, expires_at
lease_holder text NULL, lease_expires_at timestamptz NULL
attempt_count int, max_attempts int
cumulative_fee_lamports bigint
terminal_reason text NULL
trigger_slot bigint, trigger_commitment enum   -- what evidence caused this
```

**The idempotency key is the whole point of this table.** For a liquidation it takes one of two forms:

```
first in a bucket:      liquidate:{program_id}:{market}:{position}:{trigger_epoch_bucket}
follow-up after a
partial liquidation:    liquidate:{program_id}:{market}:{position}:after:{prev_signature}
```

`trigger_epoch_bucket` is a coarse time bucket (configured, e.g. 60s), **not the trigger slot**. Using
the slot would allow a new intent every slot for the same unhealthy position — exactly the
duplicate-execution hazard. A bucket means at most one intent per position per bucket, and a position
still unhealthy in the next bucket legitimately gets a new one.

**The second form exists because Aegis liquidation is partial by default** (`close_factor`). A position
can remain liquidatable *immediately after* a successful liquidation, and a bucket-only key would
suppress the legitimate follow-up until the next bucket — leaving the position under-liquidated for up
to a full bucket. Keying the follow-up on the **previous successful signature** permits it at once
while staying deterministic and duplicate-proof: the signature is a settled on-chain fact, so two
workers computing the key independently compute the same one.

**Rule:** the follow-up form is used **only** when the previous intent reached `SUCCEEDED` with a
finalized attempt. It is never derived from a pending, expired, or failed attempt — that would
reintroduce the duplicate hazard by making the key depend on an unsettled outcome.

### `transaction_attempts` — owner: `sentinel-executor`

```
attempt_id uuid PK
intent_id uuid NOT NULL FK
attempt_number int NOT NULL
signature bytea NOT NULL UNIQUE            -- known BEFORE submission
transaction_bytes bytea NOT NULL           -- the exact signed message; resubmittable verbatim
transaction_version enum
recent_blockhash bytea NOT NULL
last_valid_block_height bigint NOT NULL    -- the definitive expiry oracle
compute_unit_limit int, priority_fee_lamports bigint
simulation_result jsonb, simulated_units int
state enum: SIGNED | SUBMITTED | OBSERVED | CONFIRMED | FINALIZED
           | FAILED_ONCHAIN | EXPIRED | ABANDONED_PRE_SUBMIT | UNKNOWN
signed_at, submitted_at NULL, observed_at NULL, resolved_at NULL
observed_slot NULL, onchain_error_code NULL, onchain_error_band NULL
UNIQUE (intent_id, attempt_number)
```

| | |
|---|---|
| Conflict | Never — inserts only, with `signature` UNIQUE as the safety net |
| Indexes | `(state) WHERE state IN ('SUBMITTED','OBSERVED')`, `(intent_id)`, `(last_valid_block_height) WHERE state='SUBMITTED'` |

**`signature` and `transaction_bytes` are persisted and committed BEFORE `sendTransaction` is called.**
This is FR-17 and it is the single most important row-ordering decision in Sentinel
(`transaction-engine.md` §6).

### `reconciliation_mismatches` — IMMUTABLE — owner: `sentinel-risk`, `sentinel-executor`

```
mismatch_id, class enum (RACE_HEALED | RACE_LOST | ORACLE_CLOSED | PAUSED | SIZE_REJECTED
                          | MODEL_DIVERGENCE | ACCOUNT_REJECTED | PROJECTION_DIVERGENCE | UNKNOWN),
intent_id NULL, attempt_id NULL, entity_kind, entity_key, slot,
predicted jsonb, actual jsonb, onchain_error_code NULL, detected_at
```

Classes are from `aegis-integration.md` §12. The first four are normal operation of a permissionless
market and are counted, not alerted. The rest mean Sentinel is wrong about something.

---

## 8. Control plane

### `jobs` — OPERATIONAL — owner: `sentinel-jobs`

```
job_id bigserial PK
kind enum: backfill_range | replay_range | rematerialize_entity | snapshot_accounts
         | reconcile_provider | recompute_after_rollback | scan_program_accounts
dedupe_key text NOT NULL                    -- UNIQUE among non-terminal jobs
payload jsonb, priority smallint,
state enum: queued | leased | done | failed | quarantined
lease_holder text NULL, lease_expires_at timestamptz NULL,
attempts int, max_attempts int, last_error text NULL,
available_at timestamptz, created_at, updated_at
```

| | |
|---|---|
| Uniqueness | Partial unique index on `dedupe_key WHERE state IN ('queued','leased')` — enqueueing the same work twice is a no-op while it is outstanding |
| Claim | `SELECT ... WHERE state='queued' AND available_at <= now() ORDER BY priority, job_id FOR UPDATE SKIP LOCKED LIMIT n` |
| Wake | `LISTEN/NOTIFY` for latency, plus a polling floor so a missed notification costs latency and never correctness |
| Quarantine | After `max_attempts`, state becomes `quarantined` and an alert opens. **A quarantined job is never silently dropped and never retried forever** (`distributed-correctness.md` §7). |

This table is the answer to "is a queue necessary?" — a queue is necessary; **a broker is not**
(ADR-0004). Transactional enqueue in the same commit as the state change that caused it is a property
no external broker gives for free.

---

## 9. Retention and growth

| Table | Growth driver | Policy |
|---|---|---|
| `raw_observations` | Every block ingested | **Hot** (full payload): configurable window, default 30 days. **Cold**: partitions detached and archived to compressed files with a manifest; the manifest row stays so provenance is never lost. Replay over a cold range requires re-attach or re-fetch, and the CLI says so. |
| `instructions`, `program_logs`, `token_balance_deltas` | Same | Partitioned by slot; the same detach policy, longer default window |
| `account_observations` | Only accounts Sentinel watches — small by construction | Full retention for Aegis accounts; windowed for others |
| `aegis_events` | Aegis activity | **Full retention, forever.** It is the position-history product and it is small. |
| `position_health` | Positions × evaluation frequency | Full for positions with debt; downsampled to one row per bucket for the rest |
| `transaction_attempts` | Keeper activity | Full retention. It is the execution audit trail. |
| `jobs` | Work volume | Terminal jobs pruned after a window; quarantined jobs **never** auto-pruned |

**Adoption thresholds, so a second datastore is a measured decision and not a preference:**
introduce columnar/time-series storage only if (a) `raw_observations` hot-window size exceeds a stated
disk budget with the retention window already at its minimum useful value, **or** (b) a required
analytical query's p95 exceeds its target with correct indexes and partition pruning in place.
Both must be demonstrated with numbers from the Phase 14 campaign.

---

## 10. Table ownership

One writer per table. Enforced by distinct database roles per service, in **every** environment
including local — because a permission model that is only on in production is not a permission model.

| Writer | Tables |
|---|---|
| `sentinel-ingest` | `raw_observations`, `ingest_checkpoints`, `gap_events` |
| `sentinel-normalize` | `transactions`, `instructions`, `program_logs`, `account_observations`, `token_balance_deltas`, `decode_failures` |
| `sentinel-chainstate` | `slots`, `rollback_events`, and the `canonical`/`commitment` columns of normalized tables |
| `sentinel-aegis` | `aegis_*` except `aegis_invariant_checks` |
| `sentinel-risk` | `position_health`, `liquidation_candidates`, `market_metrics`, `aegis_invariant_checks`, `execution_intents` (insert only), `reconciliation_mismatches` |
| `sentinel-executor` (TS) | `transaction_attempts`, `execution_intents` (state advance only), `reconciliation_mismatches` |
| `sentinel-api` (TS) | **nothing** — read-only role, plus insert on `execution_intents` for operator-initiated actions |
| `sentinel-jobs` | `jobs` |

**The TypeScript services hold a role with no write grant on raw, normalized, chain-state, protocol, or
derived tables.** That is what makes `architecture.md` §5's language boundary structural rather than a
convention.

---

## 11. Migration policy

1. **Additive by default.** New columns are nullable or defaulted. A column is never repurposed.
2. **Never destructive in one step.** Drops happen a release after the last reader is gone, in a
   separate migration.
3. **Every migration is forward-only** and numbered. There are no down-migrations; recovery is restore
   plus replay, which is a capability Sentinel has and a down-migration is not.
4. **A migration that changes decoded semantics is paired with a new `decoder_versions` row and a
   replay job**, never with an in-place `UPDATE` of decoded rows (`aegis-integration.md` §8.3).
5. Partition creation is automated ahead of the head slot with a stated lead time; running out of
   partitions is an outage and has an alert.

---

## 12. Invariants

| ID | Invariant | Checked by |
|---|---|---|
| DM-01 | Every normalized row references at least one raw observation | FK + replay test |
| DM-02 | No application code updates or deletes `raw_observations` | Role permissions + test |
| DM-03 | Every table has a declared natural key and conflict policy | Schema review checklist + a test that asserts a unique index exists per declared natural key |
| DM-04 | A stale materialization write can never overwrite a newer one | `WHERE excluded.as_of_slot > current.as_of_slot` + concurrency test |
| DM-05 | Every protocol and derived row names its `decoder_version_id` | `NOT NULL` |
| DM-06 | `TRUNCATE protocol + derived` then replay yields byte-identical rows | Determinism test |
| DM-07 | No monetary or share quantity uses a floating-point type anywhere | CI grep + schema type audit (`numeric(39,0)` for `u128`, `numeric(20,0)` for `u64`) |
| DM-08 | One open alert per `(kind, entity)` | Partial unique index |
| DM-09 | One outstanding job per `dedupe_key` | Partial unique index |
| DM-10 | `execution_intents.idempotency_key` is globally unique | Unique index + duplicate-creation test |
| DM-11 | Every `transaction_attempts` row has its `signature` committed before submission | Crash-injection test |
| DM-12 | Historical health uses the market parameters in force at that slot | Backtest against `aegis_market_params_history` |
