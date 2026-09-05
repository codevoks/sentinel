# Phase 14 — Load and Performance Campaign

**Status: NOT STARTED.** **Prerequisite: Phase 12 complete and tagged (Phase 13 optional).**

> **Measure before optimizing.** This phase produces numbers first and changes code second, and only
> where a number justified it. Every optimization is documented as BEFORE / CHANGE / AFTER / DELTA /
> RISK.

## 1. Scope

1. The **benchmark harness** and a deterministic synthetic load generator (configurable block rate,
   configurable Aegis-transaction density) — **synthetic, seeded, and free**.
2. Baseline measurement of every metric in `performance-strategy.md` §2, with hardware, dataset,
   concurrency, and seed recorded.
3. **Setting the targets T1..T6** from the baseline, at the start of the phase, not guessed in advance.
4. Profiling: `cargo flamegraph`/`perf`, Node CPU profiles, and `EXPLAIN (ANALYZE, BUFFERS)` for every
   hot query.
5. **Testing the eight bottleneck hypotheses** (B-1..B-8) and recording which were right and which were
   wrong.
6. Selected optimizations, each justified by a measurement.
7. Committed baselines in `benchmarks/*.json` with a **CI regression gate**.
8. Closing **SR-4** (priority-fee distribution) from a primary source.

## 2. Explicit non-scope

**No speculative optimization.** No new features. No architectural change without an ADR. No Kafka, no
Kubernetes, no second datastore — unless a threshold in `performance-strategy.md` §6 was actually
crossed, and then via an ADR.

## 3. Evidence objective

- **Every performance claim the repository makes is backed by a committed measurement.**
- The bottleneck hypotheses are **falsified or confirmed by data**, and the wrong ones are recorded as
  wrong — which is the part most projects quietly omit.

## 4. Files

`benchmarks/*.json` · `tests/load/*` · `tools/loadgen/*` · `docs/performance-strategy.md` (results
appended)

## 5. Dependencies

Phases 1–12.

## 6. Implementation requirements — do not deviate

- **PF-1..PF-7 are absolute.** No claim without before/after; no "fast" without a number; no target
  moved to make it pass (that requires an ADR).
- Every result records **hardware, dataset, concurrency, and seed**. A number without them is not
  reproducible and therefore is not evidence.
- **≥5 runs**; report median, p95, and variance. A single run is an anecdote.
- Warm-up is discarded; cold start is measured and reported **separately**.
- **Attach the profile** to any optimization claim.
- A hypothesis that turns out **wrong is recorded as wrong**, with the data.
- The load generator is deterministic and free — no mainnet dependency.
- **B-5's mitigation (liquidation-price-bucketed incremental evaluation) is implemented only if B-5 is
  confirmed.** It is an attractive optimization and implementing it speculatively would violate PF-3.

## 7. Tests

- The load suite itself, reproducible from a seed.
- The regression gate: a deliberate performance regression is introduced and CI must fail.
- Every optimization keeps the full correctness suite green — **including RP-01..03 and FI-03..05**.
  An optimization that breaks determinism or crash safety is reverted, not accommodated.

## 8. Adversarial / failure cases

| Case | Asserted |
|---|---|
| Sustained load at 2× the reference block rate | Lag grows **visibly and boundedly**; memory plateaus; no unbounded queue |
| A firehose with the raw writer saturated (PG-7) | RSS plateaus; a gap is recorded; the scanner repairs it |
| Sustained load with a provider degraded | Priority classes hold; execution unaffected |
| Sustained load during a replay | Both complete correctly; live state remains correct |
| Sustained load with 10× the position count | Health evaluation rate measured; B-5 confirmed or falsified |
| Database under concurrent materialization from N workers | No deadlock (lock ordering); `lock_wait_p95` recorded |
| Partition exhaustion under load | Alert fires before failure |

## 9. Acceptance criteria

- [ ] Baselines measured and committed for every metric in `performance-strategy.md` §2
- [ ] **Targets T1..T6 set from the baseline** and recorded
- [ ] Gates PG-1..PG-9 evaluated; **any gate not met is recorded as not met, with the bottleneck named**
- [ ] All eight hypotheses B-1..B-8 tested; results recorded including the falsified ones
- [ ] Every optimization documented BEFORE / CHANGE / AFTER / DELTA / RISK with a profile attached
- [ ] The CI regression gate is proven by a deliberate regression
- [ ] The full correctness suite still passes after every optimization
- [ ] SR-4 closed from a primary source
- [ ] Universal checklist satisfied. Tag `phase-14-performance`.

## 10. Demo

Run the load generator with Grafana open; show lag, throughput, and memory under sustained pressure;
show a bounded queue rather than an unbounded one; show the regression gate failing on a deliberately
slowed build.

## 11. Documentation & status updates

`performance-strategy.md` gains a results section with the real numbers and the hypothesis outcomes.
`coverage-matrix.md` row 36 and row 53 become backed by committed data. `project-status.md`: the
measured figures with their conditions. **This is the first phase in which the repository may state a
performance number.**

## 12. Stop condition

**STOP after this phase.** Phase 15 has not been started.
