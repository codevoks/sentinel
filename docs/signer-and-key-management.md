# Sentinel — Signer Boundary and Key Management

**Status: FROZEN (Phase 0). Implementation in Phase 10, hardened in Phase 12.**

> **The rule everything else follows from:** the backend signs **only** transactions it constructed
> itself, from a typed intent, for an allowlisted instruction, over pinned accounts, within a value cap,
> after a successful simulation. There is no other path, and adding one requires an ADR that argues
> against this sentence.

---

## 1. Who signs what

| Actor | Holds | Signs | Where |
|---|---|---|---|
| **End user** | Their own key | Their own Aegis transactions (supply, borrow, repay, withdraw, deposit) | **In their wallet, in their browser.** Never server-side. |
| **Keeper authority** | A dedicated hot keypair with bounded funds | Liquidations, and operator-approved `absorb_bad_debt` / `accrue_interest` | The signer service |
| **Operator** | Their own key | Nothing on-chain in v1 | Operator actions create *intents*; the keeper authority signs them |
| **Sentinel API** | **Nothing** | **Nothing** | — |

**The backend never holds an end-user private key, seed phrase, or delegated signing authority
(NFR-9).** There is no import, no custody mode, and no "convenience" path. Requests to add one are
refused at the design level, not at the code-review level.

### 1.1 User transactions

Sentinel's role is limited to **construction and observation**:

```
POST /v1/tx/build     -> returns an UNSIGNED serialized transaction + a human-readable
                         description of every instruction and account it contains
(user signs in wallet)
POST /v1/tx/track     -> accepts a SIGNATURE (not bytes) and begins tracking it
```

| # | Rule |
|---|---|
| U-1 | `/tx/build` returns unsigned bytes. There is no endpoint that accepts a transaction and returns it signed. |
| U-2 | `/tx/track` accepts a **signature**, never transaction bytes. Sentinel does not relay user transactions. Relaying would mean accepting arbitrary bytes from the internet and broadcasting them under Sentinel's identity and rate limits. |
| U-3 | The build response includes a decoded description so the user's wallet is not the only place the transaction is legible. |
| U-4 | Build is a **pure function of typed parameters**. It never echoes caller-supplied instruction data or account lists into a message. |

---

## 2. Keeper key: blast radius

**Stated plainly, in the style Aegis uses for its own upgrade authority (T-30):**

> Whoever holds the keeper key can spend the SOL and the loan-asset inventory in that key's accounts.
> They cannot touch user funds, cannot touch Aegis vaults, and cannot alter protocol state — Aegis's
> account model makes that structurally impossible (`INV-ADM-01`, and liquidation is permissionless
> anyway). **The maximum loss from a fully compromised keeper key is the balance of the keeper's own
> accounts, plus the fee budget it can burn before a cap trips.**

That bound is the whole point of the design: the key is *hot* by necessity (it must sign
autonomously in seconds), so instead of pretending it is safe, its blast radius is made small,
enforced, and measurable.

| Control | Bound |
|---|---|
| Funding | Only what the configured budget requires. Excess inventory lives in a cold account and is topped up by an operator. |
| Program allowlist | Aegis + the pinned token programs + ComputeBudget only |
| Instruction allowlist | `liquidate`, and (operator-gated) `absorb_bad_debt`, `accrue_interest` |
| Value caps | Per transaction, per rolling window, per market |
| Environment isolation | A distinct key per environment. **A local key is never usable on any other cluster** — enforced by a cluster-genesis-hash check at signer startup. |
| Rotation | Documented procedure, exercised in Phase 12 |
| Revocation | Defund the key; every counterparty relationship is on-chain and permissionless, so there is nothing to revoke |

---

## 3. Signer service boundary

The signing key lives behind a narrow internal interface, not in the executor's address space:

```
sign(request: SignRequest) -> Result<Signature>

struct SignRequest {
    intent_id: Uuid,
    intent_kind: IntentKind,            // typed, not free-form
    message_bytes: Vec<u8>,             // the exact message to be signed
    decoded: DecodedMessage,            // program IDs, instruction discriminators, accounts, args
    simulation: SimulationResult,       // MUST be present and successful
    cluster_genesis_hash: Hash,
}
```

| # | Rule |
|---|---|
| SG-1 | The signer **re-decodes `message_bytes` itself** and verifies the decode matches `decoded`. It never trusts the caller's description of what it is signing. |
| SG-2 | The signer runs the full policy check (§4) **on the re-decoded message**, not on the caller's summary. |
| SG-3 | The signer refuses if `simulation` is absent, failed, or does not correspond to these exact bytes. |
| SG-4 | The signer refuses if `cluster_genesis_hash` does not match its configured cluster. **A devnet key cannot sign a mainnet message, ever.** |
| SG-5 | Every request and decision — allow or deny, with the reason — is written to an append-only audit log **before** the signature is returned. |
| SG-6 | The signer exposes **no** generic "sign these bytes" method. `SignRequest` is the only input shape and `intent_kind` is a closed enum. |
| SG-7 | The key material never leaves the signer process. It is not logged, not in an error, not in a metric label, not in a core dump path. |

In v1 the signer is a separate module with its own interface inside the keeper process. **Phase 12
moves it to a separate process** with the same interface, which is the point of specifying the boundary
now: making it a process later is a deployment change, not a redesign.

---

## 4. Policy engine

Runs on the **final, re-decoded message** immediately before signing. Every check is a hard denial;
there are no warnings.

| # | Check | Denies |
|---|---|---|
| P-1 | Every `program_id` is in the allowlist | Any unknown program |
| P-2 | Every instruction's discriminator is in the allowlist for its program | An unexpected Aegis instruction, e.g. an admin instruction |
| P-3 | Account pinning: `market` is a known market from `aegis_markets`; both vaults equal the values stored on that market; `position` is the canonical PDA of `(market, owner)`; `fee_position` is the canonical PDA of `(market, market.fee_recipient)`; token programs equal the market's pinned programs | Substituted market, vault, position, or token program |
| P-4 | The fee payer and every signer is the keeper authority | Any attempt to make another account a signer |
| P-5 | Destination accounts (where seized collateral goes) are in the keeper's own allowlisted account set | Redirection of proceeds |
| P-6 | `repay_assets ≤ per_tx_cap` and `≤ per_market_cap` | An oversized action |
| P-7 | Rolling window: cumulative repay value and cumulative fees within budget | Budget exhaustion / a fee-burn loop |
| P-8 | Priority fee ≤ ceiling; compute unit limit ≤ ceiling | A fee-drain bug |
| P-9 | The message contains **no** `SystemProgram::Transfer`, no token transfer outside the Aegis instruction's own CPIs, no `SetAuthority`, no `CloseAccount`, no `Assign` | Direct value movement, account takeover |
| P-10 | Instruction count and account count within expected bounds for the intent kind | An unexpectedly complex message |
| P-11 | `recent_blockhash` was obtained within a bounded window from a non-stale provider | A stale blockhash shortening the landing window invisibly |
| P-12 | The simulation attached to this request executed **these exact bytes** and succeeded | Simulate-one-thing-sign-another |

**A policy denial is `FATAL`, never retried** (`architecture.md` §9). It always alerts, and repeated
denials pause the keeper. Retrying a policy violation is how a bug becomes an incident.

P-9 deserves emphasis: it is the check that makes "the keeper cannot drain funds outside its role"
mechanical rather than aspirational. A compromised planner that tries to insert a transfer produces a
denial and an alert, not a loss.

---

## 5. Secret management

| # | Rule |
|---|---|
| SM-1 | **No secret in Git. Ever.** Keys, seeds, RPC tokens, database passwords, `.env` files. Enforced by `.gitignore` **and** a CI secret scanner — `.gitignore` alone is a hope. |
| SM-2 | Local development uses keypairs derived from **fixed seeds in code**, never committed files. An unshrinkable failure is worthless, and a committed test key eventually becomes a real key. |
| SM-3 | Non-local environments load the key from a secret manager or an injected file with `0600` permissions, referenced by path/URI in config, never inline. |
| SM-4 | Secrets are redacted in every log, error, metric label, span attribute, and stored provider ID. There is a test that logs a config object and asserts nothing sensitive appears. |
| SM-5 | The database role each service uses is distinct and least-privilege, **including locally** (`data-model.md` §10). A permission model that is only on in production is not a permission model. |
| SM-6 | If a secret is ever committed, it is treated as compromised: rotate, and say so. |

---

## 6. Audit log

```
signing_audit
  audit_id, requested_at, intent_id, intent_kind,
  message_hash, decoded_summary jsonb,
  policy_result enum: allowed | denied, denied_check text NULL,
  simulation_units int, simulation_success bool,
  signature bytea NULL,          -- present only when allowed
  cluster_genesis_hash, signer_identity
```

Append-only, retained indefinitely, **written before the signature is returned**. Every signature ever
produced is explainable: which intent, which policy checks passed, which simulation backed it.

---

## 7. API authorization

| Surface | Auth |
|---|---|
| Public read (markets, positions, oracle, health) | Unauthenticated, rate-limited by IP, cacheable |
| User-scoped read (my positions, my intents) | Wallet-signature auth: sign a nonce, receive a short-lived token. **No password, no email, no account.** |
| Operator actions (create an intent, pause the keeper, trigger a replay) | Separate operator credential, scoped, audited, never the same credential as read access |
| Internal (metrics, admin) | Not exposed publicly; bound to the internal interface |

| # | Rule |
|---|---|
| A-1 | **Authorization is enforced at the data-access layer**, not only at the route. A query for "positions" always carries the caller's scope. |
| A-2 | The wallet-auth nonce is single-use, short-lived, and bound to the origin, so a signature cannot be replayed. |
| A-3 | Operator actions create **typed intents with typed parameters**. There is no operator endpoint that accepts a transaction, an instruction, an account list, or SQL. |
| A-4 | Every operator action is audited with the actor identity. |
| A-5 | Rate limits apply to authenticated callers too. Authentication is not a bypass. |

---

## 8. Threats specific to signing

| ID | Threat | Mitigation | Test |
|---|---|---|---|
| SK-1 | API caller induces the backend to sign arbitrary bytes | No such path exists (§1, U-2); the signer takes typed intents only | `A-SIGN-01`: every API surface is enumerated and asserted to have no bytes-in-signature-out path |
| SK-2 | Compromised planner inserts a transfer into the message | P-9 denies it | `A-SIGN-02`: inject a `SystemProgram::Transfer`; signing must fail |
| SK-3 | Substituted vault or market account | P-3 pins both against `aegis_markets` | `A-SIGN-03` |
| SK-4 | Simulate one message, sign another | P-12 binds the simulation to the exact bytes | `A-SIGN-04` |
| SK-5 | Keeper key used on the wrong cluster | SG-4 genesis-hash check | `A-SIGN-05` |
| SK-6 | Fee-drain loop via unbounded retries | P-7/P-8 caps + intent fee ceiling + `max_attempts` | `A-SIGN-06` |
| SK-7 | Key material leaked via logs or errors | SM-4 redaction + a test that asserts it | `A-SIGN-07` |
| SK-8 | Over-sized liquidation drains inventory in one action | P-6 per-transaction and per-market caps | `A-SIGN-08` |
| SK-9 | Replay of a captured wallet-auth signature | A-2 single-use, origin-bound, short-lived nonce | `A-SIGN-09` |
| SK-10 | Operator credential used to trigger an economic action outside policy | Operator actions create intents that pass the **same** policy engine — there is no privileged bypass | `A-SIGN-10` |

**SK-10 is the important one:** operator privilege changes *what can be requested*, never *what can be
signed*. The policy engine has no bypass, for anyone.
