# Sentinel — Zero-Cost Local Architecture

**Status: FROZEN (Phase 0). Enforced from Phase 1 onward.**
**Research gate SR-7 is blocking: the local cluster must expose every RPC method Sentinel requires.**

> `make test` and `make demo` must pass on a clean clone with **no paid RPC, no API key, no faucet, and
> no hosted streaming service**. This is architectural, not aspirational — it is inherited directly
> from Aegis's ADR-0010 and it constrains Sentinel more tightly than it constrains Aegis, because
> Sentinel's whole subject is talking to a network.

---

## 1. Why this is architectural, not a convenience

An indexer built against a hosted provider is quietly unusable as evidence, for three reasons:

1. **Determinism.** Forks, gaps, reorgs, stale providers, oracle staleness, and liquidation races are
   nearly impossible to produce on demand against a public cluster and trivial to produce locally.
   **The entire Phase 12 failure-injection campaign exists because of this constraint.** Without it,
   Sentinel would have a chapter of documentation about reorgs and no test that has ever seen one.
2. **Reviewability.** Anyone can clone and reproduce every claim in minutes, with no account and no
   spend. A platform whose evidence cannot be independently reproduced is an assertion.
3. **Honesty about dependencies.** If the required path cannot run without a paid endpoint, then the
   paid endpoint is part of the architecture and should be in the diagram. Keeping it out of the
   required path keeps the diagram true.

---

## 2. How each dependency is eliminated

| Dependency | Solution |
|---|---|
| A Solana cluster | **Surfpool** in pure local mode — a full JSON-RPC surface plus WebSocket, with `surfnet_*` cheatcodes to set arbitrary accounts, mint tokens, warp and pause the clock |
| SOL and token balances | Cheatcodes / local mint authority. No faucet. |
| **Oracle prices** | **Byte-exact Pyth `PriceUpdateV2` account injection**, exactly as Aegis's own test kit does (`aegis/oracle-design.md` §5). Reading a pull price is an **account read, not a CPI**, so the Pyth program need not even be deployed. This is the dependency that usually forces a project onto a network, and Aegis's ADR-0008 removes it entirely. |
| Time | Clock warping — a year of interest accrual in microseconds, deterministically |
| The Aegis program | Built and deployed locally from the pinned Aegis revision once Aegis Phase 2 exists; until then, fixtures |
| Postgres (+ optional Redis) | Docker Compose |
| Historical chain data | The committed fixture corpus, captured from a scripted local scenario (`replay-and-backfill.md` §8) |
| Forks and reorgs | **Constructed deliberately by the fixture harness**, not waited for |
| Geyser streaming | Optional and self-hosted (`geyser-strategy.md` §6); never in a required path |
| Swap liquidity for liquidation | Not required — the v1 keeper is pre-funded (`keeper-design.md` §5) |
| Metrics/tracing backend | OpenTelemetry collector + Prometheus + Grafana in Compose |

The oracle row and the fork row are the two that matter most: they are the reason Sentinel can test its
hardest behaviors at all.

---

## 3. The local stack

```
make up      # docker compose: postgres, otel collector, grafana, [redis], surfpool
make migrate # apply migrations
make seed    # deploy Aegis (when available) and run the scripted scenario
make test    # full required suite, offline, no secrets
make demo    # the end-to-end scenario with live dashboards
make down
```

| # | Requirement |
|---|---|
| ZC-1 | `make test` passes with the network interface down, except for loopback. **This is testable and is a CI job.** |
| ZC-2 | CI runs with **no secrets configured**. A required test that needs one fails the build. |
| ZC-3 | No default configuration value points at any non-loopback host. |
| ZC-4 | Network- and provider-dependent tests are a separate, tagged tier excluded from `make test`. |
| ZC-5 | The demo requires no account, no key, and no spend. |

---

## 4. The demo

The scenario that exercises the whole platform, and the one a reviewer runs first:

```
 1. Bring up Postgres, Surfpool, telemetry.
 2. Deploy Aegis; initialize protocol; create a SOL/USDC market with the reference parameters.
 3. Inject Pyth prices: SOL $150.00 ± 0.30, USDC $1.0000 ± 0.0002.
 4. Lender supplies USDC; borrower deposits SOL collateral and borrows USDC.
 5. Sentinel ingests every step; show raw → normalized → protocol → derived in the UI,
    with the commitment label visible at each layer.
 6. Warp the clock; interest accrues; show Sentinel's accrual matching the InterestAccrued event.
 7. Drop SOL to $95.00 ± 0.20 by injecting a new price account.
 8. Sentinel detects HF ≈ 0.842495 — the exact figure from Aegis economic-model §6.5 — and
    creates a liquidation candidate.
 9. The keeper simulates, policy-checks, signs, PERSISTS, submits, and tracks to finalization.
10. Sentinel ingests the Liquidated event and reconciles predicted vs actual seizure
    against the exact figures in Aegis economic-model §7.5.
11. Kill a worker mid-flight; restart; show exactly one liquidation.
12. Inject a fork; show rollback, recompute, and a `revision` message on the WebSocket.
13. Delete all derived and protocol state; replay; show an identical digest.
14. Crash the price to zero collateral; show bad-debt detection and the loss decomposition
    (protocol first-loss, then socialization) — without acting, because that is an operator decision.
```

Steps 11, 12, and 13 are the point. Steps 1–10 are what any indexer demo shows; those three are what
distinguishes this one, and they are all free.

---

## 5. The optional network tier

Excluded from `make test`, tagged, and never referenced by a README claim:

| Optional | Requires | Why it exists |
|---|---|---|
| Devnet/mainnet ingestion soak | A public RPC endpoint | Real block composition, real transaction-version mix, real provider behavior |
| Hosted-provider comparison | Provider accounts | Capability-discovery validation against real endpoints |
| Hermes price posting | Hermes access | The keeper's optional self-post capability (`keeper-design.md` §6) |
| Hosted Yellowstone | A subscription | Latency comparison against self-hosted |
| Live-cluster keeper | Funds | Real competition, real fee markets |

**The rule:** if a README claim depends on this tier, either the claim is wrong or the tier is
misclassified.

---

## 6. Anti-patterns that erode this without anyone noticing

Each of these has broken a "runs offline" claim in a real project. Each has a CI guard.

| Anti-pattern | Guard |
|---|---|
| A default config value pointing at a public RPC | Startup assertion + config test |
| A test that "just checks one thing" against devnet | No-network CI job |
| Fetching an IDL from a network at build time | Vendored, pinned IDL |
| A dependency that phones home | Dependency review; offline build |
| A fixture regenerated from the network at test time | Fixtures are committed; regeneration is an explicit, separate command |
| `Keypair::new()` in a test | CI grep; fixed seeds only |
| A Docker image pulled from a private registry | Public base images only |
| Metrics or tracing exporting to a hosted backend by default | Local collector by default |
| The demo needing a funded account | Cheatcodes only |
| A "required" test tagged optional to make CI green | Tag audit in review |

---

## 7. What local testing cannot catch

Acknowledged rather than papered over:

- Real network congestion, real fee markets, real competition for liquidations.
- Real provider quirks: undocumented limits, inconsistent `context`/`minContextSlot` support, subtly
  different error shapes.
- Real block composition — transaction-version mix, vote-transaction density, account-update volume.
- Real oracle latency and real Hermes behavior.
- Genuine large-scale reorg behavior under network stress.

The optional tier and a devnet soak partially cover these. **The gaps are stated here so that no
result from the local suite is over-claimed**, which is the same discipline Aegis applies in its own
ADR-0010 consequences section.
