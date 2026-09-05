# Phase 11 — Aegis Liquidation Keeper

**Status: NOT STARTED.** **Prerequisite: Phase 10 complete and tagged.**
**UPSTREAM-BLOCKED: requires `@aegis/sdk` with `ix.ts` builders (Aegis ≥ Phase 9).**

> **There is no acceptable workaround for the upstream block.** Hand-building the `liquidate`
> instruction would create a second source of truth for the most dangerous instruction in the protocol
> (ADR-0001, ADR-0012). If `@aegis/sdk` does not exist, **this phase waits** and the session reports it.

## 1. Scope

1. The candidate → intent path: `sentinel-risk` creates a `liquidation_candidate` **and** an
   `execution_intent` in one transaction, with the bucketed idempotency key.
2. `sentinel-keeper`: claim, re-read, re-evaluate at `t_eval`, build via `@aegis/sdk`, resolve fresh
   Pyth accounts, simulate, policy-check, sign, persist, submit, track.
3. **The oracle freshness pre-check** — refuse to build when the expected landing time exceeds
   `publish_time + max_price_age_secs − safety_margin`.
4. **`expected_landing_latency` measured** from `transaction_attempts` p95, not guessed.
5. The rejection classifier over Aegis's banded error codes.
6. **Reconciliation**: predicted vs actual against the `Liquidated` event's `hf_before`, `seized`,
   `to_liquidator`, `protocol_cut`.
7. Keeper safety controls: master switch, per-market enable, **one-way auto-pause**, rolling loss
   budget, concurrency caps (global and per market), inventory floor.
8. Bad-debt eligibility surfaced as an **operator-triggered** intent kind — never automatic.
9. Keeper metrics and the competition model.

## 2. Explicit non-scope

**No liquidation callback / flash liquidation** — Aegis Phase 8 territory, and it needs real swap
liquidity. **No Jupiter. No Jito. No bundles. No collateral disposal or hedging.** No automatic
`absorb_bad_debt`. No durable nonces.

## 3. Evidence objective

- **The closed loop runs end to end**, and **exactly one liquidation occurs per opportunity** under
  every injected fault.
- Sentinel's predictions match the chain's ground truth **exactly** for sizing and within tolerance for
  health.

## 4. Files

`ts/apps/keeper/src/*` · `ts/packages/executor/src/planners/liquidate.ts` ·
`crates/sentinel-risk/src/intents.rs`

## 5. Dependencies

Phases 1–10, **and Aegis ≥ Phase 9**. Also benefits from Aegis Phase 6 being deployed locally so real
`Liquidated` events exist to reconcile against.

## 6. Implementation requirements — do not deviate

- **The idempotency key uses a time bucket, not the trigger slot** (ADR-0010). A slot-keyed intent is a
  duplicate-execution generator.
- Candidate and intent are created **in one transaction**.
- **Re-evaluate at claim time.** The candidate may be seconds old, and `repay` is permissionless,
  unpausable, and requires no owner signature — so a position can heal at any moment from any signer.
- **`t_eval = now + expected_landing_latency`**, with the latency **measured**.
- Sizing follows Aegis's rules exactly, including the clamp path's **upward-rounded** repay
  recomputation. Getting that direction wrong produces a transaction Aegis rejects.
- Profitability uses the **conservative bounds**, and the slippage haircut is a **stated parameter**,
  measured against realized outcomes — not a live DEX quote (no network dependency in the required
  path).
- **Check the `LIQUIDATE` pause bit on both `Protocol` and `Market`** before creating a candidate. A
  paused market produces zero candidates and an informational status, not an alert storm.
- **Indexer lag beyond threshold disables candidate creation entirely.** A lagging keeper is worse than
  no keeper.
- **Auto-pause is one-way.** Automation stops the keeper; only an operator restarts it. A false
  positive costs availability, never money.
- **`MODEL_DIVERGENCE`, `ACCOUNT_REJECTED`, `SIZE_REJECTED`, `UNKNOWN` pause the keeper.** The first
  four rejection classes are normal operation and must not alert.
- Cap concurrent in-flight intents **per market** — Aegis serializes on the `Market` account, so ten
  simultaneous transactions waste nine fees.
- **Never act on `processed` state.**

## 7. Tests

**Unit:** sizing against every Aegis edge case (E-12 through E-15); the profitability model; the
rejection classifier over every error band; the oracle deadline computation.

**Integration:** the full loop on the local cluster, price-driven, through to finalization and
reconciliation.

**Conformance:** `AEGIS-CONF-03`'s exact figures reproduced by a **real on-chain liquidation**.

## 8. Adversarial / failure cases — `keeper-design.md` §4 in full

| ID | Case | Asserted |
|---|---|---|
| KP-05 | Kill the keeper at ≥10 randomized points across the loop | **Zero duplicate liquidations** |
| KP-06 / FI-26 | A simulated competing liquidator lands first | `RACE_LOST`; no retry storm; **no alert** |
| KP-07 | A third party repays and heals the position mid-flight | `RACE_HEALED`; intent cancelled; **no alert** |
| KP-08 / FI-25 | The oracle goes stale between detection and build | Build refused; recorded as `liquidatable-but-unactionable`; **never submitted** |
| KP-09 / FI-16 | A reorg abandons the trigger | Zero duplicate intents; the per-interleaving rules of `finality-and-forks.md` §7 hold |
| KP-10 | Injected `MODEL_DIVERGENCE` | Keeper pauses; **only an operator resumes** |
| KP-11 | Out-of-allowlist instruction, unpinned account, over-cap repay | All three **fail to sign** |
| KP-12 | Attempt a plain value transfer with the keeper key | **Fails** |
| KP-13 | Indexer lag beyond threshold | Candidate creation disabled |
| KP-14 | A bad-debt-eligible position appears | Surfaced; **no automatic action** |
| — | Position becomes unprofitable between detection and claim | `CANCELLED` with `UNPROFITABLE_ON_RECHECK` |
| — | The market is paused for `LIQUIDATE` | Zero candidates; informational, not an alert |
| FI-24 | The Aegis program is upgraded | Keeper pauses immediately; in-flight intents resolve or expire |
| — | Ten positions become liquidatable in one market simultaneously | Per-market concurrency cap respected; ordered by expected profit |
| — | Insufficient loan-asset inventory | Candidate recorded `INSUFFICIENT_INVENTORY`; no intent; alert |

## 9. Acceptance criteria

- [ ] **KP-01..KP-14 all pass**
- [ ] `AEGIS-CONF-03`'s exact figures reproduced by a real on-chain liquidation
- [ ] Predicted seizure, bonus, and protocol cut match the `Liquidated` event **exactly**
- [ ] Predicted `HF` matches `hf_before` within the stated tolerance
- [ ] **KP-05: zero duplicate liquidations across ≥10 randomized kill points**
- [ ] The four normal rejection classes produce **no alerts**; the four abnormal ones pause the keeper
- [ ] `expected_landing_latency` is measured from real attempt data, not configured as a guess
- [ ] Auto-pause proven one-way: automation cannot resume itself
- [ ] Universal checklist satisfied. Tag `phase-11-keeper`.

## 10. Demo

The full flagship loop: drop the price, watch detection → candidate → simulate → policy → sign →
persist → submit → confirmed → finalized → reconciled, with the commitment label visible at every step
and the reconciliation row showing predicted vs actual. Then kill the keeper mid-flight and show
exactly one liquidation. Then make the oracle stale and show the refusal.

## 11. Documentation & status updates

`keeper-design.md` updated only via ADR if implementation revealed a genuine problem.
`project-status.md`: keeper IMPLEMENTED + TESTED + DEMOED; reconciliation results with real numbers;
the measured landing latency.

## 12. Stop condition

**STOP after this phase.** Phase 12 has not been started.
