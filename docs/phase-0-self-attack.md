# Sentinel — Phase 0 Self-Attack

**Performed before declaring Phase 0 complete. Every answer is recorded, including where it forced a
change to the design.**

The purpose is not to praise the architecture. It is to find the places where it is wrong, fix the
material ones, and state the residual ones plainly.

---

## 1. The attack

| # | Question | Answer |
|---|---|---|
| 1 | **Where can duplicate observations cause duplicate effects?** | Enumerated per path in `distributed-correctness.md` §2.3. Every path is closed by a database constraint, not by discipline. The load-bearing rule: **materialization is a fold over an idempotent event set at a pinned anchor, never a sequence of increments.** Increment-based materialization is duplicate-sensitive by construction and is banned. |
| 2 | **What if Sentinel crashes after submitting but before recording the signature?** | **That state is unreachable by construction.** The signature *and the exact signed bytes* are committed **before** `sendTransaction` (`transaction-engine.md` §6). The worst reachable state is "signed, possibly broadcast", whose recovery is to resubmit the **stored bytes** — an identical signature, therefore a no-op on-chain. Storing bytes rather than only the signature is what makes this work; re-signing would produce a new signature and a genuine duplicate. Tested at the exact boundary by FI-03/04/05 in the **required** CI tier. |
| 3 | **What if two providers disagree?** | Both payloads persist — `payload_hash` is part of the raw uniqueness key. Divergence is then classified: transient lag (counted), fork divergence (finality decides), **content divergence (breaker trips immediately, alert, provider excluded)**. Sentinel never votes, averages, or picks a majority. |
| 4 | **What if WebSocket events are lost?** | Assumed, not feared. Agave's PubSub drops under load and loses subscriptions on reconnect. Completeness comes from `getBlocks` range reconciliation; **every** transition into a live socket triggers a gap scan; gaps are recorded, repaired, and alert if unrepaired. *The WebSocket tells Sentinel when to look; HTTP tells Sentinel what is true.* |
| 5 | **What if backfill and realtime overlap?** | A no-op by construction: identical natural keys, `ON CONFLICT DO NOTHING`, one shared code path. Backfill never advances the contiguity watermark. FI-15 proves zero duplicates. |
| 6 | **How does a fork affect derived risk state?** | Rollback marks (never deletes), records the event, invalidates derived rows for affected entities, and enqueues a **scoped** recompute that **rebuilds forward from the last finalized anchor**. It never subtracts or inverts events. Affected entities are visibly `RECOMPUTING`. FI-16 at depths 1/5/30; RP-06 proves the result equals a corpus where the abandoned blocks were never observed. |
| 7 | **Can replay reproduce state deterministically?** | Yes, and it is verified by a single-value digest comparison rather than by inspection. Five determinism rules (D-1..D-5); twelve acceptance criteria (RP-01..RP-12); **RP-01/02/03 run on every commit.** One leak was found and closed — see §2, finding E. |
| 8 | **Can a malformed transaction crash a parser worker?** | No, and this is enforced rather than intended: no `unwrap`/`expect`/`panic!`/unchecked indexing on external-input paths (clippy-enforced), every decode fallible, size bounds before parsing, and a **committed corrupt-fixture corpus** in CI (FI-14). Every new crash class becomes a permanent fixture. |
| 9 | **Can one bad job poison the pipeline?** | No. Bounded attempts → `quarantined` (never deleted, never auto-retried, alerts). Quarantine isolates one unit of work; other jobs of the same kind keep flowing. FI-20. |
| 10 | **Could Sentinel execute an economically sensitive action twice?** | Four independent defenses: the `idempotency_key` unique index; one non-terminal attempt per intent (partial unique index); the sign→persist→submit ordering; and **the chain itself**, since Aegis rejects liquidating a healthy position. Two of the four are database constraints. The duplicate-execution fuzz (Phase 12) attacks this directly with randomized kill points. |
| 11 | **Could the keeper use stale state?** | Bounded three ways: every candidate carries `detected_at_slot` and the executor refuses beyond `max_staleness`; state is **re-read and re-evaluated at claim time**; and simulation runs immediately before signing. Indexer lag beyond threshold **disables candidate creation entirely** — a lagging keeper is worse than no keeper. |
| 12 | **Could a compromised API caller make the backend sign an arbitrary transaction?** | **No path exists.** `/tx/build` returns unsigned bytes; `/tx/track` accepts a signature, not bytes; the signer accepts only typed `SignRequest`s with a closed enum and **re-decodes the message itself**. `A-SIGN-01` enumerates every route and asserts it. Adding such a path requires an ADR arguing against ADR-0011. |
| 13 | **Could the keeper key drain funds outside its role?** | Policy check **P-9** rejects any message containing `SystemProgram::Transfer`, a token transfer outside the Aegis instruction's own CPIs, `SetAuthority`, `CloseAccount`, or `Assign`. Plus program/instruction allowlists, account pinning, value caps, and a cluster genesis-hash bind. **Blast radius is stated plainly: the keeper's own balance.** KP-12 attempts the drain and must fail. |
| 14 | **What happens when Aegis upgrades?** | The BPF-loader `ProgramData` hash is watched — the strongest signal, and available *before* any decode fails. On change: **keeper pauses immediately**, in-flight intents resolve or expire, a version boundary is recorded, an alert fires. Decoding continues under the existing version until a boundary is confirmed. |
| 15 | **How are old records decoded after a schema change?** | They are not re-decoded in place — ever. A new decoder version is registered with a slot range and the affected range is **replayed**. Old rows keep their `decoder_version_id`, and the raw bytes they came from are still present. This is the payoff for ADR-0008. |
| 16 | **What if Sentinel says liquidatable and Aegis rejects?** | Classified before anyone is paged, using Aegis's banded error codes. Four classes are normal operation of a permissionless market and produce **no alert**. `LOOKAHEAD_OVERSHOOT` is a tuning signal. Only `SIZE_REJECTED`, `MODEL_DIVERGENCE`, `ACCOUNT_REJECTED`, `UNKNOWN` pause the keeper. This taxonomy was **materially improved by the attack** — see §2, finding B. |
| 17 | **Is any component present purely for resume coverage?** | Examined each. **Kafka, Kubernetes, Jito, Jupiter, durable nonces, ALTs, and a second datastore are all rejected**, each with an ADR and a measured adoption threshold. Geyser survives only as an optional Phase 13 with a measurement trigger. Redis survives in three narrow roles with a working no-Redis fallback. The honest remaining answer: **Geyser is the component most at risk of being built for the wrong reason**, which is exactly why its trigger is a number and not a judgement. |
| 18 | **Is Redis actually necessary?** | **For correctness, no — and that is stated rather than rationalized.** It has three ephemeral roles, all with working fallbacks, and FI-13 removes it entirely and asserts no correctness behavior and no response body changes. If cross-instance fanout were dropped as a requirement, Redis could be removed outright. |
| 19 | **Is a queue actually necessary?** | A queue yes; **a broker no**. Transactional enqueue — creating a candidate and its intent in one transaction — is a property no external broker gives for free, and it is precisely the property that prevents lost and duplicated work in the money path. The Kafka threshold is stated as three specific measurements. |
| 20 | **Is Geyser actually necessary?** | **No.** Aegis is one program with a small bounded account set; this is not a high-cardinality indexing problem. Geyser is deferred to Phase 13 behind a measured trigger, and the coverage matrix records it as NOT COVERED as production until that trigger fires. |
| 21 | **Is Rust used where it matters rather than ceremonially?** | Yes, and the strongest evidence is not throughput — it is that **`aegis-math` is a Rust crate**, `no_std` and float-free by Aegis's own policy, which makes it directly linkable and is the single best anti-drift mechanism available. Ingestion, parsing untrusted bytes under a firehose, and the chain-state engine are the other genuine cases. |
| 22 | **Is TypeScript used where ecosystem velocity matters?** | Yes, and for one decisive reason: **`@aegis/sdk` is TypeScript**, so the keeper's `liquidate` instruction is byte-identical to the protocol's own builder. Reimplementing that in Rust would create a second source of truth for the most dangerous instruction in the protocol. The API is the secondary case. |
| 23 | **Is every claimed skill backed by future observable evidence?** | `coverage-matrix.md` names the exact artifact per row, and §4 lists what would make each claim false. Rows with no artifact are classified ADR-not-built or NOT COVERED, not quietly implied. |
| 24 | **Can a future implementation model execute the phases without redesigning?** | The economics, keys, state machines, transitions, policy checks, and acceptance criteria are specified to field and formula level, and `implementation-handoff.md` §2 states exactly where flexibility is allowed. **The residual risk is not ambiguity — it is the upstream block**: Phases 7, 8 and 11 depend on an Aegis that is itself at Phase 0. Phases 1–6 were deliberately ordered to have zero upstream dependency so that work is never blocked on someone else's schedule. |

---

## 2. Material problems found, and the changes they forced

The attack was worth running: it found five real defects, three of which would have produced incorrect
behavior in production.

### A — The roadmap would have built a throwaway sink

The provisional sequence put "basic ingestion" **before** "raw durable observation boundary +
checkpoints". Ingestion would have written somewhere else and then had its most load-bearing component
replaced one phase later.

**Fix:** merged into Phase 4. **Ingestion writes to the raw boundary from its first line of code.**
`phase-roadmap.md` §2.1 records why.

### B — A mis-tuned lookahead would have auto-paused the keeper for a non-bug

Sentinel evaluates health at `t_eval = now + expected_landing_latency`, which is correct. But an
over-estimated lookahead makes Sentinel predict liquidatability slightly *before* the chain agrees. In
the original taxonomy, every such rejection landed in `MODEL_DIVERGENCE` — which **pauses the keeper**.
A tuning error would have repeatedly disabled the automation and paged an operator about a health-engine
bug that did not exist.

**Fix:** the classifier now **recomputes health at the observed state** before deciding. If that HF is
also `< WAD`, the model genuinely disagrees (`MODEL_DIVERGENCE`, pause). If it is `≥ WAD`, the model was
right and only the lookahead is long (`LOOKAHEAD_OVERSHOOT`, tuning signal, **no pause**).
`aegis-integration.md` §12, `keeper-design.md` K-11b.

### C — The idempotency key would have suppressed a legitimate follow-up liquidation

Aegis liquidation is **partial by default** (`close_factor`). A position can remain liquidatable
immediately after a successful liquidation. A bucket-only idempotency key would have blocked the
legitimate follow-up for up to a full bucket, leaving the position under-liquidated — which in a real
market is exactly when the follow-up matters most.

**Fix:** a second key form, `liquidate:{program}:{market}:{position}:after:{prev_signature}`, used
**only** when the previous intent reached `SUCCEEDED` with a **finalized** attempt. The previous
signature is a settled on-chain fact, so two workers compute the same key independently — deterministic
and still duplicate-proof. `data-model.md` §7.

### D — The summation invariants would have produced false pages

`INV-CUS-02`, `INV-ACC-01`, and `INV-ACC-02` are sums over **every** position in a market. A single
position Sentinel never observed — a missed event, a history gap — makes the sum short and the invariant
appear violated. Sentinel would have paged an operator about an **Aegis accounting bug that did not
exist**, which is the fastest way to make an alert ignored.

**Fix:** summation invariants are evaluated **only when the market's position set is verified complete
at that slot**, cross-checked between the event-derived set and a recent `getProgramAccounts` scan. If
the sets disagree, the invariants are skipped and the **set divergence itself** alerts, classified as a
Sentinel history gap. Point-wise invariants are unaffected. `aegis-integration.md` §7.2.

### E — A live measurement leaked into the deterministic replay path

`t_eval` derives from a *measured* `expected_landing_latency`. Determinism rule D-3 forbids ambient
configuration in a deterministic processor, and this violated it: a replay would re-measure and produce
different candidates, silently breaking RP-01 in a way that would look like a flaky test rather than a
design flaw.

**Fix:** `lookahead_ms` and `risk_params_hash` are persisted on the candidate row (and `t_eval` on the
health row), and **replay reads them back rather than re-measuring**. D-3 now names this explicitly as
the one place a live measurement touches a deterministic processor, and closes it.
`data-model.md` §6, `replay-and-backfill.md` §2.

### F — The Geyser equivalence criterion was unsatisfiable by a correct implementation

GS-02 originally required Geyser and RPC to produce *identical normalized rows*. Geyser reports
`write_version` and can legitimately emit more intermediate account observations than RPC does, so a
correct implementation would have failed the test — and the likely reaction would have been to weaken
the criterion rather than to understand it.

**Fix:** GS-02 compares transactions, instructions, logs, token deltas, protocol entities, and derived
state by digest, and compares `account_observations` by **final decoded state per `(pubkey, slot)`**,
which is the property that actually matters. `geyser-strategy.md` §8.

---

## 3. Residual risks, accepted and stated

Not everything found was fixable. These are recorded so no reader has to infer them.

1. **Sentinel is blocked upstream** at Phases 7, 8, and 11 on an Aegis that is itself at Phase 0.
   Mitigated by ordering Phases 1–6 to have zero upstream dependency, and by an interim conformance path
   built on Aegis's already-frozen worked examples. **Phase 11 has no acceptable workaround and will
   wait.**
2. **Single-provider deployments cannot detect content divergence** (S-01). Multi-provider is
   configuration; the degradation is documented rather than assumed away.
3. **The keeper needs a hot key** (S-12). Bounded to its own balance by policy and caps, not by trust.
4. **Postgres is a single point of availability** (ADR-0002). It is also a single point of durability;
   nothing is lost, only availability. Kubernetes would not fix this, which is why ADR-0014 names the
   database as the binding constraint.
5. **The indexer singleton is a throughput ceiling.** The sharding seam exists (disjoint leased ranges,
   already used by backfill) and the trigger is a measurement (B-4), not a guess.
6. **Local testing cannot catch cluster-specific behavior** — real congestion, real fee markets, real
   competition, real provider quirks. Named in `zero-cost-local.md` §7 so no local result is
   over-claimed.
7. **Alpenglow lands during this project's timeline** (activation from 2026-09-28) and SR-2 is open.
   Finality timing is configuration everywhere, but a semantic change to `confirmed` would be a design
   event. Blocking for Phase 6, re-checked in Phase 12.
8. **Transaction v1 activation timing is unknown** (SR-1). Sentinel targets v0 for building and must
   decode v1 from activation. The specific hazard — a `ComputeBudget` scan silently returning a zero
   priority fee — is called out in three documents and has a CI guard, because it fails quietly.
9. **The slippage haircut in the profitability model is a parameter, not a measurement.** Honest about
   being one, and measured against realized outcomes over time.
10. **Documentation outrunning implementation** is the single most likely way this repository fails. It
    is the first row of the gap analysis, and `project-status.md`'s five-state model exists specifically
    to make it visible.

---

## 4. The question the attack could not settle

**Is the whole platform justified, or is it an elaborate answer to a problem Aegis does not have?**

Stated honestly: for Aegis at zero TVL on a local validator, a `getProgramAccounts` poll and a simple
bot would work. Sentinel's architecture is justified by the *class* of problem — an off-chain observer
and executor that must be correct under forks, crashes, and duplicate delivery — not by Aegis's current
volume.

That is a real answer, not a dodge, and it is why the coverage matrix marks performance work as
deferred, the adoption thresholds are measurements rather than preferences, and the roadmap cuts Phase
13 and 15 before Phase 12. **The engineering is calibrated to the failure modes, not to the traffic** —
and where a component could only be justified by traffic that does not exist, it was rejected with an
ADR instead of built.
