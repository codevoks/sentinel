# ADR-0001 — Rust and TypeScript responsibility split

**Status:** Accepted · **Date:** 2026-09-05 · **Phase:** 0

## Context

Sentinel spans a high-throughput ingestion pipeline, correctness-critical chain-state logic, protocol
decoding, a product API, and a transaction executor. Two languages are plausible. The failure mode to
avoid is **duplicating whole systems in both** for coverage, which produces two implementations that
drift and half the tests for each.

## Decision

**Rust owns:** the RPC/WebSocket client pool, ingestion, the raw observation writer, normalization,
the chain-state engine (commitment promotion and fork rollback), protocol decoding, risk derivation,
backfill/replay workers, the job queue, and the optional Geyser adapter.

**TypeScript owns:** the REST + WebSocket API, transaction planning/simulation/signing/submission/
tracking, the signing policy engine, and the web UI.

**The boundary between them is a PostgreSQL table** (`execution_intents`), not an RPC.

Each responsibility has exactly one owner. Where the same *concept* appears on both sides, the
mechanism preventing drift is named:

| Concept | Mechanism |
|---|---|
| Aegis economics | Both sides consume Aegis's own artifacts — `aegis-math` (Rust) and `@aegis/sdk` (TS) — which Aegis already cross-checks in its own CI via shared JSON vectors |
| PDA derivation | A shared vector file asserted by both sides in CI |

## Why Rust where it is

- **Ingestion is the only sustained-throughput path in the system.** Bounded memory under a firehose
  and predictable latency matter; a GC pause in an ingestion loop is a data gap.
- **Normalization parses untrusted bytes.** A malformed transaction must degrade one record, not
  restart a worker (S-03). Rust's parsing story is safe-by-default.
- **The chain-state engine is the most correctness-critical component**, and it is exhaustively
  property-testable without a runtime.
- **`aegis-math` is a Rust crate that is `no_std`, float-free, and free of `solana-*` dependencies** —
  by Aegis's own dependency policy. It is directly linkable from a non-Solana service. Sentinel's risk
  engine consuming it is the strongest available guarantee against economic drift, and it is only
  available in Rust.

## Why TypeScript where it is

- **`@aegis/sdk` is TypeScript.** Aegis's `ix.ts` builders are the protocol's own definition of how to
  construct a `liquidate` instruction. Consuming them makes Sentinel's most dangerous transaction
  byte-identical to the protocol's, by construction. Reimplementing account ordering and argument
  encoding in Rust would create a second source of truth for exactly the code where a mistake is most
  expensive.
- **The API is product-shaped and latency-tolerant** — it reads Postgres. Ecosystem velocity is the
  binding constraint, not throughput.
- `@solana/kit` is the current, actively-developed client, and the transaction-building surface moves
  faster there than in the Rust crates.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **All Rust** | The executor would have to reimplement Aegis's TypeScript instruction builders, creating a second source of truth for the most dangerous instruction in the protocol. The API would be slower to iterate for no benefit — it reads a database. |
| **All TypeScript** | Ingestion under a firehose with bounded memory is achievable but materially harder, and `aegis-math` — the single best anti-drift mechanism available — is not consumable. |
| **Rust ingestion + Rust API, TS only for the UI** | Loses `@aegis/sdk`. The API gain is negligible. |
| **A shared FFI/WASM core** | Adds a build dimension, a serialization boundary, and a debugging surface to solve a problem that a database table solves with durability included. |
| **gRPC between the halves** | A network hop, a schema, and a whole retry/partial-failure class, replacing a transactional handoff that is durable for free. |
| **Split by feature rather than by property** | Produces two half-systems in each language. The failure mode this decision exists to avoid. |

## Consequences

**Positive**
- One implementation per responsibility. No duplicated systems.
- The most dangerous artifacts — health math and instruction construction — are the protocol's own.
- The Rust↔TS handoff is durable by default; a crash on either side loses nothing.
- Each language is used where its actual strength applies, which is defensible in review.

**Negative**
- Two toolchains, two CI paths, two dependency-audit surfaces.
- Two places PDA derivation exists. Mitigated by shared vectors, and the mitigation is CI-blocking.
- A contributor must be comfortable in both. Accepted: the boundary is narrow and documented.
- Sentinel inherits Aegis's release cadence for `aegis-math` and `@aegis/sdk`. Mitigated by the pinning
  rule and the interim conformance path (`aegis-integration.md` §6.2).

**Enforcement**
- `architecture.md` §5 dependency rules, CI-enforced.
- The TypeScript database role has **no write grant** on raw, normalized, chain-state, protocol, or
  derived tables — in every environment including local.
- `CI-NOMATHDUP` blocks economic arithmetic inside `sentinel-risk`; it must be called, not written.
