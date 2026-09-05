# Sentinel — Observability

**Status: FROZEN (Phase 0). Implementation in Phase 12.**

> **Every alert corresponds to an operator action.** An alert with no action is noise, and noise is how
> real alerts get ignored. If a condition has no action, it is a metric or a log line, not an alert.

---

## 1. Instrumentation stack

| Signal | Choice | Rationale |
|---|---|---|
| Metrics | **OpenTelemetry** → Prometheus exposition | Vendor-neutral; the collector can route anywhere later |
| Traces | **OpenTelemetry** spans, sampled | Justified for the execution path specifically (§4) |
| Logs | **Structured JSON**, one event per line, with trace/span IDs | Correlatable with traces; greppable; machine-parseable |
| Dashboards | Grafana, provisioned as code in `infra/` | Reviewable in Git; reproducible locally |

**The whole stack runs locally in Compose** and is part of the demo. Observability that only exists in
production is untested observability.

---

## 2. Correlation

Three IDs thread through everything:

| ID | Scope | Appears in |
|---|---|---|
| `request_id` | One RPC call | `raw_observations.request_id`, provider logs, spans |
| `intent_id` | One business action | Intent, attempts, signing audit, reconciliation, every executor log |
| `trace_id` | One end-to-end flow | Spans and logs across service boundaries |

**Every stored raw byte is traceable to the request that fetched it and the provider that served it.**
That is what makes "why does Sentinel think this?" answerable rather than speculative.

---

## 3. Metrics catalogue

### 3.1 Ingestion

| Metric | Type | Labels |
|---|---|---|
| `sentinel_ingest_lag_seconds` | gauge | `stream`, `commitment` |
| `sentinel_ingest_lag_slots` | gauge | `stream`, `commitment` |
| `sentinel_finalized_lag_slots` | gauge | — |
| `sentinel_last_contiguous_slot` | gauge | `stream` |
| `sentinel_gap_backlog_slots` | gauge | — |
| `sentinel_gaps_detected_total` | counter | `cause` |
| `sentinel_gaps_unrepaired` | gauge | — |
| `sentinel_gap_repair_seconds` | histogram | — |
| `sentinel_raw_observations_written_total` | counter | `kind`, `source`, `provider` |
| `sentinel_raw_write_bytes_total` | counter | `kind` |
| `sentinel_duplicate_observations_total` | counter | `kind` |
| `sentinel_skipped_slots_total` | counter | — |

### 3.2 RPC and connectivity

| Metric | Type | Labels |
|---|---|---|
| `sentinel_rpc_requests_total` | counter | `provider`, `method`, `class`, `outcome` |
| `sentinel_rpc_latency_seconds` | histogram | `provider`, `method` |
| `sentinel_rpc_rate_limited_total` | counter | `provider` |
| `sentinel_rpc_stale_responses_total` | counter | `provider`, `method` |
| `sentinel_rpc_breaker_state` | gauge | `provider` |
| `sentinel_rpc_provider_divergence_total` | counter | `class` |
| `sentinel_ws_connected` | gauge | `provider` |
| `sentinel_ws_reconnects_total` | counter | `provider`, `reason` |
| `sentinel_ws_connection_seconds` | histogram | `provider` |
| `sentinel_ws_notifications_total` | counter | `provider`, `subscription` |

### 3.3 Chain state

| Metric | Type | Labels |
|---|---|---|
| `sentinel_rollbacks_total` | counter | — |
| `sentinel_rollback_depth_slots` | histogram | — |
| `sentinel_abandoned_blocks_total` | counter | — |
| `sentinel_promotion_lag_seconds` | histogram | `from`, `to` |
| `sentinel_stalled_finalization` | gauge | — |

### 3.4 Decoding and protocol

| Metric | Type | Labels |
|---|---|---|
| `sentinel_decode_failures_total` | counter | `stage`, `error_code` |
| `sentinel_unknown_schema_total` | counter | `account_kind` |
| `sentinel_decoder_version_active` | gauge | `account_kind`, `version` |
| `sentinel_projection_divergence_total` | counter | `entity_kind` |
| `sentinel_materialization_lag_slots` | gauge | `entity_kind` |
| `sentinel_entities_stale` | gauge | `entity_kind`, `reason` |
| `sentinel_aegis_invariant_violations_total` | counter | `invariant_id`, `market` |
| `sentinel_program_upgrade_detected_total` | counter | `program` |

### 3.5 Oracle

| Metric | Type | Labels |
|---|---|---|
| `sentinel_oracle_age_seconds` | gauge | `feed_id` |
| `sentinel_oracle_conf_bps` | gauge | `feed_id` |
| `sentinel_oracle_validation_failures_total` | counter | `feed_id`, `check` |
| `sentinel_markets_fail_closed` | gauge | `reason` |

### 3.6 Risk and keeper

| Metric | Type | Labels |
|---|---|---|
| `sentinel_positions_evaluated_total` | counter | `market` |
| `sentinel_positions_by_state` | gauge | `market`, `state` |
| `sentinel_candidates_created_total` | counter | `market`, `profitable` |
| `sentinel_candidates_open` | gauge | `market` |
| `sentinel_liquidatable_unactionable` | gauge | `market`, `reason` |
| `sentinel_keeper_paused` | gauge | `reason` |
| `sentinel_health_prediction_error` | histogram | `market` |
| `sentinel_bad_debt_eligible_positions` | gauge | `market` |
| `sentinel_keeper_inventory_units` | gauge | `mint` |

### 3.7 Execution

| Metric | Type | Labels |
|---|---|---|
| `sentinel_intents_total` | counter | `kind`, `terminal_state` |
| `sentinel_intents_in_flight` | gauge | `kind` |
| `sentinel_attempts_total` | counter | `kind`, `terminal_state` |
| `sentinel_simulation_failures_total` | counter | `error_band`, `class` |
| `sentinel_submit_failures_total` | counter | `provider`, `reason` |
| `sentinel_attempt_expired_total` | counter | — |
| `sentinel_attempt_unknown_total` | counter | — |
| `sentinel_confirmation_seconds` | histogram | — |
| `sentinel_finalization_seconds` | histogram | — |
| `sentinel_fees_spent_lamports_total` | counter | `outcome` |
| `sentinel_fork_wasted_attempts_total` | counter | — |
| `sentinel_policy_denials_total` | counter | `check` |
| `sentinel_reconciliation_mismatches_total` | counter | `class` |

### 3.8 Platform

| Metric | Type | Labels |
|---|---|---|
| `sentinel_jobs_queued` / `_leased` / `_quarantined` | gauge | `kind` |
| `sentinel_job_duration_seconds` | histogram | `kind` |
| `sentinel_db_latency_seconds` | histogram | `operation` |
| `sentinel_db_lock_wait_seconds` | histogram | `resource` |
| `sentinel_api_requests_total` | counter | `route`, `status` |
| `sentinel_api_latency_seconds` | histogram | `route` |
| `sentinel_ws_clients` | gauge | — |
| `sentinel_ws_slow_consumer_disconnects_total` | counter | — |
| `sentinel_redis_available` | gauge | — |

---

## 4. Tracing — where it earns its cost

Tracing everything is expensive and mostly useless. Sentinel traces two flows:

1. **The execution path**, end to end, **100% sampled**: candidate → claim → build → simulate → policy
   → sign → persist → submit → observe → confirm → finalize → reconcile. Every span carries
   `intent_id`. This is the flow where "what happened to this specific liquidation?" must be answerable
   exactly, and where the volume is low enough to sample fully.
2. **The ingestion path**, **sampled at a low rate** plus **always on error**: fetch → raw write →
   normalize → chain state → decode → materialize, carrying `request_id` and slot.

Everything else is metrics and logs. This is a deliberate scoping decision, not an omission.

---

## 5. Alerts and their actions

**Every row has an action. A row without one would be deleted.**

| Alert | Condition | Severity | Operator action |
|---|---|---|---|
| `IngestLagHigh` | `ingest_lag_seconds` p95 > threshold for N min | **page** | Check provider health; check `getBlock` latency; check normalizer backlog; consider a provider failover |
| `IngestStalled` | `last_contiguous_slot` unchanged for N min while the chain advances | **page** | Check the indexer singleton is alive and holds its advisory lock; check for an unrepaired gap; restart if wedged |
| `UnrepairedGap` | any gap older than threshold | **page** | Run `sentinel-backfill gaps`; if it fails, check provider `getBlock` for that range |
| `AllProvidersDown` | every breaker open | **page** | Check network and endpoints; add or restore a provider; ingestion and execution are both stopped |
| `ProviderDivergence` | content divergence detected | **page** | Identify the faulty provider from the stored payloads; remove it; **do not merge** |
| `StalledFinalization` | slots stuck at confirmed beyond deadline | **page** | Verify the chain is finalizing; verify the finality-evidence path (SR-2 after Alpenglow) |
| `DeepRollback` | rollback depth > threshold | **page** | Verify chain health; verify recompute completed; check for a Sentinel finality bug |
| `DecodeFailureSpike` | decode failures above rate | **page** | Inspect `decode_failures` payloads; a spike usually means a schema change or a new transaction version |
| `UnknownSchema` | any `UNKNOWN_SCHEMA` | **page** | An Aegis upgrade or a decoder gap. Register a decoder version and replay; **do not guess a layout** |
| `AegisProgramUpgraded` | `ProgramData` hash changed | **page** | Keeper is auto-paused. Verify the new IDL, register the decoder version, replay, then resume |
| `AegisInvariantViolation` | an off-chain-checkable invariant fails at finalized | **page** | **Triage in this order:** (1) is the state finalized? (2) is the snapshot newer than the projection? (3) is there a gap in the range? (4) only then escalate to Aegis (runbook R-2) |
| `ProjectionDivergence` | snapshot ≠ projection above rate | **page** | Events are being lost. Check gaps and log-decoding, not the decoder |
| `KeeperPaused` | `keeper_paused` set | **page** | Read the reason; resolve; resume **manually** — auto-pause is one-way by design |
| `ModelDivergence` | mismatch class `MODEL_DIVERGENCE` | **page** | **Highest severity.** Sentinel disagrees with the protocol. Keeper stays paused until reconciled |
| `PolicyDenial` | any `sentinel_policy_denials_total` increase | **page** | A planner produced a message the policy refused. Treat as a bug, never as a transient |
| `AttemptUnknown` | any attempt reaches `UNKNOWN` | **page** | Manually determine the on-chain outcome; resolve the intent; **never let automation retry it** |
| `FeeBudgetExhausted` | rolling fee budget hit | ticket | Review landing rate and race losses; adjust budget or fee strategy |
| `KeeperInventoryLow` | inventory below floor | ticket | Top up from the cold account |
| `LiquidatableUnactionable` | positions liquidatable but unactionable above threshold | ticket | Usually oracle staleness. Feeds Aegis runbook R-1 |
| `BadDebtEligible` | any position eligible | ticket | Operator decision on `absorb_bad_debt`; Aegis runbook R-3 |
| `JobsQuarantined` | quarantine depth > 0 | ticket | Inspect payload and error; fix; requeue explicitly |
| `RateLimited` | sustained rate limiting | ticket | Raise the budget, add a provider, or reduce scan frequency |
| `DbLatencyHigh` | `db_latency_seconds` p95 > threshold | ticket | Check locks, bloat, partition maintenance, and index health |
| `PartitionsExhausted` | fewer than N future partitions | **page** | Run partition maintenance — running out is an outage |
| `WsFlapping` | reconnect rate > threshold | ticket | Provider PubSub degraded; consider failover |
| `SlowConsumers` | slow-consumer disconnects above rate | ticket | Usually a client bug; verify the fanout buffer is correctly bounded |

**Severity contract:** `page` means a human is woken and the action is in this table. `ticket` means it
is handled in business hours. Nothing else exists — there is no "warning" tier that everyone ignores.

---

## 6. Dashboards

| Dashboard | Answers |
|---|---|
| **Indexer health** | Are we keeping up? Lag, contiguity, gaps, rollbacks, decode failures |
| **Provider health** | Which endpoint is degraded? Per-provider rates, latency, breakers, divergence |
| **Protocol state** | What is Aegis doing? Per-market totals, utilization, rates, positions by health state, oracle freshness |
| **Keeper** | Is automation working? Candidates, intents, attempts, landing rate, race losses, prediction error, fees, inventory |
| **Execution detail** | What happened to this liquidation? Per-intent timeline with every state transition and span |
| **Platform** | Jobs, database, API, WebSocket, resources |

Every dashboard shows the **current commitment lag** in a fixed position, because it is the context in
which every other number on the screen must be read.

---

## 7. Health endpoints

| Endpoint | Semantics |
|---|---|
| `/healthz` | Process alive. No dependency checks. Used by the supervisor. |
| `/readyz` | Dependencies reachable (Postgres, ≥1 provider). Used before accepting traffic. |
| `/statusz` | **The honest one**: ingestion lag, finalized lag, contiguity, unrepaired gaps, breaker states, keeper state, decoder versions, entity staleness counts, Redis availability. Machine-readable, and the source of the API's `meta.degraded`. |

`/statusz` is deliberately not a boolean. **"Is Sentinel healthy?" is not a yes/no question**, and
pretending it is produces exactly the monitoring that says green while the data is three hours stale.

---

## 8. Log discipline

| # | Rule |
|---|---|
| LG-1 | Structured JSON only. No `println!`, no `console.log`. |
| LG-2 | Every log carries `service`, `trace_id` where applicable, and a stable `event` name. |
| LG-3 | Every error log carries its `SEN-*` code. Alerts and tests reference codes, never message text. |
| LG-4 | **No secret, key, token, or credential in any log**, enforced by the redacting wrapper type and by a test (S-13). |
| LG-5 | Per-record logging is bounded: a spike produces a **counted, sampled** log, not one line per record. A logging system that amplifies an incident is part of the incident. |
| LG-6 | `INFO` is for state transitions an operator would care about. Per-block chatter is `DEBUG` and off by default. |
