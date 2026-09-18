# Sentinel — Aegis Integration Contract

**Status: FROZEN (Phase 0). Implementation in Phases 7–8 and 11.**
**Research gates SR-8, SR-9, SR-10 must be closed before Phase 7.**

> **Aegis is upstream and authoritative.** Sentinel observes and executes. Where Sentinel's derived
> state and Aegis's on-chain state disagree, Aegis is right and Sentinel has a bug until proven
> otherwise.

---

## 1. Source of truth

Sentinel's Aegis adapter is derived **exclusively** from the frozen Aegis Phase 0 documents, read from
the Aegis repository at a **pinned revision**. Nothing here is reconstructed from memory.

| Aegis document | What Sentinel takes from it |
|---|---|
| `docs/account-model.md` | Account inventory, PDA seeds, field layouts and sizes, signer policy, custody paths, parallelism claims |
| `docs/instruction-catalogue.md` | All 20 instructions, their accounts, writability, preconditions, events, and attack surfaces |
| `docs/economic-model.md` | Share conversions, interest accrual, valuation, health factor, liquidation math, bad-debt settlement, all rounding directions |
| `docs/oracle-design.md` | `PriceBand`, checks O-1..O-11, the fail-closed policy table |
| `docs/token-compatibility.md` | Which mints can exist in a market, measured-delta accounting, per-role policy |
| `docs/invariants.md` | The 87 invariants; the subset Sentinel can check off-chain |
| `docs/threat-model.md` | Trust boundaries Sentinel must not widen |
| `docs/governance.md` | Roles, pause semantics, parameter-change policy, migration strategy |
| `docs/architecture.md` | Module/crate structure, the `aegis-math` and `@aegis/sdk` artifacts Sentinel consumes |
| `docs/phase-roadmap.md`, `docs/project-status.md` | Which Aegis artifacts actually exist yet |

**Pinning rule.** `infra/aegis-pin.toml` records the Aegis git revision, the document hashes Sentinel's
contract was derived from, and the Aegis phase level assumed. A Sentinel phase that touches protocol
decoding must verify the pin and **stop and report** if any pinned document hash has changed. A changed
Aegis document is a design event, never a silent absorption.

---

## 2. Upstream status — what exists today

> **Reconciled in Sentinel Phase 1 (2026-09-18).** The paragraph below described reality at the
> research date (2026-09-04), when Aegis was at Phase 0. It is now stale: **Aegis has completed its
> full planned roadmap through Phase 13 and published `v0.1.0`.** Verified directly against the Aegis
> repository (`codevoks/aegis-protocol`), not assumed from this note: `git tag` lists
> `phase-01-foundation` through `phase-13-release` and `v0.1.0`; `docs/project-status.md` there states
> "Current phase: Phase 13 ... COMPLETE. This is the final planned phase."; `programs/aegis/src/lib.rs`
> declares program ID `DbRhjkZV1QSxMj5AvrYdgVsyEz8nKhoCLnSLGSKsqaF9`; `crates/aegis-math/Cargo.toml` is
> at `0.1.0`; `sdk/ts/package.json` publishes `@aegis/sdk` `0.1.0`.
>
> **This reconciliation is a status correction only.** Per Phase 1's explicit scope, Sentinel does not
> redesign around Aegis, does not begin the Aegis adapter, and does not touch Phase 7/8/11 early. The
> table below is corrected so the next session does not re-derive stale blocked-upstream assumptions;
> the actual pin, IDL extraction, and discriminator verification (SR-8/SR-9) are Phase 7 work and are
> **not performed now**.
>
> Original (2026-09-04) statement, preserved for record: "As of the research date, Aegis is at Phase 0:
> planning complete, zero code written." (`aegis/docs/project-status.md`, 2026-09-04.)

Consequences, corrected:

| Artifact Sentinel needs | Exists? (as of 2026-09-18) | Aegis phase that produced it | Sentinel gate |
|---|---|---|---|
| Program ID | **Yes** — `DbRhjkZV1QSxMj5AvrYdgVsyEz8nKhoCLnSLGSKsqaF9` | 2 (deploy) | SR-8 — still always configuration, never a hardcoded constant; formal pin happens in Phase 7 |
| Anchor IDL / Program Metadata entry | **Yes**, per Aegis Phase 9 (SDK/UI) | 1–2, 9 | SR-8 — extraction is Phase 7 work |
| Account discriminators | **Yes** (derivable from the deployed IDL) | 2 | SR-8 — verification is Phase 7 work |
| Event names + layouts | **Yes**, byte layouts fixed since Aegis Phase 2–6 | 2–6 | SR-9 — verification is Phase 7 work |
| Whether events use `emit!` (program logs) or `emit_cpi!` | Resolvable by reading the Aegis source at Phase 7 time | 2 | SR-9 — not resolved now; deferred to Phase 7 |
| `aegis-math` crate | **Yes** — `0.1.0`, `crates/aegis-math` | 1 (skeleton), 4–6 (complete) | Phase 7/8 dependency — now available |
| `@aegis/sdk` with `ix.ts` builders | **Yes** — `@aegis/sdk` `0.1.0`, `sdk/ts` | 9 | Phase 10/11 dependency — now available |
| Shared JSON test vectors (`tests/vectors/*.json`) | To be confirmed when Phase 7 reads the pinned revision | 4+ | Conformance source |
| A deployed market with real state | Feasible locally via Surfpool deployment of the released program | 2+ | Demo dependency — no longer blocked |

**Sentinel is no longer blocked upstream at Phase 7 or Phase 11** — the artifacts both phases need now
exist in `codevoks/aegis-protocol` `v0.1.0`. This does **not** change phase order or scope: `AGENTS.md`
§5 still requires exactly one phase per session, and Phase 7/8/11 remain untouched until their turn.
`phase-roadmap.md` §3 carries the same correction. **Phases 1–6 of Sentinel still have no upstream
dependency at all** — this was always true regardless of Aegis's schedule.

---

## 3. Account and PDA contract

Derived from `aegis/account-model.md` §3–7. Sentinel derives every address offline; it never scans for
accounts by heuristic and never maintains a registry Aegis does not have.

| Account | Seeds | Bump | Size | Sentinel treatment |
|---|---|---|---|---|
| `Protocol` | `[b"protocol"]` | stored, canonical | 202 | Singleton. Snapshot on change; admin/guardian/fee-recipient/pause tracked. |
| `Market` | `[b"market", collateral_mint, loan_mint, config_id_le_u16]` | stored | ~641 | **Content-addressed — there is no registry.** See §4. |
| `Position` | `[b"position", market, owner]` | stored | 145 | One per (market, owner). Discovered from events and from `getProgramAccounts`. |
| `collateral_vault` | `[b"cvault", market]` | stored on market | token account | Balance tracked for the `INV-CUS-02` check. |
| `loan_vault` | `[b"lvault", market]` | stored on market | token account | Balance tracked for the `INV-CUS-01` check. |

**Rules Sentinel inherits:**
- Only canonical bumps are valid. A non-canonical derivation is a red flag, not an alternative address.
- Every account type has a distinct seed prefix, so type confusion is impossible — Sentinel asserts the
  prefix rather than inferring the type from size.
- `Market` and `Protocol` are never closed. `Position` is closable and re-creatable **empty**; Sentinel
  must model position closure as a lifecycle transition, not as deletion, and must handle
  `close → init` on the same address without treating it as data corruption.

### 3.1 Market discovery — the one genuinely hard derivation problem

Because markets are content-addressed with no counter and no registry (`account-model.md` §2, rejected
`MarketRegistry`), **there is no on-chain enumeration of markets.** Sentinel discovers them by:

1. **Primary:** decoding `MarketCreated` events from Aegis's transaction history. This is complete for
   the whole life of the program provided Sentinel has ingested from the program's first slot.
2. **Secondary / bootstrap:** `getProgramAccounts` filtered by the `Market` account discriminator, used
   for cold start and as a periodic completeness check.
3. **Reconciliation:** the two sets must agree. A market present in (2) but not (1) means Sentinel's
   history has a gap; a market in (1) but not (2) means it was never created or the discriminator is
   wrong. **Either divergence is an alert, never a merge.**

`getProgramAccounts` is expensive and rate-limited on public endpoints. It runs on a slow schedule with
a configured interval, never on the hot path, and never as the sole source (`rpc-strategy.md` §7).

---

## 4. Field contract — what Sentinel decodes and what it means

Sentinel stores every field of every Aegis account. The fields below are the ones with *semantics*
Sentinel depends on, and each carries a rule.

### 4.1 `Market` — identity (immutable after creation)

`collateral_mint`, `loan_mint`, `collateral_token_program`, `loan_token_program`, `collateral_vault`,
`loan_vault`, `fee_recipient`, `config_id`, `collateral_decimals`, `loan_decimals`.

- **Rule:** these are immutable (`INV-ADM-06`). If Sentinel observes a change, that is a **FATAL**
  reconciliation failure — either the decoder is wrong or the pin is stale. Never absorb it.
- `*_decimals` are cached from the mints at creation and cannot go stale (mint decimals are immutable
  in both token programs). Sentinel uses the market's cached values for valuation, **not** the mint
  account, so it matches on-chain behavior exactly.

### 4.2 `Market` — oracle config (admin-mutable)

`oracle_kind` (0 = Pyth pull), `collateral_feed_id [u8;32]`, `loan_feed_id [u8;32]`,
`max_price_age_secs: u32`, `max_conf_bps: u16`.

- **Rule:** the oracle identity is the **feed ID, not an account address** (`oracle-design.md` §2,
  O-3). Pyth pull updates are ephemeral, permissionlessly-posted accounts. Sentinel indexes price
  observations by feed ID and must never pin or cache a price account address as an identity.
- A feed-ID change is classified risk-*increasing* by Aegis governance and is timelocked from Aegis
  Phase 12. Sentinel treats a feed-ID change as a **market-level alert** and invalidates every cached
  price band for that market.

### 4.3 `Market` — risk parameters (admin-mutable, bounds-checked)

`max_ltv`, `liq_threshold`, `liq_bonus`, `close_factor`, `full_liq_hf`, `liq_protocol_fee`, `fee`
(all `u128` WAD), `min_debt: u64`.

- **Rule:** Sentinel snapshots the full parameter set on every `MarketParamsUpdated` event
  (which carries a before/after snapshot) **and** on every account observation, and stores them
  versioned with `effective_from_slot`. Historical health must be recomputed with the parameters that
  were in force at that slot — not today's. This is a real replay requirement, not a nicety.
- Sentinel independently re-checks Aegis's own bound `liq_threshold·(WAD + liq_bonus)/WAD < WAD` on
  every observed parameter set. A violation is impossible on-chain (`INV-LIQ-06`), so observing one
  means Sentinel decoded wrong. **FATAL.**

### 4.4 `Market` — IRM parameters (stateless)

`base_rate_ps`, `slope1_ps`, `slope2_ps`, `u_kink`, `max_rate_ps` (per-second WAD).

- **Rule:** the IRM is stateless (Aegis ADR-0007), so Sentinel can compute the rate at any utilization
  as a pure function. There is no IRM state to track, and no chance of drift.

### 4.5 `Market` — hot accounting

`total_supply_assets: u64`, `total_supply_shares: u128`, `total_borrow_assets: u64`,
`total_borrow_shares: u128`, `collateral_fee_accrued: u64`, `last_accrual_ts: i64`.

- **Rule — the single most important semantic in this document:** `total_borrow_assets` and
  `total_supply_assets` are **lazily accrued**. They are correct as of `last_accrual_ts`, not as of
  now. Any health computation that uses them without first applying `accrue_view` to the intended
  evaluation timestamp **understates debt and overstates health**, and will produce liquidation
  candidates that the chain rejects — or, worse, miss ones it would accept. See §5.

### 4.6 `Market` — flags

`paused: u8` (bits `SUPPLY|BORROW|WITHDRAW|LIQUIDATE`), `flags: u8`
(bit0 `ack_freeze_authority`, bit1 `collateral_has_transfer_fee`).

- **Rule:** the keeper must check the `LIQUIDATE` bit on **both** `Protocol.paused` and `Market.paused`
  before creating a candidate. Aegis pauses are a legitimate operator action (`governance.md` §3), so a
  paused market produces zero candidates and an informational status — not an alert storm.
- `repay`, `deposit_collateral`, `absorb_bad_debt`, `close_position` are **structurally unpausable**
  (`INV-ADM-04`). Sentinel's `absorb_bad_debt` automation therefore never checks pause state.

### 4.7 `Position`

`market`, `owner`, `supply_shares: u128`, `borrow_shares: u128`, `collateral_amount: u64`, `bump`.

- **Rule:** one account carries all three roles (lender / borrower / collateral holder). Sentinel's
  position model must not split them into separate entities, or reconciliation against the account
  snapshot becomes impossible.
- `supply_shares` and `borrow_shares` are **shares, not assets**. Every display and every risk
  computation converts through the market totals at a stated timestamp.

### 4.8 `_reserved` bytes

`Protocol._reserved[64]`, `Market._reserved[64]`, `Position._reserved[32]` are checked to be all-zero
on-chain (`INV-ACCT-09`).

- **Rule:** Sentinel asserts them zero on decode. A non-zero reserved region means either a schema
  migration happened (Aegis Phase 12, `Migration<From,To>`) or the decoder is misaligned. Both require
  the version-resolution path in §8, and neither may be silently ignored.

---

## 5. Health computation — the exact contract

This is where Sentinel is most likely to be subtly wrong, so the sequence is specified completely.
All formulas are `aegis/economic-model.md` §3–7 and are **called**, not reimplemented (§6).

To evaluate position `P` in market `M` at evaluation time `t_eval`:

```
1.  Load M's parameters as of the slot being evaluated (versioned; §4.3).
2.  totals = accrue_view(M, t_eval)                     // economic-model §4.2, §4.5
        u        = min(WAD, total_borrow_assets·WAD / total_supply_assets)   (0 if supply == 0)
        r        = piecewise_linear(u, params), capped at max_rate_ps
        x        = r · (t_eval − M.last_accrual_ts)      // dt clamped at 0
        growth   = taylor3(x)
        interest = floor(total_borrow_assets · growth / WAD)
        total_borrow_assets += interest ; total_supply_assets += interest
3.  debt_assets = to_assets_up(P.borrow_shares, totals.total_borrow_assets, M.total_borrow_shares)
4.  price bands, per asset, from a Pyth PriceUpdateV2 observation valid at t_eval:
        every check O-1..O-11 applied exactly (oracle-design §2)
        lo = scale_to_wad_floor(price − conf, expo)
        hi = scale_to_wad_ceil (price + conf, expo)
        reject outside [MIN_PRICE_WAD 1e6, MAX_PRICE_WAD 1e30]
5.  collateral_value = floor(P.collateral_amount · price_c_lo / 10^collateral_decimals)
    debt_value       = ceil (debt_assets       · price_l_hi / 10^loan_decimals)
6.  HF = (debt_value == 0) ? u128::MAX
                           : floor(collateral_value · M.liq_threshold / debt_value)
7.  liquidatable  ⟺  HF < WAD          (STRICT — HF == WAD is NOT liquidatable, E-12)
```

### 5.1 Rules that fall out of this, all mandatory

| # | Rule | Why |
|---|---|---|
| H-1 | **`t_eval` is the intended execution time, not the last observation time.** For a candidate it is `now + expected_landing_latency`; for a historical query it is the timestamp of the slot being reported. | Debt accrues continuously. Evaluating at `last_accrual_ts` systematically understates debt. |
| H-2 | **Never use a price outside its validity window.** If no `PriceUpdateV2` observation for the feed satisfies O-5 at `t_eval`, the position's health is `UNKNOWN`, not "last known". | Aegis fails closed; Sentinel must not present an answer Aegis would refuse to act on. |
| H-3 | **`UNKNOWN` health is a first-class state**, distinct from healthy and from liquidatable, and it is surfaced in the API. | Silently defaulting to "healthy" during an oracle outage is exactly how a monitoring system lies. |
| H-4 | Health carries the commitment of the state it was computed from. A `confirmed`-derived HF is labelled as such and may be revised. | `finality-and-forks.md` §6. |
| H-5 | The conservative bounds are **not symmetric**: collateral at `lo` floored, debt at `hi` ceiled. Getting either direction wrong makes Sentinel optimistic — the dangerous direction. | `economic-model.md` §6.2. |
| H-6 | Rounding is integer and specified per operation. No floating point anywhere in the risk path, in either language. | Inherited NFR-1; CI-enforced grep. |

### 5.2 Derived read models (off-chain only, never on-chain)

- **Liquidation price**: `debt_value · 10^collateral_decimals / (collateral_amount · liq_threshold)`
  (`economic-model.md` §6.4 — explicitly an SDK/off-chain model).
- **Borrow capacity**: `floor(collateral_value · max_ltv / WAD) − debt_value`.
- **Supply/borrow APY**: derived from `r` and `u` with the fee applied; presented as an instantaneous
  rate with its `computed_at` timestamp, never as a realized return.

---

## 6. Conformance — how Sentinel proves it did not drift

Three mechanisms, in priority order.

### 6.1 Preferred: consume Aegis's own artifacts

- Rust risk engine depends on **`aegis-math`** as a path/git dependency at the pinned revision. It is
  `no_std`, float-free, and free of `solana-*`/`anchor-*` dependencies by Aegis's own dependency policy
  — which makes it directly linkable from a non-Solana service. This is not a coincidence; it is the
  property that makes this integration honest.
- TypeScript executor depends on **`@aegis/sdk`** for `pda.ts` and `ix.ts`.

### 6.2 Interim (while Aegis < Phase 4 / < Phase 9): implement against the frozen spec, prove against frozen numbers

`sentinel-aegis` implements the §5 sequence directly from `economic-model.md`, and a **blocking**
conformance test asserts the exact worked examples Aegis has already frozen:

| Vector | Source | Expected |
|---|---|---|
| `AEGIS-CONF-01` valuation | `economic-model.md` §6.5 | 10 SOL @ $150.00±0.30, 900 USDC debt @ $1.0000±0.0002, LT 0.80 → `collateral_value = 1497.0e18`, `debt_value = 900.18e18`, **HF ≈ 1.330838** |
| `AEGIS-CONF-02` liquidatable | `economic-model.md` §6.5 | SOL @ $95.00±0.20 → `collateral_value = 948.00e18`, **HF ≈ 0.842495**, liquidatable, full liquidation permitted |
| `AEGIS-CONF-03` liquidation | `economic-model.md` §7.5 | repay 900e6 → `base_seize = 9_495_569_620`, `total_seize = 9_970_348_101`, `bonus = 474_778_481`, `protocol_cut = 47_477_848`, `to_liquidator = 9_922_870_253` |
| `AEGIS-CONF-04` accrual | `economic-model.md` §4.4 | supply 1000e6 / borrow 900e6, fee 0.10, dt 86400 → `u = 0.9 WAD`, `r = 17_123_287_670`, `interest = 1_332_492`, `fee_amount = 133_249` |
| `AEGIS-CONF-05` share round-trip | `economic-model.md` §3.3 | Alice 1e9 into empty market → `1e15` shares; Bob's immediate 1e9 → marginally fewer |
| `AEGIS-CONF-06` boundary | `economic-model.md` E-12 | `HF == WAD` is **not** liquidatable; `HF == WAD − 1` is |

**These numbers exist today and are frozen.** They are the acceptance criteria for Sentinel Phase 8
and they do not depend on Aegis writing a single line of code.

### 6.3 Continuous: cross-check against the chain

Once Aegis is deployed (any cluster, including local), Sentinel runs a **shadow check**: for a sampled
set of positions, it compares its own HF against what the on-chain program would decide, inferred from
observed `liquidate` outcomes and from a read-only `simulateTransaction` of a would-be liquidation.
A disagreement writes a `reconciliation_mismatch` row classified per §9.

### 6.4 What Sentinel must never do

- Reimplement `mul_div_floor` / `mul_div_ceil` "more efficiently".
- Use `f64` anywhere in the risk path, including for a UI approximation computed server-side.
- Use `u128` multiplication without a 256-bit intermediate. Aegis found a concrete legal state
  (`shares × total_assets ≈ 3.2e44`) that overflows `u128`; the same states reach Sentinel.
- Cache a health factor without its `t_eval` and its price observation IDs.

---

## 7. Event vs account state — the dual reconstruction path

Aegis guarantees **FR-19**: every state transition emits a typed event sufficient to reconstruct
protocol state off-chain. Sentinel uses this, but does **not** trust it alone.

| Path | Source | Strength | Weakness |
|---|---|---|---|
| **A — Event projection** | Decoded Aegis events from program logs, applied in slot/instruction order | Complete history; supports point-in-time queries; cheap | Depends on Sentinel having every event; a single missed log silently diverges |
| **B — Account snapshot** | Decoded `Market`/`Position`/vault account observations | Ground truth at an instant | **Incomplete by construction** — Agave 4.2 only emits an update when the account changes, and Sentinel cannot subscribe to every position |

**Design:** path A is the primary materialization; path B is a **reconciliation input** on a schedule
and on demand. Both write to the same materialized rows, and every row records
`materialized_via = event | snapshot` plus `last_snapshot_slot`.

**Divergence policy:**
1. Snapshot wins the value. The event projection is corrected to the snapshot.
2. The divergence is recorded (`reconciliation_mismatch`, class `PROJECTION_DIVERGENCE`) with both
   values, the slot, and the event range that should have produced the snapshot's value.
3. An alert fires above a configured rate, because a persistent divergence means events are being lost
   — which is a gap-detection failure, not a decoding failure.

### 7.1 The off-chain checkable invariants

These Aegis invariants are exact equalities over quantities Sentinel observes, so Sentinel can check
them off-chain and page an operator. This directly serves Aegis runbook **R-2**.

| Aegis invariant | Sentinel check |
|---|---|
| `INV-CUS-01` | `loan_vault.amount == market.total_supply_assets − market.total_borrow_assets` |
| `INV-CUS-02` | `collateral_vault.amount == Σ(position.collateral_amount) + market.collateral_fee_accrued` |
| `INV-ACC-01` | `market.total_supply_shares == Σ(position.supply_shares)` including `fee_position` |
| `INV-ACC-02` | `market.total_borrow_shares == Σ(position.borrow_shares)` |
| `INV-ACC-03` | `total_supply_assets ≥ total_borrow_assets` |
| `INV-ACC-07` | `last_accrual_ts` monotonically non-decreasing, never above the block timestamp |
| `INV-SOLV-07` | every position's debt is `0` or `≥ min_debt` |
| `INV-LIQ-06` | `liq_threshold·(WAD + liq_bonus)/WAD < WAD` on every observed parameter set |

**Critical caveat, stated so nobody misreads an alert:** a violation observed by Sentinel is *far* more
likely to be a Sentinel bug (missed event, wrong decoder version, stale snapshot, non-finalized slot)
than an Aegis bug. The alert therefore fires with a mandatory triage order: (1) is the state finalized?
(2) is the snapshot newer than the projection? (3) is there a known gap in the slot range?
(4) only then, escalate to Aegis. `observability.md` §5 carries the runbook.

**These checks run only on `finalized` state.** Running them on `confirmed` state produces false
positives during normal operation.

### 7.2 The completeness precondition — why these checks are gated

`INV-CUS-02`, `INV-ACC-01` and `INV-ACC-02` are **sums over every position in a market**. If Sentinel
has never observed one position — a missed `PositionInitialized`, a gap in the program's history, a
decoder failure — the sum is short and the invariant *appears* violated. That would page an operator
about an Aegis accounting bug that does not exist.

**Rule: a summation invariant is evaluated only when the market's position set is verified complete at
that slot.** Completeness is established by a recent `getProgramAccounts` scan filtered to the
`Position` discriminator, cross-checked against the event-derived set. The check records
`position_set_verified_at_slot`, and:

- If the two sets **agree** → the summation invariants are evaluated.
- If they **disagree**, or the last verification is older than a configured window → the invariants are
  **skipped**, and the *set divergence itself* is what alerts, classified as a Sentinel history gap.

Point-wise invariants (`INV-CUS-01`, `INV-ACC-03`, `INV-ACC-07`, `INV-SOLV-07`, `INV-LIQ-06`) have no
such precondition — they read only market and vault state — and run continuously on finalized data.

---

## 8. Version-aware decoding

Aegis will change. `Migration<'info, From, To>` is its account-migration primitive (Aegis
`governance.md` §6), `_reserved` bytes allow additive fields without realloc, and the upgrade authority
can replace the program entirely (`T-30`).

### 8.1 Decoder registry

```
decoder_versions
  id                  serial PK
  protocol            'aegis'
  program_id          text        -- the deployed program this applies to
  account_kind        'protocol' | 'market' | 'position'
  schema_version      int         -- Sentinel's own monotonic version
  discriminator       bytea       -- 8-byte Anchor discriminator
  layout_hash         text        -- hash of the field layout this decoder implements
  effective_from_slot bigint      -- inclusive
  effective_to_slot   bigint NULL -- exclusive; NULL = current
  source              'idl' | 'spec'   -- generated from a published IDL, or hand-written from the frozen spec
  UNIQUE (protocol, program_id, account_kind, schema_version)
```

Every decoded protocol row records the `decoder_version_id` that produced it. This is what makes a
decoder upgrade a **replay**, not a migration.

### 8.2 Version detection

Detection is layered, cheapest first:

1. **Program upgrade observation.** Sentinel watches the BPF loader's `ProgramData` account for the
   Aegis program. A change in the program data hash is a **version boundary event**, recorded with its
   slot. This is the strongest signal and it is available before any decode fails.
2. **Discriminator mismatch.** An account whose discriminator is not registered → `UNKNOWN_SCHEMA`.
3. **Length mismatch.** A `Market` that is not the expected byte length → `UNKNOWN_SCHEMA`.
4. **`_reserved` non-zero.** Additive migration suspected → `SCHEMA_DRIFT`.
5. **Semantic assertion failure.** An immutable field changed, or `INV-LIQ-06` fails → `SCHEMA_DRIFT`.

### 8.3 Policy on detection

| Signal | Action |
|---|---|
| Program upgrade observed | **Pause the keeper immediately** (stop creating new intents; let in-flight ones resolve or expire). Alert. Decoding continues under the existing version until a boundary is confirmed. |
| `UNKNOWN_SCHEMA` | Do **not** guess. Write a `decode_failures` row referencing the raw observation, mark the entity `STALE`, alert. Materialization for that entity halts; it does not silently freeze at an old value without saying so. |
| `SCHEMA_DRIFT` | Same as `UNKNOWN_SCHEMA`, plus classify as additive-vs-breaking in triage. |
| New decoder registered | Backfill by **replaying** the affected slot range through the new decoder. Old rows are retained under their old `decoder_version_id`. |

**Rule: Sentinel never mutates historical decoded rows in place on a decoder upgrade.** It re-derives
them. This is the property that keeps old records interpretable: a row always states which decoder
produced it, and the raw bytes it came from are still there.

**Rule: a decoder is never inferred.** It is either generated from a published IDL (`source = 'idl'`)
or hand-written against a pinned frozen spec (`source = 'spec'`). Heuristic layout-sniffing is banned.

---

## 9. Oracle integration

Aegis reads Pyth `PriceUpdateV2` accounts directly — **an account read, not a CPI** (Aegis ADR-0008).
That has two consequences for Sentinel, one easy and one hard.

**Easy:** Sentinel can observe the same accounts by the same mechanism, and locally it can inject the
same byte-exact fixtures Aegis's test kit builds (`aegis/oracle-design.md` §5). The zero-cost path is
preserved end to end.

**Hard:** price update accounts are **ephemeral and permissionlessly posted**. There is no stable
address to subscribe to. Sentinel therefore:

1. Indexes `oracle_observations` by `(feed_id, publish_time, slot)`, not by account address, and stores
   the address only as provenance.
2. Discovers them from the transactions that post them (the Pyth receiver program's instructions) and
   from account observations of accounts owned by the receiver program.
3. Applies **every** check O-1..O-11 at read time and records which check failed when one does — so
   "why is this market fail-closed right now?" is an answerable question with a specific cause.
4. Tracks per-feed health: last publish time, current `conf/price` ratio versus the market's
   `max_conf_bps`, and time-in-fail-closed.

### 9.1 The keeper's oracle problem

A liquidation transaction must carry price accounts that satisfy `max_price_age_secs` **at the moment
the transaction executes**, not when it was built. With `max_price_age_secs = 30` for majors, and a
build→land latency of hundreds of milliseconds to seconds, the margin is real but not generous.

The keeper therefore:
- Selects the **freshest valid** observation per feed at build time and records its `publish_time`.
- Computes `price_deadline = publish_time + max_price_age_secs` and refuses to submit if the expected
  landing time exceeds it minus a configured safety margin.
- If no sufficiently fresh update exists, the keeper may **post one itself** — which requires Hermes
  and is therefore **off the zero-cost path**. This is an explicitly optional capability
  (`keeper-design.md` §7), disabled by default, and the local demo uses injected fixtures instead.

---

## 10. Token integration

Sentinel indexes token movements because Aegis's custody invariants are stated in terms of vault
balances.

- **Balance deltas come from `meta.preTokenBalances` / `meta.postTokenBalances`**, not from parsed
  instruction JSON, because the parsed shapes changed in Agave 4.2 (`ecosystem-research.md` §1.4).
- Aegis uses **measured-delta accounting** (`token-compatibility.md` §5.3): the credited amount is
  `vault_after − vault_before`, which may be less than the amount sent for a transfer-fee mint. Sentinel
  must model **both** `amount_in` and `credited` and never assume they are equal — Aegis's own
  `CollateralDeposited` event carries both, which is exactly why.
- Aegis's positive allowlist means a market's mints are already vetted (no transfer hooks, no permanent
  delegate, no close authority, no pausable, no confidential transfers). Sentinel records the accepted
  extension inventory from the `MarketCreated` event as the permanent audit record.
- `flags` bit1 (`collateral_has_transfer_fee`) tells Sentinel when `amount_in != credited` is *expected*
  rather than anomalous.

---

## 11. Instruction and event inventory

From `aegis/instruction-catalogue.md`. Sentinel must decode all 20 instructions and every event.
The columns Sentinel cares about are which ones change risk-relevant state and which are unpausable.

| # | Instruction | Event(s) | Changes market totals | Changes position | Risk-relevant |
|---|---|---|---|---|---|
| 1 | `initialize_protocol` | `ProtocolInitialized` | – | – | config |
| 2–3 | `set_pending_admin` / `accept_admin` | `AdminTransferStarted`, `AdminTransferred` | – | – | governance |
| 4 | `set_guardian` | — | – | – | governance |
| 5 | `set_protocol_pause` | `ProtocolPauseSet` | – | – | **yes** (gates keeper) |
| 6 | `create_market` | `MarketCreated` | init | init fee position | **yes** (discovery) |
| 7 | `set_market_params` | `MarketParamsUpdated` | accrues first | – | **yes** (versioned params) |
| 8 | `set_market_pause` | — | yes | – | **yes** (gates keeper) |
| 9 | `init_position` | `PositionInitialized` | – | create | lifecycle |
| 10 | `deposit_collateral` | `CollateralDeposited` | **no** | collateral += credited | **yes** (health up) |
| 11 | `withdraw_collateral` | `CollateralWithdrawn` | **no** | collateral −= amount | **yes** (health down) |
| 12 | `supply` | `Supplied` | yes | supply_shares += | liquidity |
| 13 | `withdraw` | `Withdrawn` | yes | supply_shares −= | liquidity |
| 14 | `borrow` | `Borrowed` | yes | borrow_shares += | **yes** |
| 15 | `repay` | `Repaid` | yes | borrow_shares −= | **yes** |
| 16 | `accrue_interest` | `InterestAccrued` | yes | – | **yes** (freshness) |
| 17 | `liquidate` | `Liquidated` | yes | both | **yes** (the loop) |
| 18 | `absorb_bad_debt` | `BadDebtAbsorbed` | yes | borrow_shares → 0 | **yes** (loss) |
| 19 | `withdraw_collateral_fees` | `CollateralFeesWithdrawn` | fee accrued −= | – | custody |
| 20 | `close_position` | `PositionClosed` | – | close | lifecycle |

Notes Sentinel must encode as behavior, not comments:

- **`deposit_collateral` and `withdraw_collateral` do not write `Market`** (`INV-ACCT-08`). Sentinel
  must not expect a market account update for these, and must not treat its absence as a missed event.
  This is Aegis's parallelism claim C2 and it changes what Sentinel can infer from account updates.
- **`repay` requires no owner signature and no oracle, and cannot be paused.** A position can become
  healthy at any time, from any signer. The keeper must assume this (`keeper-design.md` §4).
- **`Liquidated` carries `hf_before` and `hf_after`** — which is precisely the ground truth Sentinel
  needs to grade its own health predictions.
- **`InterestAccrued` carries `interest`, `fee_amount`, `fee_shares`, and both totals** — enough to
  verify Sentinel's accrual arithmetic against the chain's on every occurrence.
- **`accrue_interest` with `dt == 0` is a successful no-op** (E-02). Sentinel must not treat a no-op
  accrual as a state change.

---

## 12. Failure and disagreement taxonomy

When Sentinel predicts a liquidation and Aegis rejects it, the cause is classified before anyone is
paged. Aegis's error codes are banded (`aegis/architecture.md` §8), which makes this mechanical:

| Class | Symptom | Typical Aegis band | Sentinel action |
|---|---|---|---|
| **RACE_HEALED** | Position became healthy (repay/deposit landed first) | 6060–6079 solvency | Expected. Count it. No alert. |
| **RACE_LOST** | Another liquidator got there first | 6060–6079 / position already changed | Expected. Feed the competition model. |
| **ORACLE_CLOSED** | Price stale/wide at execution time | 6040–6059 oracle | Expected under stress. Alert only on sustained rate. |
| **PAUSED** | `LIQUIDATE` bit set | 6120–6139 config | Informational. Stop creating candidates for that market. |
| **SIZE_REJECTED** | Repay exceeded close factor, or left dust | 6080–6099 liquidation | **Sentinel bug** — the sizing model is wrong. Alert. |
| **LOOKAHEAD_OVERSHOOT** | Aegis says healthy, **and** Sentinel's HF recomputed at the *observed* state was also `≥ WAD` — but its HF at `t_eval` was `< WAD` | 6060–6079 | **Not a model bug — a tuning signal.** The `expected_landing_latency` lookahead is too long, so Sentinel is predicting a future the chain has not reached. Adjust the lookahead. **Does not pause the keeper.** |
| **MODEL_DIVERGENCE** | Aegis says healthy, and Sentinel's HF recomputed **at the observed state** is still `< WAD` | 6060–6079 | **Sentinel bug — highest severity.** The health engine genuinely disagrees with the protocol. Pause the keeper. |
| **ACCOUNT_REJECTED** | Wrong account, wrong program, wrong mint | 6000–6019 / 6100–6119 | **Sentinel bug.** The transaction builder is wrong. Pause the keeper. |
| **UNKNOWN** | Unmapped error code | any | Pause the keeper, alert, and treat as a decoder/version event. |

**Governing rule (`AGENTS.md` §3):** a rejection is a reconciliation signal. The first four classes are
normal operation of a permissionless market and must not generate noise. `SIZE_REJECTED`,
`MODEL_DIVERGENCE`, `ACCOUNT_REJECTED` and `UNKNOWN` indicate Sentinel is wrong about something and are
the ones that matter.

**Why `LOOKAHEAD_OVERSHOOT` is separated from `MODEL_DIVERGENCE`, and why it matters:** Sentinel
evaluates health at `t_eval = now + expected_landing_latency` (H-1), which is correct — but an
over-estimated lookahead makes Sentinel predict liquidatability slightly *before* the chain agrees.
Without this distinction, every over-long lookahead would present as `MODEL_DIVERGENCE` and **auto-pause
the keeper repeatedly for a tuning problem**. The classifier therefore recomputes health at the observed
state before deciding: if that HF is also `< WAD`, the model really disagrees with the protocol; if it
is `≥ WAD`, the model is right and only the lookahead is long. This recomputation is cheap, it uses data
Sentinel already has, and it is what keeps auto-pause meaningful.

---

## 13. What Sentinel deliberately does not do

| Not doing | Why |
|---|---|
| Recomputing Aegis's rounding "more precisely" | Any deviation makes Sentinel's answer differ from the chain's. Bit-identical or useless. |
| Presenting Sentinel's HF as authoritative in the UI | It is a prediction with a timestamp and a commitment label. The Aegis SDK's on-chain read is authoritative. |
| Calling `absorb_bad_debt` automatically without an operator switch in v1 | It is permissionless and safe, but it socializes a loss. Automating loss recognition without an operator decision is a product choice Sentinel has not earned. Available as an operator-triggered action. |
| Attempting a liquidation Aegis's own bound says would worsen health | Aegis handles the death-spiral band via `full_liq_hf`. Sentinel's sizing respects it rather than re-deriving it. |
| Indexing Aegis's `labs/` programs | They are explicitly non-production (Aegis ADR-0003). |
| Guessing a program ID or discriminator | SR-8. Configuration and IDL, always. |
