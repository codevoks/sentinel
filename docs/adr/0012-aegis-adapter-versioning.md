# ADR-0012 — One protocol, deeply, via a version-aware adapter that consumes Aegis's own artifacts

**Status:** Accepted · **Date:** 2026-09-05 · **Phase:** 0 · **Gated on SR-8, SR-9, SR-10**

## Context

Two decisions are entangled and are recorded together because separating them produces the wrong answer
to each.

1. **How many protocols?** A general indexer is the reflexive shape and produces breadth-shaped
   shallowness with nothing to reconcile against.
2. **How does Sentinel avoid becoming a second, drifting source of protocol truth?** Aegis is fully
   specified, will be implemented over 13 phases, and can be upgraded on-chain. Any independent
   reimplementation of its economics diverges — the only question is when.

## Decision

**One protocol domain in v1 — Aegis — via a version-aware adapter that consumes Aegis's own artifacts
rather than reimplementing them.**

### Conformance, in priority order

1. **Preferred: link Aegis's artifacts.** The Rust risk engine depends on **`aegis-math`** (which is
   `no_std`, float-free, and free of `solana-*` dependencies by Aegis's own policy — directly linkable
   from a non-Solana service). The TypeScript executor depends on **`@aegis/sdk`** for PDAs and
   instruction builders.
2. **Interim (while Aegis is pre-Phase-4/9): implement against the frozen spec, prove against frozen
   numbers.** Aegis has already published exact worked examples — HF ≈ 1.330838, HF ≈ 0.842495,
   `total_seize = 9_970_348_101`, `protocol_cut = 47_477_848`, `interest = 1_332_492`. These are
   `AEGIS-CONF-01..06`, they are blocking acceptance criteria, and **they exist today without Aegis
   writing a line of code.**
3. **Continuous: shadow-check against the chain.** Compare Sentinel's health against observed
   `Liquidated` events (`hf_before`) and against read-only simulations.

### Versioning

- A **decoder registry** with `(program_id, account_kind, schema_version, discriminator, layout_hash,
  effective_from_slot..to_slot, source)`, where `source` is `idl` or `spec` — **never inferred**.
- Every decoded row records its `decoder_version_id`.
- **Detection is layered**: BPF-loader `ProgramData` hash change (the strongest signal, available
  *before* any decode fails) → unknown discriminator → length mismatch → non-zero `_reserved` →
  semantic assertion failure.
- **On any of these: do not guess.** Mark the entity `UNKNOWN_SCHEMA`/`STALE`, alert, pause the keeper
  on a program upgrade.
- A new decoder version triggers a **replay**, never an in-place `UPDATE`. Old rows keep their old
  `decoder_version_id`, and the raw bytes are still there.

### The pin

`infra/aegis-pin.toml` records the Aegis revision and the hashes of the documents the contract was
derived from. A changed document is a **design event that stops the session**, never a silent
absorption.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **A general multi-protocol indexer** | Nothing to reconcile against, so every correctness claim becomes untestable. The layer boundaries make a second adapter additive; a warehouse is a different product (`product.md` §2.2). |
| **Reimplement Aegis's economics in Sentinel** | A second source of truth for health. It would diverge, and it would diverge exactly where it costs money. Aegis's `aegis-math` exists precisely so it does not have to be reimplemented. |
| **Heuristic layout sniffing / "best-effort decode"** | Produces a plausible wrong number instead of an honest `UNKNOWN_SCHEMA`. In a system that spends money on its numbers, a plausible wrong number is worse than no number. |
| **Decode from the on-chain IDL at runtime** | Attractive, but it makes decoding depend on a network fetch and on the IDL being published and current. The IDL is **vendored and pinned**; runtime fetch is not on the required path (ADR-0013). |
| **Freeze one decoder and require redeployment on upgrade** | Historical rows become uninterpretable after the first upgrade, and interpreting history is the product. |
| **Mutate historical rows on decoder upgrade** | Destroys the audit trail and makes replay non-reproducible. Replay is the correct mechanism and it already exists. |
| **Trust the event stream alone (Aegis guarantees FR-19)** | One missed log silently diverges forever. Dual reconstruction with snapshot reconciliation catches it (`aegis-integration.md` §7). |
| **Trust account snapshots alone** | Incomplete by construction since Agave 4.2, and Sentinel cannot subscribe to every position. |

## Consequences

**Positive**
- Sentinel's health math cannot drift from the protocol's, because it *is* the protocol's.
- An Aegis upgrade is a handled event with a defined procedure, not an outage.
- Historical rows stay interpretable forever: each names its decoder, and its inputs still exist.
- Off-chain invariant checking becomes possible, which serves Aegis's own runbook R-2.

**Negative**
- **Sentinel was blocked upstream** at Phase 7 (needs Aegis ≥ 6) and Phase 11 (needs Aegis ≥ 9) as of
  the 2026-09-04 research date, when Aegis was at Phase 0. **Status update, recorded in Sentinel
  Phase 1 (2026-09-18):** Aegis has since completed its full roadmap through Phase 13 and published
  `v0.1.0` (`codevoks/aegis-protocol`), so this dependency is no longer blocking — see
  `docs/project-status.md` and `docs/aegis-integration.md` §2 for the verified reconciliation. Phases
  1–6 deliberately had no upstream dependency at all, so work was never blocked on someone else's
  schedule regardless; that property is unchanged and Phase 7/8/11 still do not start early.
- Sentinel inherits Aegis's release cadence for `aegis-math` / `@aegis/sdk`. Mitigated by the pin and
  the interim conformance path.
- A version-aware decoder is more machinery than a single decoder. It is the machinery the problem
  actually has.
- SR-8 (program ID, IDL, discriminators), SR-9 (`emit!` vs `emit_cpi!`), and SR-10 (Pyth layout) were
  **open and blocked upstream** at the research date. The blocking artifacts now exist upstream; formal
  verification and pinning remain **deferred to Phase 7**, not performed early.

**Enforcement**
- `AEGIS-CONF-01..06`, `AEGIS-PDA-01`, `AEGIS-VER-01/02`, `AEGIS-INV-01..08`.
- `CI-NOMATHDUP` blocks economic arithmetic in `sentinel-risk` — it must be called, not written.
- `CI-NOAEGISLEAK` blocks any Aegis concept in `sentinel-normalize`, keeping the second-adapter seam
  real.
