# Sentinel — Distributed-System Correctness

**Status: FROZEN (Phase 0). Proven progressively; campaign in Phase 12.**

> **"Use retries" is not a design.** Every section below names a concrete failure, states what the
> system does, and names the test that produces the failure deliberately.

---

## 1. The five properties everything else serves

| # | Property | Mechanism |
|---|---|---|
| DC-1 | **At-least-once delivery, effect-once processing** | Natural keys + explicit conflict policies at every write; business idempotency keys at every externally visible effect |
| DC-2 | **Crash safety at every instant** | Data committed before checkpoints; signatures committed before submission; leases instead of in-memory ownership |
| DC-3 | **Bounded everything** | Every loop, query, batch, payload, retry sequence, and queue has a stated bound |
| DC-4 | **Recoverable, never lost** | Raw is immutable; downstream is rebuildable; nothing is deleted on failure |
| DC-5 | **Loud degradation** | Every degraded state has a metric, an alert, and a visible API representation |

---

## 2. Idempotency

### 2.1 Every write has a natural key and a conflict policy

Stated per table in `data-model.md`. The rule (`ingestion-model.md` §9): **a consumer that cannot state
its natural key and its conflict policy is not finished.**

### 2.2 The three levels of idempotency

| Level | Guards against | Mechanism |
|---|---|---|
| **Row** | Duplicate delivery of the same observation | `UNIQUE (natural key)` + `ON CONFLICT DO NOTHING` |
| **State** | A stale writer overwriting a newer materialization | `ON CONFLICT DO UPDATE ... WHERE excluded.as_of_slot > current.as_of_slot` |
| **Effect** | The same business action happening twice | `execution_intents.idempotency_key UNIQUE` + the sign-persist-submit ordering |

### 2.3 Where duplicate observations could have caused duplicate effects — and why they cannot

This is the §31 self-attack question, answered per path:

| Path | Duplicate risk | Why it cannot happen |
|---|---|---|
| Same block ingested twice (reconnect + backfill overlap) | Duplicate rows | Raw `DO NOTHING` on `(kind, natural_key, payload_hash)` |
| Same event decoded twice | Double-counted balance | `aegis_events` PK is `(signature, slot, blockhash, log_index)`; materialization is a **fold over the event set**, not an incremental `+=` against whatever is in the table |
| Same position evaluated twice at one slot | Two candidates | `liquidation_candidates` unique on `(position_pubkey, detected_at_slot)` |
| Same unhealthy position across consecutive slots | Two intents, two liquidations | `idempotency_key` uses a **time bucket**, not the slot (`data-model.md` §7) |
| Retry after an ambiguous submission | Two liquidations | Only one non-terminal attempt per intent (TX-02); resubmission reuses the **stored bytes**, producing the same signature |
| Reorg re-observes an already-processed transaction | Double-applied event | Materialization rebuilds forward from a finalized anchor; it never subtracts and never applies deltas twice |
| Two workers claim the same job | Duplicate work | `FOR UPDATE SKIP LOCKED` + lease + `dedupe_key` |

**The load-bearing sentence:** materialization is a **fold over an idempotent event set at a pinned
anchor**, not a sequence of increments. Increment-based materialization is duplicate-sensitive by
construction and is banned.

---

## 3. Worker ownership and leases

```
claim:   UPDATE ... SET lease_holder=$id, lease_expires_at=now()+$ttl
          WHERE <claimable> ... FOR UPDATE SKIP LOCKED LIMIT n
renew:   UPDATE ... SET lease_expires_at=now()+$ttl
          WHERE id=$id AND lease_holder=$me AND lease_expires_at > now()
release: UPDATE ... SET state=$terminal, lease_holder=NULL WHERE ... AND lease_holder=$me
```

| # | Rule |
|---|---|
| L-1 | Every claim writes a **lease with an expiry**. There is no in-memory-only ownership anywhere. |
| L-2 | Long operations **renew** the lease. A worker that cannot renew must **abandon the work immediately** — it has lost ownership and another worker may already hold it. |
| L-3 | Every mutation is conditioned on `lease_holder = $me AND lease_expires_at > now()`. A lost lease makes the update affect zero rows, which is detected and handled — never assumed to have succeeded. |
| L-4 | Lease TTL is **longer than the longest expected operation and shorter than the acceptable stall**. Both numbers are configured and measured, not guessed. |
| L-5 | **A lost lease never means the work did not happen.** Effect idempotency (§2.2) is what makes overlap safe; the lease only reduces waste. |

### 3.1 The singleton stages

Realtime ingestion and the chain-state engine are singletons, guarded by Postgres **advisory locks**
(`architecture.md` §6). Advisory locks rather than a leader-election protocol because:

- They are released automatically when the connection dies — no stale-leader window measured in
  minutes.
- They require no new infrastructure and no consensus system.
- The fencing story is honest: the lock is held on the same connection that does the writes, so a
  partitioned holder cannot write.

**The known limitation, stated rather than hidden:** an advisory lock is not a fencing token against a
process that is paused (GC, VM freeze) and resumes after the lock moved. Mitigation is that every write
these stages perform is idempotent and monotonic anyway — a resumed zombie writes rows that either
conflict harmlessly or fail the monotonicity guard. **The lock is an optimization; correctness rests on
idempotency.** That ordering is deliberate and is the general rule in Sentinel.

---

## 4. Named race conditions

Each row is a real interleaving with a defined outcome and a test.

| # | Race | Outcome | Test |
|---|---|---|---|
| RC-1 | Backfill and realtime write the same block simultaneously | Both `DO NOTHING`; one row | `T-RACE-01` |
| RC-2 | Two workers materialize the same market | Advisory lock per market serializes; the loser waits | `T-RACE-02` |
| RC-3 | Materialization writes while a rollback invalidates its input | Rollback holds the chain-state lock; materialization re-runs from the new anchor | `T-RACE-03` |
| RC-4 | Risk worker reads market state mid-materialization | Reads are at a **pinned `as_of_slot`**, never "latest whatever"; a torn read is impossible | `T-RACE-04` |
| RC-5 | Two risk workers create an intent for the same position | `idempotency_key` UNIQUE; the loser gets CONFLICT and treats it as success | `T-RACE-05` |
| RC-6 | Executor claims an intent whose trigger is being rolled back | Executor re-reads and re-evaluates at claim time; cancels if invalid | `T-RACE-06` |
| RC-7 | Attempt confirms while the executor is preparing a retry | The retry path re-checks attempt state under the intent lease before creating anything | `T-RACE-07` |
| RC-8 | Checkpoint advances while a gap scan is mid-flight | Gap scan operates on a snapshot range and is idempotent; overlap costs work, never correctness | `T-RACE-08` |
| RC-9 | Provider failover mid-block-fetch | The partial result is discarded, not persisted; the block is refetched whole | `T-RACE-09` |
| RC-10 | Two API instances fan out the same realtime message | Fanout is at-least-once; clients dedupe by `(entity, slot, revision)` | `T-RACE-10` |
| RC-11 | Job claimed just as its lease expires | The conditional update affects zero rows; the stale holder detects it and abandons | `T-RACE-11` |
| RC-12 | Late-arriving transaction below the materialization watermark | Triggers a scoped rematerialization from the finalized anchor, not an in-place patch | `T-RACE-12` |

---

## 5. Transaction boundaries and locking

| Operation | Boundary |
|---|---|
| Raw insert + checkpoint advance | **One transaction.** Data first, checkpoint second, both or neither. |
| Block normalization | One transaction per block. A block is all-or-nothing. |
| Commitment promotion | One transaction per promotion batch, under the chain-state advisory lock. |
| Rollback | **One transaction** for marking abandoned + recording the event + enqueuing recompute. Partial rollback state is the worst possible state. |
| Materialization of one entity | One transaction per entity per batch, under that entity's lock. |
| Intent creation | Same transaction as the candidate that justifies it. |
| Attempt persist | **Its own transaction, committed before submission.** |
| Intent state advance | One transaction, conditioned on the lease. |

Locking rules:

| # | Rule |
|---|---|
| LK-1 | **Lock ordering is fixed and documented**: chain-state → market → position → intent. Every code path acquires in that order. Deadlock is prevented by ordering, not by retry-on-deadlock. |
| LK-2 | No transaction holds a lock across an **await on the network**. Simulation, submission, and RPC calls happen **outside** database transactions, always. |
| LK-3 | **Optimistic concurrency** (the `as_of_slot` guard) for materialization — it is high-frequency and conflicts are rare. |
| LK-4 | **Pessimistic locking** (`FOR UPDATE`) for intents and jobs — conflicts are the normal case and an optimistic loop would livelock. |
| LK-5 | Every long-running statement has a `statement_timeout`. An unbounded query holding a lock is an outage. |

LK-2 is the one most commonly violated and most damaging: holding a row lock while awaiting an RPC
turns a slow provider into a database-wide stall.

---

## 6. Backpressure

| Stage | Bound | Behavior at the bound |
|---|---|---|
| WebSocket reader | Bounded channel | **Drops to counting**; the gap scanner repairs. A recorded gap beats an OOM. |
| Raw writer | Bounded batch + bounded in-flight | Applies backpressure upstream |
| Normalizer | Bounded queue depth from `last_contiguous_slot − last_normalized_slot` | Ingestion continues; the derived layers fall behind visibly |
| Decode/materialize | Bounded per-entity concurrency | Backlog grows as a metric with an alert |
| Risk evaluation | Bounded candidate rate | Excess is deferred, never dropped silently |
| Executor | Bounded in-flight intents, global and per market | New intents wait |
| API | Rate limits + connection caps | 429 with `Retry-After` |

**Governing principle:** *fall behind visibly rather than fail invisibly.* Every bound has a metric, and
sustained saturation alerts.

---

## 7. Poison jobs and quarantine

```
attempt fails
  -> attempts += 1, last_error recorded, available_at = now() + backoff(attempts)
  -> attempts >= max_attempts:
       state = 'quarantined'
       open an alert (kind='job_quarantined', entity=dedupe_key)
       DO NOT delete, DO NOT retry automatically
```

| # | Rule |
|---|---|
| Q-1 | A quarantined job is **never** auto-deleted and **never** auto-retried. It waits for a human. |
| Q-2 | Quarantine **never blocks the queue.** Other jobs of the same kind continue. A poison job degrades one unit of work, not the pipeline. |
| Q-3 | The quarantine record keeps the payload and the last error, so the failure is reproducible offline. |
| Q-4 | Quarantine depth by kind is a metric with an alert. |
| Q-5 | Requeue is an explicit operator action, after the cause is fixed. |

**Is a dead-letter queue justified?** A separate DLQ *system* is not — a `quarantined` state on the same
table gives every property a DLQ gives (isolation, retention, replay) with none of the operational
surface. The semantics matter; the infrastructure does not.

---

## 8. Crash recovery — what happens if each component dies

| Component | Dies mid-operation | On restart |
|---|---|---|
| `sentinel-indexer` | After raw insert, before checkpoint | Re-fetch and re-insert; `DO NOTHING`; checkpoint advances. **No loss, no duplicate.** |
| `sentinel-indexer` | During a WebSocket read | Reconnect, re-subscribe, gap scan over the outage window |
| Normalizer | Mid-block | The block's transaction was rolled back; reprocessed whole |
| Chain-state engine | Mid-promotion | Promotion is idempotent and monotonic; re-runs |
| Chain-state engine | **Mid-rollback** | Rollback is one transaction — it either happened or did not. Recompute jobs are idempotent. |
| Materializer | Mid-entity | Entity lease expires; another worker rebuilds from the finalized anchor |
| Risk worker | After candidate, before intent | Both are in one transaction, so this state does not exist |
| **Executor** | **After signing, before persisting** | No attempt row; nothing was broadcast; re-plan. **Safe.** |
| **Executor** | **After persisting, before submitting** | Attempt is `SIGNED`; resubmit the **stored bytes**; identical signature; no duplicate. **Safe.** |
| **Executor** | **After submitting, before recording** | Same as above — resubmission of identical bytes is a no-op on-chain. **Safe.** |
| Executor | While tracking | Attempt is `SUBMITTED`; the resolver picks it up from `lastValidBlockHeight` |
| API | Anytime | Stateless; clients reconnect and resume from cursor |
| Postgres | Anytime | Everything stalls; nothing is lost; all workers reconnect and resume from durable state |
| Redis | Anytime | Fanout and rate limiting degrade; **no correctness impact** (ADR-0003) |

The three executor rows are the FR-17 guarantee (`transaction-engine.md` §6) and each has a dedicated
crash-injection test at the exact boundary.

---

## 9. Eventual consistency and API visibility

Sentinel is eventually consistent with the chain, and it says so numerically rather than hoping nobody
notices.

| # | Rule |
|---|---|
| EC-1 | Every response carries `meta.as_of_slot`, `meta.chain_head_slot`, `meta.lag_slots`, `meta.as_of_block_time`, and `meta.commitment`. |
| EC-2 | Lag beyond a configured threshold sets `meta.degraded` with a reason code, and the UI renders it. |
| EC-3 | A client may pass `min_slot`; the API returns `409 STALE_DATA` with the current `as_of_slot` rather than serving older data. This is **read-your-writes for a client that just submitted a transaction** — the case where staleness is most confusing. |
| EC-4 | Entities in `RECOMPUTING`, `STALE`, or `UNKNOWN_SCHEMA` are labelled as such and never served silently as current. |
| EC-5 | The realtime channel emits explicit `revision` messages when a previously-sent value is superseded (`finality-and-forks.md` §6.1). |

---

## 10. Failure-injection catalogue

The Phase 12 campaign. **Every entry produces the failure deliberately; none is a mock.**

| ID | Injected failure | Asserted |
|---|---|---|
| FI-01 | Kill the indexer at 20 randomized points | No loss, no duplicate, checkpoint consistent |
| FI-02 | Kill a worker mid-lease | Work re-claimed; no duplicate effect |
| FI-03 | **Kill the executor between sign and persist** | No attempt row; nothing broadcast |
| FI-04 | **Kill the executor between persist and submit** | Resubmit stored bytes; exactly one liquidation |
| FI-05 | **Kill the executor between submit and record** | Exactly one liquidation |
| FI-06 | Drop the WebSocket repeatedly | Reconnect, re-subscribe, gap scan, no loss |
| FI-07 | Duplicate 10% of notifications | Zero duplicate rows, zero duplicate effects |
| FI-08 | Reorder notifications within a window | Identical final state |
| FI-09 | Provider returns a stale `context.slot` | Rejected as `STALE`; not used as canonical |
| FI-10 | Two providers return different blocks for one slot | Both stored; fork path engaged; no merge |
| FI-11 | Two providers return different content for one blockhash | Breaker trips immediately; alert |
| FI-12 | Restart Postgres mid-pipeline | All workers reconnect; state consistent |
| FI-13 | Remove Redis entirely | System operates in documented degraded mode |
| FI-14 | Corrupt payloads (truncated, invalid UTF-8, unknown enum, unknown discriminator, oversized) | One `decode_failures` row each; **zero worker restarts** |
| FI-15 | Backfill overlapping realtime over the same range | Zero duplicates; identical digest |
| FI-16 | Inject a fork of depth 1, 5, and 30 | Rollback, scoped recompute, correct final state |
| FI-17 | Force blockhash expiry before landing | `EXPIRED`; re-plan; one liquidation total |
| FI-18 | `sendTransaction` returns a timeout after the transaction actually landed | No duplicate; resolved by observation |
| FI-19 | `sendTransaction` returns success but the transaction never lands | `EXPIRED` at `lastValidBlockHeight`; correctly terminal |
| FI-20 | Poison job that always fails | Quarantined; queue keeps flowing |
| FI-21 | Provider rate-limits aggressively | Budget adapts; priority classes respected; execution unaffected |
| FI-22 | All providers down | Ingestion pauses loudly; execution refuses; API degraded |
| FI-23 | Clock skew between Sentinel and the chain | Block-time-derived logic unaffected; a test proves no wall-clock dependency |
| FI-24 | Aegis program upgrade observed | Keeper pauses; decode boundary recorded; alert |
| FI-25 | Oracle goes stale mid-flight | Build refuses; `liquidatable-but-unactionable` recorded |
| FI-26 | Competing liquidator lands first | `RACE_LOST`; no retry storm; no alert |
| FI-27 | Database lock contention under concurrent materialization | No deadlock (lock ordering); bounded latency |
| FI-28 | Slow consumer on the WebSocket API | Bounded buffer; slow client disconnected; others unaffected |

---

## 11. Invariants

| ID | Invariant | Checked by |
|---|---|---|
| DC-I-01 | No externally visible effect occurs twice for one intent | FI-03..05, FI-17..19, KP-05 |
| DC-I-02 | Every write states a natural key and conflict policy | Schema audit test |
| DC-I-03 | No transaction holds a database lock across a network await | Static check + code review checklist |
| DC-I-04 | Lock acquisition follows the documented order | Deadlock test under concurrency |
| DC-I-05 | Every job reaches done or quarantined; none is lost | Property test |
| DC-I-06 | Every lease-conditioned update handles the zero-rows case explicitly | Code review + test |
| DC-I-07 | Every bounded resource has a metric and an alert | Observability audit |
| DC-I-08 | No processor calls wall-clock time in a deterministic path | CI grep + replay determinism |
| DC-I-09 | Redis absence changes no observable correctness behavior | FI-13 |
| DC-I-10 | A malformed input never terminates a worker | FI-14 |
