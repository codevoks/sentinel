# Sentinel — API and Realtime Surface

**Status: FROZEN (Phase 0). Implementation in Phase 9.**

> **Two rules govern everything below.** First: **no API request causes an RPC call** — the API reads
> Postgres only (S-18). Second: **every chain-derived field carries its commitment and its staleness**;
> a value without them is a bug, not an omission.

---

## 1. Shape

| Aspect | Decision |
|---|---|
| Protocol | REST over HTTP/JSON for reads and actions; WebSocket for realtime |
| Why not GraphQL | Query-complexity control and caching become the product. A small, explicit set of resources serves real consumers better and cannot be abused into an expensive query. Revisit only with a consumer whose access pattern REST genuinely cannot serve. |
| Versioning | Path-prefixed `/v1/...`. Additive changes within a version; a breaking change means `/v2` and a documented deprecation window. |
| Auth | Public reads unauthenticated; user-scoped reads via wallet-signature auth; operator actions via a separate credential (`signer-and-key-management.md` §7) |
| Content | `application/json`; `numeric(39,0)` values are **JSON strings**, never numbers — a `u128` does not survive IEEE-754, and silently losing precision on a debt figure is the exact failure this system exists to avoid |

---

## 2. Universal response envelope

Every chain-derived response carries:

```json
{
  "data": { },
  "meta": {
    "commitment":      "confirmed",
    "as_of_slot":      312845901,
    "as_of_block_time":"2026-09-05T11:22:33Z",
    "chain_head_slot": 312845904,
    "lag_slots":       3,
    "lag_seconds":     1.1,
    "degraded":        false,
    "degraded_reasons":[],
    "decoder_version": 3
  }
}
```

| Field | Rule |
|---|---|
| `commitment` | Mandatory. Never absent, never inferred by the client. |
| `as_of_slot` / `as_of_block_time` | The state the data reflects, not the time of the request. |
| `lag_slots` / `lag_seconds` | Sentinel says how far behind it is, always. |
| `degraded` + `degraded_reasons` | Machine-readable codes: `INGEST_LAG`, `UNREPAIRED_GAP`, `RECOMPUTING`, `ORACLE_STALE`, `UNKNOWN_SCHEMA`, `PROVIDERS_DOWN`. |
| `decoder_version` | Which decoder produced the protocol data. |

**A client can always tell how much to trust a response**, and a programmatic consumer can make its own
freshness decision instead of guessing.

---

## 3. Endpoints

### 3.1 Platform

| Method | Path | Returns |
|---|---|---|
| GET | `/v1/health` | Indexer health: lag, contiguity, gaps, breakers, keeper state, decoder versions, entity staleness |
| GET | `/v1/chain/head` | Head slot per commitment, last contiguous slot, last finalized slot |
| GET | `/v1/chain/slots/{slot}` | Slot record: status, commitment, canonical, blockhash, parent |

### 3.2 Protocol

| Method | Path | Returns |
|---|---|---|
| GET | `/v1/protocol` | Aegis protocol singleton: admin, guardian, fee recipient, pause bits |
| GET | `/v1/markets` | Markets, paginated; filter by mint or status |
| GET | `/v1/markets/{market}` | Full market state, current parameters, totals, utilization, rates, accrual staleness |
| GET | `/v1/markets/{market}/params/history` | Versioned parameter history with `effective_from_slot` |
| GET | `/v1/markets/{market}/metrics` | Time series, bucketed |
| GET | `/v1/markets/{market}/events` | Event stream for the market, paginated, ordered |

### 3.3 Positions

| Method | Path | Returns |
|---|---|---|
| GET | `/v1/positions` | Paginated; filter by market, owner, `state`, `min_health`, `max_health` |
| GET | `/v1/positions/{position}` | Current state plus health, liquidation price, borrow capacity |
| GET | `/v1/positions/{position}/history` | Event-derived history from `aegis_events` |
| GET | `/v1/positions/{position}/health/history` | Health over time, with the oracle observations used |

Position health object:

```json
{
  "state": "liquidatable",
  "health_factor_wad": "842495000000000000",
  "t_eval": "2026-09-05T11:22:34Z",
  "collateral_value_wad": "948000000000000000000",
  "debt_value_wad": "900180000000000000000",
  "liquidation_price_wad": "…",
  "oracle": {
    "collateral": { "feed_id": "0x…", "publish_time": "…", "age_seconds": 4, "conf_bps": 21 },
    "loan":       { "feed_id": "0x…", "publish_time": "…", "age_seconds": 6, "conf_bps": 2 }
  },
  "market_params_from_slot": 312840000,
  "authoritative": false,
  "note": "Computed off-chain by Sentinel. The Aegis program is authoritative."
}
```

`state ∈ {healthy, liquidatable, no_debt, unknown_oracle, stale}`. **`unknown_oracle` is a real state**
— it is never silently rendered as healthy (`aegis-integration.md` H-3).

`authoritative: false` is on **every** health object, permanently. Sentinel predicts; Aegis decides.

### 3.4 Oracle

| Method | Path | Returns |
|---|---|---|
| GET | `/v1/oracle/feeds` | Per feed: last publish, age, confidence ratio, validity, which markets use it |
| GET | `/v1/oracle/feeds/{feed_id}/history` | Observations over time, including invalid ones with `failed_check` |

Surfacing invalid observations with the failing check is what makes "why is this market fail-closed?"
answerable in one request.

### 3.5 Risk and execution

| Method | Path | Returns |
|---|---|---|
| GET | `/v1/liquidations/candidates` | Open candidates with sizing and profitability |
| GET | `/v1/liquidations/history` | Executed liquidations with predicted-vs-actual reconciliation |
| GET | `/v1/intents` | Execution intents, filterable by state and kind |
| GET | `/v1/intents/{intent_id}` | Full lifecycle: every state transition, every attempt, every signature |
| GET | `/v1/bad-debt/eligible` | Positions eligible for `absorb_bad_debt`, with loss decomposition |

### 3.6 Transaction support (user flows)

| Method | Path | Semantics |
|---|---|---|
| POST | `/v1/tx/build` | **Typed parameters in, unsigned transaction out**, plus a decoded human-readable description |
| POST | `/v1/tx/track` | Accepts a **signature**, begins tracking, returns a tracking handle |
| GET | `/v1/tx/{signature}` | Status with commitment, containing slot, and the decoded Aegis effect |

**There is no endpoint that accepts transaction bytes and returns them signed, and no endpoint that
relays user bytes** (`signer-and-key-management.md` §1, S-10).

### 3.7 Operator (authenticated, audited)

| Method | Path | Semantics |
|---|---|---|
| POST | `/v1/ops/intents` | Create a typed intent (`absorb_bad_debt`, `accrue_interest`) |
| POST | `/v1/ops/keeper/pause` / `/resume` | Keeper control; resume requires the pause reason to be resolved |
| POST | `/v1/ops/replay` | Enqueue a scoped replay; **requires an explicit range or entity** |
| POST | `/v1/ops/backfill` | Enqueue a scoped backfill |
| POST | `/v1/ops/jobs/{id}/requeue` | Requeue a quarantined job |

Operator endpoints create **typed intents with typed parameters**. None accepts a transaction, an
instruction, an account list, or SQL. Operator privilege changes what may be *requested*, never what may
be *signed* (SK-10).

---

## 4. Pagination

**Cursor-based, always.** Offset pagination over an append-heavy table is both slow and incorrect under
concurrent inserts.

```
GET /v1/positions?limit=100&cursor=eyJzbG90IjozMTI4NDU5MDEsImlkIjoiLi4uIn0
->
{ "data": [...], "page": { "next_cursor": "...", "has_more": true }, "meta": {...} }
```

| # | Rule |
|---|---|
| PG-1 | The cursor encodes the **full sort key**, so it is stable under concurrent inserts. |
| PG-2 | `limit` has a hard maximum. A client asking for more gets the maximum, not an error and not the whole table. |
| PG-3 | Sort keys are a **closed enum**, never a client-supplied column name (S-16). |
| PG-4 | Historical endpoints accept `from_slot`/`to_slot` with a bounded maximum range. |
| PG-5 | Cursors are opaque and versioned; an unparseable cursor is a 400, never a silent reset to page one. |

---

## 5. Error model

```json
{
  "error": {
    "code": "SEN-API-004",
    "kind": "STALE_DATA",
    "message": "Requested min_slot 312845999 exceeds available as_of_slot 312845901",
    "retryable": true,
    "retry_after_ms": 800,
    "details": { "as_of_slot": 312845901, "requested_min_slot": 312845999 }
  },
  "meta": { }
}
```

| HTTP | Kind | Meaning |
|---|---|---|
| 400 | `INVALID_REQUEST` | Malformed parameters |
| 401 / 403 | `UNAUTHENTICATED` / `FORBIDDEN` | Auth |
| 404 | `NOT_FOUND` | Unknown entity |
| 409 | `STALE_DATA` | `min_slot` not yet reached — **read-your-writes support**, not a failure |
| 409 | `CONFLICT` | Idempotency key already used with different parameters |
| 422 | `ENTITY_UNAVAILABLE` | Entity is `RECOMPUTING`, `STALE`, or `UNKNOWN_SCHEMA` |
| 429 | `RATE_LIMITED` | With `Retry-After` |
| 503 | `DEGRADED` | Sentinel cannot serve at the required freshness |

| # | Rule |
|---|---|
| E-1 | Every error carries a stable `code`; clients and tests branch on codes, never on message text. |
| E-2 | `retryable` is explicit. Clients never have to guess. |
| E-3 | **A 409 `STALE_DATA` is better than serving old data silently.** It is the correct answer for a client that just submitted a transaction and is polling for its effect. |
| E-4 | Error messages never leak internal identifiers, SQL, stack traces, or credentials. |

---

## 6. Idempotency for write endpoints

Every `POST` that creates something accepts `Idempotency-Key`:

| # | Rule |
|---|---|
| ID-1 | Same key + same parameters → the original result, `200`, with `Idempotent-Replay: true`. |
| ID-2 | Same key + **different** parameters → `409 CONFLICT`. Silently doing the new thing under an old key is the worst outcome. |
| ID-3 | Keys are scoped to the caller and expire after a documented window. |
| ID-4 | For `/v1/ops/intents`, the key maps onto `execution_intents.idempotency_key`, so **API-level and business-level idempotency are the same mechanism** rather than two that can disagree. |

---

## 7. Rate limits

| Tier | Limit basis |
|---|---|
| Unauthenticated | Per IP, conservative |
| Authenticated user | Per token, higher |
| Operator | Per credential, highest, still limited |

`X-RateLimit-Limit`, `-Remaining`, `-Reset` on every response; `Retry-After` on 429.
Limits are enforced in Redis when available and **in-process when not** — the documented degraded mode
(ADR-0003) is per-instance limiting, which is weaker but never absent.

---

## 8. WebSocket API

### 8.1 Protocol

```
client → { "op": "subscribe", "id": "s1", "topic": "market", "key": "<pubkey>",
           "commitment": "confirmed", "cursor": "…optional…" }
server → { "op": "subscribed", "id": "s1", "cursor": "…" }
server → { "op": "event", "id": "s1", "seq": 41207, "cursor": "…",
           "commitment": "confirmed", "as_of_slot": 312845901, "data": { } }
server → { "op": "revision", "id": "s1", "seq": 41208, "reason": "ROLLBACK",
           "superseded_slot": 312845899, "data": { } }
server → { "op": "heartbeat", "chain_head_slot": …, "lag_slots": … }
```

### 8.2 Topics

| Topic | Key | Emits |
|---|---|---|
| `market` | market pubkey | State changes, params updates, metric ticks |
| `position` | position pubkey | State and health changes |
| `owner` | owner pubkey | All that owner's positions |
| `candidates` | market pubkey or `*` | Candidate created / claimed / executed / expired |
| `intent` | intent id | Every execution state transition |
| `oracle` | feed id | New observations, validity changes |
| `chain` | — | Head, commitment promotion, rollback events |

### 8.3 Semantics

| # | Rule |
|---|---|
| WS-1 | **Delivery is at-least-once.** Clients dedupe on `(topic, key, seq)`. Promising exactly-once over a reconnecting socket would be a lie. |
| WS-2 | Every message carries a `cursor`. Reconnecting with it resumes from that point within a bounded retention window. |
| WS-3 | If the cursor is older than the retention window, the server sends `resync_required` with a snapshot reference. **It never silently skips**, which is the failure mode that produces a client with a permanently wrong view. |
| WS-4 | Every message carries `commitment`. `processed` is available only on an explicitly ephemeral topic and never for a value a user acts on. |
| WS-5 | A rollback produces an explicit `revision` message. Clients are never left holding a value Sentinel knows is wrong. |
| WS-6 | Heartbeats carry chain head and lag, so a client can detect a stalled server without an application-level probe. |
| WS-7 | Per-connection subscription cap; per-connection bounded buffer; **slow consumers are disconnected** with a reason, not buffered indefinitely (S-17). |
| WS-8 | Fanout uses Redis pub/sub across API instances when available, and an in-process bus when not — degraded to single-instance fanout, never incorrect. |

---

## 9. Caching

| Response class | Policy |
|---|---|
| Public, slot-addressed (e.g. a finalized slot) | Immutable; long TTL |
| Public, head-relative (markets, positions) | Short TTL with `as_of_slot` in the cache key |
| User-scoped | `no-store`. **Never cached.** (S-22) |
| Degraded (`meta.degraded = true`) | `no-store`; degraded responses must never be served from cache after recovery |

---

## 10. Contract testing

| # | Rule |
|---|---|
| CT-1 | An OpenAPI document is generated from the code, committed, and diffed in CI. An undocumented change fails the build. |
| CT-2 | Every endpoint has a contract test asserting shape, pagination, `meta` completeness, and the error model. |
| CT-3 | Every user-scoped and operator route has an authorization test (S-14). |
| CT-4 | A test asserts **no route reaches the RPC pool** (S-18). |
| CT-5 | A test asserts **no route accepts transaction bytes for signing** (S-10). |
| CT-6 | A test asserts every chain-derived response includes a complete `meta` block (CHN-07). |
