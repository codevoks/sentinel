# Sentinel — Frontend

**Status: FROZEN (Phase 0). Implementation in Phase 15.**

> The UI exists to **demonstrate backend capabilities**, not to be a styling project. It is credible
> and secondary, in that order. Aegis makes the same call about its own app, for the same reason: the
> phase that makes the repository credible is the security and correctness campaign, not the frontend.

---

## 1. Scope

Next.js + React, consuming `sentinel-api` only. **It never talks to an RPC endpoint directly** — every
number it shows is one Sentinel is prepared to defend, with a commitment label attached.

The exception is wallet signing: the user's wallet talks to the chain, as it must.

---

## 2. Views

| View | Shows | Demonstrates |
|---|---|---|
| **Indexer health** | Ingestion lag (slots and seconds), contiguity, unrepaired gaps, provider breakers, rollback history, decode-failure rate, keeper state | That Sentinel knows and states how wrong it might be |
| **Markets** | Every Aegis market: totals, utilization, rates, accrual staleness, pause bits, parameters, oracle configuration | Protocol decoding depth |
| **Market detail** | Parameter history with `effective_from_slot`, event stream, metric time series, custody invariant checks | Version-aware decoding and off-chain invariant checking |
| **Positions** | Filterable by health state; health factor, liquidation price, borrow capacity, and the exact oracle observations used | The risk engine |
| **Position detail** | Full event-derived history; health over time; the `unknown_oracle` state rendered honestly | Event-sourced reconstruction |
| **Oracle** | Per feed: age, confidence ratio, validity, which check failed, which markets are fail-closed right now | Oracle safety understanding |
| **Liquidations** | Open candidates with sizing and profitability; executed liquidations with predicted-vs-actual | The closed loop and its grading |
| **Transaction lifecycle** | One intent's full timeline: every state, every attempt, every signature, every rejection classified | The durable execution engine — **the most distinctive screen in the product** |
| **Chain state** | Recent slots, commitment promotion, forks, abandoned blocks | That forks are modelled, not ignored |

---

## 3. Non-negotiable UI rules

These are correctness requirements, not design preferences.

| # | Rule |
|---|---|
| UI-1 | **Every chain-derived value shows its commitment.** `confirmed` and `finalized` are visually distinct, always. |
| UI-2 | **Staleness is always visible.** A persistent header shows `as_of_slot` and lag; beyond the threshold it becomes a degraded banner with the reason. |
| UI-3 | **`unknown_oracle` renders as "unknown — oracle unavailable"**, with the failing check named. It is never rendered as healthy. |
| UI-4 | **`RECOMPUTING` and `STALE` entities are labelled**, with their last known-good slot. Pre-rollback values are never shown as current. |
| UI-5 | **Every health figure is labelled as Sentinel's prediction**, with a note that the Aegis program is authoritative. |
| UI-6 | **A `revision` message updates the view and says so** — a value that changed because of a reorg is visibly marked, not silently swapped. |
| UI-7 | **All signing happens in the user's wallet.** The UI never transmits a key, a seed, or a signed transaction to the backend; it submits a **signature** for tracking. |
| UI-8 | **`u128` values are strings end to end.** No JavaScript `Number` ever touches a share, an asset amount, or a WAD value. |
| UI-9 | The realtime client dedupes on `(topic, key, seq)` and handles `resync_required` by re-fetching, never by silently skipping. |
| UI-10 | Errors are rendered from their `code`, not their message text. |

UI-8 deserves emphasis: `Number.MAX_SAFE_INTEGER` is about `9.0e15`, and a WAD health factor is
around `1e18`. Any code path that parses one into a `Number` silently loses precision on the single
most important figure in the product.

---

## 4. What the UI deliberately is not

| Not | Why |
|---|---|
| A design showcase | Phase 15 is the last phase for a reason. Polish crowding out the correctness campaign is a named failure mode (`coverage-matrix.md` §4). |
| A trading interface | `product.md` §3. |
| A protocol admin panel | Aegis admin actions belong to Aegis's own app; Sentinel observes them. |
| A replacement for the Aegis SDK's authoritative reads | For anything a user acts on, the SDK's on-chain read is authoritative and the UI says so. |
| Mobile-first or heavily animated | Operator-facing density beats marketing polish here. |

---

## 5. Acceptance criteria (Phase 15)

| ID | Criterion |
|---|---|
| UI-A1 | Every view renders correctly against the local stack with no network beyond loopback |
| UI-A2 | Commitment labels present on every chain-derived value — asserted by a test, not by review |
| UI-A3 | The degraded banner appears when `meta.degraded` is set, with the reason |
| UI-A4 | A live reorg during the demo produces a visible revision |
| UI-A5 | The transaction-lifecycle view shows every state transition of a real liquidation, including a failed attempt and its classification |
| UI-A6 | No `Number` is used for any `u128`/WAD value — asserted by a lint rule and a test |
| UI-A7 | The full demo (`zero-cost-local.md` §4) is followable end to end in the UI |
