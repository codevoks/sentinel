# Phase 5 — Normalization, Backfill & Replay

**Status: NOT STARTED.** **Prerequisite: Phase 4 complete and tagged.**

> This phase establishes the property everything later depends on: **replay determinism**. RP-01..RP-03
> join the required CI tier here and stay there forever.

## 1. Scope

1. `sentinel-normalize`: raw → `transactions`, `instructions`, `program_logs`,
   `account_observations`, `token_balance_deltas`, and the existence/skipped facts on `slots`.
2. Transaction decoding for **legacy, v0, and v1**, including the v1 `transactionConfig` bitmask and the
   per-version priority-fee semantics.
3. Instruction extraction including inner instructions and stack heights; log extraction preserving
   order and invoke/success framing.
4. Token balance deltas from `meta.pre/postTokenBalances`.
5. **The replay driver and the determinism harness**: `replay_runs`, `output_digest`, and the CLI.
6. Backfill driver hardening (ranges, leases, priority, quarantine).
7. The corrupt-input fixture corpus and the fixture-capture tooling.

## 2. Explicit non-scope

**No protocol concepts.** `CI-NOAEGISLEAK` blocks any Aegis reference in this crate — this is the seam
that makes a second protocol adapter additive, and it is worth defending from the first commit. No
commitment promotion or fork handling (Phase 6). No risk. No API.

## 3. Evidence objective

- **Replay produces byte-identical state**, verified by digest, on every commit.
- **A malformed input degrades one record, never a worker** — proven against a committed corpus.
- Transaction v1 is decoded correctly, including the fee-semantics change that silently returns zero for
  naive implementations.

## 4. Files

`crates/sentinel-normalize/src/{transaction,instruction,logs,token,account,slot}.rs` ·
`crates/sentinel-replay/src/{driver,digest,manifest}.rs` · `crates/bins/sentinel-backfill/` (replay
subcommands) · `infra/fixtures/{corpus,corrupt}/`

## 5. Dependencies

Phases 1–4. **SR-1** must be re-checked here: whether the pinned Rust crates decode v1, and what the
cluster currently produces.

## 6. Implementation requirements — do not deviate

- **Determinism rules D-1..D-5** (`replay-and-backfill.md` §2) are absolute:
  no wall clock, no randomness, no ambient configuration, deterministic ordering by
  `(slot, transaction_index, ix_index, inner_index)`, no dependence on prior derived state.
- `priority_fee_source` is recorded on every transaction. **Never store a v1 absolute-lamport fee and a
  v0 micro-lamport-derived fee in one column without the discriminator.**
- Enum-like fields from RPC are **open**: an unrecognized variant is stored verbatim and counted, never
  dropped (`ecosystem-research.md` §1.3).
- Token deltas come from pre/post balances, **not** parsed instruction JSON.
- **No panic on external input.** Every decode is fallible; a failure writes one `decode_failures` row
  and processing continues.
- `output_digest` excludes audit-only columns and canonicalizes `numeric` values, so the comparison is
  about data, not about insertion artifacts.
- **Replay never touches the network** (RP-09). A missing raw observation fails with `SEN-RPL-001` and
  enqueues a backfill.

## 7. Tests

**Unit:** a per-version transaction corpus; `transactionConfig` bit combinations including the
"both priority-fee bits or invalid" rule; ALT-referencing v0 messages; inner-instruction nesting; log
framing; token-delta extraction; truncated and malformed inputs at every layer.

**Property:** `P-IDEM-1..3`, `P-DET-1`, `P-DET-2`, `P-TIME-1` (no deterministic processor reads the
clock).

**Integration:** normalize a full captured corpus; compare row counts and digests against a committed
expectation.

## 8. Adversarial / failure cases

| ID | Case | Asserted |
|---|---|---|
| FI-14 | The corrupt corpus: truncated payloads, invalid UTF-8, unknown enum variants, oversized data | One `decode_failures` row each; **zero worker restarts** |
| RP-04 | Kill the replay worker at ≥20 randomized points | Final digest unchanged |
| RP-05 | Duplicate 10% of raw rows | Digest unchanged; no duplicate downstream row |
| RP-03 | Split the range into N sub-ranges, shuffled | Digest matches the single-range run |
| RP-09 | Replay with a raw observation deleted | Fails with `SEN-RPL-001`, enqueues backfill, **does not fetch** |
| FI-08 | Reorder notifications within a window | Identical final state |
| — | A v1 transaction whose fee would be read as zero by a `ComputeBudget` scan | `priority_fee_source = config_mask` and the correct absolute value |
| — | A transaction referencing an unknown program | Normalized fine; unknown-program instructions are data, not errors |

## 9. Acceptance criteria

- [ ] **RP-01, RP-02, RP-03 pass in CI on every commit**
- [ ] RP-04, RP-05, RP-09 pass in the failure-injection tier
- [ ] FI-14 passes against the full corrupt corpus with zero worker restarts
- [ ] `P-DET-1`, `P-DET-2`, `P-TIME-1` pass
- [ ] Legacy, v0, and v1 transactions all decode, with per-version fee semantics correct
- [ ] `CI-NOAEGISLEAK` passes — no protocol concept in the normalizer
- [ ] The fixture corpus is committed, deterministic, secret-free, and within its size budget
- [ ] SR-1 status updated with what was verified about v1 support and cluster activation
- [ ] Universal checklist satisfied. Tag `phase-05-normalize-replay`.

## 10. Demo

Ingest a scripted workload; show normalized rows; `TRUNCATE` them; run
`sentinel-replay range --scope normalize`; show an identical digest. Then feed the corrupt corpus and
show `decode_failures` filling while the worker stays up.

## 11. Documentation & status updates

`replay-and-backfill.md` and `ingestion-model.md` updated only via ADR if implementation revealed a
genuine problem. `project-status.md`: normalization and replay IMPLEMENTED + TESTED + DEMOED; the
determinism proof with real digests; SR-1 updated.

## 12. Stop condition

**STOP after this phase.** Phase 6 has not been started.
