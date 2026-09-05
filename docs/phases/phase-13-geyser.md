# Phase 13 — Optional Geyser / Yellowstone Adapter

**Status: NOT STARTED. OPTIONAL.** **Prerequisite: Phase 12 complete and tagged.**
**Blocking research gate: SR-3. Gated on the ADR-0006 adoption trigger.**

> **Do not start this phase unless the ADR-0006 trigger has been met by measurement.** If the Phase 14
> baseline has not been taken, or the measurement does not show the trigger, this phase is skipped and
> the coverage matrix continues to record Geyser as **NOT COVERED as production** — which is an honest
> outcome, not a gap.

## 1. Scope

1. Close **SR-3** from primary sources: current plugin/client/proto versions, whether stock Agave is
   supported or a patched fork is required, and whether resume-from-slot exists.
2. `sentinel-geyser`: `YellowstoneSource` implementing the existing `ObservationSource` trait.
3. Subscriptions: slots (confirmed + finalized), accounts filtered by the Aegis program owner, accounts
   filtered by the Pyth receiver owner, transactions filtered to the Aegis program including failures.
4. Source selection with **fallback to RPC**, a probation window against flapping, and a **gap scan on
   every transition in either direction**.
5. Independent lag measurement of the stream against the HTTP head.
6. Documented self-hosted local setup (validator + plugin), outside `make up`.
7. A measured before/after comparison, committed as benchmark data.

## 2. Explicit non-scope

**No change to any downstream stage.** No hosted-provider dependency in any required path. No change to
gap detection, commitment, fork handling, or decoding — if any of those needs to change, that is
evidence the RPC design was wrong and it is an ADR, not an edit.

## 3. Evidence objective

- **The interface seam is real**: two implementations, and the downstream produces **identical**
  normalized rows from either.
- Geyser changes latency, **not correctness** — proven, not asserted.

## 4. Files

`crates/sentinel-geyser/src/*` (feature-gated) · `infra/geyser/{config.json,README.md}` ·
`benchmarks/geyser-comparison.json`

## 5. Dependencies

Phases 1–12. **SR-3 is blocking.** If stock-Agave compatibility or resume semantics cannot be
established from a primary source, that is reported and the phase does not silently assume.

## 6. Implementation requirements — do not deviate

- **`YellowstoneSource` implements the existing trait unchanged.** If it cannot, stop and report — the
  trait was wrong, and that is a finding.
- **Assume no replay/resume** (GY-4). Recovery is the same slot-range reconciliation used everywhere
  else, which is why assuming the worst costs nothing.
- **The RPC WebSocket path stays configured and warm** whenever Geyser is primary. Fallback is a switch,
  not a cold start.
- **Every source transition, in either direction, triggers a gap scan.** Duplicate observations are free;
  a gap is not.
- Account observations from Geyser may key on `(pubkey, slot, write_version)`; the source's
  `capabilities` decides. Both key forms are supported.
- **No required test uses Geyser**, and `make test` passes with the crate absent.
- The local path is **self-hosted**, never a hosted endpoint.
- The Geyser-enabled setup is **not** part of `make up` — building a validator plugin is a heavier local
  dependency than the required path is allowed to have.

## 7. Tests

**Unit:** protobuf message → `Observation` mapping for every subscription type; filter construction.

**Integration (self-hosted validator + plugin):** connect, subscribe, receive; ingest a slot range; then
ingest the **same range via RPC** and compare digests.

## 8. Adversarial / failure cases

| ID | Case | Asserted |
|---|---|---|
| GS-03 / FI-06 | Kill the Geyser stream mid-ingestion | Fallback to RPC engages; gap scan runs; **no data lost** |
| — | Geyser stalls without disconnecting | Independent lag measurement detects it; fallback engages |
| — | Rapid Geyser up/down flapping | Probation window damps it; no thrash |
| — | Geyser and RPC both active over an overlapping range | Duplicate raw rows are `DO NOTHING`; identical downstream |
| — | Geyser reports a `write_version` the RPC path cannot | Both key forms coexist; digests still match |
| GS-04 | Build with the feature disabled and the crate absent | `make test` passes |

## 9. Acceptance criteria

- [ ] **SR-3 closed** from primary sources — versions, stock-Agave compatibility, resume semantics
- [ ] **GS-01**: no downstream stage changed
- [ ] **GS-02**: Geyser and RPC over the same range produce **identical** normalized rows (digest)
- [ ] GS-03: stream kill → fallback + gap scan → no loss
- [ ] GS-04: required suite passes with the feature disabled and the crate absent
- [ ] GS-05: a committed before/after of `ingest_lag_seconds` and `keeper_detection_latency`
- [ ] GS-07: the self-hosted local setup is reproducible from documentation, with no paid service
- [ ] Universal checklist satisfied. Tag `phase-13-geyser`.

## 10. Demo

Run the same slot range through both sources and show identical digests. Show the latency difference
with real numbers. Kill the Geyser stream mid-run and show the fallback plus gap scan keeping the data
complete.

## 11. Documentation & status updates

`geyser-strategy.md` updated with the SR-3 answers. `ecosystem-research.md` §5 updated.
`coverage-matrix.md` row 8 changes from optional to PRODUCTION **only if the trigger was met and the
adapter shipped**. `project-status.md`: the measured comparison with real numbers.

## 12. Stop condition

**STOP after this phase.** Phase 14 has not been started.
