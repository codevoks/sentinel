# Phase 9 — REST and Realtime API

**Status: NOT STARTED.** **Prerequisite: Phase 8 complete and tagged.**

## 1. Scope

1. `sentinel-api` (TypeScript): every endpoint in `api-design.md` §3 except the operator routes that
   create execution intents, which arrive with Phase 10.
2. The **universal response envelope** with complete `meta` on every chain-derived response.
3. Cursor pagination with a closed sort-key enum and hard limits.
4. The error model with stable `SEN-*` codes, including **`409 STALE_DATA`** for `min_slot`
   read-your-writes.
5. WebSocket: subscribe, cursor resume, `revision` on rollback, heartbeats with lag, bounded
   per-connection buffers, slow-consumer disconnect.
6. Auth: public reads; wallet-signature auth with a single-use, origin-bound, short-lived nonce.
7. Rate limiting via Redis when present, **in-process when not**.
8. Fanout via Redis pub/sub when present, **in-process bus when not**.
9. Generated OpenAPI, committed and diffed in CI.

## 2. Explicit non-scope

**No `/tx/build`, no `/tx/track`, no operator intent creation** — those depend on the execution engine
and arrive in Phase 10. No UI. **No endpoint may call the RPC pool.** No caching of
authorization-dependent responses.

## 3. Evidence objective

- **No API path can reach the RPC pool** (S-18) — asserted by a test, not by convention.
- **Every chain-derived response carries a complete `meta` block** (CHN-07) — asserted by a test on
  every endpoint.
- Redis can be removed and **no response body changes**.

## 4. Files

`ts/apps/api/src/{routes,ws,auth,pagination,errors,meta,ratelimit,fanout}/` · `ts/packages/db/` ·
`openapi.yaml`

## 5. Dependencies

Phases 1–8. **SR-6** (`@solana/kit` subscription surface) is not needed here — the API subscribes to
Postgres, not to the chain.

## 6. Implementation requirements — do not deviate

- **`u128`/WAD values are JSON strings**, never numbers. `Number.MAX_SAFE_INTEGER` is ~`9.0e15` and a
  WAD health factor is ~`1e18`; a number here silently corrupts the most important figure in the
  product.
- **Authorization is enforced at the data-access layer**, not only at the route. Every query carries
  the caller's scope.
- Sort keys, filters, and pagination fields are a **closed enum**. No client string reaches SQL
  (`CI-NOSQLFMT`).
- Cursors encode the **full sort key** and are opaque and versioned. An unparseable cursor is a 400,
  never a silent reset to page one.
- **WebSocket delivery is at-least-once and says so.** Clients dedupe on `(topic, key, seq)`.
- **A cursor older than retention yields `resync_required`, never a silent skip** (WS-3).
- A rollback produces an explicit `revision` message (WS-5).
- Slow consumers are **disconnected with a reason**, never buffered indefinitely.
- User-scoped responses are `no-store`. Degraded responses are `no-store`.
- The API database role is **read-only** on everything except operator-initiated `execution_intents`
  (which arrive in Phase 10).

## 7. Tests

**Contract (`CT-01..CT-06`):** shape, pagination, `meta` completeness, error model, authorization per
route, no-RPC-reachability, and (from Phase 10) no-bytes-in-signature-out.

**Unit:** cursor encode/decode round-trip and tamper rejection; error mapping; `meta` construction;
`u128` string serialization for the maximum value.

**Integration:** every endpoint against a populated database; WebSocket subscribe → receive → drop →
reconnect with cursor → **no gap and no duplicate**; a rollback producing a `revision`.

## 8. Adversarial / failure cases

| ID | Case | Asserted |
|---|---|---|
| `A-API-01` | Cross-user access attempted on every user-scoped route | Denied at the data layer |
| `A-SQL-01` | Injection corpus against every parameterized endpoint and every sort/filter field | No injection; closed enums reject |
| `A-WS-01` | Connection flood | Caps enforced; existing clients unaffected |
| `A-WS-02` / FI-28 | Slow consumer | Bounded buffer; disconnected with a reason; other clients unaffected |
| `A-SIGN-09` | Replay of a captured auth signature | Rejected — single-use, origin-bound, short-lived |
| `A-DOS-03` | Max-page and deep-pagination abuse | Hard limits enforced |
| `A-CACHE-01` | Cross-scope cache key | User-scoped responses are never cached |
| FI-13 / `A-CACHE-02` | **Redis removed entirely** | API works; **no response body changes**; rate limiting degrades to per-instance |
| `A-RPC-03` | Static and runtime assertion that no route reaches the RPC pool | Passes |
| — | Requesting `min_slot` beyond `as_of_slot` | `409 STALE_DATA` with the current slot, **not** stale data |
| — | Sentinel lagging beyond threshold | `meta.degraded` with a reason code on every response |

## 9. Acceptance criteria

- [ ] `CT-01..CT-06` pass
- [ ] `A-API-01`, `A-SQL-01`, `A-WS-01`, `A-WS-02`, `A-SIGN-09`, `A-DOS-03`, `A-CACHE-01/02`,
      `A-RPC-03` all pass
- [ ] FI-13 passes: Redis removed, no correctness change, no body change
- [ ] WebSocket resume produces **no gap and no duplicate** across a forced disconnect
- [ ] A rollback produces a `revision` message to a subscribed client
- [ ] Every `u128`/WAD field is a string, asserted for the maximum value
- [ ] OpenAPI generated, committed, and diffed in CI
- [ ] Universal checklist satisfied. Tag `phase-09-api`.

## 10. Demo

Curl every endpoint and show the `meta` block. Open a WebSocket, drive protocol activity, kill the
connection, reconnect with the cursor, show continuity. Inject a fork and show the `revision`. Stop
Redis and show the API still working.

## 11. Documentation & status updates

`api-design.md` updated only via ADR if implementation revealed a genuine problem. `project-status.md`:
API IMPLEMENTED + TESTED + DEMOED.

## 12. Stop condition

**STOP after this phase.** Phase 10 has not been started.
