# ADR-0013 — Zero-cost, local-first architecture

**Status:** Accepted · **Date:** 2026-09-05 · **Phase:** 0

## Context

An indexing and execution platform is the *easiest* kind of project to build against a hosted RPC
provider and a hosted streaming endpoint. It is the path of least resistance, and it quietly makes the
repository unusable as evidence.

The constraint binds harder on Sentinel than on Aegis: Aegis is a program that can be tested in an
in-process SVM, whereas Sentinel's entire subject is talking to a network.

## Decision

**Every required test, the full demo, and the UI must run with no network beyond loopback, no secrets,
no API key, no faucet, and no paid service.**

Enforced by:
- **CI runs with no secrets configured.** A required test needing one fails the build.
- **A no-network CI job** runs `make test` with the interface down except loopback.
- **No default configuration value points at any non-loopback host.**
- Network- and provider-dependent work is an **optional, tagged tier** excluded from `make test`.

## Why this is architectural, not a convenience

1. **Determinism.** Forks, gaps, dropped notifications, stale providers, oracle staleness, and
   liquidation races are nearly impossible to produce on demand against a public cluster and trivial to
   produce locally. **The entire Phase 12 failure-injection campaign exists because of this
   constraint.** Without it, Sentinel would have a chapter of documentation about reorgs and no test
   that had ever seen one.
2. **Reviewability.** Anyone can clone and reproduce every claim in minutes, with no account and no
   spend. A platform whose evidence cannot be independently reproduced is an assertion.
3. **Honest dependency accounting.** If the required path cannot run without a paid endpoint, then the
   paid endpoint is part of the architecture and belongs in the diagram. Keeping it out of the required
   path keeps the diagram true.

## How each dependency is eliminated

| Dependency | Solution |
|---|---|
| A Solana cluster | **Surfpool** in pure local mode — full JSON-RPC + WebSocket plus `surfnet_*` cheatcodes |
| SOL and tokens | Cheatcodes / local mint authority. No faucet. |
| **Oracle prices** | **Byte-exact Pyth `PriceUpdateV2` account injection**, exactly as Aegis's test kit does. Reading a pull price is an **account read, not a CPI**, so the Pyth program need not be deployed. Aegis's ADR-0008 removes the dependency that usually forces a project onto a network — and Sentinel inherits it directly. |
| Time | Clock warping — a year of accrual in microseconds |
| Historical data | The committed fixture corpus, captured from a scripted local scenario |
| **Forks** | **Constructed deliberately by the fixture harness**, not waited for |
| Streaming | Optional, self-hosted (ADR-0006) |
| Swap liquidity | Not required — the v1 keeper is pre-funded |
| Telemetry backend | Local OTel collector + Prometheus + Grafana in Compose |

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **Develop against devnet with a hosted RPC** | Flaky, non-deterministic, cannot produce adversarial conditions on demand, and requires an account. Every failure-injection claim would be untestable. |
| **A paid RPC in CI with secrets** | CI would depend on a credential, so a fork could not run the suite, and "runs offline" would be false. |
| **A hosted Geyser endpoint for realtime tests** | Same, plus a subscription cost. |
| **A recorded HTTP cassette of a real cluster** | Useful for some unit tests and used for the corrupt-input corpus, but it cannot exercise reconnects, forks, or execution. A real local RPC server can. |
| **Mock the RPC layer entirely in tests** | Then the tests exercise the mock, not the client. The most-used code becomes the least-tested — precisely the reasoning Aegis used to reject a mock oracle (its ADR-0008). |
| **Allow "just one" network-dependent required test** | The camel's nose. One becomes several, and the offline claim quietly dies. The tier boundary is binary. |

## Consequences

**Positive**
- Adversarial scenarios are cheap, which is why there can be one per threat and one per failure mode.
- Deterministic failures; a seed reproduces any of them.
- No cost or credential barrier for any reviewer.
- The demo is runnable by anyone in minutes.

**Negative**
- **Local testing cannot catch cluster-specific behavior**: real congestion, real fee markets, real
  provider quirks, real block composition, real competition. Acknowledged in `zero-cost-local.md` §7;
  the optional tier and a devnet soak partially cover it.
- Fixture builders must track real formats. **This is a feature** — a format change fails loudly rather
  than silently producing wrong data.
- Surfpool's fidelity to a real validator is a dependency. SR-7 (blocking, Phase 1) verifies it exposes
  every method Sentinel requires; if it does not, that is an architectural finding to surface, not to
  work around.

**Enforcement**
- CI with no secrets; a dedicated no-network job.
- `zero-cost-local.md` §6 lists the specific anti-patterns that erode this without anyone noticing, each
  with a guard.
- If a README claim depends on the optional tier, either the claim is wrong or the tier is
  misclassified.
