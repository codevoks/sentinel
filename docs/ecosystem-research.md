# Sentinel — Current Ecosystem & Tooling Research

**Research date: 2026-09-05.**
**Status: FROZEN for Phase 0. Re-verification is a mandatory Phase 1 task (§12).**

This document records what was verified about the Solana ecosystem *at the research date*, which
sources were used, and which Sentinel design decisions depend on each finding. Nothing in Sentinel may
be built on a tutorial or a memory of "how Solana worked"; it must trace to an entry here or to a newer
verified entry added by a later phase.

**Relationship to Aegis:** Aegis's own `docs/ecosystem-research.md` (research date 2026-09-04) covers
the on-chain toolchain — Anchor 1.x, Pyth, LiteSVM/Surfpool/Mollusk, Token-2022, `@solana/kit`. That
document is **inherited, not duplicated**. This one covers the off-chain surface Aegis does not: RPC
behavior, subscriptions, commitment/finality, transaction formats and fees, streaming infrastructure,
and the Rust/TypeScript client ecosystem as it applies to an indexer and executor.

---

## 0. Verification protocol

Every claim below is either (a) read from a source at the research date and cited, or (b) marked
`UNVERIFIED` / `ASSUMED`. Implementation phases MUST re-run the verification steps in §12 before
pinning anything, because this file will drift — and, given the two network upgrades in flight (§2,
§3), it will drift faster than Aegis's.

**Local machine state at research date** — inherited from the Aegis measurement of 2026-09-04, not
independently re-measured:

| Tool | Local version | Status |
|---|---|---|
| `solana` (Agave CLI) | 2.2.21 | **STALE — Phase 1 must upgrade** |
| `rustc` / `cargo` | 1.88.0 | Adequate; re-check against pinned crates |
| `node` | v22.12.0 | Adequate |
| `docker` / `docker compose` | not measured | **Phase 1 must measure** |
| `psql` / PostgreSQL | not measured | **Phase 1 must measure** |

---

## 1. Agave 4.2 — the changes that matter to an indexer

Agave 4.2 was recommended for mainnet adoption in August 2026; validator upgrades began 2026-08-11.
Five changes are directly load-bearing for Sentinel.

### 1.1 Account update suppression — **highest-impact finding for ingestion**

Agave 4.2 emits an account update **only when the account actually changes**, reported as roughly **80%
fewer events** on account subscriptions and gRPC account streams. Previously, every writable account in
a transaction produced an update. **Only the fee payer is guaranteed to update.**

**Consequences for Sentinel, all mandatory:**
- Sentinel must **never** infer "a transaction touched this account" from the presence of an account
  notification, and must never treat a missing notification as an error or a health signal.
- Account-state ingestion is therefore **incomplete by construction** and cannot be the only source of
  protocol state. Transaction/instruction/event ingestion is the primary reconstruction path, with
  account snapshots as a *reconciliation* input. This is the direct justification for the dual-path
  design in `docs/aegis-integration.md` §7.
- Any "liveness by update frequency" health check is invalid.

### 1.2 Transaction v1 and `getBlock` failure

A block containing a v1 transaction makes `getBlock` fail with error **-32015** unless
`maxSupportedTransactionVersion` is set. The same applies to `getTransaction` and
`getTransactionsForAddress` with `transactionDetails = full`.

**Consequence:** every historical-fetch call site in Sentinel sets an explicit
`maxSupportedTransactionVersion`. A missing parameter is a hard bug that presents as an unexplained
ingestion stall — recorded as threat **S-04** and covered by a required test.

### 1.3 `DeactivatedStake` reward type

A new `rewardType` value that closed-enum parsers silently drop.

**Consequence:** Sentinel's normalized layer treats every enum-like field from RPC as **open**: an
unrecognized value is stored verbatim and counted, never dropped and never fatal. This is a general
rule (`docs/ingestion-model.md` §11), not a one-off patch.

### 1.4 Token-2022 parser changes

Confidential-transfer instructions replace `source`/`destination` with a single `account` field;
extension arrays now populate fully instead of returning empty; new instruction types
(`unwrapLamports`, `confidentialBurn`) arrive parsed rather than raw.

**Consequence:** Sentinel does not depend on RPC's *parsed* JSON for anything protocol-critical. Token
movements are derived from `meta.preTokenBalances` / `meta.postTokenBalances` and from raw instruction
data, which are stable. Parsed JSON is stored as a convenience field only. Aegis rejects confidential
transfers outright (`token-compatibility.md` §2), so the confidential path is out of scope for the
Aegis adapter but must not crash the general token decoder.

### 1.5 Slot timing is not a constant

Hardcoded 400ms slot durations were already ~12.5% wrong at the research date (mainnet ~350ms), moving
to 300ms at epoch 1024 (2026-08-28) and targeting 200ms (SIMD-0525, four 50ms steps).

**Consequence (NFR-14):** Sentinel derives all durations from block timestamps or wall clock. **No
staleness window, expiry, timeout, or lag metric is expressed in slots** except where the chain itself
does (e.g. `lastValidBlockHeight`, which is block-height-based and therefore correct). This mirrors
Aegis's NFR-13 for the same underlying reason.

Sources: <https://www.helius.dev/blog/agave-4-2-migration-checklist> ·
<https://solana.com/upgrades/agave-4-2-release-overview>

**Decisions affected:** ADR-0005, ADR-0008, ADR-0009; `ingestion-model.md`, `aegis-integration.md` §7.

---

## 2. Transaction v1 (SIMD-0385) — **in flight at the research date**

The most consequential *pending* change. Status at research date: SIMD-0385 is in **Review**; local
testing is available; testnet activation was targeted for **2026-09-02**; mainnet activation is
described as weeks away with the date **TBD**.

Verified wire format from the SIMD:

```
VersionByte (u8)            // MUST be 129 for v1
LegacyHeader (u8, u8, u8)
TransactionConfigMask (u32)
LifetimeSpecifier [u8; 32]
NumInstructions (u8)
NumAddresses (u8)
Addresses [[u8; 32]]
ConfigValues [[u8; 4]]
InstructionHeaders [(u8, u8, u16)]
InstructionPayloads
Signatures [[u8; 64]]
```

`TransactionConfigMask` bits:

| Bits | Field | Encoding |
|---|---|---|
| 0 and 1 (both, or invalid) | **Priority fee** | 8-byte LE `u64`, **absolute lamports** |
| 2 | Compute-unit limit | 4-byte LE `u32` |
| 3 | Loaded-accounts data size limit | 4-byte LE `u32` |
| 4 | Requested heap size | 4-byte LE `u32` |

Limits: **4096-byte** transaction · 12 signatures · **64 accounts** · 64 instructions.
**v1 does not support address lookup tables** — all addresses are inlined as full 32-byte keys.

### Consequences for Sentinel — decoding

- **Priority fee and compute budget are not instructions in v1.** Any code that computes a
  transaction's priority fee by scanning for `ComputeBudget` instructions returns **zero, silently**,
  for every v1 transaction. Sentinel's fee attribution must read `transactionConfig` when present and
  `ComputeBudget` instructions otherwise, and must record **which** path produced the value.
- Priority fee semantics differ: legacy/v0 is micro-lamports **per compute unit** (total =
  `ceil(price × CU_limit / 1e6)`); v1 is an **absolute lamport total**. Storing them in one column
  without a discriminator would corrupt every fee metric.
- The normalized `transactions` table therefore carries `tx_version`, `priority_fee_lamports`,
  `priority_fee_source` (`config_mask` | `compute_budget_ix` | `absent`), and `compute_unit_limit`.

### Consequences for Sentinel — building transactions

- The 64-account limit is unchanged, so v1 buys **bytes, not account slots**. Aegis's own
  `INV-RES-06` states every Aegis instruction fits a legacy 1232-byte transaction with no ALT, and
  `liquidate` uses 14 accounts + program. **Sentinel therefore has no need for ALTs and no need for
  v1.** It targets v0 and must remain correct when the cluster activates v1.
- Sentinel must nonetheless *decode* v1 from the moment it activates, because other people's
  transactions will be in the blocks it ingests.

**`UNVERIFIED` / research gate SR-1:** mainnet activation date and whether `@solana/kit` 8.x and the
Rust client line encode/decode v1 at the versions Sentinel pins.

Sources: <https://github.com/solana-foundation/solana-improvement-documents/blob/main/proposals/0385-transaction-v1.md> ·
<https://solana.com/news/transaction-v1-and-the-alt-trade-off> ·
<https://solanacompass.com/news/solana-v1-transactions-now-testable-locally-as-mainnet-activation-nears>

**Decisions affected:** `data-model.md` §4, `transaction-engine.md` §5, coverage matrix (ALTs).

---

## 3. Alpenglow / Agave 4.3 — finality semantics are changing **now**

At the research date, Agave 4.3 carries Alpenglow, with feature activation beginning **2026-09-28** and
effect landing over the following epoch boundaries (October 2026). Alpenglow replaces TowerBFT with the
**Votor** voting algorithm, targets roughly **150ms finality** versus ~12.8s today, and **eliminates
vote transactions** — validators exchange votes directly through a structure called Pool.

**Consequences for Sentinel:**

1. **Finality timing is configuration, never a constant.** No code may assume "32 slots" or "~13
   seconds" to finality. Timeouts for the confirmed→finalized promotion are configurable with a
   conservative default and are measured, not assumed (`finality-and-forks.md` §5).
2. **The `confirmed`→`finalized` gap narrows dramatically.** This is good for Sentinel — the window in
   which a fork can invalidate derived state shrinks — but the architecture must not *depend* on the
   gap being small, because Sentinel must also work against a local validator and against a cluster
   before activation.
3. **Vote transactions disappearing changes block composition.** Any metric or filter assuming a
   large fraction of vote transactions (or filtering them out) changes meaning. Sentinel filters votes
   explicitly at the source and records the filter it applied, rather than inferring.
4. **Sentinel must not conflate "fast finality" with "no forks."** Rollback handling remains mandatory.

**`UNVERIFIED` / research gate SR-2:** whether the `confirmed` commitment level retains its current
meaning under Votor, and whether RPC exposes any new commitment or finality-proof surface. This must be
closed before Phase 6 (fork & finality engine) and re-checked in Phase 12.

Sources: <https://solanacompass.com/news/anza-publishes-agave-43-release-schedule-alpenglow-consensus-targets-september-28-mainnet-activation> ·
<https://solana.com/upgrades/agave-4-2-release-overview>

**Decisions affected:** ADR-0009, `finality-and-forks.md`, Phase 6 acceptance criteria.

---

## 4. RPC WebSocket subscriptions — reliable enough to build on, not reliable enough to trust

Verified characteristics at the research date:

| Finding | Consequence for Sentinel |
|---|---|
| Agave's native PubSub **drops messages under load**; it runs subscription processing on a single worker thread by default. | WebSocket is a *latency* source, never a *completeness* source. Completeness comes from slot-range reconciliation over HTTP. |
| **Subscriptions do not survive reconnect** — they must be re-established explicitly. | The connection manager owns a declarative subscription set and re-establishes it on every connect, then triggers a gap scan. |
| Every disconnect creates a potential gap; the recommended pattern is to track the last processed slot and replay. | Exactly Sentinel's checkpoint + gap-detector design (`ingestion-model.md` §7–8). |
| Unthrottled reconnection becomes a self-inflicted outage. | Reconnect uses bounded exponential backoff with full jitter and a circuit breaker (`rpc-strategy.md` §6). |
| **`blockSubscribe` is documented as unstable**, requires `--rpc-pubsub-enable-block-subscription` plus `--enable-rpc-transaction-history`, and drops or oversizes under volume. | **`blockSubscribe` is not used in any required path.** Sentinel uses `slotSubscribe` + `accountSubscribe` + `logsSubscribe` for latency and HTTP `getBlock` for completeness. |

**Design conclusion, stated once and inherited everywhere:** *the WebSocket tells Sentinel when to
look; HTTP tells Sentinel what is true.* Every realtime path has a slower, complete counterpart, and
the two reconcile.

Sources: <https://rpcfast.com/blog/solana-websocket-subscriptions> ·
<https://solana.com/docs/rpc/websocket/blocksubscribe> · <https://blog.quicknode.com/blazar-solana-websocket-engine/>

**Decisions affected:** ADR-0005, ADR-0007, `ingestion-model.md` §4–8, `rpc-strategy.md`.

---

## 5. Yellowstone gRPC / Geyser — the optional path

- Yellowstone (Triton One's "Dragon's Mouth") is the de-facto open-source Geyser gRPC plugin
  (`rpcpool/yellowstone-grpc`). It exposes **slots, accounts, transactions, blocks, entries** streams
  with filters, plus unary calls, over `processed / confirmed / finalized`.
- Reported version signals at the research date: **v12.2.0 (March 2026)** adding compression options
  and refined entry filters; **client v13.1.0+** handles reconnect automatically and **v13.3.0+** makes
  the reconnect-on-outage behavior selectable. Agave 4.2 migration guidance names
  `yellowstone-grpc-client` **13.3.0+** and `yellowstone-grpc-proto` **12.6.0+**.
  *(These version lines are inconsistent across sources — see gate SR-3.)*
- It is genuinely self-hostable and free: build the plugin, write `geyser-config.json`, run
  `agave-validator --geyser-plugin-config`. One source claims a patched Triton validator fork is
  required; the plugin's own README shows plain `solana-validator --geyser-plugin-config`.

**`UNVERIFIED` / research gate SR-3:** the exact current plugin/client/proto versions, whether stock
Agave 4.2+ is supported without a fork, and whether the stream supports **resume-from-slot** (no source
consulted documented replay). *Sentinel must assume no replay* — the Geyser adapter's gap handling is
the same slot-range reconciliation used for WebSocket, which is why it can share the interface.

**Design conclusion:** Geyser solves a *latency and event-volume* problem, not a *correctness* problem.
Sentinel's correctness model is identical with or without it, which is why it is deferrable to Phase 13
and why no required test may use it.

Sources: <https://github.com/rpcpool/yellowstone-grpc> ·
<https://github.com/rpcpool/yellowstone-grpc/blob/master/CHANGELOG.md> ·
<https://docs.triton.one/project-yellowstone/dragons-mouth-grpc-subscriptions> ·
<https://www.alchemy.com/overviews/solana-geyser-plugin>

**Decisions affected:** ADR-0006, `geyser-strategy.md`, Phase 13.

---

## 6. Priority fees

- Local, not global: Solana has no single mempool; fee pressure is **per writable account**.
  `getRecentPrioritizationFees` reads samples from the last 150 blocks and, when given accounts,
  reflects transactions locking **all** of them as writable.
- Correct usage passes the **writable state accounts** the transaction will lock (for a liquidation:
  the market, both vaults, the position), **never program IDs**.
- Its weakness is documented: during contention it often reports near-zero samples that do not reflect
  what is needed to land on a hot account.
- Legacy/v0 formula: `priority_fee = ceil(compute_unit_price_micro_lamports × compute_unit_limit / 1e6)`.
  **v1 replaces this with an absolute lamport total** (§2).

**Contradiction found, recorded rather than resolved — research gate SR-4:** sources at the research
date disagree on fee distribution, one stating "100% to the validator" and another "50% burned, 50% to
the block-producing validator." Sentinel does not depend on the answer (it affects economics
reporting, not execution), but no document or dashboard may state a split until this is verified from a
primary source.

**Design conclusion:** the keeper's fee strategy is (a) simulate to get a real CU figure, (b) set the
CU limit to measured usage plus a stated margin, (c) derive the fee from
`getRecentPrioritizationFees` over the exact writable set, clamped to a configured floor and ceiling,
(d) escalate on retry within a per-intent cumulative fee budget. Every one of these is a measured input
to the Phase 14 campaign, not a tuned constant.

Sources: <https://solana.com/docs/rpc/http/getrecentprioritizationfees> ·
<https://docs.chainstack.com/docs/solana-estimate-priority-fees-getrecentprioritizationfees> ·
<https://www.helius.dev/blog/priority-fees-understanding-solanas-transaction-fee-mechanics>

**Decisions affected:** `transaction-engine.md` §7, `keeper-design.md` §6.

---

## 7. Rust client ecosystem

- The client surface is split across `solana-rpc-client` (HTTP, with `nonblocking` async variants) and
  `solana-pubsub-client` (WebSocket, with a `nonblocking` async client), re-exported by
  `solana-client`. Both blocking and async APIs exist; Sentinel uses the **async** ones throughout,
  on Tokio.
- Aegis's research records two independent version lines that are easy to confuse: the **Agave
  validator** line (4.1 / 4.2 / 4.3-alpha) and the **Solana SDK/CLI crate** line (3.x). Agave 4.2
  guidance separately names a `solana-client` **4.2+** for v1-transaction support.

**`UNVERIFIED` / research gate SR-5:** the exact crate names and versions Sentinel should pin for
`solana-rpc-client` / `solana-pubsub-client` / `solana-transaction-status` (or their successors) that
support v1 transaction decoding, and their MSRV. Phase 1 must record resolved versions from
`Cargo.lock`.

**Design conclusion:** Sentinel's `sentinel-rpc` crate wraps these behind its own trait rather than
exposing them, so a crate reorganization is a one-file change and provider-specific behavior stays
contained (`rpc-strategy.md` §2).

Sources: <https://docs.rs/solana-rpc-client> · <https://docs.rs/solana-pubsub-client> ·
Aegis `docs/ecosystem-research.md` §3.

---

## 8. TypeScript client ecosystem

- **`@solana/kit`** (v8 line; Aegis verified **8.2.0**, published ~2026-08-31) is the current SDK;
  `@solana/web3.js` is legacy. Kit is modular and tree-shakeable.
- Kit's subscription API is signal-driven: `rpc.accountNotifications(address).subscribe({ abortSignal })`,
  with a newer `reactiveStore({ abortSignal })` on `PendingRpcSubscriptionsRequest` that returns
  synchronously and exposes `retry()` for reconnect. Recreating a subscription after a disconnect
  requires a **fresh AbortSignal**, with backoff.
- Anchor 1.x's TS package is **`@anchor-lang/core`** (not `@coral-xyz/anchor`); legacy IDL instructions
  were removed in favor of the **Program Metadata** program, which can store IDLs on-chain.
- **Codama** converts an Anchor IDL into a Codama IDL tree and generates TypeScript and Rust clients,
  including account deserialization.

**Design conclusion:** Sentinel's TypeScript side consumes **Aegis's own `@aegis/sdk`** for
transaction construction rather than generating a second client, so instruction layouts cannot drift
between the protocol's SDK and its keeper (`architecture.md` §3, `aegis-integration.md` §6). Codama is
the fallback path if `@aegis/sdk` is not yet available, and that fallback is explicitly gated.

**`UNVERIFIED` / research gate SR-6:** whether `@solana/kit` 8.x exposes a stable subscription-resume
primitive, and the exact Kit API surface for building/serializing v0 transactions at the pinned version.

Sources: <https://github.com/anza-xyz/kit> · <https://www.solanakit.com/docs/guides/rpc-subscriptions> ·
<https://solana.com/docs/programs/codama/clients> · <https://solana.com/developers/guides/advanced/idls> ·
Aegis `docs/ecosystem-research.md` §1, §7.

---

## 9. Local development and testing environment

Inherited from Aegis, with Sentinel-specific consequences:

| Tool | Role for Sentinel | Note |
|---|---|---|
| **Surfpool** | The local cluster for every ingestion, execution, and keeper test. It exposes a **full JSON-RPC surface** (which LiteSVM does not) plus `surfnet_*` cheatcodes to set accounts, mint tokens, time-travel, and pause the clock. | This is the enabling tool for Sentinel's zero-cost path: deterministic account injection is how Aegis produces Pyth prices without a network, and Sentinel reuses it. |
| **LiteSVM** | Not usable as an ingestion target — it is an in-process SVM, not an RPC server. Used only if Sentinel needs to *produce* fixtures. | Stated so nobody tries. |
| **`agave-validator`** | Needed only for the optional Geyser path (Phase 13), because the plugin loads into a validator. | Not on the required path. |
| **Docker Compose** | Postgres (+ optional Redis) for local and CI. | ADR-0014. |

**Version conflict found — research gate SR-7:** Aegis's research (2026-09-04) records Surfpool
**1.5.0 (July 2026)**; a source consulted for Sentinel records **1.1.2 (April 2026)** as "current
verified". These cannot both be right. Phase 1 must run `surfpool --version` and record the truth,
and must confirm that the version in use exposes the cheatcodes and the JSON-RPC methods Sentinel's
ingestion depends on (`getBlock` with `maxSupportedTransactionVersion`, `getSignaturesForAddress`,
`slotSubscribe`, `accountSubscribe`, `logsSubscribe`, `getRecentPrioritizationFees`,
`simulateTransaction`).

**This is the single most important Phase 1 gate**: if the local cluster does not expose a method
Sentinel requires, an architectural assumption is wrong and must be surfaced, not worked around.

Sources: <https://solana.com/docs/tools/surfpool> · <https://docs.surfpool.run/> ·
<https://github.com/solana-foundation/surfpool> · Aegis `docs/ecosystem-research.md` §5.

---

## 10. MEV / Jito — awareness, not dependency

At the research date, the Jito-Solana client runs under **>95% of active stake**, Jito tips account for
**>60% of priority-fee volume**, and bundles provide atomic, leader-direct submission with a tip
account requirement. Liquidation is named as a primary bundle use case.

**Sentinel's position:** Jito is **NOT COVERED as production** in v1.

- It requires a live cluster and a relay; it cannot exist on the zero-cost path.
- Aegis liquidation is permissionless and profitable by construction (`INV-LIQ-08`), so bundle
  inclusion is a *competitiveness* optimization, not a correctness requirement.
- Adding it now would be exactly the CV-driven architecture `AGENTS.md` §14 forbids.

What Sentinel **does** do: the keeper's competition model (`keeper-design.md` §4) explicitly accounts
for losing races to bundle-submitting competitors, and the coverage matrix records Jito as
**DOC/awareness** with the specific adoption trigger — a measured loss rate to competitors above a
stated threshold in a live-cluster campaign.

Sources: <https://chainstack.com/jito-explained-bundles-tips-mev-solana/> ·
<https://rpcfast.com/blog/jito-explained-bundles-tips-mev-solana>

---

## 11. Known-unstable / deprecated — do not use

| Item | Status |
|---|---|
| `@solana/web3.js` (v1 style) | Legacy. Use `@solana/kit`. |
| `@coral-xyz/anchor` | Renamed. Use `@anchor-lang/core`. |
| `blockSubscribe` | Documented unstable; drops under load; requires extra validator flags. **Not in any required path.** |
| `getConfirmedBlock`, `getConfirmedBlocks`, `getConfirmedSignaturesForAddress`, `getSignatureConfirmation`, `confirmTransaction`, `getTotalSupply` | Removed in the Agave 2.0 line. Do not use. |
| `getBlock` / `getTransaction` without `maxSupportedTransactionVersion` | **Fails** on v1 transactions. Always pass it. |
| Priority fee derived by scanning `ComputeBudget` instructions | Silently returns 0 for v1. Read `transactionConfig` first. |
| Slot-count-derived durations (staleness, timeouts, "N slots ≈ M seconds") | Unsafe under SIMD-0525 and Alpenglow. Use timestamps. |
| Assuming an account update per writable account | False since Agave 4.2. |
| Closed-enum parsing of RPC enum fields | Silently drops new variants (e.g. `DeactivatedStake`). |
| `solana-test-validator` | Superseded by Surfpool for our workflows. |
| Address lookup tables as a Sentinel requirement | Aegis instructions fit a legacy transaction (`INV-RES-06`); v1 removes ALTs entirely. |

---

## 12. Phase 1 re-verification commands (mandatory)

Phase 1 must run these, paste real output into `docs/project-status.md`, and update this file if
anything has moved:

```bash
solana --version
surfpool --version
rustc --version && cargo --version
node --version && docker --version && docker compose version
psql --version

npm view @solana/kit version
npm view @anchor-lang/core version
cargo search solana-rpc-client --limit 1
cargo search solana-pubsub-client --limit 1
cargo search solana-transaction-status --limit 1
cargo search yellowstone-grpc-client --limit 1

# against the local cluster brought up by `make up`:
curl -s -X POST -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getVersion"}' http://127.0.0.1:8899
```

Anything that contradicts this document is a **research finding**, not an inconvenience: update this
file, note the delta in `docs/project-status.md`, and open an ADR if a *decision* changes.

---

## 13. Open verification items carried into later phases

| ID | Question | Gate phase | Status |
|---|---|---|---|
| SR-1 | Transaction v1 mainnet activation date; whether the pinned Rust and Kit versions encode/decode v1 | 4 (decode), 10 (build) | OPEN |
| SR-2 | Whether `confirmed` retains its meaning under Alpenglow/Votor, and any new finality surface in RPC | 6 | OPEN |
| SR-3 | Current Yellowstone plugin/client/proto versions; stock-Agave compatibility; whether resume-from-slot exists | 13 | OPEN |
| SR-4 | Priority-fee distribution (burn split vs 100% to validator) from a primary source | 14 | OPEN |
| SR-5 | Exact Rust client crate names/versions/MSRV supporting v1 decoding | 1 | OPEN |
| SR-6 | `@solana/kit` 8.x subscription-resume primitive and v0 build/serialize surface | 3, 9 | OPEN |
| SR-7 | True Surfpool version and its exposed RPC method set vs Sentinel's requirements | **1 (blocking)** | OPEN |
| SR-8 | Aegis program ID, deployed IDL, and Anchor account discriminators — **do not exist yet**; Aegis is at Phase 0 | 7 | OPEN (blocked upstream) |
| SR-9 | Whether Aegis emits `emit!` (program-log) events or `emit_cpi!`, which changes the log-decoding path | 7 | OPEN (blocked upstream) |
| SR-10 | Pyth receiver program ID and `PriceUpdateV2` account layout post-2026-08-26 upgrade (Aegis RV-3/RV-4) | 7 | OPEN (inherited from Aegis) |
| SR-11 | Postgres version to pin, and whether `LISTEN/NOTIFY` throughput suffices for the job-queue wake path at target load | 2, 14 | OPEN |
