# Phase 8 — Derived Risk State

**Status: NOT STARTED.** **Prerequisite: Phase 7 complete and tagged.**
**Partially upstream-blocked: the preferred path needs `aegis-math` (Aegis ≥ Phase 4–6).**

> **Sentinel does not implement Aegis's economics — it calls them.** Where the crate does not exist
> yet, Sentinel implements from the frozen specification and proves conformance against Aegis's own
> frozen worked examples, which are available today.

## 1. Scope

1. `sentinel-risk`: health evaluation per `aegis-integration.md` §5, using `aegis-math` where available.
2. **Off-chain `accrue_view`** — applying interest to `t_eval` before every health computation.
3. `position_health` with `state ∈ {healthy, liquidatable, no_debt, unknown_oracle, stale}`, recording
   the exact oracle observations and parameter version used.
4. Liquidation sizing: close factor, `full_liq_hf`, dust rule, seizure, bonus, protocol cut, and the
   **collateral clamp** with upward-rounded repay recomputation.
5. `liquidation_candidates` with a profitability model, `expires_at`, and `reason_unprofitable`.
6. `market_metrics`.
7. `aegis_invariant_checks` — the off-chain-checkable Aegis invariants, run **only on finalized state**.
8. Event-driven evaluation with a **periodic floor**.
9. Bad-debt eligibility detection with loss decomposition (**detection only** — no action).

## 2. Explicit non-scope

**No execution, no intents, no transaction building, no signing** (Phases 10–11). No API. **No
automatic `absorb_bad_debt`** — ever, in this phase or any other, without an operator.

## 3. Evidence objective

- Sentinel's health and sizing reproduce **Aegis's exact frozen numbers**.
- The off-chain invariant checks are real and would catch a genuine accounting divergence — serving
  Aegis's own runbook R-2.

## 4. Files

`crates/sentinel-risk/src/{health,accrual,sizing,profitability,candidates,metrics,invariants}.rs`

## 5. Dependencies

Phases 1–7. **Preferred:** `aegis-math` linked. **Interim:** implemented from `economic-model.md` and
gated on `AEGIS-CONF-01..06`.

**When `aegis-math` becomes available, the Sentinel-side reimplementation is deleted**, not kept "for
comparison". Two implementations is the problem this decision exists to avoid.

## 6. Implementation requirements — do not deviate

- **H-1: `t_eval` is the intended execution time**, not the last observation time. For a candidate,
  `now + expected_landing_latency`; for a historical query, that slot's timestamp. Evaluating at
  `last_accrual_ts` systematically understates debt.
- **H-2: never use a price outside its validity window.** No valid observation → `unknown_oracle`, not
  "last known".
- **H-3: `unknown_oracle` is a first-class state** and is surfaced. Defaulting to healthy during an
  outage is how a monitoring system lies.
- **H-5: the conservative bounds are asymmetric** — collateral at `lo` floored, debt at `hi` ceiled.
  Either direction wrong makes Sentinel optimistic, which is the dangerous direction.
- `HF < WAD` is **strict**. `HF == WAD` is not liquidatable.
- Historical health uses **the parameters in force at that slot** (`aegis_market_params_history`).
- Sizing follows Aegis's rules exactly, including `full_liq_hf` and the death-spiral band. **Sentinel
  does not re-derive the band**; a market violating the recommended bound is a configuration alert.
- Profitability uses the **same conservative bounds Aegis uses**. Mid prices would make the keeper
  optimistic and lose money.
- An unprofitable candidate is **recorded, not discarded** — the unprofitable-but-liquidatable set is a
  first-class protocol risk signal.
- **Invariant checks run only on finalized state.** On confirmed state they produce false positives.
- **No floating point.** `CI-NOFLOAT`. No economic arithmetic written in this crate (`CI-NOMATHDUP`).

## 7. Tests

**Conformance (blocking):**
- `AEGIS-CONF-01` HF ≈ 1.330838 · `AEGIS-CONF-02` HF ≈ 0.842495, liquidatable, full liquidation
  permitted · `AEGIS-CONF-03` `total_seize = 9_970_348_101`, `protocol_cut = 47_477_848`,
  `to_liquidator = 9_922_870_253` · `AEGIS-CONF-04` `interest = 1_332_492`, `fee_amount = 133_249` ·
  `AEGIS-CONF-05` share round-trip · `AEGIS-CONF-06` `HF == WAD` boundary.

**Property:** `P-HEALTH-1/2` (monotonicity in collateral, price, debt); `P-SIZE-1/2` (seizure never
exceeds collateral; repayment never exceeds debt).

**Unit:** decimals pairs across `0..=12`; `expo` extremes; zero-debt and zero-collateral; dust rule;
the collateral clamp path; `unknown_oracle` on every oracle failure mode.

**Integration:** the scripted lifecycle drives a position healthy → liquidatable → liquidated → bad
debt, with health asserted at each step and the invariant checks holding throughout.

## 8. Adversarial / failure cases

| Case | Asserted |
|---|---|
| Oracle stale at `t_eval` | `unknown_oracle`, **not** healthy |
| Oracle confidence above `max_conf_bps` | `unknown_oracle`, with the failing check recorded |
| Health computed **without** accruing to `t_eval` | A deliberate mutation test: the value differs measurably, proving H-1 is load-bearing rather than decorative |
| Historical health computed with **today's** parameters | Mutation test: differs; proves the versioning is load-bearing |
| A market whose `full_liq_hf < LT·(1+b)` | Configuration alert; sizing still follows Aegis's rules |
| A position at exactly `HF == WAD` | Not liquidatable |
| A vault balance manipulated in the fixture to break `INV-CUS-01` | The invariant check fires with the correct expected/actual |
| A position with debt below `min_debt` | `INV-SOLV-07` check fires |
| A candidate whose expiry passes | Marked `expired`, never acted on later |

## 9. Acceptance criteria

- [ ] **`AEGIS-CONF-01..06` pass exactly** — these are the phase's core gate
- [ ] `P-HEALTH-1/2`, `P-SIZE-1/2` pass
- [ ] The two mutation tests (skip accrual; use current parameters for history) both **fail** when the
      mechanism is removed and pass when restored
- [ ] `unknown_oracle` produced for every oracle failure mode, never `healthy`
- [ ] `AEGIS-INV-01..08` implemented and firing correctly on a deliberately-broken fixture
- [ ] Unprofitable candidates recorded with a reason
- [ ] Bad-debt eligibility detected with loss decomposition; **nothing acts on it**
- [ ] Replay determinism holds with the derived layer included
- [ ] If `aegis-math` is available, it is **linked** and the Sentinel-side implementation is deleted
- [ ] Universal checklist satisfied. Tag `phase-08-risk`.

## 10. Demo

Drive the price down; watch `position_health` transition healthy → liquidatable with the exact HF from
Aegis's worked example; watch a candidate appear with its sizing matching Aegis's §7.5 figures; make the
oracle stale and watch the state become `unknown_oracle` rather than healthy.

## 11. Documentation & status updates

`aegis-integration.md` updated if a formula's interpretation needed clarification — as a finding, with
an ADR if a decision changed. `project-status.md`: risk engine IMPLEMENTED + TESTED + DEMOED;
conformance results with real numbers.

## 12. Stop condition

**STOP after this phase.** Phase 9 has not been started.
