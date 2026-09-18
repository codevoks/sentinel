#!/usr/bin/env bash
# Phase 2 demo (docs/phases/phase-02-data-model.md §10): "make migrate on
# an empty database, then a script that attempts each forbidden operation
# and shows the database refusing it — the schema defending itself."
#
# Every statement below is expected to FAIL. A PASS line means PostgreSQL
# refused the forbidden operation; a FAIL line means it did not (and this
# script exits non-zero) — the inverse of the usual convention, because the
# assertion under test is "the database says no."
#
# Prerequisites: `make up` (local Postgres reachable at 127.0.0.1:5432) and
# `make migrate` already run. Zero-cost, local-only — no network beyond
# loopback/Docker (docs/zero-cost-local.md).

set -uo pipefail
cd "$(dirname "$0")/.."

# This repository's dev containers have no local `psql` client on the host
# (verified: `which psql` finds nothing), so every statement runs through
# `docker exec` into the live `compose-postgres-1` container, the same
# pattern `make migrate` and this phase's own manual verification used
# throughout. Still entirely local/loopback (docs/zero-cost-local.md) — no
# host network dependency, just a different transport to the same Postgres.
PG_CONTAINER="${SENTINEL_PG_CONTAINER:-compose-postgres-1}"
BOOT_USER=sentinel_bootstrap
RUST_USER=sentinel_rust
TS_USER=sentinel_ts
DB=sentinel

FAIL=0

psql_as() {
	local user="$1"
	shift
	docker exec "$PG_CONTAINER" psql -U "$user" -d "$DB" -v ON_ERROR_STOP=1 "$@"
}

expect_rejected() {
	local label="$1"
	local user="$2"
	local sql="$3"
	if psql_as "$user" -c "$sql" >/dev/null 2>&1; then
		echo "FAIL (should have been rejected but succeeded): $label"
		FAIL=1
	else
		echo "PASS (rejected as required): $label"
	fi
}

echo "=== DM-02: raw_observations is append-only ==="
expect_rejected "sentinel_rust UPDATE raw_observations" sentinel_rust \
	"UPDATE raw_observations SET slot = slot + 1"
expect_rejected "sentinel_rust DELETE FROM raw_observations" sentinel_rust \
	"DELETE FROM raw_observations"

echo
echo "=== TypeScript role cannot write raw/normalized/protocol/derived tables ==="
expect_rejected "sentinel_ts INSERT into raw_observations" sentinel_ts \
	"INSERT INTO raw_observations (kind, natural_key, slot, commitment, source, provider_id, payload, payload_hash, payload_encoding) VALUES ('block','demo-forbidden',1,'confirmed','rpc_http','p','\\x00','\\x00','borsh')"
expect_rejected "sentinel_ts UPDATE slots" sentinel_ts \
	"UPDATE slots SET canonical = true"
expect_rejected "sentinel_ts INSERT into aegis_markets" sentinel_ts \
	"INSERT INTO aegis_markets (market_pubkey) VALUES ('\\x00')"

echo
echo "=== DM-08: one open alert per (kind, entity) ==="
psql_as sentinel_rust -c \
	"INSERT INTO alerts (kind, severity, entity_kind, entity_key) VALUES ('demo_alert','info','demo','entity-1')" >/dev/null 2>&1
expect_rejected "second OPEN alert for the same (kind, entity)" sentinel_rust \
	"INSERT INTO alerts (kind, severity, entity_kind, entity_key) VALUES ('demo_alert','info','demo','entity-1')"

echo
echo "=== DM-10: execution_intents.idempotency_key is globally unique ==="
psql_as sentinel_ts -c \
	"INSERT INTO execution_intents (intent_id, idempotency_key, kind, market_pubkey, params, constraints, state, expires_at, max_attempts, trigger_slot, trigger_commitment) VALUES (gen_random_uuid(), 'demo-idempotency-key', 'liquidate', '\\x00', '{}', '{}', 'CREATED', now() + interval '5 minutes', 3, 1, 'confirmed')" >/dev/null 2>&1
expect_rejected "duplicate idempotency_key, different kind" sentinel_ts \
	"INSERT INTO execution_intents (intent_id, idempotency_key, kind, market_pubkey, params, constraints, state, expires_at, max_attempts, trigger_slot, trigger_commitment) VALUES (gen_random_uuid(), 'demo-idempotency-key', 'custom', '\\x00', '{}', '{}', 'CREATED', now() + interval '5 minutes', 3, 1, 'confirmed')"

echo
echo "=== Numeric exactness: numeric(39,0) rejects overflow, not truncation ==="
expect_rejected "value exceeding numeric(39,0) precision (10^39, one digit beyond 39-digit precision)" sentinel_bootstrap \
	"SELECT '1000000000000000000000000000000000000000'::numeric(39,0)"

echo
echo "=== Partitioning: missing partition fails clearly, no catch-all ==="
expect_rejected "insert at a slot with no created partition" sentinel_rust \
	"INSERT INTO raw_observations (kind, natural_key, slot, commitment, source, provider_id, payload, payload_hash, payload_encoding) VALUES ('block','demo-no-partition',999999999,'confirmed','rpc_http','p','\\x00','\\x01','borsh')"

echo
if [ "$FAIL" -eq 0 ]; then
	echo "Phase 2 demo: PostgreSQL refused every forbidden operation. The schema is defending itself."
	exit 0
else
	echo "Phase 2 demo: at least one forbidden operation was NOT rejected. See FAIL lines above."
	exit 1
fi
