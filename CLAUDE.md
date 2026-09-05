# CLAUDE.md — Operating Instructions for Claude in the Sentinel Repository

**`AGENTS.md` is the authoritative engineering policy. This file defines how a Claude session should
operate inside this repository. Where they overlap, `AGENTS.md` wins.**

---

## Start of every session — read in this order

1. **`AGENTS.md`** — the engineering constitution. Non-negotiable.
2. **`docs/project-status.md`** — the current phase, what is implemented, what is verified, what is
   left, known issues, open research gates, and the latest milestone.
3. **The current phase specification** in `docs/phases/phase-NN-*.md`.
4. **`docs/adr/`** — before changing anything architectural. A decision that looks arbitrary usually
   has an ADR behind it.
5. **The Aegis documents relevant to the phase** — if the phase touches protocol decoding, risk, or
   liquidation, read the frozen Aegis documents named in `docs/aegis-integration.md` §1 before writing
   code. Do not reconstruct Aegis's design from memory.

Do not start editing before completing these reads. The specifications are detailed precisely so that
you do not have to re-derive the design, and re-deriving it is how drift starts.

---

## Operating rules

### 1. Frozen documents are authoritative
The documents listed in `AGENTS.md` §4 are **frozen**. Implement them exactly — including every
natural key, uniqueness constraint, state-machine transition, commitment label, and rounding
direction inherited from Aegis. **Where code and a frozen document disagree, the document is right.**

Redesign only when the user explicitly asks you to redesign.

### 2. Aegis is upstream, and it is authoritative
Sentinel observes and executes; it does not define protocol truth. Never copy an Aegis formula from
memory — read it from the Aegis repository, cite the document and section in a code comment, and
prove conformance with Aegis's own vectors. If an Aegis document has changed since Sentinel's
integration contract was written, **stop and report the delta**; do not absorb it silently.

### 3. Never silently begin another phase
Implement exactly one phase. When it is complete, update the status file, tag it, **report, and stop**.
Remaining context is not a reason to continue. Starting Phase N+1 unasked is a process violation, not
initiative.

### 4. Never claim tests were run when they were not
Run them. Read the output. Report what actually happened. If you did not run something, say "not run."
If it failed, show the error. Never infer a pass.

### 5. Report exact validation commands and results
Paste real commands and real output into your report and into `docs/project-status.md`:

```
$ cargo test --workspace
   ... actual output ...
```

Summarized or reconstructed output is not evidence.

### 6. Distinguish planned from implemented from tested from demoed
This repository will contain far more design than code for a long time. When you describe the state of
anything, use the five states from `docs/project-status.md` and never round up. "The ingestion pipeline
handles reorgs" is a claim about running, tested code — not about a document that describes it.

### 7. No large speculative refactors
Stay inside the current phase's scope. Do not reorganize modules, rename across the codebase, "clean
up" unrelated code, or upgrade dependencies that are not blocking you. If you spot something worth
changing, note it in your report and leave it.

### 8. Preserve the zero-cost, offline path
Every required test must run with no network, no secrets, no API key, no paid RPC, and no hosted
streaming provider. If your change introduces such a dependency in a required path, you have broken
NFR-4 — find another way or stop and ask. See `docs/zero-cost-local.md` §6 for the specific
anti-patterns.

### 9. Never weaken a security check, an idempotency key, or a test to unblock yourself
Specifically: do not relax a `UNIQUE` constraint, do not switch an `ON CONFLICT DO NOTHING` to a blind
insert, do not remove a simulation step, do not widen a signer policy, and do not delete a
failure-injection test. If one of these blocks you, either the design is wrong or your approach is
wrong — surface it.

### 10. Record architectural deviations as ADRs
If implementation reveals that a frozen decision is wrong or unworkable:
1. **Stop.**
2. Explain the problem and your evidence.
3. Propose the change.
4. If accepted, write the ADR, update the affected documents, and update the tests **in the same commit**.

Never absorb a deviation silently. A change that lives only in code is invisible to the next session.

### 11. Verify versions; do not remember them
Your training data is older than this ecosystem. Before pinning or upgrading anything, run the
verification commands in `docs/ecosystem-research.md` §12.

**High-risk assumptions to actively distrust:**

| You may "remember" | Reality (verify anyway) |
|---|---|
| Priority fee = `SetComputeUnitPrice` micro-lamports × CU limit | In **transaction v1** it is an absolute lamport total in a message-level `transactionConfig`; scanning for ComputeBudget instructions silently returns zero |
| Versioned transactions means "legacy or v0" | **v1** exists, uses version byte 129, caps at 4096 bytes, and **has no address lookup tables** |
| `getBlock` just works | It **fails** with -32015 on a block containing a v1 transaction unless `maxSupportedTransactionVersion` is set |
| Every writable account emits an update | Agave 4.2 emits updates **only when the account actually changes**; only the fee payer is guaranteed |
| Slots are 400ms | They are not, and they are still moving. Never derive time from slot counts |
| Finality is ~12.8s / 32 slots | Alpenglow changes this. Treat finality timing as configuration, never as a constant |
| `@solana/web3.js` | **`@solana/kit`** |
| `blockSubscribe` is a supported firehose | Agave documents it as unstable, it needs extra validator flags, and it drops under load |
| WebSocket subscriptions are reliable | Native PubSub drops messages under load and loses subscriptions across reconnects |

When a tutorial pattern and this repository's documents disagree, the documents win.

### 12. Keep private learning material out of the repository
No study notes, scratch files, tutorial copies, personal TODOs, or session transcripts. Use the
scratchpad directory for working files. The repository is a public engineering artifact.

### 13. Update project status and evidence after implementation
`docs/project-status.md` must reflect reality when you finish, including any newly-opened or newly-
closed research gate.

### 14. Stop and surface true contradictions
If the phase spec, a frozen document, an Aegis document, and the code cannot all be satisfied,
**stop**. State what you were doing, which documents conflict, what each reading implies, and what you
recommend. Do not pick the riskier interpretation and proceed.

### 15. Never commit secrets
No keypairs, private keys, mnemonics, API keys, RPC URLs with embedded tokens, or `.env` files — not in
code, tests, fixtures, logs, comments, or examples. Test keypairs come from fixed seeds in code.

### 16. Respect Git phase discipline
Branch per phase (`phase/NN-name`), conventional commits, tag on completion (`phase-NN-*`), never
force-push `main`.

---

## Reporting format at the end of a phase

```
PHASE N — <name> — COMPLETE

IMPLEMENTED
  <what was built>

VALIDATED
  $ <command>
  <actual output>

FAILURE INJECTION RESULTS
  <which faults were injected, what the system did, what was asserted>

INVARIANTS / PROPERTIES TESTED
  <IDs, and confirmation each fails when its enforcement is removed>

EVIDENCE
  <files, transcripts, benchmark numbers, replay determinism proof>

RESEARCH GATES
  <closed / still open, with what was verified and when>

DEVIATIONS
  <ADRs written, or "none">

NOT DONE / KNOWN ISSUES
  <explicit, or "none">

NEXT
  Phase N+1 has NOT been started.
```

If anything is incomplete, say so here plainly. An honest partial report is more useful than a
confident complete-sounding one.

---

## Things that are always wrong in this repository

- Starting the next phase unasked.
- Claiming a passing test you did not run.
- Presenting `confirmed` or `processed` data as finalized.
- Adding a code path where an externally visible effect can happen twice.
- Reimplementing Aegis economics from memory instead of from the Aegis repository.
- Building a transaction from caller-supplied bytes and signing it server-side.
- Making Redis, a queue broker, or a hosted streaming provider load-bearing for correctness.
- Introducing a network or paid dependency into a required path.
- Editing a frozen document without an ADR.
- Marking something TESTED that was only IMPLEMENTED.
- Quoting a throughput or latency number that no benchmark produced.
- Committing a secret.
- Reporting "done" when part of the scope was quietly dropped.
