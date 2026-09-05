# Sentinel — Testing Strategy

**Status: FROZEN (Phase 0). Built progressively; campaign in Phase 12.**

> **A mocked happy path is not distributed-systems evidence.** The tests that make this repository
> credible are the ones that produce a failure deliberately and assert what the system did about it.

---

## 1. The six tiers

| Tier | What it covers | Runtime target | Runs |
|---|---|---|---|
| **T1 — Unit** | Pure functions: parsers, codecs, state-machine transition tables, retry/backoff, key derivation, health arithmetic | seconds | every commit |
| **T2 — Property** | Idempotency, determinism, monotonicity, state-machine legality, bounds | < 2 min | every commit |
| **T3 — Integration** | Real Postgres + real local validator (Surfpool) + real Sentinel components | < 10 min | every commit |
| **T4 — Protocol conformance** | Sentinel's Aegis understanding against Aegis's frozen numbers and, when available, its artifacts | < 5 min | every commit |
| **T5 — Failure injection** | Deliberate crashes, drops, duplicates, reorders, forks, corruption | < 30 min | nightly + before every tag; **crash-boundary subset is required on every commit** |
| **T6 — Load / performance** | Throughput, latency, backlog behavior under sustained load | minutes–hours | on demand + before a release |

**Every tier runs offline with no secrets** (NFR-4). CI runs with no secrets configured, so a required
test that needs one fails the build.

---

## 2. T1 — Unit

The things that are cheap to test exhaustively and expensive to get wrong:

| Area | Tests |
|---|---|
| Transaction decoding | legacy / v0 / v1 messages; `transactionConfig` bit combinations; priority-fee extraction per version; ALT-referencing v0; malformed and truncated inputs |
| Instruction/log parsing | inner-instruction nesting, stack heights, invoke/success framing, base64 `Program data:` extraction |
| Aegis account decoding | every field of `Protocol` / `Market` / `Position`; boundary values; `_reserved` non-zero rejection; wrong discriminator; wrong length |
| PDA derivation | every Aegis seed set, cross-checked against a shared fixture file the TypeScript side also asserts |
| Oracle validation | O-1..O-11 individually violated; boundary at exactly `max_price_age_secs` and exactly `max_conf_bps`; `expo` extremes; the `MIN/MAX_PRICE_WAD` bounds |
| Health arithmetic | the §6.2 valuation directions; `HF == WAD` vs `WAD − 1`; zero debt; zero collateral |
| Liquidation sizing | close factor, `full_liq_hf`, dust rule, collateral clamp with upward-rounded repay |
| State machines | every legal transition; every illegal transition rejected |
| Retry/backoff | bounds, jitter distribution, non-retryable fast-fail |
| Natural keys | key construction is total and collision-free for the domain |
| Error classification | every error maps to exactly one class |

**Rule:** unit tests assert **specific** errors, never "it failed". A test that asserts only failure is
not testing the check it claims to test — inherited directly from Aegis's `AGENTS.md` §9.

---

## 3. T2 — Property

| ID | Property |
|---|---|
| `P-IDEM-1` | Applying any observation sequence twice equals applying it once |
| `P-IDEM-2` | Applying an observation set in any order yields the same final state (within an ordering domain) |
| `P-IDEM-3` | Duplicating any subset of observations changes nothing |
| `P-DET-1` | Replay of the same raw rows yields the same `output_digest` |
| `P-DET-2` | Range-splitting and shuffling the replay yields the same digest |
| `P-MONO-1` | Commitment never decreases except into `abandoned` |
| `P-MONO-2` | `last_contiguous_slot` never decreases |
| `P-MONO-3` | An intent never leaves a terminal state |
| `P-SM-1` | No sequence of events drives an intent or attempt into an illegal state |
| `P-SM-2` | Every attempt terminates: no reachable state loops forever without an external event |
| `P-BOUND-1` | Retry sequences are bounded in count and total time under every generated error pattern |
| `P-BOUND-2` | Per-intent cumulative fees never exceed the ceiling |
| `P-KEY-1` | Distinct logical observations never collide on a natural key |
| `P-TIME-1` | No deterministic processor's output depends on wall-clock time |
| `P-HEALTH-1` | Health is monotone in collateral amount and in collateral price |
| `P-HEALTH-2` | Health is monotone decreasing in debt |
| `P-SIZE-1` | Computed seizure never exceeds position collateral |
| `P-SIZE-2` | Computed repayment never exceeds outstanding debt |

Generators are **biased toward the dangerous region**: near-`WAD` health factors, empty and maximal
markets, dust amounts, `u64`/`u128` extremes, decimals pairs across `0..=12`, and slots at fork
boundaries. Random uniform values almost never find real bugs.

**All fixtures use fixed seeds.** An unshrinkable failure is worthless.

---

## 4. T3 — Integration

Real components. Real Postgres. Real local validator.

| Scenario | Asserts |
|---|---|
| Cold start against a fresh cluster | Slots ingested, contiguity established, checkpoint advances |
| Ingest a scripted Aegis lifecycle | Every event decoded, materialization matches account snapshots |
| Gap creation and repair | A deliberately skipped range is detected, backfilled, contiguity restored |
| Commitment promotion | Slots progress `confirmed → finalized`; derived rows promote with them |
| Snapshot vs projection reconciliation | Divergence detection fires when a snapshot is withheld and events are dropped |
| Full replay | `TRUNCATE` protocol+derived, replay, digest matches |
| API contract | Every endpoint's response shape, pagination, commitment metadata, error model |
| WebSocket resume | Disconnect, reconnect with a cursor, no gap and no duplicate delivered |
| Transaction lifecycle | Build → simulate → sign → persist → submit → confirmed → finalized, against the local cluster |
| Multi-worker | Concurrent workers over one queue produce no duplicate work |

**The local cluster is Surfpool** — a full JSON-RPC surface plus cheatcodes to set accounts, mint
tokens, warp the clock, and pause it. Byte-exact Pyth accounts are injected exactly as Aegis's own test
kit does (`aegis/oracle-design.md` §5), which is what makes the oracle path testable offline.

---

## 5. T4 — Protocol conformance

The tests that prove Sentinel understands Aegis rather than approximating it.

| ID | Test |
|---|---|
| `AEGIS-CONF-01..06` | Aegis's exact frozen worked examples (`aegis-integration.md` §6.2) |
| `AEGIS-PDA-01` | Every Aegis PDA derivation matches a shared vector file, asserted in **both** Rust and TypeScript |
| `AEGIS-LAYOUT-01` | Decoded account sizes match the documented sizes; `_reserved` is zero |
| `AEGIS-EVENT-01` | Every event in the catalogue decodes with its documented fields |
| `AEGIS-INV-01..08` | The off-chain-checkable Aegis invariants hold across the fixture corpus at finalized slots |
| `AEGIS-VER-01` | An unknown discriminator produces `UNKNOWN_SCHEMA`, marks the entity stale, and alerts — **never a guess** |
| `AEGIS-VER-02` | Registering a new decoder version and replaying produces correctly-tagged rows and mutates no old row |
| `AEGIS-ORACLE-01..11` | Sentinel's O-1..O-11 implementation matches Aegis's semantics check by check |

**When `aegis-math` exists**, `AEGIS-CONF-*` additionally asserts Sentinel's results equal the crate's
directly, and any Sentinel-side reimplementation is deleted. **When Aegis publishes
`tests/vectors/*.json`**, those become an additional required input — Sentinel consumes the protocol's
own cross-language vectors rather than inventing its own.

---

## 6. T5 — Failure injection

The 28-entry catalogue in `distributed-correctness.md` §10, plus the corrupt-input corpus.

| Mechanism | How |
|---|---|
| Process kill | A supervisor kills workers at **randomized, seeded** points; the seed is recorded so a failure is reproducible |
| Crash-boundary kill | Deterministic kill points at the exact sign/persist/submit boundaries (FI-03..05) — **required tier, every commit** |
| Network fault | `FaultInjectingProvider` implements `RpcProvider` and produces timeouts, 429s, 5xx, stale `context.slot`, malformed bodies, and content divergence |
| Duplicate/reorder | A stream wrapper that duplicates and reorders notifications within a window |
| Fork | The fixture harness constructs forks of stated depth deterministically — **not** hoped for from a real cluster |
| Corruption | The committed corrupt-payload corpus: truncated, invalid UTF-8, unknown enum variants, unknown discriminators, oversized |
| Infrastructure | Postgres restarted mid-pipeline; Redis removed entirely; disk-full simulation on the raw writer |

**Rule:** a failure-injection test asserts a **specific** recovery outcome — a row count, a digest, an
alert raised, a state reached. "It did not crash" is not an assertion.

---

## 7. T6 — Load and performance

Methodology in `performance-strategy.md`. **No numbers are stated in Phase 0.** The harness produces
them; the roadmap gates on them.

---

## 8. What is deliberately not tested, and why

| Not tested | Reason |
|---|---|
| Real mainnet behavior under congestion | Cannot be produced on demand or for free. Partially covered by an optional network-tagged tier. |
| Hosted-provider-specific quirks | Would require paid accounts. Covered by capability discovery and by treating every provider as untrusted. |
| Aegis's own on-chain correctness | That is Aegis's test suite. Sentinel tests its **interpretation** of Aegis, not Aegis. |
| Real DEX slippage | No swap integration in v1 (`product.md` §3). The slippage haircut is a parameter, measured against realized outcomes over time. |
| Byzantine RPC collusion | Multiple providers colluding to return identical wrong data. Out of scope; stated in S-01's residual. |
| Browser/UI end-to-end flows | The UI is functional, secondary, and covered by API contract tests plus a smoke test. |

**Stating these is the point.** An untested area that is named is a known limitation; an untested area
that is not named is a false claim.

---

## 9. CI gates

Every commit:

- `cargo test --workspace` (T1–T4), `cargo clippy -- -D warnings`, `cargo fmt --check`
- TypeScript: type-check, lint, unit and contract tests
- Integration tier against ephemeral Postgres + Surfpool
- Replay determinism RP-01, RP-02, RP-03
- Crash-boundary failure injection FI-03, FI-04, FI-05
- Migration up-from-empty and up-from-previous-release
- Secret scan; dependency advisory scan
- **CI grep guards** (each blocking):

| Guard | Bans |
|---|---|
| `CI-NOFLOAT` | `f32`/`f64` in any risk, decode, or money path |
| `CI-NOSLOTTIME` | Durations derived from slot counts |
| `CI-NOPANIC` | `unwrap`/`expect`/`panic!` on external-input paths |
| `CI-NOSQLFMT` | String-formatted SQL |
| `CI-NORAWCLIENT` | Direct Solana client construction outside `sentinel-rpc` |
| `CI-NOAEGISLEAK` | Any Aegis concept inside `sentinel-normalize` |
| `CI-NOMATHDUP` | Economic arithmetic implemented inside `sentinel-risk` rather than called |
| `CI-NOMAXVER` | `getBlock`/`getTransaction` without `maxSupportedTransactionVersion` |
| `CI-NOSECRET` | Key material, tokens, `.env` content |

Nightly: full T5 and the load smoke test.
Before a tag: T5 in full, T6, and the mutation check.

---

## 10. Traceability

Every threat (`S-nn`), invariant (`ING-*`, `CHN-*`, `TX-*`, `DM-*`, `RPC-*`, `DC-I-*`), and acceptance
criterion (`RP-*`, `KP-*`, `GS-*`) names at least one test ID. **A build-enforced traceability check
fails CI when a referenced test ID does not exist** — the same mechanism Aegis uses for its 87
invariants, and for the same reason.

**Enforcement rule:** *a mitigation without a falsifying test is a hope, not a mitigation.* Phase
completion requires the phase's tests to exist, to **fail when the mechanism is deliberately removed**,
and to pass when it is restored. A test that passes both ways is testing nothing.
