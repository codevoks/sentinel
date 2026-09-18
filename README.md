# Sentinel

**A replayable Solana indexer and durable execution engine, built as the operational counterpart to
[Aegis Protocol](https://github.com/codevoks/aegis-protocol).**

> **STATUS: PHASE 1 — FOUNDATION & LOCAL INFRASTRUCTURE COMPLETE.**
> The Rust and TypeScript workspace skeletons, the Compose-based local stack (Postgres, Surfpool, OTel
> Collector, Prometheus, Grafana, optional Redis), the migration runner, typed configuration with
> secret redaction, and telemetry foundations are implemented and tested. **No ingestion, no RPC
> client, no decoding, no API routes, and no Aegis integration exist yet** — that is Phases 3–11, not
> this one. See [`docs/project-status.md`](docs/project-status.md) for the authoritative, per-component
> state (implemented/tested/demoed/documented/committed, tracked separately and honestly).

---

## What Sentinel is

Sentinel observes Solana durably, reconstructs protocol state it can prove, derives risk from it, and
acts on that risk through a transaction engine that can crash at any moment without doing anything
twice. Its flagship integration is **Aegis**, and its flagship closed loop is **automated liquidation**:

```
oracle or state change → ingest → decode → materialize → evaluate health → candidate
  → simulate → policy → sign → persist → submit → track → reconcile → publish
```

The organizing principle is that **an off-chain observer is defined by how it is wrong**. Chains fork,
RPC lies, WebSockets drop, workers crash mid-flight, and the protocol — not the observer — is the
authority. Sentinel's architecture is the set of decisions that make each of those survivable and
*provable*, rather than merely unlikely.

## The four decisions that shape it

1. **An immutable raw observation boundary before any decoding** ([ADR-0008](docs/adr/0008-raw-observation-boundary.md)).
   Every observation is persisted byte-exact before anything interprets it. That one decision buys
   replay determinism, retroactive decoder fixes, forensics, and provider-divergence detection.
2. **Business intent is separate from transaction attempt** ([ADR-0010](docs/adr/0010-intent-vs-attempt.md)).
   Retries, blockhash expiry, failover, and reorgs all create new *transactions* for the same *intent*.
   Conflating them is exactly how an economically sensitive operation executes twice.
3. **`processed` is never persisted; every value carries its commitment** ([ADR-0009](docs/adr/0009-commitment-and-fork-model.md)).
   Provisional state is never presented as final, and a fork is handled by rebuilding forward from a
   finalized anchor — never by inverting events.
4. **Aegis is upstream and authoritative** ([ADR-0012](docs/adr/0012-aegis-adapter-versioning.md)).
   Sentinel consumes Aegis's own `aegis-math` and `@aegis/sdk` rather than reimplementing its
   economics, so its health math cannot drift from the protocol's — because it *is* the protocol's.

## Planned properties

- Everything runs **offline and free** — no paid RPC, no API key, no faucet, no hosted streaming
  service.
- **Delete all derived state, replay, reproduce a byte-identical digest** — verified on every commit.
- **Kill any worker at any instant and get exactly one on-chain effect** — the sign/persist/submit
  crash boundaries are tested in the required CI tier, not nightly.
- **28 failure-injection scenarios and 25 threats**, each with a named test that must fail when its
  mitigation is removed.
- Every performance claim backed by committed before/after measurements. **No number is stated before
  Phase 14 measures it.**

## Documentation

Read in this order:

| Document | Contents |
|---|---|
| [`docs/product.md`](docs/product.md) | Thesis, the product critique that reshaped it, non-goals, requirements |
| [`docs/architecture.md`](docs/architecture.md) | Components, **language ownership and why**, dependency rules |
| [`docs/aegis-integration.md`](docs/aegis-integration.md) | **The Aegis contract**, health sequence, conformance, version handling |
| [`docs/ingestion-model.md`](docs/ingestion-model.md) | Sources, lifecycle, dedup, gaps, checkpoints, backpressure |
| [`docs/finality-and-forks.md`](docs/finality-and-forks.md) | Commitment semantics, promotion, rollback |
| [`docs/data-model.md`](docs/data-model.md) | **Every table: keys, conflict policy, mutability, owner** |
| [`docs/replay-and-backfill.md`](docs/replay-and-backfill.md) | Determinism rules and the replay proof |
| [`docs/transaction-engine.md`](docs/transaction-engine.md) | Intent/attempt state machines; the crash-safe ordering |
| [`docs/keeper-design.md`](docs/keeper-design.md) | The liquidation loop and its eighteen adversarial cases |
| [`docs/signer-and-key-management.md`](docs/signer-and-key-management.md) | Signer boundary, policy engine, blast radius |
| [`docs/rpc-strategy.md`](docs/rpc-strategy.md) | Provider pool, capabilities, breaker, freshness |
| [`docs/geyser-strategy.md`](docs/geyser-strategy.md) | The optional high-performance path and its trigger |
| [`docs/distributed-correctness.md`](docs/distributed-correctness.md) | Named races, leases, backpressure, crash recovery |
| [`docs/threat-model.md`](docs/threat-model.md) | Trust boundaries, 25 threats, accepted residual risks |
| [`docs/testing-strategy.md`](docs/testing-strategy.md) | Six tiers and the traceability rule |
| [`docs/performance-strategy.md`](docs/performance-strategy.md) | Methodology and gates — **no results yet** |
| [`docs/observability.md`](docs/observability.md) | Metrics, traces, and an operator action per alert |
| [`docs/api-design.md`](docs/api-design.md) | REST, realtime, pagination, errors, idempotency |
| [`docs/external-integrations.md`](docs/external-integrations.md) | What is integrated, and what is deliberately not |
| [`docs/zero-cost-local.md`](docs/zero-cost-local.md) | How everything runs free and offline |
| [`docs/ui.md`](docs/ui.md) | Frontend scope and its correctness rules |
| [`docs/coverage-matrix.md`](docs/coverage-matrix.md) | Topic coverage and honest gap analysis |
| [`docs/ecosystem-research.md`](docs/ecosystem-research.md) | Dated toolchain research and open verification gates |
| [`docs/phase-roadmap.md`](docs/phase-roadmap.md) | The 15 implementation phases |
| [`docs/phase-0-self-attack.md`](docs/phase-0-self-attack.md) | **The six defects the self-attack found, and their fixes** |
| [`docs/project-status.md`](docs/project-status.md) | **Current state of everything** |
| [`docs/adr/`](docs/adr/) | 14 architecture decision records |

Contributor rules: [`AGENTS.md`](AGENTS.md) (engineering constitution) and [`CLAUDE.md`](CLAUDE.md)
(Claude session workflow).

## Planned stack

Rust (Tokio) for ingestion, normalization, chain state, protocol decoding and risk ·
TypeScript (`@solana/kit`) for the API, the execution engine and the UI ·
PostgreSQL as the only canonical store · Redis optional and never canonical ·
Docker Compose · Surfpool for the local cluster · OpenTelemetry.

Kafka, Kubernetes, Jito, Jupiter, durable nonces and address lookup tables are each **rejected with an
ADR and a measured adoption threshold** — not omitted by accident.

## Quickstart

**Right now (Phase 1):** `git clone` gets you a working local stack and a real (if mostly-empty) Rust
and TypeScript workspace. Every command below has actually been run against this repository — see
[`docs/project-status.md`](docs/project-status.md) for the pasted output.

```bash
make up               # docker compose: postgres, surfpool, otel collector, prometheus, grafana, redis
make migrate           # apply migrations (bounded retry; fails clearly if Postgres is unreachable)
make test              # cargo test --workspace — offline, no secrets, no faucet, no paid RPC
make lint               # cargo clippy --workspace --all-targets -D warnings
make fmt                # cargo fmt --all -- --check
make verify-versions    # prints every real, currently-installed toolchain version
```

There is no `make demo` yet — there is nothing to demo until ingestion exists (Phase 4+).

The TypeScript workspace builds, lints, and tests independently:

```bash
cd ts && npm install && npm run build && npm run lint && npm test
```

The exact install commands, pinned versions, and verification steps are specified in
[`docs/phases/phase-01-foundation.md`](docs/phases/phase-01-foundation.md), and the real command output
is recorded in [`docs/project-status.md`](docs/project-status.md).

## Relationship to Aegis

Sentinel is an **off-chain observer and executor**. Aegis remains authoritative for protocol economics,
account ownership, PDA derivations, state transitions, health calculation, liquidation semantics,
oracle validation, token compatibility, governance, and on-chain invariants.

**Aegis has completed its full planned roadmap through Phase 13 and published `v0.1.0`** (reconciled
2026-09-18, verified directly against [`codevoks/aegis-protocol`](https://github.com/codevoks/aegis-protocol) —
see [`docs/project-status.md`](docs/project-status.md) for the evidence). Sentinel's Phases 1–6 were
always ordered to have **zero** upstream dependency regardless of Aegis's schedule; Phases 7, 8, and 11
are no longer upstream-blocked, but per `AGENTS.md` §5 they still start only in their own turn, one
phase per session — Phase 1 does not touch the Aegis adapter.

## Status and honesty

**Sentinel is not implemented, not audited, and must not be operated against real capital.**

Sentinel cannot cause a loss of user funds in Aegis — Aegis's account model makes that structurally
impossible, and liquidation is permissionless. Sentinel's worst realistic outcome is **spending its own
keeper balance and reporting wrong numbers**, and every mitigation in the threat model is calibrated to
that honest scope. See [`docs/threat-model.md` §4](docs/threat-model.md) for the accepted residual risks
and [`docs/phase-0-self-attack.md` §3](docs/phase-0-self-attack.md) for the ones the self-attack could
not eliminate.

## License

[Apache-2.0](LICENSE).
