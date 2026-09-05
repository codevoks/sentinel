# Sentinel — Architecture Decision Records

Every consequential decision has an ADR. Every ADR states **what was rejected and why** — an ADR
without that is not finished.

**Write an ADR for:** a change to any frozen document · a new external dependency of consequence · a
deviation from a phase specification · a change to the data model, commitment semantics, execution
semantics, or security posture · a rejected alternative worth recording.

**Do not write an ADR for:** routine implementation choices, internal function decomposition, naming,
test organization, or anything listed as flexible in `docs/implementation-handoff.md` §2.

Format: **Context · Decision · Alternatives considered (and why rejected) · Consequences · Status.**
Number sequentially. Never renumber.

| ADR | Decision | Status |
|---|---|---|
| [0001](0001-rust-typescript-split.md) | Rust owns ingestion/decode/risk; TypeScript owns API/execution/UI | Accepted |
| [0002](0002-postgres-canonical-store.md) | PostgreSQL is the only canonical store | Accepted |
| [0003](0003-redis-bounded-role.md) | Redis is optional and never canonical | Accepted |
| [0004](0004-no-kafka-postgres-queue.md) | No message broker; a Postgres-backed job table | Accepted |
| [0005](0005-rpc-websocket-baseline.md) | RPC + WebSocket is the baseline; HTTP is the completeness authority | Accepted |
| [0006](0006-optional-geyser-adapter.md) | Geyser/Yellowstone is an optional adapter behind the same interface | Accepted |
| [0007](0007-at-least-once-idempotent.md) | At-least-once observation, effect-once processing | Accepted |
| [0008](0008-raw-observation-boundary.md) | An immutable raw observation boundary before any decoding | Accepted |
| [0009](0009-commitment-and-fork-model.md) | Explicit commitment model; `processed` is never persisted | Accepted |
| [0010](0010-intent-vs-attempt.md) | Business intent is separate from transaction attempt | Accepted |
| [0011](0011-signer-boundary.md) | The backend signs only self-constructed, policy-checked transactions | Accepted |
| [0012](0012-aegis-adapter-versioning.md) | One protocol, deeply, via a version-aware adapter that consumes Aegis's artifacts | Accepted |
| [0013](0013-zero-cost-local.md) | Zero-cost, local-first architecture | Accepted |
| [0014](0014-no-kubernetes.md) | Docker Compose; no Kubernetes initially | Accepted |
