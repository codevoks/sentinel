# ADR-0011 — The backend signs only self-constructed, policy-checked transactions

**Status:** Accepted · **Date:** 2026-09-05 · **Phase:** 0

## Context

Sentinel must sign autonomously — a keeper that needs a human for each liquidation is not a keeper. It
also serves a public API. The combination is where backends get drained: an endpoint that accepts
transaction bytes, or a signer that trusts its caller's description of what it is signing.

## Decision

**The backend signs only transactions it constructed itself, from a typed intent, for an allowlisted
instruction, over pinned accounts, within a value cap, after a successful simulation of those exact
bytes.**

Four structural rules:

1. **No end-user keys, ever.** Users sign in their own wallet. `/tx/build` returns **unsigned** bytes;
   `/tx/track` accepts a **signature**, never bytes. There is no relay.
2. **The signer takes typed `SignRequest`s only**, with a closed `intent_kind` enum. There is no
   generic "sign these bytes" method.
3. **The signer re-decodes the message itself** and verifies it against the caller's description. It
   never trusts the caller's account of what it is signing.
4. **The policy engine has no bypass, for anyone** — including operators. Operator privilege changes
   what may be *requested*, never what may be *signed*.

Twelve policy checks (`signer-and-key-management.md` §4), of which three carry most of the weight:

- **P-3 account pinning** — market, vaults, position, fee position, and token programs all pinned
  against `aegis_markets` and canonical PDA derivation.
- **P-9 negative instruction check** — no `SystemProgram::Transfer`, no token transfer outside the Aegis
  instruction's own CPIs, no `SetAuthority`, no `CloseAccount`, no `Assign`. This is what makes "the
  keeper cannot drain funds outside its role" mechanical rather than aspirational.
- **P-12 simulation binding** — the attached simulation must have executed *these exact bytes*.

Plus a cluster **genesis-hash check**: a local key can never sign a message for another cluster.

## Blast radius, stated plainly

> A fully compromised keeper key can spend the SOL and loan-asset inventory in its own accounts. It
> cannot touch user funds, cannot touch Aegis vaults, and cannot alter protocol state — Aegis's account
> model makes that structurally impossible. **The maximum loss is the keeper's own balance, plus the
> fees it can burn before a cap trips.**

That bound is the design. The key is hot by necessity, so instead of pretending it is safe, its blast
radius is made small, enforced, and measurable. This is Sentinel's analogue of Aegis's T-30 — with the
difference that Sentinel's worst case is bounded and Aegis's is not.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **A generic signing endpoint** | The single worst outcome in the system (S-10). Any caller who reaches it signs anything. |
| **Relaying user-signed transactions** | Accepting arbitrary bytes from the internet and broadcasting them under Sentinel's identity, rate limits, and reputation. No product need — wallets broadcast. |
| **Custodial user keys "for convenience"** | Turns an observer into a custodian, with a regulatory and security surface nothing in `product.md` justifies. |
| **Trusting the planner's description in `SignRequest`** | A compromised or buggy planner then signs anything. Re-decoding costs microseconds and removes the entire class. |
| **Policy checks in the planner rather than the signer** | Checks must run at the last point before the irreversible action, on the final bytes. Anywhere earlier is bypassable by a later mutation. |
| **Simulation optional for latency** | Simulation is the only pre-flight proof the precondition still holds and the only source of a real CU figure. Skipping it trades a bounded latency saving for an unbounded correctness risk. |
| **An HSM / remote signer in v1** | Correct direction, disproportionate now. The **interface** is specified so that moving the signer to its own process (Phase 12) or to an HSM later is a deployment change, not a redesign. |
| **An operator override for policy** | An override is a bypass with a nicer name. If an operator needs something policy forbids, the policy is wrong and changing it is a reviewed change. |

## Consequences

**Positive**
- No path exists from the public API to a signature. Asserted by an enumerating test, not by review.
- A compromised planner produces a denial and an alert, not a loss.
- Every signature ever produced is explainable from the audit log: which intent, which checks, which
  simulation.
- The key's worst case is a number an operator chose.

**Negative**
- Policy false positives block legitimate actions. **Deliberate**: a denial costs availability, a
  bypass costs money. Denials alert, so a false positive is visible immediately.
- Re-decoding in the signer duplicates work. Microseconds, for the removal of a whole trust assumption.
- The keeper needs a hot key. **Accepted and stated** (S-12), with the blast radius bounded above.

**Enforcement**
- `A-SIGN-01` enumerates every route and asserts no bytes-in/signature-out path exists.
- `A-SIGN-02` injects a transfer into a planned message; signing must fail.
- `A-SIGN-04` simulates one message and signs another; must fail.
- KP-12 attempts a plain value transfer with the keeper key; must fail.
- Policy denials are `FATAL` and never retried (`architecture.md` §9).
