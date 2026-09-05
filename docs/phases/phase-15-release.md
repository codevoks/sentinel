# Phase 15 — UI, Integrated Demo, Security Review & Release

**Status: NOT STARTED.** **Prerequisite: Phase 14 complete and tagged.**

## 1. Scope

1. The Next.js application: the nine views in `ui.md` §2, consuming `sentinel-api` only.
2. **The full integrated demo** (`zero-cost-local.md` §4), scripted and reproducible end to end.
3. **A security review pass**: re-walk every threat `S-01..S-25` against the implemented system, and
   confirm each mitigation is present, tested, and falsifiable.
4. Operator runbooks completed and exercised.
5. **An honest self-review**, in the style of the Phase 0 self-attack, against the *implemented* system
   rather than the design.
6. README rewritten to describe what **exists**, with every claim linked to its artifact.
7. Final `docs/project-status.md` reflecting reality across all five states.
8. Release tag and a documented deployment procedure.

## 2. Explicit non-scope

**No new backend features.** No new integrations. No performance work (Phase 14 is done). No design
showcase — the UI is credible and secondary, in that order.

## 3. Evidence objective

**Every claim in the README is backed by a file, a test, a benchmark, or a failure-injection result
that a reader can run offline in minutes**, with no account and no spend. If any claim is not, it is
removed, not softened.

## 4. Files

`ts/apps/web/*` · `docs/runbooks/*` · `README.md` · `docs/project-status.md` ·
`docs/security-review.md`

## 5. Dependencies

Phases 1–14. The **live** demo additionally requires a deployed Aegis; the fixture-driven demo does not.

## 6. Implementation requirements — do not deviate

- **UI-1..UI-10 are correctness requirements, not design preferences.** In particular: commitment
  labels everywhere; visible staleness; `unknown_oracle` rendered honestly; `RECOMPUTING` labelled;
  every health figure marked as Sentinel's prediction with Aegis authoritative; **`u128` values as
  strings end to end**.
- **All signing happens in the user's wallet.** The UI never transmits a key or a signed transaction to
  the backend; it submits a **signature** for tracking.
- The demo requires **no account, no key, and no spend**.
- **The README describes what exists**, in the tense that is true. Planned work is labelled planned.
- The security review is a **re-walk against the implementation**, not a re-read of the threat model.
  A threat whose mitigation turns out weaker than documented is a **finding**, recorded with an ADR.
- The self-review answers the same questions as the Phase 0 self-attack, **against the built system**,
  and records what it forced to change.

## 7. Tests

- `UI-A1..UI-A7`.
- A UI smoke test over every view against the local stack.
- A lint rule and a test asserting **no `Number` is used for any `u128`/WAD value**.
- The full demo script as an automated end-to-end test, so it cannot silently rot.
- The complete suite: T1–T6, the FI campaign, the adversarial suite, the mutation check.

## 8. Adversarial / failure cases

| Case | Asserted |
|---|---|
| A live reorg during the demo | The UI shows a visible revision (UI-A4) |
| Sentinel lagging during the demo | The degraded banner appears with the reason |
| An oracle made stale during the demo | Positions render `unknown_oracle`, **not** healthy |
| A failed liquidation attempt | The lifecycle view shows the attempt and its classification (UI-A5) |
| A `u128` maximum value rendered | Exact, as a string, no precision loss (UI-A6) |
| The demo run on a machine with the network down except loopback | Completes fully |

## 9. Acceptance criteria

- [ ] `UI-A1..UI-A7` pass
- [ ] The full demo runs end to end **offline**, scripted, from a clean clone
- [ ] The demo includes the three distinguishing steps: **crash-and-recover with exactly one effect**,
      **fork and recompute**, and **replay to an identical digest**
- [ ] Security review completed: every `S-01..S-25` re-walked against the implementation, with findings
      recorded
- [ ] Runbooks completed and each exercised at least once
- [ ] Self-review completed and its findings addressed or recorded
- [ ] **Every README claim links to its artifact**; unsupported claims are removed
- [ ] `docs/project-status.md` reflects reality across IMPLEMENTED / TESTED / DEMOED / DOCUMENTED /
      COMMITTED
- [ ] `coverage-matrix.md` updated: every row's artifact now exists, or the row is reclassified honestly
- [ ] All research gates SR-1..SR-11 closed or explicitly carried with a reason
- [ ] Universal checklist satisfied. Tag `phase-15-release`.

## 10. Demo

The full scripted scenario with the UI open and Grafana beside it — the thing a reviewer watches once
and understands the whole system from.

## 11. Documentation & status updates

README rewritten. `docs/security-review.md` created. `docs/project-status.md` final. `coverage-matrix.md`
updated to reality. Any frozen document that implementation proved wrong is corrected **via an ADR**,
never silently.

## 12. Stop condition

**STOP.** v1 is complete. Anything beyond this is a new roadmap with its own Phase 0.
