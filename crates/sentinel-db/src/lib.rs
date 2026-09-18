//! Postgres access layer for Sentinel: connection management and the
//! migration runner.
//!
//! Phase 1 scope only: connect with bounded retry, and run/verify
//! migrations. No typed row structs exist yet because no business schema
//! exists yet (`docs/phases/phase-01-foundation.md` §2, explicit non-scope).

use std::time::Duration;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;

/// The single source of schema truth (`docs/architecture.md` §4).
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../infra/migrations");

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("failed to connect to Postgres after {attempts} attempt(s): {reason}")]
    ConnectFailed { attempts: u32, reason: String },

    #[error("migration failed: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

#[derive(Debug, Clone)]
pub struct ConnectOptions {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
    pub max_attempts: u32,
    pub retry_backoff: Duration,
    /// Hard ceiling on a single connection attempt. A closed/firewalled
    /// port must not be allowed to stall the bounded-retry guarantee behind
    /// an underlying library's own connect timeout.
    pub attempt_timeout: Duration,
}

/// Connects to Postgres with a **bounded** retry loop.
///
/// Postgres being briefly unavailable during Compose startup is expected;
/// Postgres being unavailable forever is not, and must fail loudly rather
/// than retry forever (`docs/phases/phase-01-foundation.md` §8).
pub async fn connect_with_retry(opts: &ConnectOptions) -> Result<PgPool, DbError> {
    let connect_opts = PgConnectOptions::new()
        .host(&opts.host)
        .port(opts.port)
        .username(&opts.user)
        .password(&opts.password)
        .database(&opts.database);

    let mut last_reason = "no attempt was made".to_string();
    for attempt in 1..=opts.max_attempts {
        let attempt_result = tokio::time::timeout(
            opts.attempt_timeout,
            PgPoolOptions::new()
                .max_connections(5)
                .connect_with(connect_opts.clone()),
        )
        .await;

        let reason = match attempt_result {
            Ok(Ok(pool)) => return Ok(pool),
            Ok(Err(e)) => e.to_string(),
            Err(_elapsed) => format!("attempt exceeded {:?} timeout", opts.attempt_timeout),
        };

        tracing::warn!(
            attempt,
            max_attempts = opts.max_attempts,
            error = %reason,
            "postgres connection attempt failed"
        );
        last_reason = reason;
        if attempt < opts.max_attempts {
            tokio::time::sleep(opts.retry_backoff).await;
        }
    }

    Err(DbError::ConnectFailed {
        attempts: opts.max_attempts,
        reason: last_reason,
    })
}

/// Runs every pending migration. Safe to call repeatedly: sqlx tracks
/// applied migrations by checksum in `_sqlx_migrations` and is a no-op on a
/// database that is already up to date.
pub async fn run_migrations(pool: &PgPool) -> Result<(), DbError> {
    MIGRATOR.run(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_database_url() -> String {
        std::env::var("SENTINEL_TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgres://sentinel_bootstrap:sentinel_local_dev_only_bootstrap@127.0.0.1:5432/sentinel"
                .to_string()
        })
    }

    async fn connect_for_test() -> Option<PgPool> {
        let url = test_database_url();
        let connect = PgPoolOptions::new()
            .acquire_timeout(Duration::from_secs(2))
            .connect(&url);
        match connect.await {
            Ok(pool) => Some(pool),
            Err(e) => {
                eprintln!(
                    "skipping sentinel-db integration test: Postgres not reachable at {}: {e}\n\
                     start it with `make up` (this test only ever talks to loopback)",
                    test_database_url()
                );
                None
            }
        }
    }

    #[tokio::test]
    async fn migrate_from_empty_database_succeeds() {
        let Some(pool) = connect_for_test().await else {
            return;
        };
        run_migrations(&pool)
            .await
            .expect("migrations must apply cleanly");

        let role_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_catalog.pg_roles WHERE rolname IN ('sentinel_rust', 'sentinel_ts')",
        )
        .fetch_one(&pool)
        .await
        .expect("role query must succeed");
        assert_eq!(
            role_count, 2,
            "0001_init.sql must create both least-privilege application roles"
        );
    }

    #[tokio::test]
    async fn migration_re_run_is_idempotent() {
        let Some(pool) = connect_for_test().await else {
            return;
        };
        run_migrations(&pool).await.expect("first run must succeed");
        run_migrations(&pool)
            .await
            .expect("second run against an up-to-date database must be a no-op, not an error");
    }

    #[tokio::test]
    async fn connect_with_retry_fails_clearly_and_boundedly_when_postgres_is_unreachable() {
        // Port 1 is a real, syntactically valid port that nothing binds to
        // as an unprivileged Postgres server, so this exercises the actual
        // bounded-retry-then-fail path without depending on Postgres being
        // up or down elsewhere.
        let opts = ConnectOptions {
            host: "127.0.0.1".to_string(),
            port: 1,
            user: "sentinel_bootstrap".to_string(),
            password: "irrelevant".to_string(),
            database: "sentinel".to_string(),
            max_attempts: 2,
            retry_backoff: Duration::from_millis(10),
            attempt_timeout: Duration::from_secs(2),
        };

        let started = std::time::Instant::now();
        let result = connect_with_retry(&opts).await;
        let elapsed = started.elapsed();

        assert!(
            result.is_err(),
            "connecting to an unreachable Postgres must fail, not hang"
        );
        match result.unwrap_err() {
            DbError::ConnectFailed { attempts, .. } => assert_eq!(attempts, 2),
            other => panic!("expected ConnectFailed, got {other:?}"),
        }
        assert!(
            elapsed < Duration::from_secs(10),
            "bounded retry must not silently retry forever; took {elapsed:?}"
        );
    }
}
