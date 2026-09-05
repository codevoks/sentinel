# Phase 4 — Raw Observation Boundary & Ingestion

**Status: NOT STARTED.** **Prerequisite: Phase 3 complete and tagged.**

> **Ingestion writes to the raw boundary from its first line of code.** There is no interim sink and no
> "we'll add durability later" — that ordering is why this phase exists as one unit
> (`phase-roadmap.md` §2.1).

## 1. Scope

1. The `ObservationSource` trait, with `RpcHttpSource` (pull) and `RpcWsSource` (push).
2. **The raw observation writer** — batched, bounded, `ON CONFLICT DO NOTHING`, one writer only.
3. Ingestion of slots, blocks, transactions, account states, program logs.
4. **Checkpointing**: `last_contiguous_slot` and `head_slot`, advanced **in the same transaction** as
   the data and **after** it.
5. **Gap detection** via `getBlocks` range reconciliation, with `gap_events` recorded.
6. **Backfill jobs** for gap repair, using the same code path with a different range.
7. The indexer **singleton** guarded by a Postgres advisory lock.
8. **Backpressure**: bounded channels; on saturation the WebSocket reader **drops to counting** and the
   gap scanner repairs.
9. Scheduled `getProgramAccounts` snapshotting at lowest priority.
10. Ingestion metrics.

## 2. Explicit non-scope

**No normalization** — raw rows only. No commitment promotion or fork handling (Phase 6). No decoding.
No Aegis. No API. The `slots` table is written only as far as recording existence and skipped status;
canonical-chain logic is Phase 6.

## 3. Evidence objective

- **Nothing is lost and nothing is duplicated** across crashes, reconnects, duplicates, and
  backfill/realtime overlap.
- **Completeness is provable**, not hoped: contiguity over `getBlocks` plus recorded gaps.

## 4. Files

`crates/sentinel-ingest/src/{source,http,ws,raw_writer,checkpoint,gaps,backfill,subscriptions}.rs` ·
`crates/bins/sentinel-indexer/` · `crates/bins/sentinel-backfill/`

## 5. Dependencies

Phases 1–3. SR-1 affects only how v1 transactions are *decoded* (Phase 5); raw ingestion stores bytes
and is version-agnostic — which is itself an argument for the raw boundary.

## 6. Implementation requirements — do not deviate

- **Blocks are the unit of ingestion** on the completeness path. A block is fetched and stored whole; a
  partial fetch is discarded, never persisted (RC-9).
- `getBlocks` is the **authoritative** answer to which slots produced a block (G-1).
- **The checkpoint advances after the data, in the same transaction** (C-2, C-3). A crash re-delivers;
  it never skips.
- `last_contiguous_slot` is a **watermark**, not a cursor. Backfill never advances it (BF-2).
- **Every transition into a live WebSocket state triggers a gap scan** (W-2) — including a clean
  restart.
- Scans are bounded by `MAX_SCAN`; a long outage produces many bounded jobs, never one unbounded query.
- A **skipped slot satisfies contiguity** and is not a gap.
- Under backpressure, **prefer a recorded gap over unbounded memory** — and this is only safe because
  the gap scanner exists and is tested here.
- Only `sentinel-ingest` writes `raw_observations`.
- Payload size is bounded **before** storage; an oversized payload produces a `decode_failures` row
  naming the size.

## 7. Tests

**Unit:** natural-key construction per observation kind; batch assembly; checkpoint advance logic
including the skipped-slot case; gap-set computation from a `getBlocks` result.

**Property:** `P-IDEM-1..3` at the raw layer; `P-MONO-2`.

**Integration (Surfpool):** cold start; sustained ingestion; contiguity maintained; restart resumes at
the checkpoint; `getProgramAccounts` snapshot lands as raw rows.

## 8. Adversarial / failure cases

| ID | Injected | Asserted |
|---|---|---|
| FI-01 | Kill the indexer at 20 randomized, seeded points | No loss, no duplicate; checkpoint consistent with stored data |
| ING-03 | Kill **between the raw write and the checkpoint advance** | On restart the slot is re-fetched and re-inserted as a no-op; the checkpoint then advances |
| FI-06 | Drop the WebSocket repeatedly | Reconnect, re-subscribe, gap scan, no loss |
| FI-07 | Duplicate 10% of notifications and 10% of fetched blocks | Zero duplicate rows |
| FI-15 | Backfill a range overlapping live ingestion | Zero duplicates; identical row set |
| — | Deliberately skip a slot range, then let the scanner run | Gap detected, `gap_events` written, backfilled, contiguity restored, `repaired_at` set |
| — | Saturate the raw writer while notifications flood in | Memory plateaus; a gap is recorded; the scanner repairs it |
| — | Two indexer processes started | The second **fails to acquire the advisory lock** and exits cleanly |
| S-11 | Oversized payload | Rejected before parsing; `decode_failures` row; worker continues |
| — | Provider returns a partial block | Discarded, not persisted; the slot is refetched |

## 9. Acceptance criteria

- [ ] `ING-01..ING-10` all proven by test
- [ ] FI-01, FI-06, FI-07, FI-15 pass with specific assertions
- [ ] Contiguity holds continuously through a 30-minute sustained run with injected faults
- [ ] Every gap detected is repaired, and an artificially unrepairable gap surfaces as an open alert
- [ ] The singleton lock prevents a second indexer
- [ ] Memory is bounded under a firehose (RSS plateaus)
- [ ] Backfill and realtime share one code path (asserted by a test that runs the same function both ways)
- [ ] Universal checklist satisfied. Tag `phase-04-ingestion`.

## 10. Demo

Start the indexer against Surfpool; run a transaction workload; kill the indexer mid-flight; restart;
show contiguity intact and no duplicates. Then manually delete a slot range from raw, show the gap
detected and repaired, with `gap_events` and the metrics visible.

## 11. Documentation & status updates

`ingestion-model.md` updated only via ADR if implementation revealed a genuine problem.
`project-status.md`: ingestion IMPLEMENTED + TESTED + DEMOED; failure-injection results with real
output.

## 12. Stop condition

**STOP after this phase.** Phase 5 has not been started.
