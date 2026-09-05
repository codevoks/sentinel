# Sentinel — External Integrations

**Status: FROZEN (Phase 0).**

> **Rule: an external integration enters Sentinel only when it solves a product problem Sentinel
> actually has.** Breadth of integrations is not evidence of skill; a well-motivated integration with a
> clearly stated trust boundary is. This rule is inherited verbatim from Aegis's `composability.md`.

---

## 1. Integration inventory

| Integration | Phase | Trusted for | NOT trusted for | Required offline? |
|---|---|---|---|---|
| **Solana RPC / WebSocket** | 3–4 | Transporting a response | Content, freshness, completeness, or agreeing with any other provider | **Yes** (local validator) |
| **Aegis Protocol** | 7, 11 | Being authoritative over its own state | Never changing; being decodable without a pinned schema | **Yes** (local deploy / fixtures) |
| **SPL Token / Token-2022** | 5, 7 | Nothing directly — Sentinel only *reads* balances | Any relationship between amount sent and amount credited | **Yes** |
| **Pyth pull oracle** | 7 | Nothing until O-1..O-11 pass | Availability, freshness, correctness, being the right feed | **Yes** (byte-exact account injection) |
| **PostgreSQL** | 2 | Durability and isolation | — | **Yes** |
| **Redis** | 9 | Ephemeral fanout, rate limiting, short-TTL hints | **Anything durable or authoritative** | **Yes** — and Sentinel runs without it |
| **OpenTelemetry collector** | 12 | Transport of telemetry | — | **Yes** (local) |
| **Yellowstone / Geyser** | 13 | Lower-latency delivery | Completeness, ordering across reconnects, replay | **No — optional tier** |
| **Hermes (Pyth off-chain)** | optional | Fetching signed price updates | — | **No — optional tier** |
| **Jupiter** | **not integrated** | — | — | — |
| **Jito** | **not integrated** | — | — | — |

---

## 2. SPL Token and Token-2022 — the only token work Sentinel does

Sentinel indexes token movements because **Aegis's custody invariants are stated in terms of vault
balances** (`INV-CUS-01`, `INV-CUS-02`), and Sentinel checks those off-chain. That is the product
reason; there is no other.

| Concern | Handling |
|---|---|
| Balance deltas | From `meta.pre/postTokenBalances`, **not** parsed instruction JSON — the parsed shapes changed in Agave 4.2 (`ecosystem-research.md` §1.4) |
| Transfer fees | Aegis uses **measured-delta accounting**: credited = `vault_after − vault_before`. Sentinel models `amount_in` and `credited` **separately** and never assumes they are equal. `market.flags` bit1 says when a difference is expected |
| Extension policy | Aegis's positive allowlist means a market's mints are already vetted. Sentinel records the accepted extension inventory from `MarketCreated` as the permanent audit record |
| Confidential transfers | Rejected by Aegis (`token-compatibility.md` §2), so out of scope for the Aegis adapter — but the general token decoder must not **crash** on one appearing elsewhere in a block (S-03) |
| Precision | `numeric(39,0)` for `u128`, `numeric(20,0)` for `u64`. No floating point, anywhere |

---

## 3. Jupiter — considered and rejected for v1

Aegis names Jupiter as its Phase 8 liquidity router, reached **only through an untrusted liquidation
callback**, and classifies it as optional-tier because it needs real mainnet liquidity
(`aegis/composability.md` §4).

**Sentinel does not integrate Jupiter in v1.** The reasoning, recorded rather than the conclusion:

| Argument | Assessment |
|---|---|
| "The keeper needs to swap seized collateral" | It does not, in v1. The keeper is **pre-funded** and accumulates collateral; disposal is an operator action. Automating disposal would make Sentinel a trading system, which `product.md` §3 excludes. |
| "Swap-and-repay removes the capital requirement" | True, and it is genuinely valuable — but it requires **Aegis's Phase 8 liquidation callback**, which does not exist. Building the client half of a protocol feature that has not been designed yet is inventing an interface. |
| "It would demonstrate composability" | **Not a product reason.** |
| "It improves profitability modelling" | A live quote would improve the slippage estimate — at the cost of a network dependency on the required path, a quote-validity window, and a whole trust boundary, to refine one parameter that is measured against realized outcomes anyway. |

### 3.1 The conditions under which it enters

An ADR would be required, and it would have to establish all four:

1. Aegis Phase 8 has shipped the liquidation callback, with its trust boundary specified.
2. Measured data shows capital constraint is actually limiting the keeper — candidates skipped for
   `INSUFFICIENT_INVENTORY` above a stated rate.
3. The integration lives strictly in the **optional network tier**; the required path stays pre-funded.
4. The trust boundary is specified before code: the callback is trusted for **nothing**, all state is
   re-read after the CPI, quote validity is bounded, slippage is capped, the transaction is simulated,
   and the program allowlist is extended explicitly rather than opened.

**Until all four hold, integrating Jupiter would be exactly the keyword-driven padding both
repositories forbid.**

---

## 4. Jito / MEV — awareness without dependency

At the research date, Jito-Solana runs under >95% of active stake and tips are >60% of priority-fee
volume; bundles give atomic, leader-direct submission, and liquidation is a primary bundle use case
(`ecosystem-research.md` §10).

**Not integrated in v1**, because:

- It requires a live cluster and a relay — impossible on the zero-cost path.
- Aegis liquidation is profitable by construction (`INV-LIQ-08`), so bundle inclusion is a
  **competitiveness** optimization, not a correctness requirement.
- Sentinel has no measurement yet showing it loses races often enough to matter.

**What Sentinel does instead:** the keeper's competition model explicitly accounts for losing to
bundle-submitting competitors (`keeper-design.md` K-1), `race_loss_rate` is a first-class metric, and
the coverage matrix records Jito as **DOC/awareness** with a stated adoption trigger — a measured loss
rate above threshold in a live-cluster campaign.

---

## 5. Rejected integrations

| Rejected | Why |
|---|---|
| A second protocol adapter in v1 | One protocol done deeply beats three done shallowly (`product.md` §2.2). The seam exists; using it is a v2 decision with a product reason. |
| A DEX/AMM integration for price cross-checks | Sentinel's price source must be **the one Aegis uses**, or its health predictions diverge from the chain's decisions by construction. A second price source would be actively harmful here. |
| Cross-chain indexing | Enormous trust surface, no product reason. |
| A hosted indexing API as a fallback source | Would make Sentinel's correctness depend on another indexer's correctness — an unauditable dependency, and a strange one for a project whose subject is indexing correctly. |
| A notification service (email/SMS/Discord) | Alerting integration is deployment configuration, not architecture. The alert *definitions* are the engineering (`observability.md` §5). |
| A wallet adapter in the backend | Users sign in their own wallet, in their browser. |
| An external secrets service in v1 | Environment/file-based secrets with a documented interface; a manager is a deployment choice (`signer-and-key-management.md` §5). |

---

## 6. What Sentinel exposes to integrators

Sentinel is designed to be built **on**, not only to build on things:

- **A versioned REST API** with explicit commitment semantics and honest staleness metadata.
- **A resumable WebSocket stream** with cursors, explicit revisions on rollback, and at-least-once
  semantics stated rather than implied.
- **Idempotency keys** on every write endpoint, mapped onto the same business idempotency the execution
  engine uses.
- **A complete Aegis event history**, reconstructed and queryable, which is exactly the query the chain
  cannot answer.
- **`/statusz`**, machine-readable, so a consumer can make its own freshness decision instead of
  trusting a green light.
