# Sentinel — Replay, Backfill and Determinism

**Status: FROZEN (Phase 0). Implementation in Phase 5, proven in Phase 8, regression-gated thereafter.**

> The flagship correctness property: **delete every derived and protocol row, replay the raw
> observations, and reproduce byte-identical state.** Everything in this document exists to make that
> sentence true and continuously verified rather than aspirational.

---

## 1. Four operations, deliberately distinguished

They are constantly confused, and confusing them produces either duplicated effects or silent gaps.

| Operation | Reads | Writes | Fetches from chain? | Purpose |
|---|---|---|---|---|
| **Backfill** | — | raw | **Yes** | Acquire raw observations Sentinel never had (gap repair, cold start, historical import) |
| **Replay** | raw | normalized → protocol → derived | **No** | Re-derive downstream layers from raw Sentinel already holds |
| **Rematerialization** | protocol events | protocol materialized + derived | No | Rebuild one entity's state from its event history |
| **Recompute** | protocol + derived | derived | No | Recompute derived values (health, candidates) without touching materialization |

**Rule:** replay **never** touches the network. If a replay needs a byte Sentinel does not have, it
fails with a specific error naming the missing observation and enqueues a backfill — it does not
silently fetch. That separation is what makes replay deterministic.

---

## 2. Determinism requirements

A replay is deterministic if, given the same raw rows, it produces the same downstream rows. Every
processor must therefore satisfy all five:

| # | Requirement | Concrete rule |
|---|---|---|
| D-1 | **No wall-clock input** | A processor never calls `now()`. Every time value comes from the block (`block_time`) or from an explicit `t_eval` parameter. Wall clock appears only in `observed_at`/`created_at` audit columns, which are excluded from determinism comparison. |
| D-2 | **No randomness** | No random IDs in derived rows. Surrogate keys are deterministic: derived from `(entity, slot)`, or the row has no surrogate key at all. |
| D-3 | **No ambient configuration** | Every parameter that affects output is either read from chain state (market params, versioned by slot) or is a **declared, hashed** replay parameter recorded with the run. **Any value derived from a live measurement — notably the keeper's `expected_landing_latency`, which sets `t_eval` — is persisted on the row that used it (`position_health.t_eval`, `liquidation_candidates.lookahead_ms`, `risk_params_hash`) and replay reads it back rather than re-measuring.** This is the one place a live measurement leaks into a deterministic processor, and it is closed explicitly. |
| D-4 | **Deterministic ordering** | Processing order is `(slot, transaction_index, ix_index, inner_index)` — read from the data, never from arrival order, job scheduling, or a `LIMIT` without `ORDER BY`. |
| D-5 | **No dependency on prior derived state** | A processor's output is a function of its inputs and the entity's state at a **finalized** anchor point, never of whatever happened to be in the derived table. |

D-3's "declared, hashed" clause is what catches the sneaky failures: an operator changing a health
evaluation lookahead between two replays produces different candidates, and the run manifest makes that
visible instead of mysterious.

### 2.1 The determinism harness

```
replay_runs
  run_id uuid PK
  slot_low, slot_high
  scope enum: normalize | protocol | derived | all
  decoder_version_set  jsonb    -- which decoder versions were active
  params_hash          text     -- hash of every declared replay parameter
  code_version         text     -- git sha
  started_at, finished_at
  output_digest        text     -- see below
  status enum: running | complete | failed | mismatch
```

`output_digest` is a stable hash over the replayed rows: for each affected table, rows ordered by their
natural key, with audit-only columns (`observed_at`, `created_at`, surrogate sequence IDs) excluded, and
`numeric` values in canonical form. Two runs with the same `(slot range, decoder_version_set,
params_hash, code_version)` **must** produce the same `output_digest`. A mismatch fails CI.

This makes "is it deterministic?" a single-value comparison rather than a diffing exercise.

---

## 3. Idempotent materialization

Replay writes into tables that may already contain rows. Every write is one of:

| Class | Policy | Example |
|---|---|---|
| Immutable fact | `INSERT ... ON CONFLICT DO NOTHING` | `instructions`, `aegis_events` |
| Promotable | `INSERT ... ON CONFLICT DO UPDATE` with a **monotonicity guard** in the `WHERE` | `slots.commitment` |
| Materialized snapshot | `INSERT ... ON CONFLICT DO UPDATE ... WHERE excluded.as_of_slot > current.as_of_slot` | `aegis_markets`, `aegis_positions` |
| Derived-per-slot | `ON CONFLICT DO NOTHING` (a value at a slot is a fact about that slot) | `position_health` |

**No processor issues an unguarded `UPDATE` or a `DELETE` of a row it did not create in this run.**
Full-scope replay truncates the target tables up front; scoped replay does not, which is why the guards
must be right.

---

## 4. Replay boundaries

A replay is always bounded by a slot range and a scope, and **the range must start at a safe anchor.**

```
Safe anchor rules:
  - normalize scope:  any slot. Normalization is per-transaction and stateless.
  - protocol scope:   the entity's last FINALIZED materialization checkpoint, or the
                      slot of its creation event (MarketCreated / PositionInitialized).
  - derived scope:    any slot, provided the protocol state at that slot is `current`.
```

Starting a protocol replay mid-history from an arbitrary slot without an anchor would apply events to
whatever state happened to be there — non-deterministic by construction. The anchor is what makes
"rebuild forward, never subtract" (`finality-and-forks.md` RB-3) implementable.

---

## 5. Parallelism and safety

| Operation | Parallel? | Isolation mechanism |
|---|---|---|
| Backfill | Yes | Disjoint leased slot ranges; raw writes are `DO NOTHING` so overlap is harmless anyway |
| Normalize replay | Yes | Per-block; blocks are independent |
| Protocol replay | **Serialized per market** | Advisory lock on the market. Markets are independent (Aegis ADR-0004 makes this true on-chain; Sentinel inherits it) |
| Derived recompute | Yes | Per position, read-only against a pinned protocol state |
| Full-scope replay | **Exclusive** | A global replay lock; the API reports `REBUILDING` |

**Replay while realtime ingestion continues is supported and is the normal case.** It is safe because:
- Replay only writes downstream layers; ingestion only writes raw.
- Materialization guards (`as_of_slot > current`) prevent a replay of old slots from overwriting newer
  live state.
- The chain-state engine's advisory lock serializes promotion/rollback against nothing replay does.

The one case that is **not** safe and is explicitly forbidden: a full-scope replay that truncates
protocol tables while the keeper is running. The replay driver **must** disable candidate creation for
the duration and re-enable it only after materialization is `current`. Enforced by a flag the risk
worker checks, and covered by a failure-injection test.

---

## 6. Decoder upgrades

The workflow, which is the payoff for the raw boundary (ADR-0008):

```
1. Register the new decoder version with effective_from_slot.
2. Enqueue a protocol-scope replay over the affected range.
3. Replay writes new protocol rows tagged with the new decoder_version_id.
4. Old rows are RETAINED under their old decoder_version_id — never updated in place.
5. Derived state is recomputed from the new protocol rows.
6. The run's output_digest is recorded and compared against expectation.
```

Consequences:
- **Historical records stay interpretable.** A row always names the decoder that produced it, and the
  bytes it came from are still in `raw_observations`.
- **A decoder bug found in month three is fixable retroactively**, which is not true of any pipeline
  that decodes on ingest and discards the input.
- **Two decoder versions can coexist** across a version boundary slot, which is exactly what an Aegis
  program upgrade produces.

---

## 7. Acceptance criteria — the replay proof

These are the criteria a phase must satisfy to claim replayability. They are executable, not
descriptive.

| ID | Criterion |
|---|---|
| **RP-01** | `TRUNCATE` every protocol and derived table; run a full-scope replay over the fixture range; `output_digest` equals the pre-truncation digest. |
| **RP-02** | Run the same replay twice; both digests match. |
| **RP-03** | Run the replay with the range split into N disjoint sub-ranges processed in a shuffled order; the digest matches the single-range run. |
| **RP-04** | Kill the replay worker at a random point (≥20 randomized kill points across the corpus); restart; the final digest matches. |
| **RP-05** | Duplicate 10% of the raw rows (same natural key, same payload); replay; the digest is unchanged and no duplicate downstream row exists. |
| **RP-06** | Inject a fork into the fixture corpus; replay; the digest matches a corpus where the abandoned blocks were never observed. |
| **RP-07** | Rematerialize a single market from its creation event; the result equals the full-replay result for that market. |
| **RP-08** | Register a second decoder version over part of the range; replay; rows carry the correct `decoder_version_id` per slot and no old row was mutated. |
| **RP-09** | Replay with a missing raw observation fails with `SEN-RPL-001` naming the observation and enqueues a backfill — it does **not** fetch and does **not** silently continue. |
| **RP-10** | Replay concurrent with live ingestion leaves live state correct and the replayed range correct; assertion runs over both. |
| **RP-11** | Aegis conformance vectors (`aegis-integration.md` §6.2) reproduce exactly after a full replay. |
| **RP-12** | The off-chain-checkable Aegis invariants hold over every finalized slot in the replayed corpus. |

**RP-01 through RP-03 run in CI on every commit** over a small committed fixture corpus. RP-04 through
RP-12 run in the failure-injection and nightly tiers (`testing-strategy.md` §3).

---

## 8. Fixture corpus

Replay tests need real chain data that is committed, small, deterministic, and free.

| Property | Decision |
|---|---|
| Source | Captured from a **local Surfpool cluster** running a scripted Aegis scenario — market creation, supply, borrow, deterministic price movement, liquidation, bad debt. Byte-exact Pyth accounts are injected exactly as Aegis's own test kit does (`aegis/oracle-design.md` §5). |
| Format | The raw observation rows themselves, exported as compressed JSONL with a manifest. Replaying the fixture is literally loading raw rows — the same path production uses. |
| Determinism | Fixed keypair seeds, fixed clock warps, fixed price trajectory. **No `Keypair::new()`, ever** — an unshrinkable failure is worthless. |
| Fork fixture | A second corpus containing a deliberately constructed fork, produced by the harness rather than hoped for from a real cluster. |
| Corruption fixture | A third corpus with truncated payloads, invalid UTF-8, unknown enum variants, oversized payloads, and unknown account discriminators. |
| Size | Bounded and committed. If it grows past the stated budget, it is sampled, not paged in from a network. |
| Secrets | None. Enforced by the same CI scan as the rest of the repository. |

---

## 9. Backfill specifics

| # | Rule |
|---|---|
| BF-1 | Backfill uses the **same code path** as realtime ingestion, with a different slot range. There is no separate historical pipeline. |
| BF-2 | Backfill never advances `last_contiguous_slot`; the gap scanner owns that watermark (`ingestion-model.md` C-1, B-3). |
| BF-3 | Ranges are bounded and leased. A long outage produces many bounded jobs, never one unbounded query. |
| BF-4 | Backfill is the **lowest** request-priority class above scheduled scans and yields to realtime under provider pressure. Falling behind the head is worse than backfilling slowly. |
| BF-5 | A range that fails `max_attempts` times is quarantined with its error and alerts; it is neither dropped nor retried forever. |
| BF-6 | **Cold start** — bootstrapping Aegis history — is a backfill of the program's signature history via `getSignaturesForAddress` paged backwards from the head, plus block fetches for the slots it names. It is explicitly *not* a full-chain scan. |
| BF-7 | Backfilling into a partition that has been archived requires re-attaching it; the CLI states this rather than failing obscurely. |

---

## 10. Operator interface

```
sentinel-backfill gaps                          # repair every open gap
sentinel-backfill range --from S --to E         # fetch raw for a slot range
sentinel-backfill program --id <aegis> --from S # program-history cold start

sentinel-replay range --from S --to E --scope protocol|derived|all [--dry-run]
sentinel-replay entity --market <pubkey> [--from-finalized-anchor]
sentinel-replay verify --run <uuid>             # recompute digest and compare
sentinel-replay digest --from S --to E          # compute a digest without writing
```

Every command:
- refuses to run without an explicit slot range or entity — **no implicit "everything"**;
- prints the plan and the estimated row counts before doing anything, and `--dry-run` stops there;
- records a `replay_runs` row with its parameter hash and code version;
- is safely interruptible (RP-04) and safely repeatable (RP-02).
