# ADR-0006 — Geyser/Yellowstone as an optional adapter behind the same interface

**Status:** Accepted · **Date:** 2026-09-05 · **Phase:** 0 · **Implementation gated on SR-3**

## Context

Yellowstone gRPC ("Dragon's Mouth") is the standard Geyser plugin for high-performance Solana
streaming. It is genuinely better than RPC WebSocket at the two things Sentinel finds hardest with
RPC: **latency** and **subscribing to many accounts by owner**. It is also either a validator to
operate or a subscription to pay for.

## Decision

Geyser is an **optional `ObservationSource` implementation**, introduced in Phase 13, **after** the
Phase 14 measurement discipline has a baseline to compare against.

Three constraints make "optional" real rather than rhetorical:

1. **It implements the same trait as the RPC source.** No downstream stage changes — not the raw
   writer, not normalization, not chain state, not decoding, not gap detection.
2. **No required test may use it**, and `make test` passes with the crate absent (feature-gated).
3. **Adoption requires a measurement, not a preference** — see the trigger below.

## Why it changes latency and not correctness

Geyser solves notification loss, latency, and account-update volume. It does **not** solve gaps
(a stream still disconnects), forks (unchanged), or completeness (HTTP range reconciliation remains
the authority). No consulted source documented resume-from-slot, so Sentinel **assumes there is none**
(SR-3) — which costs nothing, because recovery is the same slot-range reconciliation used everywhere
else.

That is the whole argument: **if adopting Geyser required changing a correctness argument, that would
be evidence the RPC design was wrong.**

## Adoption trigger

Implemented only when the Phase 14 campaign shows **either**:

1. `ingest_lag_seconds` p95 exceeds target with the RPC path already tuned (batching, provider pool,
   request priorities correct), **or**
2. `keeper_detection_latency` p95 is dominated by ingestion rather than evaluation, **and** the measured
   `race_loss_rate` is above the stated threshold.

Absent one of those, Geyser is **NOT COVERED as production** in the coverage matrix, and the matrix
says so rather than implying otherwise.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **Geyser as the baseline** | Requires operating a validator with a plugin or paying for a hosted endpoint. Breaks ADR-0013 and makes the baseline unrunnable for most reviewers. |
| **Geyser-only, no RPC** | No completeness authority, no historical fetch, no gap repair, no simulation/submission path. Not viable. |
| **A hosted Yellowstone provider as the required path** | A paid dependency in a required path. Directly forbidden by ADR-0013. |
| **Skipping Geyser entirely** | Defensible — Aegis's volume does not need it. Kept as an *optional* phase because the adapter is small, the interface already exists, and it is a genuine measured comparison rather than a keyword. |
| **A different Geyser plugin** | Yellowstone is the de-facto open-source standard with an active changelog. No reason to differ. |
| **Writing a custom Geyser plugin** | The interesting engineering is in the consumer, not in re-solving the plugin. |

## Consequences

**Positive**
- The interface seam is proven by having two implementations, which is what makes "pluggable source" a
  fact rather than a claim.
- A measured before/after becomes committed benchmark evidence rather than an assertion.
- Losing Geyser is a degradation with automatic fallback plus a gap scan, never an outage.

**Negative**
- A second ingestion path to test. Mitigated by GS-02: both sources over the same slot range must
  produce **identical** normalized rows, compared by digest.
- Plugin/validator version coupling is an operational burden RPC does not have. It is a genuine reason
  to keep Geyser optional, and it is stated in `geyser-strategy.md` §7.
- SR-3 is unresolved: current versions, stock-Agave compatibility, and resume semantics. **Blocking for
  Phase 13.**

**Enforcement**
- `make test` with the feature disabled is a CI job.
- GS-01 asserts no downstream stage changed.
- GS-04 asserts the required suite passes with the crate absent.
