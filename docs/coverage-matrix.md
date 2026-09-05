# Sentinel — Topic Coverage Matrix and Gap Analysis

**Status: FROZEN (Phase 0). Classifications may change only via ADR.**

Classification: **PRODUCTION** (in the shipped services) · **LAB** (a scoped, benchmarked artifact) ·
**TEST** (demonstrated through the test suite) · **ADR** (a recorded, argued decision, including a
decision *not* to build something) · **NOT COVERED** (with a stated reason).

Multiple classifications are normal and are the honest answer.

**The rule: no artifact, no claim.** Every row names the exact future repository artifact that will
prove it. A row whose artifact does not exist is a plan, and `docs/project-status.md` says so.

---

## 1. Matrix

| # | Topic | PROD | LAB | TEST | ADR | Evidence artifact |
|---|---|:--:|:--:|:--:|:--:|---|
| 1 | Solana fundamentals (accounts, programs, slots, blocks) | ✅ | | ✅ | | `sentinel-normalize`; `slots`/`transactions`/`instructions` schema |
| 2 | Commitment levels & finality | ✅ | | ✅ | ✅ | `sentinel-chainstate`; ADR-0009; `P-MONO-1`; `CHN-01..10` |
| 3 | Forks & reorg handling | ✅ | | ✅ | ✅ | `rollback_events`; FI-16 at depths 1/5/30; RP-06 |
| 4 | Rust async / Tokio | ✅ | | ✅ | ✅ | Every Rust service; bounded channels; backpressure test FI-28; ADR-0001 |
| 5 | Solana JSON-RPC | ✅ | | ✅ | ✅ | `sentinel-rpc`; capability discovery; `RPC-01..10`; ADR-0005 |
| 6 | RPC resilience (pool, breaker, failover, backoff) | ✅ | | ✅ | ✅ | `FaultInjectingProvider`; FI-09, FI-21, FI-22; `rpc-strategy.md` |
| 7 | WebSocket subscriptions | ✅ | | ✅ | ✅ | `RpcWsSource`; reconnect+gap-scan test FI-06; ADR-0005 |
| 8 | Geyser / Yellowstone | ⬜ *optional* | | ⬜ | ✅ | Phase 13 `sentinel-geyser`; GS-01..07; **ADR-0006 with a measured adoption trigger** |
| 9 | Transaction parsing (legacy / v0 / **v1**) | ✅ | | ✅ | | `sentinel-normalize`; per-version unit corpus; `A-VER-01` |
| 10 | Account decoding | ✅ | | ✅ | ✅ | `sentinel-aegis`; `AEGIS-LAYOUT-01`; ADR-0012 |
| 11 | Anchor interoperability (IDL, discriminators, `emit!` logs) | ✅ | | ✅ | ✅ | Vendored IDL; `aegis_events` decoding; `AEGIS-EVENT-01`; SR-9 |
| 12 | SPL Token | ✅ | | ✅ | | `token_balance_deltas` from pre/post balances |
| 13 | Token-2022 | ✅ | | ✅ | ✅ | Measured-delta modelling (`amount_in` vs `credited`); extension inventory from `MarketCreated` |
| 14 | Versioned transactions | ✅ | | ✅ | | `transactions.version`; v1 `transactionConfig` decoding |
| 15 | Address lookup tables | ⬜ | | ✅ | ✅ | **Decoded, never used.** Aegis `INV-RES-06` says every instruction fits a legacy tx; v1 removes ALTs. `transaction-engine.md` §4.5 |
| 16 | Compute budget | ✅ | | ✅ | | Simulation-derived CU limit; `transaction_attempts.compute_unit_limit` |
| 17 | Priority fees | ✅ | | ✅ | | Writable-account-scoped estimation; per-version source discriminator; SR-4 |
| 18 | Transaction simulation | ✅ | | ✅ | ✅ | Mandatory pre-sign simulation; `A-SIGN-04`; ADR-0011 |
| 19 | Blockhash expiry & ambiguity resolution | ✅ | | ✅ | ✅ | `lastValidBlockHeight` resolution; FI-17, FI-18, FI-19; ADR-0010 |
| 20 | Transaction signing | ✅ | | ✅ | ✅ | Signer service + 12-check policy engine; `A-SIGN-01..10`; ADR-0011 |
| 21 | Durable nonces | ⬜ | | | ✅ | **Analyzed and rejected with the conditions under which it would be right** — `transaction-engine.md` §4.6 |
| 22 | Indexing pipeline | ✅ | | ✅ | ✅ | The whole raw→normalized→protocol→derived stack; ADR-0008 |
| 23 | Backfill | ✅ | | ✅ | | `sentinel-backfill`; `BF-1..7`; FI-15 |
| 24 | Replay & determinism | ✅ | | ✅ | ✅ | `replay_runs.output_digest`; **RP-01..RP-12**; ADR-0008 |
| 25 | Deduplication / idempotency | ✅ | | ✅ | ✅ | Natural keys per table; `P-IDEM-1..3`; FI-07; ADR-0007 |
| 26 | Gap detection & repair | ✅ | | ✅ | | `gap_events`; `getBlocks` reconciliation; FI-06 |
| 27 | PostgreSQL schema design | ✅ | | ✅ | ✅ | `infra/migrations/`; partitioning; per-table roles; ADR-0002 |
| 28 | Postgres partitioning & retention | ✅ | | ✅ | | Slot-range partitions; detach/archive; `PartitionsExhausted` alert |
| 29 | Redis | ✅ *bounded* | | ✅ | ✅ | Fanout/rate-limit/hints only; **FI-13 removes it entirely**; ADR-0003 |
| 30 | Queues & workers | ✅ | | ✅ | ✅ | `jobs` with `SKIP LOCKED` + leases + quarantine; FI-20; **ADR-0004 with a measured Kafka threshold** |
| 31 | Distributed-system correctness | ✅ | | ✅ | ✅ | `distributed-correctness.md`; 12 named races `T-RACE-01..12`; 28 FI entries |
| 32 | Crash recovery | ✅ | | ✅ | | FI-01..05; the sign→persist→submit boundary tests in the **required** tier |
| 33 | REST API design | ✅ | | ✅ | | OpenAPI diffed in CI; `CT-01..06` |
| 34 | Realtime WebSocket API | ✅ | | ✅ | | Cursor resume; `revision` on rollback; FI-28 |
| 35 | Observability (metrics/traces/logs) | ✅ | | ✅ | | OTel; ~70 metrics; **every alert has a runbook action** |
| 36 | Load & performance testing | | ✅ | ✅ | | `benchmarks/*.json`; Phase 14; CI regression gate |
| 37 | Failure injection | | | ✅ | | The 28-entry catalogue, nightly + required subset |
| 38 | Security engineering | ✅ | | ✅ | ✅ | 25 threats `S-01..25`, each with a named test and a mutation check |
| 39 | Protocol integration (Aegis) | ✅ | | ✅ | ✅ | Version-aware adapter; `AEGIS-CONF-01..06`; ADR-0012 |
| 40 | Off-chain protocol invariant checking | ✅ | | ✅ | | `aegis_invariant_checks`; `AEGIS-INV-01..08`; serves Aegis runbook R-2 |
| 41 | Liquidation keeper automation | ✅ | | ✅ | ✅ | `sentinel-keeper`; `KP-01..14`; `keeper-design.md` |
| 42 | Oracle safety (Pyth) | ✅ | | ✅ | | O-1..O-11 implemented and individually violated in `A-ORA-01..11` |
| 43 | Fixed-point / exact arithmetic | ✅ | | ✅ | ✅ | `numeric(39,0)`; `CI-NOFLOAT`; `aegis-math` consumed, not reimplemented |
| 44 | Docker / Compose | ✅ | | ✅ | ✅ | `infra/compose/`; ADR-0014 |
| 45 | CI | ✅ | | ✅ | | Required tiers, grep guards, traceability, no-network job, secret scan |
| 46 | Kubernetes tradeoff | | | | ✅ | **ADR-0014 with a stated adoption threshold.** Not built. |
| 47 | Kafka tradeoff | | | | ✅ | **ADR-0004 with a measured adoption threshold.** Not built. |
| 48 | MEV / Jito concepts | | | | ✅ | `external-integrations.md` §4; `race_loss_rate` metric; **awareness with a stated adoption trigger.** Not integrated. |
| 49 | Jupiter / external CPI awareness | | | | ✅ | `external-integrations.md` §3 — four conditions required before integration. Not integrated. |
| 50 | Frontend integration | ✅ | | ✅ | | Next.js app; `UI-A1..A7` |
| 51 | Architecture documentation | | | | ✅ | This `docs/` tree + Mermaid diagrams |
| 52 | Threat modelling | | | | ✅ | `threat-model.md`, 25 threats with trust boundaries and residuals |
| 53 | Benchmark evidence | | ✅ | ✅ | ✅ | Committed `benchmarks/*.json` + regression gate; **no number stated before it is measured** |

---

## 2. Explicit NOT COVERED

| Topic | Status | Reason |
|---|---|---|
| Block explorer / whole-chain indexing | **NOT COVERED** | Destroys every correctness claim by removing anything to reconcile against (`product.md` §2.1). |
| Multi-protocol analytics warehouse | **NOT COVERED (v1)** | Breadth-shaped shallowness. The layer seam is real and tested (`CI-NOAEGISLEAK`); using it is a v2 with a product reason. |
| Mempool / pre-execution data | **NOT COVERED** | Requires privileged infrastructure; cannot exist on the zero-cost path. |
| Jito bundles | **NOT COVERED as PRODUCTION** | Needs a live cluster and a relay. Covered as ADR/awareness with a measured adoption trigger. |
| Jupiter routing | **NOT COVERED (v1)** | Requires Aegis Phase 8's callback, which does not exist. Four conditions stated in `external-integrations.md` §3.1. |
| Durable nonces | **NOT COVERED (v1)** | Would remove `lastValidBlockHeight` as the termination oracle. Analyzed, rejected, with the conditions under which it would be right. |
| Address lookup tables as a requirement | **NOT COVERED as required** | Aegis instructions fit a legacy transaction; v1 transactions remove ALTs. Decoded, never used. |
| Kubernetes | **NOT COVERED** | ADR-0014 with an adoption threshold. |
| Kafka / any broker | **NOT COVERED** | ADR-0004 with a measured threshold. |
| Cross-chain indexing | **NOT COVERED** | No product reason. |
| NFT / social / token-launch analytics | **NOT COVERED** | No product reason. |
| Custodial key management | **NOT COVERED — permanently** | ADR-0011. Structural, not a scoping decision. |
| Trading / hedging / collateral disposal | **NOT COVERED** | Would make Sentinel a trading system (`product.md` §3). |
| Aegis's own on-chain correctness | **NOT COVERED** | That is Aegis's test suite. Sentinel tests its **interpretation**. |
| Machine learning / anomaly detection | **NOT COVERED** | Deterministic invariant checks are strictly better here and are actually verifiable. |

---

## 3. Honest self-assessment of coverage *quality*

Breadth is not evidence. Where the depth is load-bearing versus merely present:

| Topic | Depth | Note |
|---|---|---|
| Idempotency & at-least-once | **Deep** | Three layers, per-table natural keys, and the fold-not-increment rule that makes it structural |
| Crash-safe execution | **Deep** | The sign→persist→submit ordering with a crash test at each boundary in the required CI tier |
| Fork / finality handling | **Deep** | `(slot, blockhash)` keying, rebuild-forward-never-subtract, per-interleaving execution rules, tested at three depths |
| Replay determinism | **Deep** | A digest-based harness with 12 acceptance criteria, three of them on every commit |
| RPC resilience | **Deep** | Capability discovery, request classes, freshness checks, divergence detection with immediate breaker trip |
| Protocol integration | **Deep** | Consumes the protocol's own artifacts; conformance against its frozen worked examples; version-aware with a no-guessing rule |
| Signer security | **Deep** | 12 policy checks on re-decoded final bytes, with a stated and bounded blast radius |
| Observability | **Moderate–deep** | ~70 metrics and an action per alert; depth proven only when the Phase 12 campaign runs |
| Performance | **Deferred by design** | Methodology and gates only. **No number is claimed in Phase 0**, which is the honest state |
| Geyser | **Moderate — deliberately** | An optional adapter with a measured trigger, not a headline |
| Frontend | **Moderate — deliberately** | Functional and honest about commitment; not a design showcase |
| Token-2022 | **Moderate** | Sentinel only reads balances; the depth lives in Aegis. Sentinel's contribution is refusing to assume `amount_in == credited` |

Anything marked "Moderate" or "Deferred" is scoped that way on purpose and is labelled rather than
inflated.

---

## 4. Gap analysis — what would make these claims false

| Gap | Risk | Mitigation |
|---|---|---|
| **Documentation outruns implementation** | The most likely failure: 25 excellent planning documents and a half-built indexer. Worse than no plan. | Phase gating; `project-status.md` tracks five states separately; no phase completes without evidence |
| **Blocked upstream on Aegis forever** | Phases 7, 8, 11 depend on an upstream project also at Phase 0 | Phases 1–6 have **zero** Aegis dependency; the interim conformance path uses Aegis's already-frozen numbers; blocked phases are reported blocked, never faked |
| **Replay determinism claimed but not continuously checked** | It decays silently the moment a processor reads the clock | RP-01..03 in **every** CI run; `CI-NOSLOTTIME` and the no-wall-clock property test |
| **Failure injection that does not inject** | A "chaos" suite that never actually kills anything | Each FI entry asserts a **specific** outcome; the mutation check requires the test to fail when the mechanism is removed |
| **Idempotency asserted, not tested** | A duplicate path added later with no constraint | DM-03 asserts a unique index exists per declared natural key; FI-07 duplicates 10% of the stream |
| **Commitment labels dropped in one code path** | A single unlabelled field makes the whole guarantee false | Type-level enforcement plus `CT-06` on every response |
| **A hidden network dependency in a required test** | The zero-cost claim dies quietly | CI with no secrets **and** a dedicated no-network job |
| **Performance claimed without measurement** | The exact failure `AGENTS.md` §9 forbids | No number is written until the harness produces it; committed baselines with a regression gate |
| **Sonnet redesigning during implementation** | Architectural drift across phases | Frozen specs; `CLAUDE.md` requires an ADR for deviations; the flexibility list is explicit |
| **A component added for coverage** | Kafka/Kubernetes/Geyser/Jito appearing because they look good | Each has an ADR with a **measured** adoption threshold, and the matrix classifies them as ADR-not-built |
| **Sentinel presented as authoritative over Aegis** | The category error this project exists to avoid | `authoritative: false` on every health object; `AGENTS.md` §3; the reconciliation taxonomy treats disagreement as a Sentinel bug first |
| **Phase 15 UI absorbing the schedule** | Polish crowding out the correctness campaign | Phase 12 is the priority phase, and the roadmap says to cut 13 and 15 before it |

---

## 5. The single most important claim

> Every capability asserted in the README is backed by a file, a test, a benchmark, or a
> failure-injection result that a reader can run offline in minutes, with no account and no spend.

If that stops being true, the repository has failed at its purpose regardless of how much of this
matrix is ticked. `project-status.md` exists to keep it true, and it separates IMPLEMENTED from TESTED
from DEMOED specifically so that "done" cannot be claimed loosely.
