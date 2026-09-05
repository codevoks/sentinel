# Phase 12 — Observability, Failure Injection & Recovery

**Status: NOT STARTED.** **Prerequisite: Phase 11 complete and tagged.**
**Re-check research gate SR-2 (Alpenglow will have activated by now).**

> **The phase that makes the repository credible.** Every failure-mode claim made in Phases 1–11 is
> produced deliberately here and its recovery asserted specifically. If time is constrained, cut Phases
> 13 and 15 before cutting this one.

## 1. Scope

1. Full OpenTelemetry instrumentation: the ~70 metrics of `observability.md` §3, structured logs with
   `SEN-*` codes, and traces (**100% on the execution path**, sampled plus always-on-error on
   ingestion).
2. Grafana dashboards provisioned as code: indexer health, provider health, protocol state, keeper,
   execution detail, platform.
3. **Every alert from `observability.md` §5, each with its runbook**, and the one-open-alert-per-entity
   constraint proven.
4. `/healthz`, `/readyz`, and **`/statusz`** — the honest, non-boolean one.
5. **The complete 28-entry failure-injection campaign** (`distributed-correctness.md` §10).
6. **The per-threat adversarial suite** — one named test per `S-01..S-25`.
7. **The mutation check** on the highest-severity mitigations.
8. The duplicate-execution fuzz objective and the policy-bypass search.
9. Operator runbooks in `docs/runbooks/`.
10. The signer moved to its **own process**, using the interface already specified.

## 2. Explicit non-scope

No new features. No Geyser. No performance optimization (Phase 14 — **measure before optimizing**). No
UI work.

## 3. Evidence objective

- **Every failure-mode claim in this repository is backed by a test that produces the failure.**
- **Every mitigation is falsifiable**: removing it makes its test fail.
- Every alert has been fired at least once in testing, and its runbook has been followed.

## 4. Files

`crates/sentinel-telemetry/src/*` · `infra/compose/observability/*` · `infra/grafana/*` ·
`tests/failure-injection/*` · `tests/adversarial/*` · `docs/runbooks/*.md` · `ts/apps/signer/`

## 5. Dependencies

Phases 1–11.

## 6. Implementation requirements — do not deviate

- **Every alert corresponds to an operator action.** A condition with no action is a metric, not an
  alert. Rows without one are deleted rather than kept "for visibility".
- Severity is binary: **`page`** (a human is woken, the action is in the table) or **`ticket`**. There
  is no warning tier for everyone to ignore.
- One open alert per `(kind, entity)`, enforced by the partial unique index. This is the difference
  between an alerting system and a pager DoS.
- **`/statusz` is not a boolean.** "Is Sentinel healthy?" is not a yes/no question, and pretending it is
  produces monitoring that shows green while the data is hours stale.
- Logging is **bounded and sampled** under a spike (LG-5). A logging system that amplifies an incident
  is part of the incident.
- **No secret in any log, metric label, or span attribute**, enforced by the redacting type and by a
  test.
- **Failure-injection tests assert specific outcomes** — a row count, a digest, a state, an alert
  raised. "It did not crash" is not an assertion.
- Kill points are **seeded and recorded**, so any failure is reproducible.
- **The mutation check is a gate**: for each listed mitigation, remove the enforcement, confirm the test
  fails, restore it. A mitigation whose test passes both ways is not being tested.

## 7. Tests

The full campaign, run nightly and before every tag, with the crash-boundary subset (FI-03..05) and
determinism subset (RP-01..03) staying in the **required** tier:

- **FI-01..FI-28** — the whole catalogue.
- **`S-01..S-25`** — one adversarial test per threat.
- **`A-SIGN-01..10`**, **`A-ORA-01..11`**, **`A-IDENT-01..03`**, **`A-PARSE-01..08`**,
  **`A-RPC-01..03`**, **`A-API-01/02`**, **`A-SQL-01`**, **`A-WS-01/02`**, **`A-DOS-01..03`**,
  **`A-SEC-01/02`**, **`A-CACHE-01/02`**, **`A-SSRF-01`**, **`A-OPS-01`**.
- **`T-RACE-01..12`** — the twelve named races, under real concurrency.
- **Duplicate-execution fuzz**: randomized kill points across the full liquidation loop; assert exactly
  one on-chain effect per intent.
- **Policy-bypass search**: generated messages attempting to reach a signature through every planner
  path; all denied.

## 8. Adversarial / failure cases

This phase *is* the adversarial case list. The additions specific to it:

| Case | Asserted |
|---|---|
| Every alert condition triggered deliberately | The alert fires exactly once, with the correct entity and severity, and its runbook resolves it |
| The same condition persisting | **One** open alert, not one per evaluation |
| A log spike | Bounded, sampled, does not amplify the incident |
| The signer as a separate process, killed mid-request | The executor handles it; no signature produced; no partial state |
| Mutation: remove the sign-before-submit ordering | FI-04 **fails** |
| Mutation: remove the `idempotency_key` unique index | The duplicate-execution fuzz **fails** |
| Mutation: remove policy check P-3 / P-9 / P-12 | The corresponding `A-SIGN-*` **fails** |
| Mutation: remove `maxSupportedTransactionVersion` | `A-VER-01` **fails** |

## 9. Acceptance criteria

- [ ] **All 28 FI entries pass with specific assertions**
- [ ] **One adversarial test per `S-01..S-25`, all passing**
- [ ] **The mutation check passes**: every listed mitigation, when removed, makes its test fail
- [ ] `T-RACE-01..12` pass under real concurrency
- [ ] The duplicate-execution fuzz finds no double effect across a full campaign
- [ ] The policy-bypass search reaches no signature
- [ ] Every alert has fired at least once in testing and has a runbook that was followed
- [ ] `DC-I-01..DC-I-10` all proven
- [ ] Dashboards provisioned as code and rendering against the local stack
- [ ] The signer runs as its own process
- [ ] SR-2 re-checked post-Alpenglow; any behavioral change recorded (**and an ADR if a decision
      changes**)
- [ ] Universal checklist satisfied. Tag `phase-12-observability`.

## 10. Demo

Run the chaos suite live against the local stack with Grafana open: kill workers, drop providers,
inject forks and duplicates, corrupt payloads, remove Redis, restart Postgres — and show the system
recovering, the alerts firing, and the final state correct. Then run the mutation check and show a
mitigation's removal breaking its test.

## 11. Documentation & status updates

`observability.md` updated with any alert added or removed during the campaign. `docs/runbooks/`
created. `threat-model.md` updated if a new threat class was found — **which is a good outcome, not a
failure**. `project-status.md`: observability IMPLEMENTED + TESTED + DEMOED; the full campaign results
with real output.

## 12. Stop condition

**STOP after this phase.** Phase 13 has not been started.
