# Sentinel — Product Thesis, Critique, and Scope

**Status: FROZEN (Phase 0). Changing anything here requires an ADR.**

---

## 1. Final product thesis

> **Sentinel is the off-chain nervous system for an on-chain protocol: it observes Solana durably,
> reconstructs protocol state it can prove, derives risk from it, and acts on that risk through a
> transaction engine that can crash at any moment without doing anything twice.**
>
> Its flagship integration is **Aegis Protocol**, and its flagship closed loop is **automated
> liquidation**: oracle or state change → ingest → decode → materialize → evaluate health → candidate →
> simulate → submit → track → reconcile → publish.
>
> The organizing principle is that **an off-chain observer is defined by how it is wrong**. Chains
> fork, RPC lies, WebSockets drop, workers crash mid-flight, and the protocol — not the observer — is
> the authority. Sentinel's architecture is the set of decisions that make each of those survivable and
> *provable*, rather than merely unlikely.

One sentence: *Sentinel is a replayable Solana indexer and durable execution engine, built as the
operational counterpart to Aegis rather than as a general-purpose data platform.*

---

## 2. Product critique — challenging the brief

### 2.1 Is "a Solana indexing and execution platform" the right core product?

**Assessed honestly: yes, but only when it is anchored to one protocol it must be correct about.**

Arguments *for*:

- The pairing with Aegis creates a **closed, falsifiable loop**. An indexer with no consumer can be
  subtly wrong forever; an indexer whose output drives a keeper that spends real value gets its errors
  reported back by the chain, in the form of rejected transactions. That feedback is what makes the
  correctness work real rather than decorative.
- Liquidation forces the hard subjects simultaneously: at-least-once delivery, fork reconciliation,
  lazily-accrued on-chain state, oracle staleness, competitive execution, ambiguous submission
  outcomes, and key-blast-radius control. No other single product produces that density honestly.
- Aegis is *already specified to formula and field level*. That removes the usual excuse for a vague
  indexer ("the protocol doesn't exist yet") and makes the integration contract checkable.

Arguments *against*, taken seriously:

- **"A Solana indexer" is one of the most-cloned portfolio projects that exists.** A `getProgramAccounts`
  poller with a Postgres table and a dashboard is a liability, not an asset. The differentiation must
  come from durability, replay, fork correctness, and execution safety — not from the category.
- **A general analytics platform is breadth-shaped shallowness.** Indexing "all of Solana" means
  indexing nothing well, and it makes every correctness claim untestable because there is no ground
  truth to reconcile against.
- **An off-chain system can quietly become a second source of protocol truth**, which is a genuine
  anti-pattern: it invites users to trust a number Sentinel computed over the number the chain would
  compute.

**Resolution:** keep the platform, reject the general-analytics shape, anchor everything to one
protocol, and make "the chain is authoritative" a structural property rather than a disclaimer.

### 2.2 CHANGE 1 — One protocol, deeply, instead of many protocols, shallowly

*(ADR-0012)*

Sentinel indexes exactly one protocol domain in v1: **Aegis**. SPL Token / Token-2022 movements are
indexed because Aegis custody accounting depends on them, not because tokens are a topic.

Why this is an improvement, not a simplification:

1. **It makes correctness checkable.** Aegis publishes exact invariants (`INV-CUS-01/02` are byte-exact
   equalities) and exact worked examples. Sentinel can assert that its reconstruction of a market
   satisfies the protocol's own invariants — a claim a general indexer cannot make about anything.
2. **It makes decoding versionable rather than heuristic.** One protocol with a published account model
   and IDL can have a registry of decoder versions with explicit ranges. "Best-effort decode whatever
   shows up" cannot.
3. **It makes the keeper possible.** Automated execution requires knowing the exact preconditions the
   program will enforce. That knowledge does not generalize.

Cost accepted: Sentinel is not reusable as a generic indexing product on day one. The **layer
boundaries** (raw → normalized → protocol adapter → derived) are designed so a second protocol adapter
is an additive change, and that seam is tested by keeping the normalized layer free of any Aegis
concept.

### 2.3 CHANGE 2 — The raw observation boundary is a first-class product surface, not an implementation detail

*(ADR-0008)*

Most indexers decode on the way in and persist only the decoded result. Sentinel persists an
**immutable raw observation** first, and decodes as a separate, restartable step.

Why:

- **Replay becomes real.** "Delete derived state, replay, get the same answer" is only meaningful if the
  inputs survive independently of the decoder that consumed them. A decoder bug found in month three is
  fixable retroactively.
- **Forensics become possible.** When Sentinel and Aegis disagree, the argument is settled by the bytes
  that were actually observed, tagged with which provider returned them and when.
- **Decoder upgrades stop being migrations.** A new decoder version re-reads the same raw rows.

Cost accepted: storage amplification and a second write on the hot path. `docs/data-model.md` §9 states
the retention policy and the measured-cost gate that would force pruning.

### 2.4 CHANGE 3 — Business intent is separated from transaction attempt

*(ADR-0010)*

The naive model is one row per transaction. Sentinel splits it: an **execution intent** ("liquidate
position P for at most X") is the unit of business idempotency; a **transaction attempt** (one signed
message, one signature) is the unit of chain identity. One intent may produce several attempts.

Why: retries, blockhash expiry, RPC failover, and ambiguous submissions all create *new transactions*
for the *same intent*. Conflating them is exactly how an economically sensitive operation executes
twice. Splitting them makes "did this business action already happen?" answerable without asking the
chain about a signature that may never have existed.

### 2.5 CHANGE 4 — The optional high-performance path is optional in the build, not just in the README

*(ADR-0006)*

Geyser/Yellowstone is behind the same `ObservationSource` interface as RPC/WebSocket, is introduced
only after ingestion lag has been *measured* against a target, and the required test suite never uses
it. A hosted streaming subscription is never required to run, test, or demo Sentinel.

### 2.6 Is the product coherent?

| Question | Answer |
|---|---|
| Who consumes Sentinel's data? | The Aegis UI, protocol operators, and Sentinel's own keeper. All three are real consumers with different freshness requirements. |
| Why not read the chain directly? | For a single position, you should — and Aegis's SDK does. Sentinel exists for the queries the chain cannot answer: history, *which* positions are unhealthy across all markets, oracle degradation over time, and reconciliation between predicted and actual outcomes. |
| Why does the keeper belong here rather than in Aegis? | Aegis explicitly declines to depend on a keeper (`architecture.md` §7). Liquidation is permissionless; a keeper is an independent economic actor. Putting it in the observer is the correct boundary. |
| What is Sentinel authoritative over? | Its own observation history and its own execution history. Nothing else. |
| What happens if Sentinel is wrong? | The chain rejects the transaction, and the mismatch is recorded, classified, and alerted on. Being wrong is a designed-for outcome, not an exception. |
| Could this become a real product? | Yes — the seams are deliberate: one protocol adapter → many; RPC source → Geyser source; Postgres job table → a broker at a stated threshold; single-node compose → orchestration at a stated threshold. |

---

## 3. Non-goals (explicit, permanent for v1)

| Not building | Why not |
|---|---|
| A block explorer | Solved, undifferentiated, and would require indexing everything — which destroys every correctness claim. |
| A general multi-protocol analytics warehouse | Breadth-shaped shallowness. The layer boundaries make a second adapter cheap; a warehouse is a different product. |
| A wallet, or any custody of user keys | The backend never holds a user private key. Non-negotiable (`docs/signer-and-key-management.md`). |
| A trading system, market maker, or strategy engine | Liquidation is a protocol safety mechanism with a defined on-chain precondition. Discretionary trading is not, and has no product reason here. |
| A mempool / pre-execution alpha product | Requires privileged infrastructure, is adversarial in a different domain, and cannot be demonstrated on the zero-cost path. |
| Cross-chain indexing | Enormous surface, no product reason. |
| NFT / social / token-launch analytics | No product reason. |
| A hosted multi-tenant SaaS with billing | Operational product work orthogonal to the engineering thesis. |
| Kafka, Kubernetes, a service mesh, a second datastore | Rejected with documented adoption thresholds (ADR-0004, ADR-0014, ADR-0003). Adding them without crossing the threshold is CV-driven architecture. |
| An alternate authority over Aegis state | The anti-pattern this entire document is organized against. |

**Rule:** anything on this list may only enter Sentinel through an ADR that states the *product* reason.
"Demonstrates X" is not a product reason.

---

## 4. Personas and use cases

### P1 — Protocol operator ("Aegis steward")
Runs the protocol and must know, quickly and truthfully, whether it is healthy.
- U1: See per-market state — totals, utilization, rate, accrual freshness — with an explicit commitment
  label and an explicit staleness figure.
- U2: See oracle health per feed: last publish time, confidence ratio, and whether the market is
  currently fail-closed for borrow/liquidate.
- U3: See open bad-debt exposure and liquidation history.
- U4: Be paged when an Aegis invariant that Sentinel can check off-chain (`INV-CUS-01`, `INV-CUS-02`,
  `INV-ACC-03`) appears violated — which is the trigger for Aegis runbook R-2.
- **Critical guarantee:** Sentinel must never show a provisional value as final, and must always be able
  to say how far behind the chain it is.

### P2 — Borrower / lender (via the Aegis UI)
- U5: See position health, liquidation price, and accrued interest, with freshness stated.
- U6: See their own position history reconstructed from events.
- U7: Track a transaction they submitted from their own wallet through to finalization.
- **Constraint:** Sentinel's numbers are *informational*; the Aegis SDK's on-chain read is
  authoritative, and the UI must say so where the two could diverge.

### P3 — Liquidation keeper (Sentinel's own automation)
- U8: Continuously evaluate every position's health against fully-accrued debt at the intended
  execution time — not at the last-observed accrual timestamp.
- U9: Create a liquidation candidate with an explicit profitability model and an explicit expiry.
- U10: Simulate, size compute and priority fee, sign under policy, submit, and track to finalization.
- U11: Never execute the same business intent twice, including across a crash between signing and
  submission.
- U12: Record every rejection with its Aegis error code and classify it as race, staleness, or model
  divergence.

### P4 — Sentinel operator (SRE)
- U13: See ingestion lag, gap count, reconnect rate, decode-failure rate, and queue depth, with an
  alert for each that maps to a documented action.
- U14: Replay a slot range after a decoder fix, without downtime and without duplicating effects.
- U15: Fail over between RPC providers and see the switch in the telemetry.
- U16: Rebuild all derived state from raw and confirm byte-identical output.

### P5 — Integrating engineer
- U17: Consume a versioned REST API and a resumable WebSocket stream with documented commitment
  semantics and idempotency keys.
- U18: Run the entire platform locally, free, deterministically, in minutes.

---

## 5. Functional requirements

| ID | Requirement |
|---|---|
| FR-1 | Sentinel ingests slots, blocks, transactions, instructions, account states, and program logs from a configurable set of Solana RPC/WebSocket providers. |
| FR-2 | Every ingested observation is written to an immutable raw layer before any decoding occurs, tagged with source, provider, commitment, and receipt time. |
| FR-3 | Ingestion survives process restart without loss or duplication of effect, resuming from a durable checkpoint. |
| FR-4 | Sentinel detects slot gaps and repairs them by backfill, and records every detected and repaired gap. |
| FR-5 | Duplicate delivery of the same observation produces no duplicate row and no duplicate effect. |
| FR-6 | Sentinel maintains an explicit commitment state per slot and promotes `processed`→`confirmed`→`finalized` as evidence arrives. |
| FR-7 | Sentinel detects fork/rollback — a slot whose block is not on the finalized chain — and marks the affected observations abandoned without deleting them. |
| FR-8 | Derived state affected by an abandoned slot is recomputed from the surviving chain. |
| FR-9 | The normalized layer represents Solana primitives with no protocol-specific concepts. |
| FR-10 | A version-aware Aegis adapter decodes Aegis accounts and events into protocol entities, recording which decoder version produced each row. |
| FR-11 | Sentinel materializes Aegis market and position state and reconciles the event-derived projection against periodic account-state snapshots. |
| FR-12 | Sentinel computes position health using Aegis's own arithmetic, accrued to a specified timestamp, and identifies liquidation candidates. |
| FR-13 | Deleting all protocol and derived state and replaying the raw layer reproduces byte-identical protocol and derived state. |
| FR-14 | Sentinel exposes a versioned REST API for protocol health, markets, positions, position history, liquidation candidates, oracle state, intent status, and indexer health. |
| FR-15 | Sentinel exposes a WebSocket API with subscription, resume-from-cursor, and explicit commitment labelling. |
| FR-16 | Sentinel implements a durable transaction execution state machine separating business intent from transaction attempt. |
| FR-17 | A signed transaction's signature is durably recorded **before** submission, so a crash never produces an unattributable in-flight transaction. |
| FR-18 | Sentinel resolves ambiguous submission outcomes deterministically using blockhash expiry, and never leaves an attempt in an unknown state indefinitely. |
| FR-19 | The keeper executes the full Aegis liquidation loop end-to-end and records the on-chain outcome against its prediction. |
| FR-20 | Every server-side signing operation passes a policy check (allowed program, allowed instruction, pinned accounts, value caps) and a mandatory simulation. |
| FR-21 | Sentinel emits OpenTelemetry-compatible metrics, structured logs, and traces, and every alert maps to a documented operator action. |
| FR-22 | An optional Geyser/Yellowstone source implements the same observation interface, with automatic fallback to RPC. |

## 6. Non-functional requirements

| ID | Requirement |
|---|---|
| NFR-1 | **Delivery is assumed at-least-once.** Every consumer is idempotent under duplication and reordering within its stated ordering domain. |
| NFR-2 | **Every persisted observation and every API field carries explicit commitment semantics.** |
| NFR-3 | **Raw observations are immutable.** Application code never updates or deletes them. |
| NFR-4 | The full required test suite and the core demo run offline with no paid RPC, no API key, no faucet, and no hosted streaming service. |
| NFR-5 | Postgres is the only canonical store. Redis is optional; Sentinel runs correctly without it in a documented degraded mode. |
| NFR-6 | No externally visible effect may occur more than once for a single business intent, including across process crashes at any point in the lifecycle. |
| NFR-7 | Every derived table is rebuildable from canonical data by a deterministic, restartable process. |
| NFR-8 | No unbounded loop, query, payload, batch, or retry sequence anywhere in the system. |
| NFR-9 | The backend never holds an end-user private key, and no API path causes the backend to sign caller-supplied transaction bytes. |
| NFR-10 | The keeper key's maximum loss is bounded by a stated, enforced budget, not by trust. |
| NFR-11 | Every performance claim is backed by committed before/after measurements from the benchmark harness. |
| NFR-12 | Every failure-mode claim is backed by a failure-injection test that produces the failure deliberately. |
| NFR-13 | No secret, keypair, or `.env` value is ever committed. |
| NFR-14 | Time is derived from block timestamps or wall clock, never from slot arithmetic — slot durations are non-constant and still changing. |
| NFR-15 | All external input is size-bounded and validated before parsing; a malformed payload degrades one record, never a worker. |

---

## 7. What "done" means for v1

Sentinel v1 is complete when a reader can, on a laptop with no accounts and no money:

1. Clone the repository and run one command that brings up the full stack against a local validator and
   runs the entire test suite offline.
2. Read `docs/` and understand the ingestion model, the commitment/fork semantics, the data model, the
   execution state machine, and the threat model before reading any code.
3. Run a demo that drives an Aegis position from healthy → liquidatable → liquidated, and watch it flow
   through ingestion, decoding, materialization, candidate creation, simulation, submission,
   confirmation, and reconciliation — with the commitment label visible at every step.
4. Kill any worker at any point in that demo, restart it, and observe that the outcome is unchanged and
   nothing executed twice.
5. Delete every derived and protocol row, replay, and see byte-identical state.
6. Point at any platform claim and find the test, benchmark, failure-injection result, or ADR that
   substantiates it.
