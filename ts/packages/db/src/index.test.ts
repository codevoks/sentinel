import { test } from "node:test";
import assert from "node:assert/strict";
import { Pool } from "pg";
import {
  SENTINEL_DB_PACKAGE_VERSION,
  numericToBigInt,
  bigIntToNumericParam,
  fetchOpenAlerts,
  createOperatorExecutionIntent,
  fetchExecutionIntent,
} from "./index.js";

test("package skeleton exports a version marker", () => {
  assert.equal(SENTINEL_DB_PACKAGE_VERSION, "0.1.0");
});

test("numeric round-trip helpers never touch a float, even at u128::MAX", () => {
  const u128Max = 340282366920938463463374607431768211455n;
  const text = bigIntToNumericParam(u128Max);
  assert.equal(text, "340282366920938463463374607431768211455");
  assert.equal(numericToBigInt(text), u128Max);
});

function tsPoolUrl(): string {
  return (
    process.env.SENTINEL_TEST_TS_DATABASE_URL ??
    "postgres://sentinel_ts:sentinel_local_dev_only_ts@127.0.0.1:5432/sentinel"
  );
}

async function connectOrSkip(): Promise<Pool | null> {
  const pool = new Pool({ connectionString: tsPoolUrl(), connectionTimeoutMillis: 3000 });
  try {
    await pool.query("SELECT 1");
    return pool;
  } catch (e) {
    console.error(`skipping: Postgres not reachable at ${tsPoolUrl()}: ${String(e)}`);
    await pool.end();
    return null;
  }
}

// Requirement: "TypeScript forbidden writes rejected" — proven against a
// real live connection authenticated as sentinel_ts, not asserted from
// reading the grants file.
test("sentinel_ts role is rejected by PostgreSQL when it attempts to write a raw/normalized/protocol table", async () => {
  const pool = await connectOrSkip();
  if (!pool) return;
  try {
    await assert.rejects(
      () =>
        pool.query(
          "INSERT INTO raw_observations (kind, natural_key, slot, commitment, source, provider_id, payload, payload_hash, payload_encoding) " +
            "VALUES ('block', 'ts-write-attempt-from-node-test', 1, 'confirmed', 'rpc_http', 'p', '\\x00', '\\x00', 'borsh')",
        ),
      /permission denied/i,
      "sentinel_ts must never be able to write raw_observations (architecture.md §5, data-model.md §10)",
    );

    await assert.rejects(
      () => pool.query("UPDATE slots SET canonical = true"),
      /permission denied/i,
      "sentinel_ts must never be able to write the chain-state layer",
    );

    await assert.rejects(
      () => pool.query("INSERT INTO aegis_markets (market_pubkey) VALUES ('\\x00')"),
      /permission denied/i,
      "sentinel_ts must never be able to write the protocol layer",
    );
  } finally {
    await pool.end();
  }
});

test("sentinel_ts CAN read every layer and write its own execution-layer tables", async () => {
  const pool = await connectOrSkip();
  if (!pool) return;
  try {
    // Read access across layers (data-model.md §10: "sentinel-api — nothing
    // [to write] — read-only role").
    await pool.query("SELECT count(*) FROM raw_observations");
    await pool.query("SELECT count(*) FROM aegis_markets");

    const openAlerts = await fetchOpenAlerts(pool);
    assert.ok(Array.isArray(openAlerts));

    const intentId = crypto.randomUUID();
    const idempotencyKey = `liquidate:ts-test:${intentId}`;
    await createOperatorExecutionIntent(pool, {
      intentId,
      idempotencyKey,
      kind: "custom",
      marketPubkey: Buffer.from("market-ts-test"),
      positionPubkey: null,
      params: {},
      constraints: {},
      expiresAt: new Date(Date.now() + 5 * 60 * 1000),
      maxAttempts: 3,
      triggerSlot: 42n,
      triggerCommitment: "confirmed",
    });

    const fetched = await fetchExecutionIntent(pool, intentId);
    assert.ok(fetched);
    assert.equal(fetched?.idempotency_key, idempotencyKey);
    assert.equal(fetched?.state, "CREATED");

    // DM-10 proven from the TS side too: a duplicate idempotency_key must
    // be rejected.
    await assert.rejects(
      () =>
        createOperatorExecutionIntent(pool, {
          intentId: crypto.randomUUID(),
          idempotencyKey, // same key
          kind: "custom",
          marketPubkey: Buffer.from("market-ts-test"),
          positionPubkey: null,
          params: {},
          constraints: {},
          expiresAt: new Date(Date.now() + 5 * 60 * 1000),
          maxAttempts: 3,
          triggerSlot: 43n,
          triggerCommitment: "confirmed",
        }),
      /duplicate key|unique/i,
    );
  } finally {
    await pool.end();
  }
});
