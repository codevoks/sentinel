# ADR-0005 — RPC + WebSocket baseline; HTTP is the completeness authority

**Status:** Accepted · **Date:** 2026-09-05 · **Phase:** 0

## Context

Sentinel must observe Solana. The available mechanisms are HTTP JSON-RPC (pull), the native PubSub
WebSocket (push), and a Geyser plugin stream (push, privileged). Each has different completeness,
latency, and cost properties, and the choice determines whether the whole correctness model is sound.

Verified behavior at the research date (`ecosystem-research.md` §4):
- Agave's native PubSub **drops messages under load**; subscription processing runs on a single worker
  thread by default.
- **Subscriptions do not survive reconnect** and must be re-established.
- **`blockSubscribe` is documented unstable**, needs two extra validator flags, and drops or oversizes
  under volume.
- Since Agave 4.2, an account update is emitted **only when the account actually changes**; only the
  fee payer is guaranteed.

## Decision

**HTTP JSON-RPC is the completeness authority. The WebSocket is a latency hint. Neither is optional.**

Stated as one sentence, inherited by every other document:

> **The WebSocket tells Sentinel when to look; HTTP tells Sentinel what is true.**

Concretely:
- **Blocks are the unit of ingestion** on the completeness path — self-delimiting, ordered,
  attributable, and complete for logs and token balances.
- `getBlocks(range)` is the **authoritative** answer to "which slots produced a block". Absence from
  the stream is never evidence.
- `slotSubscribe` drives head tracking; `accountSubscribe` and `logsSubscribe` are wake signals.
- **`blockSubscribe` is not used in any required path.**
- Every transition into a live WebSocket state triggers a **gap scan**.
- Every historical fetch sets `maxSupportedTransactionVersion`, an explicit `commitment`, and
  `minContextSlot` where supported.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **WebSocket-only ingestion** | Drops messages under load, loses subscriptions on reconnect, and cannot establish slot completeness. Would make every correctness claim in the repository unsound. |
| **`blockSubscribe` as the firehose** | Documented unstable, needs non-default validator flags (so the zero-cost local path could not rely on it), and drops under volume. |
| **Polling only, no WebSocket** | Correct but slow. Detection latency would dominate `keeper_end_to_end_latency` and make the keeper uncompetitive, for no correctness gain. |
| **Geyser as the baseline** | Requires running a validator with a plugin, or a paid hosted endpoint. Would break the zero-cost requirement (ADR-0013) and make the *baseline* depend on infrastructure most reviewers cannot run. ADR-0006. |
| **`getSignaturesForAddress` as the primary loop** | Address-scoped; cannot establish slot completeness; would miss anything not touching the watched address, including the slot structure the fork model needs. |
| **Account subscriptions as the primary state source** | Incomplete by construction since Agave 4.2, and Sentinel cannot subscribe to every position account. Account state is a *reconciliation* input, not the primary path (`aegis-integration.md` §7). |

## Consequences

**Positive**
- Completeness is provable: contiguity over `getBlocks` plus recorded gaps.
- The correctness model is **independent of push reliability**, so a dropped notification costs latency
  and nothing else.
- The baseline runs against a single local validator with no API key.
- Adding Geyser later changes latency, not correctness (ADR-0006).

**Negative**
- Higher request volume than a pure push design. Mitigated by request classes, budgets, and batching;
  measured in Phase 14 (hypothesis B-2).
- Higher latency than Geyser. Accepted for the baseline; measured, with a stated adoption trigger.
- Double handling: a notification arrives *and* the block is fetched. Deliberate — the notification's
  only job is to reduce the time to the fetch.

**Enforcement**
- `CI-NOMAXVER` blocks any `getBlock`/`getTransaction` call without `maxSupportedTransactionVersion`.
- `CI-NORAWCLIENT` blocks direct client construction outside `sentinel-rpc`.
- ING-05 asserts contiguity; ING-06 asserts every gap is repaired or visibly unrepaired.
