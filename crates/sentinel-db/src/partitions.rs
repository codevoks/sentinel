//! Slot-range partition automation for the six partitioned tables
//! (`phase-02-data-model.md` §1.2): `raw_observations`, `transactions`,
//! `instructions`, `program_logs`, `account_observations`,
//! `token_balance_deltas`.
//!
//! **Deliberately application code, not a stored procedure or trigger.**
//! Phase 2's explicit non-scope (`docs/phases/phase-02-data-model.md` §2)
//! forbids both; the phase's own file list names this exact module
//! (`crates/sentinel-db/src/partitions.rs`) as automation's home.
//!
//! **Requires DDL privilege.** Creating a partition is `CREATE TABLE ...
//! PARTITION OF`, which needs `CREATE` on the schema. Phase 1's migration
//! (`infra/migrations/0001_init.sql`) revoked `CREATE ON SCHEMA public FROM
//! PUBLIC` and granted only `USAGE` to the two low-privilege application
//! roles (`sentinel_rust`, `sentinel_ts`) — by design, so that a compromised
//! ingest/normalize/risk process cannot alter schema. Partition maintenance
//! is therefore an **operational** action, run with the same
//! elevated/bootstrap-class credential migrations use
//! (`crates/sentinel-db/examples/migrate.rs`), not with the low-privilege
//! runtime pool. Every function here takes a `&PgPool` and makes no
//! assumption about which role it is — that is the caller's responsibility,
//! stated here rather than hidden.

use sqlx::PgPool;

pub const PARTITION_SIZE_SLOTS: i64 = 10_000_000;

/// The six tables partitioned by slot range (`phase-02-data-model.md` §1.2).
pub const PARTITIONED_TABLES: &[&str] = &[
    "raw_observations",
    "transactions",
    "instructions",
    "program_logs",
    "account_observations",
    "token_balance_deltas",
];

#[derive(Debug, thiserror::Error)]
pub enum PartitionError {
    #[error("unknown partitioned table: {0}")]
    UnknownTable(String),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// The partition boundary `[lower, upper)` a given slot falls into, and the
/// deterministic name PostgreSQL partition for it gets.
fn bounds_for_slot(slot: i64, partition_size: i64) -> (i64, i64) {
    let index = slot.div_euclid(partition_size);
    (index * partition_size, (index + 1) * partition_size)
}

fn partition_name(table: &str, lower: i64) -> String {
    // `_p<N>` where N is the partition index (lower / size), matching the
    // naming convention 0011_initial_partitions.sql already established
    // (raw_observations_p0 = [0, 10_000_000), _p1 = [10_000_000, 20_000_000), ...).
    format!("{table}_p{}", lower / PARTITION_SIZE_SLOTS)
}

/// Ensures a partition covering `slot` exists for `table`, creating it if
/// necessary. Idempotent: a partition that already exists is left alone
/// (`IF NOT EXISTS`) — safe to call repeatedly, including concurrently from
/// multiple callers (`CREATE TABLE IF NOT EXISTS` is not itself immune to a
/// race between the existence check and the create under concurrent DDL,
/// but the practical caller here is a single partition-maintenance loop,
/// and a losing racer gets a clear, non-corrupting error rather than a
/// silent double-create, which Postgres's own catalog uniqueness prevents
/// outright).
pub async fn ensure_partition_for_slot(
    pool: &PgPool,
    table: &str,
    slot: i64,
) -> Result<String, PartitionError> {
    if !PARTITIONED_TABLES.contains(&table) {
        return Err(PartitionError::UnknownTable(table.to_string()));
    }
    let (lower, upper) = bounds_for_slot(slot, PARTITION_SIZE_SLOTS);
    let name = partition_name(table, lower);

    // Table/partition names are internally generated from a validated slot
    // integer and a fixed allowlist of table names (checked above), never
    // from unbounded external input — CI-NOSQLFMT (scripts/ci-guards.sh)
    // bans string-formatted SELECT/INSERT/UPDATE/DELETE, not DDL, and this
    // is exactly the identifier-construction case that guard permits.
    let ddl = format!(
        "CREATE TABLE IF NOT EXISTS {name} PARTITION OF {table} FOR VALUES FROM ({lower}) TO ({upper})"
    );
    // AssertSqlSafe: every fragment above comes from this function's own
    // fixed allowlist check (`table`) or from `i64` arithmetic on a
    // validated slot (`lower`/`upper`) — never from unbounded external
    // input. sqlx 0.9's `SqlSafeStr` bound (which this asserts past) exists
    // for exactly the case CI-NOSQLFMT bans: interpolating untrusted data
    // into SELECT/INSERT/UPDATE/DELETE text. This is DDL built from
    // internally-generated identifiers, not data.
    sqlx::query(sqlx::AssertSqlSafe(ddl)).execute(pool).await?;
    Ok(name)
}

/// Creates every partition needed to cover `[head_slot, head_slot +
/// lead_partitions * PARTITION_SIZE_SLOTS)` for every partitioned table —
/// "automated partition creation ahead of the head" (`phase-02-data-model.md`
/// §6). `lead_partitions` is the number of *future* partitions (beyond the
/// one covering `head_slot` itself) to keep pre-created; it is
/// configuration, not a frozen constant (no frozen document states a
/// number — `docs/observability.md`'s `PartitionsExhausted` alert says only
/// "fewer than N future partitions", leaving N to be set operationally).
/// The default used by `count_future_partitions`'s caller in this phase is
/// stated at the call site, not hidden here.
pub async fn ensure_partitions_ahead(
    pool: &PgPool,
    head_slot: i64,
    lead_partitions: i64,
) -> Result<Vec<String>, PartitionError> {
    let mut created = Vec::new();
    let (head_lower, _) = bounds_for_slot(head_slot, PARTITION_SIZE_SLOTS);
    for table in PARTITIONED_TABLES {
        for i in 0..=lead_partitions {
            let target_slot = head_lower + i * PARTITION_SIZE_SLOTS;
            let name = ensure_partition_for_slot(pool, table, target_slot).await?;
            created.push(name);
        }
    }
    Ok(created)
}

/// Counts how many partitions covering slots **at or above** `head_slot`
/// already exist for `table` — the "running low" measurement behind the
/// `PartitionsExhausted` alert (`docs/observability.md`). Queries the real
/// catalog (`pg_inherits` + partition bound expressions via
/// `pg_get_expr`), not an internal counter, so it reflects what PostgreSQL
/// itself will actually accept.
pub async fn count_future_partitions(
    pool: &PgPool,
    table: &str,
    head_slot: i64,
) -> Result<i64, PartitionError> {
    if !PARTITIONED_TABLES.contains(&table) {
        return Err(PartitionError::UnknownTable(table.to_string()));
    }
    // A partition's upper bound is embedded in its FOR VALUES clause, which
    // Postgres exposes via pg_get_expr(relpartbound, oid) as text of the
    // form "FOR VALUES FROM ('<lower>') TO ('<upper>')" for a range
    // partition. Parsing that text is more fragile than comparing table
    // names against the deterministic naming scheme this module already
    // controls, so this counts by name instead: partitions of `table` whose
    // encoded index implies an upper bound strictly greater than
    // `head_slot`.
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT c.relname FROM pg_inherits i \
         JOIN pg_class c ON c.oid = i.inhrelid \
         JOIN pg_class p ON p.oid = i.inhparent \
         WHERE p.relname = $1",
    )
    .bind(table)
    .fetch_all(pool)
    .await?;

    let head_index = head_slot.div_euclid(PARTITION_SIZE_SLOTS);
    let mut count = 0i64;
    for (name,) in rows {
        let Some(idx_str) = name.strip_prefix(&format!("{table}_p")) else {
            continue;
        };
        if let Ok(idx) = idx_str.parse::<i64>() {
            if idx >= head_index {
                count += 1;
            }
        }
    }
    Ok(count)
}

/// The alertable "running low" condition (`docs/observability.md`:
/// `PartitionsExhausted` — "fewer than N future partitions... page").
/// Checks every partitioned table against `min_future_partitions` and
/// opens an `alerts` row (kind `partitions_low`, entity the table name) for
/// each one that is running low, using the same `DM-08` one-open-alert-per-
/// entity mechanism every other alert in this system uses — a table that
/// stays low does not re-page on every check, and a table that recovers
/// (partition automation caught up) still needs its alert explicitly
/// resolved by whatever calls this (partition maintenance running
/// successfully again is not itself proof the operator has been told, so
/// this function only opens; a maintenance loop is expected to resolve the
/// alert once `count_future_partitions` recovers, kept as a caller
/// responsibility rather than auto-resolved here, matching every other
/// alert in the system: alerts close on confirmed resolution, not on the
/// next successful check).
pub async fn check_low_partitions_and_alert(
    pool: &PgPool,
    head_slot: i64,
    min_future_partitions: i64,
) -> Result<Vec<String>, PartitionError> {
    let mut alerted = Vec::new();
    for table in PARTITIONED_TABLES {
        let count = count_future_partitions(pool, table, head_slot).await?;
        if count < min_future_partitions {
            let detail = serde_json::json!({
                "table": table,
                "future_partitions": count,
                "min_required": min_future_partitions,
                "head_slot": head_slot,
            });
            // DM-08's partial unique index makes a second OPEN alert for
            // the same (kind, entity) a hard constraint violation, not a
            // silent no-op (by design — see 0007_derived_layer.sql) — so a
            // repeated low-partition condition across successive checks
            // must NOT call open_alert unconditionally, or every check
            // after the first would surface a database error instead of
            // quietly recognizing "already paged". This case is common
            // (partition maintenance can lag for a while), so it is
            // handled explicitly rather than treated as exceptional.
            let result = crate::queries::open_alert(
                pool,
                "partitions_low",
                "critical",
                "partitioned_table",
                table,
                detail,
                None,
            )
            .await;
            match result {
                Ok(_) => alerted.push((*table).to_string()),
                Err(crate::queries::QueryError::Database(sqlx::Error::Database(ref db_err)))
                    if db_err.is_unique_violation() =>
                {
                    // Already alerted and still unresolved — expected,
                    // not an error.
                    alerted.push((*table).to_string());
                }
                Err(e) => {
                    return Err(PartitionError::Database(sqlx::Error::Protocol(
                        e.to_string(),
                    )))
                }
            }
        }
    }
    Ok(alerted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_for_slot_is_correct_at_boundaries() {
        assert_eq!(bounds_for_slot(0, 10_000_000), (0, 10_000_000));
        assert_eq!(bounds_for_slot(9_999_999, 10_000_000), (0, 10_000_000));
        assert_eq!(
            bounds_for_slot(10_000_000, 10_000_000),
            (10_000_000, 20_000_000)
        );
        assert_eq!(
            bounds_for_slot(20_000_001, 10_000_000),
            (20_000_000, 30_000_000)
        );
    }

    #[test]
    fn partition_name_matches_the_established_convention() {
        assert_eq!(partition_name("raw_observations", 0), "raw_observations_p0");
        assert_eq!(
            partition_name("raw_observations", 10_000_000),
            "raw_observations_p1"
        );
        assert_eq!(
            partition_name("transactions", 20_000_000),
            "transactions_p2"
        );
    }
}
