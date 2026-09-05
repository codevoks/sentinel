# Phase 7 — Aegis Protocol Adapter

**Status: NOT STARTED.** **Prerequisite: Phase 6 complete and tagged.**
**UPSTREAM-BLOCKED: requires Aegis ≥ Phase 6 (deployed program, IDL, events, liquidation).**
**Blocking research gates: SR-8, SR-9, SR-10.**

> **Read the Aegis documents before writing a line.** `account-model.md`, `instruction-catalogue.md`,
> `token-compatibility.md`, `oracle-design.md`, and the ADRs. Do not reconstruct the design from
> memory; the pinning rule (`aegis-integration.md` §1) exists for exactly this.

## 1. Scope

1. The **decoder registry**: `decoder_versions` with `source ∈ {idl, spec}`, discriminators, layout
   hashes, and slot ranges.
2. Account decoders for `Protocol`, `Market`, `Position`, and the two vault token accounts.
3. **Event decoding** from program logs — all 18 events in the catalogue.
4. **Dual reconstruction**: the event projection (primary) and account snapshots (reconciliation),
   both writing the same materialized rows with `materialized_via` recorded.
5. **Market discovery**: `MarketCreated` events primary, `getProgramAccounts` secondary, with
   divergence between them alerting rather than merging.
6. `aegis_market_params_history` — the **versioned** parameter set.
7. **Oracle observations** indexed by `feed_id`, with all of O-1..O-11 applied and the failing check
   recorded.
8. **Version detection**: `ProgramData` hash watching, unknown discriminator, length mismatch, non-zero
   `_reserved`, semantic assertion failure — each with the documented action.
9. Vault balance tracking for the custody invariants.

## 2. Explicit non-scope

**No health computation, no candidates, no risk** (Phase 8). No execution. No API. **No liquidation
transaction building** — that is Phase 10/11 and needs `@aegis/sdk`.

## 3. Evidence objective

- Sentinel's reconstruction of Aegis state **matches the chain's**, proven by snapshot reconciliation
  and by decoding a full lifecycle.
- **A schema change is detected and refused, never guessed** — which is the property that keeps
  historical data trustworthy.

## 4. Files

`crates/sentinel-aegis/src/{registry,accounts,events,materialize,discovery,oracle,version,vaults}.rs` ·
`infra/aegis-pin.toml` · vendored IDL

## 5. Dependencies

Phases 1–6, **and Aegis ≥ Phase 6**.

**If Aegis is not there yet**, this phase cannot complete. What *can* be done, and should be reported
as partial rather than as done:
- The registry, the version-detection machinery, and the materialization framework — all protocol-shape
  agnostic.
- Decoders written from the frozen spec with `source = 'spec'`, tested against fixtures built by hand
  from the documented layouts.
- **What cannot be done:** closing SR-8 (real program ID, IDL, discriminators) or SR-9 (`emit!` vs
  `emit_cpi!`). Guessing either is forbidden.

## 6. Implementation requirements — do not deviate

- **A decoder is never inferred.** It is generated from a published IDL or written against a pinned
  frozen spec. Heuristic layout-sniffing is banned (ADR-0012).
- **On any version signal, do not guess**: mark `UNKNOWN_SCHEMA`/`STALE`, alert, and — on a program
  upgrade — **pause the keeper** (once it exists).
- **Never mutate historical decoded rows on a decoder upgrade.** Register a version and replay.
- **Materialization is a fold over the event set at a pinned anchor**, never an increment against
  whatever is in the table.
- The materialization upsert carries the `as_of_slot` monotonicity guard.
- **Immutable market fields changing is FATAL**, not a merge. So is an observed parameter set violating
  `liq_threshold·(WAD + liq_bonus)/WAD < WAD`, which is impossible on-chain.
- **Oracle identity is the `feed_id`, never the account address.** Store the address as provenance only.
- **`deposit_collateral` and `withdraw_collateral` do not write `Market`.** Do not expect a market
  account update for them, and do not treat its absence as a missed event.
- `_reserved` is asserted zero; non-zero means migration or misalignment, both of which take the version
  path.
- Model `amount_in` and `credited` **separately** for token movements; never assume they are equal.
- Snapshot wins on divergence; the divergence is **recorded and alerted**, because it means events are
  being lost.

## 7. Tests

**Unit:** every field of every account; boundary values; wrong discriminator; wrong length; non-zero
`_reserved`; every event's fields; log framing with nested invokes; O-1..O-11 individually violated.

**Conformance (T4):** `AEGIS-PDA-01` (shared vector file, asserted in Rust here and in TypeScript in
Phase 10); `AEGIS-LAYOUT-01`; `AEGIS-EVENT-01`; `AEGIS-ORACLE-01..11`; `AEGIS-VER-01/02`.

**Integration:** a full scripted Aegis lifecycle on the local cluster — create market, supply, deposit,
borrow, accrue, repay, liquidate, bad debt — with materialization matching account snapshots at every
step.

## 8. Adversarial / failure cases

| Case | Asserted |
|---|---|
| A look-alike program with identical layouts (`A-IDENT-01`) | Rejected on program-owner validation |
| A non-canonical PDA bump (`A-IDENT-02`) | Rejected |
| Correct layout, wrong owner (`A-IDENT-03`) | Rejected |
| An unknown account discriminator (`AEGIS-VER-01`) | `UNKNOWN_SCHEMA`, entity stale, alert, **no guess** |
| Non-zero `_reserved` | `SCHEMA_DRIFT`, same path |
| An immutable market field appears to change | **FATAL**, loud, not merged |
| Events withheld while snapshots continue | Projection divergence detected and alerted |
| `ProgramData` hash changes (FI-24) | Version-boundary event recorded, alert raised |
| A market present in `getProgramAccounts` but with no `MarketCreated` event | Alert — a history gap, not a merge |
| Every oracle check violated in turn | The specific `failed_check` recorded, and the observation marked invalid |
| A transfer-fee collateral market | `amount_in ≠ credited` handled; `flags` bit1 recorded |

## 9. Acceptance criteria

- [ ] **SR-8 and SR-9 closed** (or the phase reported as blocked with exactly what is missing)
- [ ] SR-10 closed: Pyth receiver program ID and `PriceUpdateV2` layout verified
- [ ] All 18 events decode; a full lifecycle materializes correctly
- [ ] Materialized state matches account snapshots at every step of the lifecycle
- [ ] `AEGIS-PDA-01`, `AEGIS-LAYOUT-01`, `AEGIS-EVENT-01`, `AEGIS-ORACLE-01..11` pass
- [ ] `AEGIS-VER-01/02` pass: unknown schema is refused, and a decoder upgrade replays without mutating
      old rows
- [ ] Market discovery agrees between the two sources; a deliberate divergence alerts
- [ ] `aegis_market_params_history` correctly versions a `MarketParamsUpdated`
- [ ] Replay determinism still holds with the protocol layer included (RP-01 extended)
- [ ] `infra/aegis-pin.toml` records the Aegis revision and document hashes
- [ ] Universal checklist satisfied. Tag `phase-07-aegis-adapter`.

## 10. Demo

Run the scripted Aegis lifecycle; watch raw → normalized → protocol entities appear with commitment
labels; withhold an event and watch snapshot reconciliation catch and report the divergence; upgrade
the program and watch the version boundary recorded and the entity refuse to guess.

## 11. Documentation & status updates

`aegis-integration.md` updated where real artifacts differ from the spec-derived contract — **as a
finding, with an ADR if a decision changes**. `ecosystem-research.md` for SR-8/9/10.
`project-status.md`: adapter IMPLEMENTED + TESTED + DEMOED; the pinned Aegis revision.

## 12. Stop condition

**STOP after this phase.** Phase 8 has not been started.
