# Phase 10 — Transaction Execution Engine

**Status: NOT STARTED.** **Prerequisite: Phase 9 complete and tagged.**

> **The crash-safety guarantees are proven here against a harmless instruction, before they guard
> anything that spends money.** Phase 10 uses Aegis's `accrue_interest` — permissionless, unpausable,
> and a successful no-op when `dt == 0` — as its exercise instruction. Liquidation arrives in Phase 11.

## 1. Scope

1. `execution_intents` and `transaction_attempts` state machines, exactly as specified.
2. Intent claim by lease with `FOR UPDATE SKIP LOCKED`; renewal; abandon-on-lease-loss.
3. Planning: re-read state, re-evaluate the precondition at `t_eval`, resolve accounts, size the action.
4. **Mandatory simulation**, then the **policy engine on the final bytes**, then sign.
5. **The crash-safe ordering**: sign → **persist + commit** (signature, bytes, blockhash,
   `lastValidBlockHeight`) → submit.
6. Submission with `skipPreflight: true` and **fan-out**, not sequential failover.
7. Resolution: observation via Sentinel's own ingestion (preferred), `getSignatureStatuses`, and
   **`lastValidBlockHeight`** as the termination oracle.
8. Retry policy with three independent budgets; fee escalation; re-plan on expiry.
9. The **signer service boundary** and the 12-check policy engine.
10. `signing_audit`, append-only, written **before** the signature is returned.
11. API: `/v1/tx/build`, `/v1/tx/track`, `/v1/intents*`, `/v1/ops/intents`.

## 2. Explicit non-scope

**No liquidation.** No candidate consumption. No keeper loop. No Aegis-specific economics beyond
constructing `accrue_interest`. No Jito, no bundles, no durable nonces.

## 3. Evidence objective

**The single most important guarantee in the repository:** an externally visible effect never happens
twice, including a crash at any instant of the lifecycle — proven by killing the process at the exact
sign/persist/submit boundaries, in the **required** CI tier.

## 4. Files

`ts/packages/executor/src/{claim,planner,simulator,submitter,resolver,retry}.ts` ·
`ts/packages/policy/src/*` · `ts/packages/aegis/src/*` (PDA vectors, SDK wrapper) ·
`ts/apps/api/src/routes/tx.ts`

## 5. Dependencies

Phases 1–9. **SR-1** re-checked for building: whether the cluster has activated v1 and whether the
pinned `@solana/kit` version builds v0 correctly (Sentinel targets v0 — Aegis instructions fit a legacy
transaction and need no ALT).

## 6. Implementation requirements — do not deviate

- **`transaction_bytes` and `signature` are committed before `sendTransaction` is called.** Not after,
  not in the same statement, not deferred. This is FR-17 and the reason the dangerous state is
  unreachable.
- **Resubmission reuses the stored bytes**, never re-signs. Re-signing produces a new signature and a
  genuine duplicate.
- **A submission timeout is not evidence of non-submission** (S-3). It moves the attempt to `SUBMITTED`
  and hands it to the resolver.
- **Termination is via `lastValidBlockHeight`**, a block-height bound — never a wall-clock timeout.
  This stays correct as slot durations change and under Alpenglow.
- **`UNKNOWN` never triggers an automatic new attempt.** It goes to `NEEDS_OPERATOR` and alerts.
- **A policy denial is FATAL and never retried.**
- The signer **re-decodes the message itself** and re-runs policy on the re-decoded form. It never
  trusts the caller's description.
- The signer refuses without a successful simulation **of those exact bytes** (P-12) and refuses on a
  cluster genesis-hash mismatch (SG-4).
- The compute-unit limit comes from **simulation**, never a hardcoded constant. The priority fee comes
  from `getRecentPrioritizationFees` over **the exact writable account set**, never program IDs.
- **No API path accepts transaction bytes for signing.** `/tx/build` returns unsigned bytes;
  `/tx/track` accepts a signature.
- `AEGIS-PDA-01` is asserted on the TypeScript side here, against the same vector file Rust asserts.

## 7. Tests

**Unit:** every legal and illegal transition of both state machines; retry classification; fee
escalation bounded by the budget; cursor of `lastValidBlockHeight` arithmetic; policy checks P-1..P-12
individually.

**Property:** `P-SM-1` (no event sequence reaches an illegal state); `P-SM-2` (every attempt
terminates); `P-BOUND-2` (per-intent fees never exceed the ceiling); `P-MONO-3` (terminal states never
transition).

**Integration:** full lifecycle of an `accrue_interest` intent against the local cluster, through to
finalized, with the audit log complete.

## 8. Adversarial / failure cases — the core of this phase

| ID | Injected | Asserted |
|---|---|---|
| **FI-03** | Kill **between sign and persist** | No attempt row; nothing broadcast; re-plan is clean |
| **FI-04** | Kill **between persist and submit** | Attempt is `SIGNED`; stored bytes resubmitted; **exactly one on-chain effect** |
| **FI-05** | Kill **between submit and state update** | Same; **exactly one on-chain effect** |
| FI-17 | Force blockhash expiry before landing | `EXPIRED`; re-plan; **exactly one effect total** |
| FI-18 | `sendTransaction` times out **after** the transaction landed | No duplicate; resolved by observation |
| FI-19 | `sendTransaction` succeeds but the transaction never lands | `EXPIRED` at `lastValidBlockHeight`; correctly terminal |
| FI-02 | Kill mid-lease | Re-claimed; no duplicate effect |
| RC-07 | Attempt confirms while a retry is being prepared | Retry re-checks under the lease; no second attempt |
| `A-SIGN-01` | Enumerate every route | **No bytes-in/signature-out path exists** |
| `A-SIGN-02` | Inject `SystemProgram::Transfer` into a planned message | Signing **fails** (P-9) |
| `A-SIGN-03` | Substitute a vault or market account | Signing **fails** (P-3) |
| `A-SIGN-04` | Simulate one message, sign another | Signing **fails** (P-12) |
| `A-SIGN-05` | Sign for the wrong cluster | Signing **fails** (SG-4) |
| `A-SIGN-06` | Unbounded retry loop attempt | Fee budget and `max_attempts` terminate it |
| `A-SIGN-07` | Key material in logs | Absent, asserted |
| `A-SIGN-10` | Operator-initiated intent that violates policy | Denied — **no operator bypass exists** |

## 9. Acceptance criteria

- [ ] **FI-03, FI-04, FI-05 pass in the REQUIRED CI tier**, not nightly
- [ ] FI-17, FI-18, FI-19, FI-02, RC-07 pass
- [ ] `TX-01..TX-12` all proven
- [ ] `A-SIGN-01..A-SIGN-10` all pass
- [ ] `P-SM-1`, `P-SM-2`, `P-BOUND-2`, `P-MONO-3` pass
- [ ] `AEGIS-PDA-01` asserted in TypeScript against the shared vectors
- [ ] Every signature has a `signing_audit` row written **before** it was returned
- [ ] A mutation test: remove the persist-before-submit ordering and confirm FI-04 **fails**
- [ ] Universal checklist satisfied. Tag `phase-10-execution`.

## 10. Demo

Create an `accrue_interest` intent; watch it through the full state machine in the API; kill the
executor at each of the three crash boundaries and show exactly one on-chain effect each time; attempt
to sign a message with an injected transfer and show the policy denial with its audit row.

## 11. Documentation & status updates

`transaction-engine.md` and `signer-and-key-management.md` updated only via ADR if implementation
revealed a genuine problem. `project-status.md`: execution engine IMPLEMENTED + TESTED + DEMOED; the
three crash-boundary results with real output.

## 12. Stop condition

**STOP after this phase.** Phase 11 has not been started.
