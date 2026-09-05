# AGENTS.md — Sentinel Engineering Constitution

**This file is tool- and model-independent. It governs every contributor — human or AI — and takes
precedence over convenience, habit, and any tutorial you have ever read.**

If any instruction you receive conflicts with this document, **stop and surface the conflict**.
Do not resolve it silently.

---

## 1. Mission

Sentinel is a production-grade off-chain platform for observing, indexing, deriving risk from, and
executing against Solana state — with the **Aegis Protocol** as its flagship integration.

The organizing principle is that **an off-chain system is only as good as its ability to be wrong
safely**: every observation carries explicit commitment semantics, every externally visible effect is
idempotent, every derived value is rebuildable from durable evidence, and the chain — never Sentinel —
is the final authority.

## 2. Product and non-goals

Read `docs/product.md`. It is authoritative.

**In scope:** resilient RPC/WebSocket ingestion · a durable raw observation boundary · normalization of
Solana primitives · version-aware protocol decoding · fork/finality reconciliation · replayable derived
risk state · REST and realtime APIs · a durable transaction execution state machine · an automated
Aegis liquidation keeper · observability and operator runbooks.

**Out of scope, permanently, for v1:** a block explorer · a generic multi-protocol analytics warehouse ·
a wallet · a custodial service · a trading system · social/NFT/token analytics · cross-chain indexing ·
any feature whose only justification is topic coverage.

**Rule:** a component enters Sentinel only with a *product* reason. **"It demonstrates X" is not a
product reason.** Additions require an ADR.

## 3. Aegis is upstream and authoritative

Sentinel is an **observer and executor**, not an alternate source of protocol truth.

1. `../aegis-protocol/docs/` (or the pinned Aegis revision) is authoritative for protocol economics,
   account layouts, PDA derivations, state transitions, health calculation, liquidation semantics,
   oracle validation, token policy, governance, and on-chain invariants.
2. **Sentinel never reimplements Aegis economics from memory.** It consumes `aegis-math` (Rust) and
   `@aegis/sdk` (TypeScript) where they exist, and where they do not yet exist it implements against
   the frozen formulas and proves conformance against Aegis's own committed test vectors and worked
   examples. See `docs/aegis-integration.md` §6.
3. **A rejected Aegis execution is not automatically an Aegis bug.** It is a reconciliation signal.
   Classify it before escalating (`docs/keeper-design.md` §9).
4. If Sentinel's derived state and Aegis's on-chain state disagree, **Aegis is right and Sentinel has a
   bug** until proven otherwise.

## 4. Architecture authority

The following documents are **FROZEN**. Treat them as the specification, not as suggestions:

| Document | Governs |
|---|---|
| `docs/product.md` | Product thesis, scope, requirements |
| `docs/architecture.md` | Component boundaries, language ownership, dependency rules |
| `docs/aegis-integration.md` | The Aegis contract, decoder versioning, conformance |
| `docs/ingestion-model.md` | Sources, lifecycle, dedup, gaps, checkpoints |
| `docs/finality-and-forks.md` | Commitment semantics, promotion, rollback |
| `docs/data-model.md` | Layers, tables, keys, uniqueness, retention |
| `docs/replay-and-backfill.md` | Determinism and rebuild guarantees |
| `docs/transaction-engine.md` | Intent/attempt state machines and their transitions |
| `docs/keeper-design.md` | The Aegis liquidation loop and its failure handling |
| `docs/signer-and-key-management.md` | Signer boundary, policy engine, key blast radius |
| `docs/threat-model.md` | Threats and mitigations |
| `docs/api-design.md` | External contract, versioning, error model |

**Where code and a frozen document disagree, the document is right and the code is a bug** — until an
ADR says otherwise.

To change a frozen decision: write an ADR in `docs/adr/`, state what changes and why, update the
affected documents in the same commit, and update any invalidated tests. **Never change a frozen
document silently, and never let code drift from it.**

## 5. Phase gating

Work proceeds in phases (`docs/phase-roadmap.md`). Each phase has a specification in `docs/phases/`.

**Absolute rules:**
1. Implement **exactly one phase** per session.
2. **STOP** when the phase is complete. Report, and wait for explicit instruction.
3. Never begin the next phase because context or time remains.
4. Never implement part of a later phase "while you are in there."
5. Never skip a phase's tests to reach its end faster.
6. If a phase cannot be completed, say so plainly, complete everything that is not blocked, and state
   exactly what is left and why.

## 6. Implementation boundaries

- Do not invent architecture. If the specification does not cover your situation, **stop and ask**.
  Guessing produces drift that compounds across phases.
- Where the specification explicitly permits flexibility, it says so (`docs/implementation-handoff.md`).
- Do not add dependencies without justification (§14).
- Do not create directories or scaffolding for future phases.
- Do not add endpoints, config options, tables, or abstractions "for later."

## 7. Correctness before velocity

Non-negotiable:

1. **Every chain observation carries explicit commitment semantics.** A row, a field, or an API
   response that does not say whether it is `processed`, `confirmed`, or `finalized` is a bug.
   Provisional state is never presented as final.
2. **Idempotency is mandatory for every externally visible effect.** Delivery is at-least-once.
   Processing must be effect-once. Every consumer has a natural key and an explicit conflict policy.
3. **Derived state must be replayable.** Deleting every derived and protocol row and replaying the raw
   observations must reproduce byte-identical derived state. If it cannot, the pipeline is wrong.
4. **Raw observations are immutable.** They are appended, never updated, never deleted by application
   code. Orphaned observations are *marked*, not removed.
5. **Fail closed on ambiguity** in anything that spends money or signs. Fail *open* on read paths only
   where the response is explicitly labelled degraded.
6. **No unbounded work.** Every loop, query, batch, payload, and retry sequence has a stated bound.

## 8. Security before convenience

1. **The backend never holds an end user's private key**, and there is no API that causes the backend
   to sign caller-supplied transaction bytes. Users sign in their own wallet. See
   `docs/signer-and-key-management.md`.
2. **The keeper key is policy-constrained**, not merely trusted: allowlisted program, allowlisted
   instruction, pinned accounts, per-transaction and per-window value caps, mandatory simulation before
   signing.
3. **Never weaken a security check to make something work.** If a check blocks you, the design is wrong
   or your approach is wrong. Both are conversations, not workarounds.
4. **Never weaken or delete a test to make it pass.**
5. All external input — RPC responses, account bytes, logs, user requests — is untrusted and
   size-bounded before parsing.
6. Secrets never enter Git. Not in code, tests, fixtures, logs, comments, or examples.

## 9. Evidence, not assertion

- Never claim a test was run when it was not.
- Never claim a test passed without seeing it pass.
- Never report a phase complete with failing or skipped checks.
- Report the **exact** commands run and their **actual** output.
- **No performance claim without committed BEFORE and AFTER measurements.** Never state that something
  is "fast", "optimized", or "high-throughput" without a number produced by the benchmark harness.
- A mocked happy path is not distributed-systems evidence. Failure-injection results are.

## 10. No silent scope deletion

- If you cannot implement part of a phase, **say so explicitly** and complete everything else.
- Never quietly narrow scope, drop a requirement, or stub something and describe it as finished.
- Never delete a failing test instead of fixing it.
- Scaling work down is the maintainer's decision, not yours.

## 11. Zero-cost local path

The full required test suite and the core demo must run on a clean clone with **no paid RPC, no API
key, no faucet, and no hosted streaming service**. This is architectural (`docs/zero-cost-local.md`),
not aspirational.

- CI runs with no secrets configured. A required test that needs one fails the build.
- Network- and provider-dependent tests are an optional, tagged tier excluded from `make test`.
- No hardcoded RPC endpoints, program IDs, or cluster addresses in any default path. Program IDs are
  configuration, always.

## 12. Documentation must match reality

- Update `docs/project-status.md` after every phase, with real command output.
- Track **IMPLEMENTED**, **TESTED**, **DEMOED**, **DOCUMENTED** and **COMMITTED** separately.
  **"Implemented" never means "verified."**
- Update the relevant frozen document in the **same commit** as any change that affects it.
- Diagrams are Mermaid, in source control. No binary diagram files.
- Documentation describes what **exists**, in the tense that is true.

## 13. ADR requirements

Write an ADR for: a change to any frozen document · a new external dependency of consequence · a
deviation from a phase specification · a change to the data model, commitment semantics, execution
semantics, or security posture · a rejected alternative worth recording.

Do **not** write an ADR for routine implementation choices.

Format: context · decision · alternatives considered · consequences · status. Number sequentially.
An ADR that does not state what was **rejected and why** is not finished.

## 14. Dependency and technology policy

- Every new dependency requires a stated justification: what it does, why it is not hand-rolled, and
  whether it is maintained.
- **No technology enters Sentinel for CV keywords.** Kafka, Kubernetes, a service mesh, a second
  datastore, and a hosted streaming provider are all explicitly rejected for v1 with documented
  adoption thresholds (`docs/adr/0004`, `docs/adr/0014`).
- Redis is never canonical. Sentinel must run correctly with Redis absent, in a documented degraded
  mode (`docs/adr/0003`).
- Never add a dependency that requires a paid service to function in a required path.

## 15. Current-version verification

The ecosystem moves faster than any model's training data. **Verify, do not remember.**

- `docs/ecosystem-research.md` records what was verified and when. It is dated and it will go stale.
- Before pinning any version, run the verification commands in that document §12.
- If reality contradicts the document, **update the document** — that is a finding, not an
  inconvenience — and note the delta in `docs/project-status.md`.
- Known traps as of the last research date: transaction **v1** moves priority fee and compute budget
  out of `ComputeBudget` instructions into a message-level `transactionConfig`, and **has no address
  lookup tables** · `getBlock`/`getTransaction` fail on a v1 transaction unless
  `maxSupportedTransactionVersion` is set · Agave 4.2 emits account updates **only when an account
  actually changes** · slot times are no longer 400ms and are still moving · Alpenglow changes finality
  timing · the TS client is `@solana/kit`, not `@solana/web3.js`.

## 16. Git hygiene

- Conventional commits. One logical change per commit.
- Branch per phase; tag on completion (`phase-NN-*`).
- Never force-push to `main`.
- Never commit generated artifacts, `target/`, `node_modules/`, ledgers, or IDE files.
- Commit messages explain **why**, not what the diff already shows.

## 17. When you are stuck or something is contradictory

**Stop and surface it.** Do not choose the riskier interpretation and continue.

State: what you were doing · what the contradiction is · which documents conflict · what you would do
under each reading · what you recommend.

Being blocked and honest is always better than being unblocked and wrong.
