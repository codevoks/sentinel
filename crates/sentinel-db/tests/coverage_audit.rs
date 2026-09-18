//! Closure-fix regression guard (docs/phases/phase-02-data-model.md §1.4,
//! the "typed row structs and a sqlx access layer in sentinel-db for every
//! table" requirement).
//!
//! This test queries the REAL live Postgres catalog (`information_schema`/
//! `pg_catalog`, not migration SQL text) for the canonical table inventory,
//! and cross-checks it against a hardcoded list of tables this crate has
//! typed Rust representation/access for. If a canonical table is created
//! later without adding its typed coverage here, this test FAILS and names
//! the missing table(s) explicitly — that is its entire purpose.
//!
//! # Coverage matrix (28 canonical tables, verified 2026-09-18 against a
//! live `postgres:18-alpine` instance running the full migration set)
//!
//! | Table                          | Rust row struct               | sentinel_rust access (per 0010_grants.sql) |
//! |---------------------------------|-------------------------------|---------------------------------------------|
//! | raw_observations                | RawObservationRow             | INSERT, SELECT (append-only, DM-02)         |
//! | decode_failures                 | DecodeFailureRow              | INSERT, UPDATE, SELECT                      |
//! | slots                           | SlotRow                       | INSERT, UPDATE, SELECT (monotonic)          |
//! | transactions                    | TransactionRow                | INSERT, UPDATE, SELECT (monotonic)          |
//! | instructions                    | InstructionRow                | INSERT, SELECT (immutable)                  |
//! | program_logs                    | ProgramLogRow                 | INSERT, SELECT (immutable)                  |
//! | account_observations            | AccountObservationRow         | INSERT, SELECT (immutable)                  |
//! | token_balance_deltas            | TokenBalanceDeltaRow          | INSERT, SELECT (immutable)                  |
//! | rollback_events                 | RollbackEventRow              | INSERT, SELECT (immutable)                  |
//! | gap_events                      | GapEventRow                   | INSERT, UPDATE, SELECT (promotable)         |
//! | ingest_checkpoints              | IngestCheckpointRow           | INSERT, UPDATE, SELECT (promotable)         |
//! | provider_health                 | ProviderHealthRow             | INSERT, UPDATE, SELECT (materialized)       |
//! | decoder_versions                | DecoderVersionRow             | INSERT, SELECT (immutable)                  |
//! | aegis_protocol_state            | AegisProtocolStateRow         | INSERT, UPDATE, SELECT (monotonic)          |
//! | aegis_markets                   | AegisMarketRow                | INSERT, UPDATE, SELECT (monotonic)          |
//! | aegis_market_params_history     | AegisMarketParamsHistoryRow   | INSERT, SELECT (immutable)                  |
//! | aegis_positions                 | AegisPositionRow              | INSERT, UPDATE, SELECT (monotonic)          |
//! | aegis_events                    | AegisEventRow                 | INSERT, SELECT (immutable)                  |
//! | aegis_oracle_observations       | AegisOracleObservationRow     | INSERT, SELECT (immutable)                  |
//! | aegis_invariant_checks          | AegisInvariantCheckRow        | INSERT, SELECT (immutable)                  |
//! | position_health                 | PositionHealthRow             | INSERT, SELECT (immutable-per-slot)         |
//! | liquidation_candidates          | LiquidationCandidateRow       | INSERT, UPDATE, SELECT (materialized)       |
//! | market_metrics                  | MarketMetricRow               | INSERT, UPDATE, SELECT (materialized)       |
//! | alerts                          | AlertRow                      | INSERT, UPDATE, SELECT (owner: any)         |
//! | execution_intents               | ExecutionIntentRow            | INSERT, SELECT (create only — TS advances)  |
//! | transaction_attempts            | TransactionAttemptRow         | none (owner: sentinel-executor, TS-side)    |
//! | reconciliation_mismatches       | ReconciliationMismatchRow     | INSERT, SELECT (immutable)                  |
//! | jobs                            | JobRow                        | INSERT, UPDATE, SELECT (control plane)      |
//!
//! `transaction_attempts` has a Rust row struct (`TransactionAttemptRow`)
//! and an insert helper (`insert_transaction_attempt`) already, even though
//! `sentinel_rust` holds no grant on it in `infra/migrations/0010_grants.sql`
//! — that helper predates this closure fix and is retained unchanged
//! (`crates/sentinel-jobs`/tests reference it); no NEW writer was invented
//! for a table `sentinel_rust` does not own.

use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use std::collections::BTreeSet;
use std::time::Duration;

fn bootstrap_url() -> String {
    std::env::var("SENTINEL_TEST_BOOTSTRAP_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://sentinel_bootstrap:sentinel_local_dev_only_bootstrap@127.0.0.1:5432/sentinel"
            .to_string()
    })
}

async fn connect() -> Option<PgPool> {
    match PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&bootstrap_url())
        .await
    {
        Ok(p) => Some(p),
        Err(e) => {
            eprintln!("skipping coverage_audit: Postgres not reachable: {e}");
            None
        }
    }
}

/// Every canonical table this crate has typed Rust access for — the row
/// struct list in `crate::tables` plus the pre-existing `TransactionAttemptRow`
/// (see the module doc above for the per-table justification). This is the
/// list that must equal the live catalog's table inventory, minus
/// `_sqlx_migrations` (schema-runner bookkeeping, not a canonical table).
fn rust_covered_tables() -> BTreeSet<&'static str> {
    [
        "raw_observations",
        "decode_failures",
        "slots",
        "transactions",
        "instructions",
        "program_logs",
        "account_observations",
        "token_balance_deltas",
        "rollback_events",
        "gap_events",
        "ingest_checkpoints",
        "provider_health",
        "decoder_versions",
        "aegis_protocol_state",
        "aegis_markets",
        "aegis_market_params_history",
        "aegis_positions",
        "aegis_events",
        "aegis_oracle_observations",
        "aegis_invariant_checks",
        "position_health",
        "liquidation_candidates",
        "market_metrics",
        "alerts",
        "execution_intents",
        "transaction_attempts",
        "reconciliation_mismatches",
        "jobs",
    ]
    .into_iter()
    .collect()
}

/// Queries the REAL catalog for every top-level table in the `public`
/// schema — partitioned parents (`relkind = 'p'`) and plain tables
/// (`relkind = 'r'`), excluding physical child partitions
/// (`relispartition`) since those are not independently-named canonical
/// tables — `docs/data-model.md` never lists a partition by its physical
/// name, only its parent.
async fn live_canonical_tables(pool: &PgPool) -> BTreeSet<String> {
    let rows = sqlx::query(
        "SELECT c.relname AS name \
         FROM pg_class c \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = 'public' \
           AND c.relkind IN ('r', 'p') \
           AND c.relispartition = false \
           AND c.relname <> '_sqlx_migrations'",
    )
    .fetch_all(pool)
    .await
    .expect("catalog query must succeed");
    rows.into_iter()
        .map(|r| r.try_get::<String, _>("name").unwrap())
        .collect()
}

#[tokio::test]
async fn every_canonical_table_has_typed_rust_coverage() {
    let Some(pool) = connect().await else {
        return;
    };

    let live = live_canonical_tables(&pool).await;
    let covered = rust_covered_tables();

    let missing_coverage: Vec<&String> = live
        .iter()
        .filter(|t| !covered.contains(t.as_str()))
        .collect();
    assert!(
        missing_coverage.is_empty(),
        "the following canonical table(s) exist in the live schema but have NO typed Rust row \
         struct / query coverage in crates/sentinel-db — this is exactly the closure defect this \
         test exists to prevent: {missing_coverage:?}"
    );

    // The reverse direction also matters: a name in `rust_covered_tables()`
    // that no longer exists in the schema means this list (and the doc
    // comment above) has drifted from reality and must be corrected.
    let stale_coverage: Vec<&&str> = covered.iter().filter(|t| !live.contains(**t)).collect();
    assert!(
        stale_coverage.is_empty(),
        "rust_covered_tables() names table(s) that no longer exist in the live schema — stale \
         coverage list, update it: {stale_coverage:?}"
    );

    assert_eq!(
        live.len(),
        28,
        "expected exactly 28 canonical tables per docs/data-model.md \
         (raw:2, normalized:5, chain-state:4, protocol:8, derived:4 incl. alerts, execution:3, \
         control-plane:1, minus slots counted once under chain-state / transactions under \
         normalized — see the coverage matrix in this file's module doc); got {}: {:?}",
        live.len(),
        live
    );
}

/// Proves the audit mechanism itself would fail if coverage regressed:
/// simulates "a table was added to the schema but not covered" without
/// touching the real schema or `rust_covered_tables()`, by asserting the
/// filter logic used above actually flags a synthetic gap.
#[test]
fn the_audit_mechanism_itself_detects_a_missing_table() {
    let covered = rust_covered_tables();
    let mut live_plus_one_uncovered_table: BTreeSet<String> =
        covered.iter().map(|s| s.to_string()).collect();
    live_plus_one_uncovered_table.insert("a_new_table_nobody_typed_yet".to_string());

    let missing_coverage: Vec<&String> = live_plus_one_uncovered_table
        .iter()
        .filter(|t| !covered.contains(t.as_str()))
        .collect();

    assert_eq!(
        missing_coverage,
        vec![&"a_new_table_nobody_typed_yet".to_string()],
        "the audit's own filter logic must name exactly the synthetic uncovered table"
    );
}
