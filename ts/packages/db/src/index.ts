/**
 * Typed Postgres read layer for the TypeScript side (`sentinel_ts` role).
 *
 * Scope (phase-02-data-model.md §"Files": `ts/packages/db/*`; requirement
 * 13 of the Phase 2 task): only typed representations for tables the TS
 * role may actually touch, respecting its least-privilege boundary
 * (`docs/data-model.md` §10, `infra/migrations/0010_grants.sql`). No API,
 * executor, keeper, or Aegis-adapter business logic lives here — those are
 * later phases (9, 10, 11, 7). No ORM: every query below is a hand-written,
 * parameterized SQL statement executed through `pg`'s `Pool`, exactly the
 * same "typed access layer, not an object-relational mapper" shape
 * `crates/sentinel-db` uses on the Rust side. PostgreSQL migrations
 * (`infra/migrations/`) remain the sole schema authority — this package
 * has no migration capability and no DDL.
 *
 * **Numeric exactness (DM-07 / phase-02-data-model.md "Implementation
 * requirements"):** `node-postgres` returns `numeric` columns as plain
 * `string` by default (it does not parse them to `number`), which is
 * already exact. This module additionally exposes `numericToBigInt`/
 * `bigIntToNumericParam` so callers work with native `bigint` — itself an
 * arbitrary-precision integer type, never a floating-point type — end to
 * end, the same guarantee `crates/sentinel-db/src/numeric.rs` provides on
 * the Rust side.
 */

import { Pool, type PoolConfig } from "pg";

export const SENTINEL_DB_PACKAGE_VERSION = "0.1.0" as const;

// ---------------------------------------------------------------------
// Numeric exactness helpers — never route a numeric(39,0)/numeric(20,0)
// value through `Number`.
// ---------------------------------------------------------------------

/** Converts a `numeric` column's driver-returned text to an exact `bigint`. */
export function numericToBigInt(text: string): bigint {
  return BigInt(text);
}

/** Converts a `bigint` to the exact decimal text used as a bind parameter. */
export function bigIntToNumericParam(value: bigint): string {
  return value.toString();
}

// ---------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------

export function createPool(config: PoolConfig): Pool {
  return new Pool(config);
}

// ---------------------------------------------------------------------
// Row types — only for tables `sentinel_ts` may read or write
// (data-model.md §10). Every other table's row shape (raw/normalized/
// chain-state/protocol/derived) is deliberately NOT represented here: this
// package has no business reading their full row shape yet (no TS
// consumer exists before Phase 9's API), and typing them would invite a
// write helper to follow, which the role boundary forbids.
// ---------------------------------------------------------------------

export interface AlertRow {
  alert_id: string; // bigint identity column; returned as string by pg for int8
  kind: string;
  severity: string;
  entity_kind: string;
  entity_key: string;
  opened_at: Date;
  resolved_at: Date | null;
  detail: unknown;
  runbook_id: string | null;
}

export interface ExecutionIntentRow {
  intent_id: string;
  idempotency_key: string;
  kind: string;
  market_pubkey: Buffer;
  position_pubkey: Buffer | null;
  params: unknown;
  constraints: unknown;
  state: string;
  created_at: Date;
  updated_at: Date;
  expires_at: Date;
  lease_holder: string | null;
  lease_expires_at: Date | null;
  attempt_count: number;
  max_attempts: number;
  cumulative_fee_lamports: string;
  terminal_reason: string | null;
  trigger_slot: string;
  trigger_commitment: string;
}

export interface TransactionAttemptRow {
  attempt_id: string;
  intent_id: string;
  attempt_number: number;
  signature: Buffer;
  transaction_bytes: Buffer;
  transaction_version: string;
  recent_blockhash: Buffer;
  last_valid_block_height: string;
  compute_unit_limit: number | null;
  priority_fee_lamports: string | null;
  simulation_result: unknown;
  simulated_units: number | null;
  state: string;
  signed_at: Date;
  submitted_at: Date | null;
  observed_at: Date | null;
  resolved_at: Date | null;
  observed_slot: string | null;
  onchain_error_code: string | null;
  onchain_error_band: string | null;
}

// ---------------------------------------------------------------------
// Queries — sentinel_ts's actual grants (infra/migrations/0010_grants.sql):
// SELECT on every layer; INSERT/UPDATE on execution_intents,
// transaction_attempts, reconciliation_mismatches, alerts. Nothing here
// ever issues INSERT/UPDATE/DELETE against a raw/normalized/chain-state/
// protocol/derived table — that is what makes the language boundary
// structural (data-model.md §10's closing sentence), and it is enforced
// twice: by the database role (proven in
// crates/sentinel-db/tests/adversarial.rs's role_permissions tests) and,
// here, by this package simply never emitting such a statement.
// ---------------------------------------------------------------------

export async function fetchOpenAlerts(pool: Pool): Promise<AlertRow[]> {
  const result = await pool.query<AlertRow>(
    `SELECT alert_id, kind, severity, entity_kind, entity_key, opened_at, resolved_at, detail, runbook_id
     FROM alerts WHERE resolved_at IS NULL ORDER BY alert_id`,
  );
  return result.rows;
}

export interface CreateOperatorIntentInput {
  intentId: string; // uuid
  idempotencyKey: string;
  kind: string;
  marketPubkey: Buffer;
  positionPubkey: Buffer | null;
  params: unknown;
  constraints: unknown;
  expiresAt: Date;
  maxAttempts: number;
  triggerSlot: bigint;
  triggerCommitment: string;
}

/**
 * `sentinel-api`'s one write path: an operator-initiated execution intent
 * (data-model.md §10: "sentinel-api (TS) — nothing [else] — read-only
 * role, plus insert on execution_intents for operator-initiated actions").
 * No `ON CONFLICT`: a duplicate `idempotency_key` must surface as a real
 * error (DM-10), matching `crates/sentinel-db::queries::create_execution_intent`.
 */
export async function createOperatorExecutionIntent(
  pool: Pool,
  input: CreateOperatorIntentInput,
): Promise<void> {
  await pool.query(
    `INSERT INTO execution_intents
       (intent_id, idempotency_key, kind, market_pubkey, position_pubkey, params, constraints,
        state, expires_at, max_attempts, trigger_slot, trigger_commitment)
     VALUES ($1, $2, $3, $4, $5, $6, $7, 'CREATED', $8, $9, $10, $11)`,
    [
      input.intentId,
      input.idempotencyKey,
      input.kind,
      input.marketPubkey,
      input.positionPubkey,
      input.params,
      input.constraints,
      input.expiresAt,
      input.maxAttempts,
      bigIntToNumericParam(input.triggerSlot), // trigger_slot is `bigint` (numeric(20,0) domain), never a JS number
      input.triggerCommitment,
    ],
  );
}

export async function fetchExecutionIntent(
  pool: Pool,
  intentId: string,
): Promise<ExecutionIntentRow | null> {
  const result = await pool.query<ExecutionIntentRow>(
    `SELECT intent_id, idempotency_key, kind, market_pubkey, position_pubkey, params, constraints,
            state, created_at, updated_at, expires_at, lease_holder, lease_expires_at,
            attempt_count, max_attempts, cumulative_fee_lamports, terminal_reason,
            trigger_slot, trigger_commitment
     FROM execution_intents WHERE intent_id = $1`,
    [intentId],
  );
  return result.rows[0] ?? null;
}

export async function fetchTransactionAttemptsForIntent(
  pool: Pool,
  intentId: string,
): Promise<TransactionAttemptRow[]> {
  const result = await pool.query<TransactionAttemptRow>(
    `SELECT attempt_id, intent_id, attempt_number, signature, transaction_bytes, transaction_version,
            recent_blockhash, last_valid_block_height, compute_unit_limit, priority_fee_lamports,
            simulation_result, simulated_units, state, signed_at, submitted_at, observed_at, resolved_at,
            observed_slot, onchain_error_code, onchain_error_band
     FROM transaction_attempts WHERE intent_id = $1 ORDER BY attempt_number`,
    [intentId],
  );
  return result.rows;
}
