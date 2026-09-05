# Phase 3 — RPC Abstraction & Resilient Client

**Status: NOT STARTED.** **Prerequisite: Phase 2 complete and tagged.**

## 1. Scope

1. `RpcProvider` and `RpcPool` traits and their HTTP implementation (`rpc-strategy.md` §1).
2. **Capability discovery** by probing at startup and on reconnect; a missing capability is an explicit
   `None`, and a provider configured for a class it cannot serve is a **startup error**.
3. Request classes with per-provider budgets enforced **client-side before the request is made**.
4. Bounded retry with exponential backoff and **full jitter**; non-retryable errors fail fast.
5. Health-weighted selection; the circuit breaker with `Closed / Degraded / Open / HalfOpen`.
6. **Freshness checking**: `minContextSlot` where supported, `context.slot` validated against the known
   head everywhere else; a stale response is `STALE`, never used as canonical.
7. WebSocket connection manager with a **declarative subscription set**, re-established on every
   connect, with the state machine from `ingestion-model.md` §4.
8. `broadcast()` fan-out for submission (no sequential failover).
9. `FixtureProvider` and `FaultInjectingProvider` implementing the same traits.
10. Per-provider metrics and the `provider_health` table.

## 2. Explicit non-scope

No ingestion, no raw writes, no decoding beyond what a response type needs, no gap detection, no
transaction building or signing. The pool is a library in this phase; nothing consumes it yet.

## 3. Evidence objective

That **resilience is tested, not asserted** — every failure mode is produced by
`FaultInjectingProvider` and the recovery outcome is asserted specifically.

## 4. Files

`crates/sentinel-rpc/src/{provider,pool,health,breaker,budget,capabilities,ws,fixtures,fault}.rs`

## 5. Dependencies

Phases 1–2. **SR-6** informs the TypeScript side later; this phase is Rust-only.

## 6. Implementation requirements — do not deviate

- **No call site constructs a raw client.** `CI-NORAWCLIENT` enforces it.
- Every `getBlock`/`getTransaction` sets `maxSupportedTransactionVersion`. `CI-NOMAXVER` enforces it.
- Every call carries an explicit `commitment`. There is no provider-default path.
- Every request carries a `request_id`, surfaced for later storage on raw rows.
- **`sendTransaction` fans out; it never fails over sequentially** (RT-5).
- Half-open probes are cheap and read-only. Never a submission, never a scan.
- **Content divergence trips the breaker immediately**, regardless of sample size (CB-4).
- The breaker requires a **minimum sample size** for failure-ratio trips (CB-1) — tripping on two
  startup failures is a self-inflicted outage.
- **All-providers-open is a distinct, loud state**, never silently equivalent to idle (CB-6).
- Credentials are redacted everywhere, including in the stored `provider_id`.

## 7. Tests

**Unit:** backoff bounds and jitter distribution; error classification (every error maps to exactly one
of the five classes); non-retryable fast-fail; budget accounting; breaker transitions including the
minimum-sample rule; capability parsing.

**Property:** `P-BOUND-1` — every generated error pattern produces a retry sequence bounded in count
and in total time.

**Integration (Surfpool):** capability probe against the real local cluster; every method Sentinel needs
called successfully; WebSocket connect → subscribe → notification → disconnect → reconnect →
re-subscribe.

## 8. Adversarial / failure cases — all via `FaultInjectingProvider`

| Injected | Asserted |
|---|---|
| Timeouts, 429 with and without `Retry-After`, 5xx | Bounded retry, backoff applied, breaker counts |
| Stale `context.slot` (FI-09) | Response classified `STALE`, **not used**, provider health degraded |
| Malformed response body | `MALFORMED`, no panic, error surfaced |
| Two providers returning different blocks for one slot (FI-10) | Both surfaced to the caller; no merge |
| Two providers returning different content for one blockhash (FI-11) | **Breaker trips immediately**; alert |
| One provider open, another healthy | Selection routes away; classes respected |
| **All providers open** (FI-22) | Distinct loud state; caller receives `NoHealthyProvider` |
| Aggressive rate limiting (FI-21) | Budget adapts; `Execution` class unaffected; low classes starve first |
| WebSocket flapping (FI-06) | Bounded reconnect; subscriptions re-established; no unbounded reconnect storm |
| A credential embedded in a URL | Never appears in a log, metric label, error, or stored provider ID |

## 9. Acceptance criteria

- [ ] `RPC-01..RPC-10` all proven by test
- [ ] Capability discovery works against Surfpool and against a fixture provider missing methods
- [ ] A provider configured for an unsupported class fails **at startup**
- [ ] FI-09, FI-10, FI-11, FI-21, FI-22 pass with specific assertions
- [ ] `P-BOUND-1` passes
- [ ] `broadcast()` fans out to all healthy providers; a single failure does not fail the broadcast
- [ ] Credential redaction test passes (`A-SEC-01`)
- [ ] `CI-NORAWCLIENT` and `CI-NOMAXVER` pass
- [ ] Universal checklist satisfied. Tag `phase-03-rpc`.

## 10. Demo

A CLI that fetches a block through the pool while faults are injected live: kill a provider, watch the
breaker open, watch selection route away, watch it half-open and recover — with the metrics visible in
Grafana.

## 11. Documentation & status updates

`rpc-strategy.md` updated only if implementation revealed a genuine problem (via ADR).
`project-status.md`: RPC layer IMPLEMENTED + TESTED; the real capability results for the local cluster.

## 12. Stop condition

**STOP after this phase.** Phase 4 has not been started.
