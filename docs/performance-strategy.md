# Sentinel — Performance Strategy

**Status: FROZEN (Phase 0). Measurement campaign in Phase 14.**

> **No number in this document is a result.** Phase 0 defines *what* is measured, *how*, and *what
> gate* each measurement must pass. Inventing a benchmark result in a planning document is the exact
> failure `AGENTS.md` §9 exists to prevent.

---

## 1. Rules

| # | Rule |
|---|---|
| PF-1 | **No performance claim without committed BEFORE and AFTER measurements** from the harness. |
| PF-2 | Every optimization is documented as **BEFORE / CHANGE / AFTER / DELTA / RISK**. |
| PF-3 | **Measure before optimizing.** Speculative optimization is forbidden. |
| PF-4 | Never say "fast", "optimized", "high-throughput", or "efficient" without a number. |
| PF-5 | Every benchmark states its **hardware, dataset, concurrency, and seed**. A number without them is not reproducible and therefore is not evidence. |
| PF-6 | Benchmarks are committed as data (`benchmarks/*.json`) with a CI regression gate, not pasted into prose. |
| PF-7 | A target that is not met is **recorded as not met**, with the bottleneck named. Moving the target to make it pass requires an ADR. |

---

## 2. What is measured

### 2.1 Ingestion

| Metric | Definition |
|---|---|
| `ingest_lag_seconds` | `now − block_time` of the highest contiguously-ingested slot |
| `ingest_lag_slots` | `chain_head_slot − last_contiguous_slot` |
| `blocks_per_second` | Blocks fully normalized per second, steady state |
| `transactions_per_second` | Transactions normalized per second |
| `instructions_per_second` | Instructions normalized per second |
| `account_updates_per_second` | Account observations persisted per second |
| `raw_write_bytes_per_second` | Raw layer write throughput |
| `raw_write_amplification` | Raw bytes persisted ÷ useful decoded bytes |
| `gap_repair_latency_seconds` | Gap detection → contiguity restored |
| `backfill_slots_per_second` | Historical throughput |

### 2.2 Replay

| Metric | Definition |
|---|---|
| `replay_slots_per_second` | Full-scope replay throughput |
| `replay_full_corpus_seconds` | Wall time for the reference corpus |
| `rematerialize_market_seconds` | One market rebuilt from its creation event |

### 2.3 Risk and keeper

| Metric | Definition |
|---|---|
| `health_evaluations_per_second` | Positions evaluated per second |
| `keeper_detection_latency` | Chain event → candidate row written |
| `keeper_plan_latency` | Candidate → simulation complete |
| `keeper_sign_to_submit_latency` | Simulation complete → `sendTransaction` returns |
| `keeper_submit_to_observed_latency` | Submit → signature observed in a block |
| `keeper_end_to_end_latency` | Chain event → transaction observed |
| `landing_rate` | Attempts observed ÷ attempts submitted |
| `race_loss_rate` | `RACE_LOST` ÷ attempts |

`keeper_submit_to_observed_latency` p95 is also the **measured input** to `expected_landing_latency`
(`keeper-design.md` §2.3), which is why it is not merely a dashboard number.

### 2.4 API

| Metric | Definition |
|---|---|
| `api_p50/p95/p99_latency` | Per endpoint |
| `api_throughput` | Requests/second at a stated concurrency |
| `ws_fanout_latency` | Database commit → message delivered to a subscribed client |
| `ws_concurrent_connections` | Sustained connections with bounded memory |

### 2.5 Database

| Metric | Definition |
|---|---|
| `db_write_latency_p95` | Per hot table |
| `db_read_latency_p95` | Per hot query |
| `db_size_growth_per_1m_slots` | Storage per unit of chain |
| `db_bloat_ratio` | Dead-tuple ratio on hot tables |
| `lock_wait_p95` | Contention on materialization and intent claims |

### 2.6 Resources

`process_rss` and `process_cpu` per service, steady state and at peak; **memory under a firehose** with
the WebSocket reader saturated — the case where an unbounded buffer would show up.

---

## 3. Methodology

| Aspect | Decision |
|---|---|
| **Environment** | A single documented machine (CPU, cores, RAM, disk type), stated with every result. No cloud-instance variability in the primary numbers. |
| **Dataset** | Two: (a) the committed fixture corpus — small, deterministic, CI-runnable; (b) a generated synthetic corpus at a stated scale for load work. Both seeded. |
| **Load generation** | A generator that produces blocks at a configurable rate with a configurable Aegis-transaction density. **Synthetic, deterministic, and free** — no dependence on mainnet volume. |
| **Warm-up** | Discarded; steady state only. Cold-start is measured separately and reported separately. |
| **Repetition** | ≥5 runs; report median and p95; report variance. A single run is an anecdote. |
| **Isolation** | One workload at a time for component numbers; a combined run for end-to-end. Both reported. |
| **Profiling** | `cargo flamegraph` / `perf` for Rust; `--cpu-prof` for Node; `EXPLAIN (ANALYZE, BUFFERS)` for every query in a hot path. **Attach the profile to any optimization claim.** |
| **Regression gate** | Committed baselines with a stated tolerance; CI fails on regression beyond it. |

---

## 4. Expected bottlenecks (hypotheses to test, not conclusions)

Stated in advance so the measurement can falsify them — which is the difference between a hypothesis
and a rationalization.

| # | Hypothesis | How it would show | If true |
|---|---|---|---|
| B-1 | **Raw-layer write bandwidth** dominates ingestion | `raw_write_bytes_per_second` saturating disk while CPU is idle | Batch larger, compress harder, shorten hot retention, consider payload-level dedup |
| B-2 | JSON parsing of `getBlock` responses dominates CPU | Flamegraph dominated by deserialization | Streaming parse; consider a binary encoding where a provider offers one |
| B-3 | Normalized-layer index maintenance dominates writes | High write latency, high WAL volume | Fewer indexes, deferred index creation on backfill partitions |
| B-4 | The indexer singleton becomes the ceiling | CPU-bound single process while the machine is idle | Shard by slot range — the seam already exists in backfill (§7) |
| B-5 | Health evaluation is O(positions) per oracle update and dominates the risk worker | Evaluation rate falling as positions grow | Incremental evaluation: bucket positions by liquidation price and only evaluate the band a price move crosses |
| B-6 | Materialization lock contention per market | `lock_wait_p95` rising with concurrency | Batch events per market per transaction |
| B-7 | WebSocket fanout is O(clients × messages) in the API | `ws_fanout_latency` growing with connections | Per-topic fanout with shared serialization; Redis pub/sub across instances |
| B-8 | `getProgramAccounts` scans dominate provider budget | Rate-limit events correlated with the scan schedule | Longer interval; rely more on the event path (already the primary) |

**B-5 is the most likely real one**, and its mitigation is genuinely interesting: liquidation price is a
monotone function of collateral price, so positions can be kept in a sorted structure and a price move
only needs to touch the crossed band. That optimization is **not** implemented speculatively — it is
implemented if and when the measurement shows it is needed (PF-3).

---

## 5. Acceptance gates

Targets are set at the **start of Phase 14** from the Phase 12 baseline, not guessed in Phase 0. The
*form* of each gate is frozen now:

| Gate | Form |
|---|---|
| PG-1 | `ingest_lag_seconds` p95 ≤ **T1** while sustaining the reference block rate |
| PG-2 | No unrepaired gap older than **T2** under sustained load |
| PG-3 | `replay_full_corpus_seconds` ≤ **T3**, deterministic across runs |
| PG-4 | `keeper_end_to_end_latency` p95 ≤ **T4** on the local cluster |
| PG-5 | `api_p95_latency` ≤ **T5** per endpoint at the reference concurrency |
| PG-6 | `ws_fanout_latency` p95 ≤ **T6** at the reference connection count |
| PG-7 | Memory bounded under a firehose: RSS plateaus rather than growing |
| PG-8 | `db_size_growth_per_1m_slots` within the stated disk budget at the configured retention |
| PG-9 | No regression beyond tolerance versus the committed baseline |

**A gate that cannot be met is recorded as not met, with the bottleneck named** (PF-7). That is a
result, not a failure of the document.

---

## 6. Scaling thresholds — when the architecture changes

Each row is the measurement that would justify a technology this repository currently rejects. Until
then, adopting it is CV-driven architecture (`AGENTS.md` §14).

| Technology | Adoption threshold |
|---|---|
| **Kafka / a durable log** (ADR-0004) | Sustained ingestion above the rate at which Postgres-backed job dispatch is measurably the bottleneck, **or** ≥3 independent consumer groups each needing independent replay of the same firehose, **or** a cross-datacenter fan-out requirement. All three currently absent. |
| **Kubernetes** (ADR-0014) | More than one machine is genuinely required, **or** an availability SLO demands automated failover that Compose cannot provide. |
| **Geyser / Yellowstone** (ADR-0006) | `ingest_lag_seconds` p95 above target with the RPC path tuned, **or** `keeper_detection_latency` dominated by ingestion **and** a measured competitive loss rate above threshold. |
| **A columnar / time-series store** (`data-model.md` §9) | Raw hot-window size exceeds the disk budget at minimum useful retention, **or** a required analytical query's p95 exceeds target with correct indexes and partition pruning already in place. |
| **Sharded indexer** (B-4) | The singleton is CPU-bound while the machine is not. The seam is the existing disjoint-range leasing used by backfill. |
| **A read replica** | Read load measurably interferes with write latency on the primary. |
| **Redis as more than fanout/rate-limit** | Never without an ADR. It is non-canonical by construction (ADR-0003). |

---

## 7. Deliberate non-optimizations

Recorded so a later reader does not mistake them for oversights:

| Not optimized | Why |
|---|---|
| Raw-payload storage cost | Replayability and forensics are worth the bytes (ADR-0008). Revisited only against the §6 threshold. |
| Double-write (raw then normalized) | The boundary is the architecture, not an inefficiency. |
| Per-request allocation in the API | It reads Postgres; the query dominates. Optimizing here would be measuring the wrong thing. |
| Cross-market parallel materialization beyond the market lock | Aegis's own contention model bounds it (`aegis/account-model.md` §8). Matching the protocol's shape is simpler and equally fast. |
| Custom binary encoding for raw payloads | Compressed JSON is adequate and debuggable. Revisited only if B-1 proves true. |
| Multi-process ingestion | The singleton is simpler and correct. Revisited only against B-4. |
