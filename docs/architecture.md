# Sentinel — System Architecture

**Status: FROZEN (Phase 0). Component boundaries, language ownership, and dependency rules may change
only via ADR.**

---

## 1. System overview

```mermaid
flowchart TB
    subgraph CHAIN["Solana"]
        RPC["JSON-RPC (HTTP)"]
        WS["PubSub (WebSocket)"]
        GEY["Geyser / Yellowstone gRPC<br/>OPTIONAL — Phase 13"]
        AEGIS["Aegis program"]
        PYTH["Pyth price update accounts"]
    end

    subgraph RUST["Rust services"]
        SRC["sentinel-rpc<br/>provider pool, health, breaker"]
        ING["sentinel-ingest<br/>slot / tx / account / log ingestion"]
        RAW[("RAW OBSERVATIONS<br/>immutable, append-only")]
        NORM["sentinel-normalize<br/>Solana primitives"]
        FORK["sentinel-chainstate<br/>commitment promotion, fork rollback"]
        DEC["sentinel-decode<br/>protocol adapter registry"]
        RISK["sentinel-risk<br/>materialize + health + candidates"]
    end

    subgraph PG[("PostgreSQL — the only canonical store")]
        L1["raw"]
        L2["normalized"]
        L3["protocol (aegis)"]
        L4["derived (risk, candidates)"]
        L5["execution (intents, attempts)"]
        L6["jobs, checkpoints, reconciliation"]
    end

    subgraph TS["TypeScript services"]
        API["sentinel-api<br/>REST + WebSocket"]
        TXE["sentinel-executor<br/>plan / simulate / sign / submit / track"]
        WEB["sentinel-web<br/>Next.js"]
    end

    RD[("Redis — OPTIONAL<br/>fanout, rate limit, ephemeral hints")]

    RPC --> SRC
    WS --> SRC
    GEY -.-> ING
    SRC --> ING
    ING --> RAW
    RAW --> NORM --> PG
    NORM --> FORK --> PG
    PG --> DEC --> PG
    PG --> RISK --> PG
    PG --> API
    PG --> TXE
    TXE -->|signed tx| RPC
    API --> WEB
    API -.-> RD
    AEGIS -.->|read| RPC
    PYTH -.->|read| RPC
```

**One canonical store. One direction of flow. Every arrow into Postgres is idempotent.**

The pipeline is a sequence of restartable, independently-checkpointed stages. No stage holds state that
is not recoverable from Postgres, and no stage may skip the raw boundary.

---

## 2. Layer contract

| Layer | Owns | Authoritative? | Rebuildable from |
|---|---|---|---|
| **Raw** | Exact provider payloads, tagged with source, provider, commitment, receipt time | **Yes — the only irreplaceable layer** | The chain (by re-fetch), nothing else |
| **Normalized** | Slots, blocks, transactions, instructions, account observations, token balance deltas, program logs | No | Raw |
| **Chain state** | Slot commitment, canonical-chain membership, fork/abandonment marks | No | Raw + normalized |
| **Protocol** | Aegis markets, positions, events, oracle observations — decoded by a versioned adapter | No | Normalized |
| **Derived** | Health, liquidation candidates, market metrics, alerts | No | Protocol |
| **Execution** | Intents and attempts | **Yes — Sentinel's own history** | Nothing (it is a record of actions taken) |

**Rule (NFR-7):** everything except **raw** and **execution** can be truncated and rebuilt. That is the
replay guarantee, and it is the reason the boundary between execution and derived state is so sharply
drawn: an intent records something Sentinel *did*, which no replay can reproduce.

---

## 3. Language ownership — who owns what, and why

**Rule: no responsibility is implemented in both languages.** Each row below names one owner. The two
places where the same *concept* appears in both languages are called out explicitly with the mechanism
that prevents drift.

### 3.1 Rust owns

| Responsibility | Why Rust |
|---|---|
| RPC/WebSocket client pool, health tracking, breakers, failover | Long-lived connection management with strict backpressure and bounded memory under a firehose. Tokio gives structured concurrency and cancellation; a GC pause in an ingestion loop is a gap. |
| Slot / transaction / account / log ingestion | Highest sustained throughput path in the system. Per-message allocation matters here and nowhere else. |
| Raw observation writing | Must not be the bottleneck; batched `COPY`/multi-row insert under a bounded buffer. |
| Normalization and transaction/instruction parsing | CPU-bound byte work over untrusted input. Rust's parsing story is safe-by-default; a malformed payload must degrade a record, not a worker. |
| Chain-state engine (commitment promotion, fork detection, rollback) | Correctness-critical, high-frequency, and needs to be exhaustively property-tested without a runtime. |
| Protocol decoding (Aegis adapter) and the decoder-version registry | Must consume **`aegis-math`** directly — a `no_std`, float-free Rust crate with no `solana-*` dependency, which is exactly what Aegis built it to be (`aegis/architecture.md` §2). |
| Health / risk derivation and candidate generation | Same reason: it *is* `aegis-math`, invoked over materialized state. |
| Backfill and replay workers | Throughput-bound and must be deterministic. |
| Geyser/Yellowstone adapter (Phase 13) | The client is a Rust crate; the interface is the Rust `ObservationSource`. |

### 3.2 TypeScript owns

| Responsibility | Why TypeScript |
|---|---|
| REST + WebSocket product API | Product-shaped, iterated frequently, latency-tolerant (it reads Postgres). Ecosystem velocity is the binding constraint, not throughput. |
| Transaction planning, simulation, signing, submission, tracking | **Because Aegis's own SDK is TypeScript.** `@aegis/sdk` (`aegis/architecture.md` §5) exposes `ix.ts` transaction builders on `@solana/kit`. Sentinel consuming those builders means the keeper's `liquidate` instruction is byte-identical to the protocol's own, by construction. Reimplementing Aegis's account ordering and argument encoding in Rust would create a second source of truth for the most dangerous instruction in the protocol. |
| Web UI | Next.js, consuming the API. |

### 3.3 The two deliberate overlaps, and how drift is prevented

1. **Aegis economics appear in Rust (risk engine) and TypeScript (SDK read models).**
   Sentinel does not reimplement either. It consumes `aegis-math` (Rust) and `@aegis/sdk` (TS), which
   Aegis *already* cross-checks against each other via shared JSON vectors in CI
   (`aegis/architecture.md` §5, `I-SDK-01`). Sentinel adds one conformance test of its own
   (`docs/aegis-integration.md` §6) asserting its materialized state reproduces Aegis's worked examples
   exactly. **If Sentinel ever needs a formula Aegis has not exposed, the correct action is to ask
   Aegis to expose it, not to reimplement it.**

2. **PDA derivation appears in Rust (decoder) and TypeScript (tx builder).**
   Both derive from the same frozen seeds (`aegis/account-model.md` §3–6). A single fixture file of
   `(inputs → expected address)` vectors is asserted by both sides in CI. Divergence fails the build.

### 3.4 The Rust ↔ TypeScript boundary is a Postgres table, not an RPC

The keeper's detection half (Rust) and execution half (TypeScript) communicate **only** through the
`execution_intents` table:

```
sentinel-risk (Rust)        →  INSERT liquidation_candidate
                            →  INSERT execution_intent (idempotency_key UNIQUE)
sentinel-executor (TS)      →  SELECT ... FOR UPDATE SKIP LOCKED
                            →  plan / simulate / sign / persist attempt / submit / track
                            →  UPDATE intent state
```

Why a table rather than a service call or a broker:

- The handoff is **durable by default**. A crash on either side loses nothing.
- The idempotency key is enforced by the database, in the same transaction that creates the work.
- There is no partial-failure mode where the intent was created but the message was lost, or the
  message was delivered twice.
- It removes an entire network hop, an entire serialization format, and an entire class of retry logic
  from the most safety-critical path in the system.

This is also the honest answer to "is a queue necessary?" — the queue is a table (ADR-0004).

---

## 4. Component inventory

### Rust workspace (`crates/`)

```
crates/
  sentinel-core/        # shared types: Slot, Commitment, ObservationId, natural keys, errors
  sentinel-config/      # typed config, env loading, no defaults that point at a network
  sentinel-db/          # sqlx access layer, migrations runner, typed row structs
  sentinel-rpc/         # ObservationSource + RpcProvider traits, pool, health, breaker, failover
  sentinel-ingest/      # slot/tx/account/log ingestors, subscription manager, raw writer
  sentinel-normalize/   # raw -> normalized decoders (Solana primitives only, no protocol concepts)
  sentinel-chainstate/  # commitment promotion, canonical chain, fork detection, rollback
  sentinel-decode/      # protocol adapter trait + registry + version resolution
  sentinel-aegis/       # the Aegis adapter: account layouts, event decoding, materialization
  sentinel-risk/        # health, candidates, market metrics  (depends on aegis-math)
  sentinel-jobs/        # Postgres-backed job queue: claim, lease, retry, quarantine
  sentinel-telemetry/   # OpenTelemetry setup, metric registry, structured logging
  sentinel-replay/      # replay + backfill drivers, determinism harness
  sentinel-geyser/      # OPTIONAL (Phase 13) — Yellowstone ObservationSource implementation

  bins/
    sentinel-indexer    # runs ingest + normalize + chainstate + decode + risk workers
    sentinel-backfill   # one-shot / ranged backfill and replay CLI
```

### TypeScript workspace (`ts/`)

```
ts/
  packages/
    db/          # generated types + query layer for the tables TS reads/writes
    aegis/       # thin wrapper over @aegis/sdk; PDA vectors; version pinning
    executor/    # intent claim, planner, simulator, signer client, submitter, tracker
    policy/      # signing policy engine: program/instruction allowlist, account pins, caps
  apps/
    api/         # REST + WebSocket
    keeper/      # the executor loop (a thin runner over packages/executor)
    web/         # Next.js UI
```

### Infrastructure (`infra/`)

```
infra/
  compose/       # postgres, (optional) redis, surfpool, otel collector, grafana
  migrations/    # SQL migrations — the single source of schema truth
  fixtures/      # deterministic chain fixtures for replay tests (no secrets, fixed seeds)
```

---

## 5. Dependency rules (CI-enforced where possible)

```
sentinel-core        → (nothing internal)
sentinel-config      → core
sentinel-db          → core
sentinel-rpc         → core, config
sentinel-ingest      → core, config, db, rpc, jobs, telemetry
sentinel-normalize   → core, db, telemetry
sentinel-chainstate  → core, db, telemetry
sentinel-decode      → core, db
sentinel-aegis       → core, db, decode          (+ aegis-math)
sentinel-risk        → core, db, aegis           (+ aegis-math)
sentinel-jobs        → core, db
sentinel-replay      → core, db, normalize, chainstate, decode, risk
sentinel-geyser      → core, config, rpc(traits only), telemetry
```

| Rule | Rationale |
|---|---|
| `sentinel-normalize` **must not** reference any Aegis or protocol concept | This is the seam that makes a second protocol adapter additive. A CI grep for `aegis` in that crate blocks the build. |
| `sentinel-risk` **must not** implement economic arithmetic; it calls `aegis-math` | Prevents a third implementation of Aegis's economics. A CI grep bans `mul_div`, `WAD`, and float literals in that crate outside test vectors. |
| No crate writes to a table owned by another stage | Table ownership is declared in `data-model.md` §10 and checked by a schema-ownership test. |
| No crate other than `sentinel-ingest` writes the raw layer | Raw is append-only from exactly one place. |
| TypeScript **never** writes raw, normalized, protocol, or derived tables | TS reads them and writes only `execution_*`. Enforced by a restricted database role in every environment including local. |
| No `unwrap()` / `expect()` / `panic!` on a path that processes external input | A malformed payload must produce a `decode_failures` row, not a worker restart. Clippy-enforced. |
| Redis is behind a trait with a working no-op/in-process implementation | Guarantees ADR-0003's "runs correctly without Redis" claim stays true. |
| No hardcoded RPC URL, program ID, or cluster name outside config | Zero-cost path (ADR-0013). |

---

## 6. Process topology

Sentinel runs as **six processes** in the default local and single-node deployment:

| Process | Language | Scaling model | Crash impact |
|---|---|---|---|
| `sentinel-indexer` | Rust | **Exactly one per chain** (singleton, enforced by an advisory lock on the checkpoint row) | Ingestion pauses; resumes from checkpoint; gap detector repairs the interval |
| `sentinel-worker` | Rust | Horizontal; work claimed by lease | Leased jobs expire and are re-claimed |
| `sentinel-api` | TS | Horizontal (stateless) | Clients reconnect and resume from cursor |
| `sentinel-keeper` | TS | **At most one active** per keeper identity (advisory lock); more instances are allowed but only one holds the lock | In-flight intents resume from their persisted state |
| `postgres` | — | Single primary | Everything stalls; nothing is lost |
| `redis` *(optional)* | — | Single | Degraded fanout and rate limiting; no correctness impact |

The **indexer singleton** is deliberate and is the simplest correct answer at Sentinel's scale.
Concurrent ingestion of *disjoint slot ranges* during backfill is supported (`replay-and-backfill.md`
§5) and is the sharding seam if the singleton ever becomes the bottleneck — a threshold recorded in
`performance-strategy.md` §7, not a speculative design.

---

## 7. Request lifecycle — worked example: a liquidation, end to end

```mermaid
sequenceDiagram
    participant C as Chain
    participant I as sentinel-ingest (Rust)
    participant R as raw layer (PG)
    participant N as normalize + chainstate (Rust)
    participant A as sentinel-aegis (Rust)
    participant K as sentinel-risk (Rust)
    participant X as sentinel-executor (TS)
    participant API as sentinel-api (TS)

    C-->>I: slotNotification(confirmed) / logsNotification
    I->>R: INSERT raw_observation (ON CONFLICT DO NOTHING)
    I->>R: advance ingest checkpoint (same tx)
    N->>N: parse block -> slots, txs, instructions, logs, token deltas
    N->>N: link slot to parent; set commitment=confirmed
    A->>A: resolve decoder version; decode Aegis events + account snapshots
    A->>A: materialize market/position; reconcile event projection vs snapshot
    K->>K: accrue_view(market, t_exec) via aegis-math; compute HF with Pyth band
    K->>K: HF < WAD and profitable and not already claimed?
    K->>X: INSERT liquidation_candidate + execution_intent (idempotency_key UNIQUE)
    X->>X: claim intent (FOR UPDATE SKIP LOCKED, lease)
    X->>X: build tx via @aegis/sdk; fetch fresh Pyth accounts; blockhash
    X->>C: simulateTransaction  (MANDATORY)
    X->>X: policy check (program, instruction, accounts, caps)
    X->>X: sign
    X->>R: INSERT transaction_attempt(signature, bytes, last_valid_block_height)  ← COMMIT BEFORE SUBMIT
    X->>C: sendTransaction
    X->>C: poll signature status / observe via ingestion
    C-->>I: transaction lands (confirmed -> finalized)
    A->>A: decode Liquidated event; update position + market
    K->>K: reconcile predicted vs actual; write reconciliation row
    API-->>API: publish realtime update with commitment label
```

**Ordering rule, uniform across the pipeline:**
`observe → persist raw → checkpoint → normalize → establish chain position → decode → materialize →
derive → act`.

Two properties follow. Persisting raw before checkpointing means a crash re-delivers rather than skips.
Establishing chain position before decoding means no protocol state is ever derived from a slot whose
place in the chain is unknown.

---

## 8. What is deliberately absent

| Absent | Why |
|---|---|
| Kafka / NATS / RabbitMQ | Postgres `FOR UPDATE SKIP LOCKED` + `LISTEN/NOTIFY` covers every queueing need at Sentinel's scale, with transactional enqueue as a free property. Adoption threshold in ADR-0004. |
| Kubernetes | One machine runs the whole system. Compose is the honest deployment. Adoption threshold in ADR-0014. |
| A second datastore (ClickHouse, TimescaleDB, S3 parquet) | No query Sentinel needs is outside Postgres's reach at this scale. Threshold in `data-model.md` §9. |
| A microservice per stage | Stages are *workers*, not services. They share a database and a process where that is simpler. |
| An ORM | `sqlx` with checked SQL in Rust; hand-written SQL in TS. The schema is the contract. |
| An in-memory cache of protocol state | Postgres is the state. A cache would be a second source of truth with its own invalidation bugs. |
| A "current state" table maintained by triggers | Materialization is an explicit, restartable, replayable worker. Triggers are invisible to replay. |
| gRPC between Sentinel's own components | The only inter-component contract is the database schema. |
| An admin API that can move value | There is no server-side path to arbitrary signing (`signer-and-key-management.md` §3). |

The last row is a rule, not an observation: **no endpoint, job type, or config option may cause the
backend to sign a transaction it did not construct itself from a typed intent.**

---

## 9. Error model and taxonomy

Every error in Sentinel is classified into exactly one of five classes, because the class determines
the automatic response:

| Class | Meaning | Automatic response |
|---|---|---|
| **TRANSIENT** | Provider timeout, 429, 5xx, connection reset, lock contention | Bounded retry with jittered backoff; count toward breaker |
| **STALE** | Response is older than required (`minContextSlot` unmet, price too old) | Re-issue against a different provider or wait; never accept |
| **MALFORMED** | Payload failed validation or decoding | Record a `decode_failures` row with the raw reference; continue |
| **CONFLICT** | Uniqueness violation, lease lost, optimistic-concurrency miss | Treat as success-by-someone-else; do not retry the effect |
| **FATAL** | Configuration wrong, schema mismatch, policy violation, invariant breach | Stop the worker loudly; page an operator; never continue past it |

Rules:
- **A CONFLICT is never an error to the caller.** It is the idempotency mechanism working.
- **A FATAL is never retried.** Retrying a policy violation is how a bug becomes an incident.
- Every error carries a stable code (`SEN-<AREA>-<NNN>`) so alerts and tests reference codes rather
  than message text. Areas: `RPC`, `ING`, `NRM`, `CHN`, `DEC`, `RSK`, `EXE`, `API`, `POL`, `DB`.

`SEN-POL-*` (policy) and `SEN-CHN-*` (chain-state) are the two bands that must never be downgraded to
a warning.
