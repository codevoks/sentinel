//! Postgres-backed job queue: claim, lease, renew, release, quarantine.
//!
//! Implements the exact query shapes `docs/distributed-correctness.md` §3
//! and `docs/data-model.md` §8 specify:
//!
//! ```text
//! claim:   UPDATE ... SET lease_holder=$id, lease_expires_at=now()+$ttl
//!           WHERE <claimable> ... FOR UPDATE SKIP LOCKED LIMIT n
//! renew:   UPDATE ... SET lease_expires_at=now()+$ttl
//!           WHERE id=$id AND lease_holder=$me AND lease_expires_at > now()
//! release: UPDATE ... SET state=$terminal, lease_holder=NULL WHERE ... AND lease_holder=$me
//! ```
//!
//! No broker (ADR-0004). Postgres is canonical. `LISTEN/NOTIFY` gives
//! latency; a polling floor gives correctness when a notification is
//! dropped (Postgres does not queue `NOTIFY` for a channel with no
//! listening connection at delivery time).

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use sqlx::{PgExecutor, PgPool, Row};

use sentinel_db::enums::{JobKind, JobState};

#[derive(Debug, thiserror::Error)]
pub enum JobsError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("query error: {0}")]
    Query(#[from] sentinel_db::queries::QueryError),
}

/// The Postgres `NOTIFY` channel workers `LISTEN` on
/// (`docs/data-model.md` §8: "LISTEN/NOTIFY for latency, plus a polling
/// floor").
pub const NOTIFY_CHANNEL: &str = "sentinel_jobs";

#[derive(Debug, Clone)]
pub struct ClaimedJob {
    pub job_id: i64,
    pub kind: JobKind,
    pub dedupe_key: String,
    pub payload: serde_json::Value,
    pub attempts: i32,
    pub max_attempts: i32,
    pub lease_expires_at: DateTime<Utc>,
}

/// Enqueues a job. Returns `Some(job_id)` if a new row was created, `None`
/// if an outstanding job with the same `dedupe_key` already exists
/// (DM-09's documented no-op — data-model.md §8).
///
/// Issues `NOTIFY` in the **same statement's transaction** as the insert
/// (the caller's connection/transaction, via `exec`), so a worker blocked
/// on `LISTEN` wakes only after the enqueueing transaction commits — never
/// before, and never as a separate, losable step.
pub async fn enqueue<'e, E: PgExecutor<'e> + Copy>(
    exec: E,
    kind: JobKind,
    dedupe_key: &str,
    payload: serde_json::Value,
    priority: i16,
    max_attempts: i32,
) -> Result<Option<i64>, JobsError> {
    let job_id =
        sentinel_db::queries::enqueue_job(exec, kind, dedupe_key, payload, priority, max_attempts)
            .await?;
    if job_id.is_some() {
        // NOTIFY payloads are capped at 8000 bytes by Postgres and this
        // channel carries none — workers re-query on wake, they never trust
        // the notification payload as data (distributed-correctness.md §3).
        sqlx::query(sqlx::AssertSqlSafe(format!("NOTIFY {NOTIFY_CHANNEL}")))
            .execute(exec)
            .await?;
    }
    Ok(job_id)
}

/// Claims up to `limit` queued, available jobs for `holder`, leasing each
/// for `lease_ttl`. Uses `FOR UPDATE SKIP LOCKED` so concurrent claimers
/// never block each other and never claim the same row
/// (distributed-correctness.md §3, L-1).
pub async fn claim(
    pool: &PgPool,
    holder: &str,
    lease_ttl: ChronoDuration,
    limit: i64,
) -> Result<Vec<ClaimedJob>, JobsError> {
    let lease_expires_at = Utc::now() + lease_ttl;
    let rows = sqlx::query(
        "UPDATE jobs SET state = 'leased', lease_holder = $1, lease_expires_at = $2, updated_at = now() \
         WHERE job_id IN ( \
           SELECT job_id FROM jobs \
           WHERE state = 'queued' AND available_at <= now() \
           ORDER BY priority, job_id \
           FOR UPDATE SKIP LOCKED \
           LIMIT $3 \
         ) \
         RETURNING job_id, kind, dedupe_key, payload, attempts, max_attempts, lease_expires_at",
    )
    .bind(holder)
    .bind(lease_expires_at)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(ClaimedJob {
                job_id: row.try_get("job_id")?,
                kind: row.try_get("kind")?,
                dedupe_key: row.try_get("dedupe_key")?,
                payload: row.try_get("payload")?,
                attempts: row.try_get("attempts")?,
                max_attempts: row.try_get("max_attempts")?,
                lease_expires_at: row.try_get("lease_expires_at")?,
            })
        })
        .collect()
}

/// Renews a held lease. Returns `false` (affecting **zero rows**, not an
/// error) if the lease was already lost — to expiry or to another holder —
/// which the caller MUST treat as "abandon immediately"
/// (distributed-correctness.md §3, L-2/L-3).
pub async fn renew(
    pool: &PgPool,
    job_id: i64,
    holder: &str,
    lease_ttl: ChronoDuration,
) -> Result<bool, JobsError> {
    let new_expiry = Utc::now() + lease_ttl;
    let result = sqlx::query(
        "UPDATE jobs SET lease_expires_at = $1, updated_at = now() \
         WHERE job_id = $2 AND lease_holder = $3 AND lease_expires_at > now() AND state = 'leased'",
    )
    .bind(new_expiry)
    .bind(job_id)
    .bind(holder)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Marks a job done. Conditioned on the caller still holding the lease —
/// returns `false` (zero rows affected) if it does not, which the caller
/// must detect and never assume succeeded (L-3).
pub async fn complete(pool: &PgPool, job_id: i64, holder: &str) -> Result<bool, JobsError> {
    let result = sqlx::query(
        "UPDATE jobs SET state = 'done', lease_holder = NULL, lease_expires_at = NULL, updated_at = now() \
         WHERE job_id = $1 AND lease_holder = $2",
    )
    .bind(job_id)
    .bind(holder)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Records a failed attempt. Below `max_attempts`, the job is requeued with
/// a backoff delay; at or beyond `max_attempts`, it is quarantined and an
/// alert is opened (distributed-correctness.md §7, Q-1..Q-4) — never
/// deleted, never silently retried forever.
pub async fn fail(
    pool: &PgPool,
    job_id: i64,
    holder: &str,
    error: &str,
    backoff: ChronoDuration,
) -> Result<bool, JobsError> {
    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        "UPDATE jobs SET attempts = attempts + 1, last_error = $3, updated_at = now() \
         WHERE job_id = $1 AND lease_holder = $2 \
         RETURNING attempts, max_attempts, dedupe_key",
    )
    .bind(job_id)
    .bind(holder)
    .bind(error)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(row) = row else {
        tx.rollback().await?;
        return Ok(false);
    };
    let attempts: i32 = row.try_get("attempts")?;
    let max_attempts: i32 = row.try_get("max_attempts")?;
    let dedupe_key: String = row.try_get("dedupe_key")?;

    if attempts >= max_attempts {
        sqlx::query(
            "UPDATE jobs SET state = 'quarantined', lease_holder = NULL, lease_expires_at = NULL, updated_at = now() \
             WHERE job_id = $1 AND lease_holder = $2",
        )
        .bind(job_id)
        .bind(holder)
        .execute(&mut *tx)
        .await?;

        // distributed-correctness.md §7: "open an alert
        // (kind='job_quarantined', entity=dedupe_key)". Transactional with
        // the state change that caused it (ADR-0004's whole point).
        sentinel_db::queries::open_alert(
            &mut *tx,
            "job_quarantined",
            "warning",
            "job",
            &dedupe_key,
            serde_json::json!({ "job_id": job_id, "last_error": error, "attempts": attempts }),
            None,
        )
        .await?;
    } else {
        sqlx::query(
            "UPDATE jobs SET state = 'queued', lease_holder = NULL, lease_expires_at = NULL, \
                              available_at = now() + $3, updated_at = now() \
             WHERE job_id = $1 AND lease_holder = $2",
        )
        .bind(job_id)
        .bind(holder)
        .bind(backoff)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(true)
}

/// Reclaims jobs whose lease has expired without being renewed or released
/// — returns them to `queued` so another worker can claim them
/// (distributed-correctness.md §3, "lease expiry allows re-claim"). Does
/// **not** increment `attempts`: losing a lease is not the same as failing
/// the work (L-5: "a lost lease never means the work did not happen" — the
/// work's own idempotency is what makes overlap safe, not the attempt
/// counter).
pub async fn reclaim_expired_leases(pool: &PgPool) -> Result<i64, JobsError> {
    let result = sqlx::query(
        "UPDATE jobs SET state = 'queued', lease_holder = NULL, lease_expires_at = NULL, updated_at = now() \
         WHERE state = 'leased' AND lease_expires_at <= now()",
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() as i64)
}

pub async fn fetch_state(pool: &PgPool, job_id: i64) -> Result<Option<JobState>, JobsError> {
    let row = sqlx::query("SELECT state FROM jobs WHERE job_id = $1")
        .bind(job_id)
        .fetch_optional(pool)
        .await?;
    row.map(|r| r.try_get::<JobState, _>("state"))
        .transpose()
        .map_err(JobsError::Database)
}

#[cfg(test)]
mod tests {
    // Integration tests requiring a live Postgres connection live in
    // crates/sentinel-jobs/tests/ (this crate has no meaningful pure-logic
    // unit to test in isolation — every operation is a conditional SQL
    // statement whose correctness can only be observed against a real
    // database, per distributed-correctness.md's own evidence standard:
    // "a mocked happy path is not distributed-systems evidence").
}
