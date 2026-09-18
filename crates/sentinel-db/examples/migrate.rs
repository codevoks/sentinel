//! `make migrate` entry point: connect with bounded retry and apply every
//! pending migration.
//!
//! Deliberately a Cargo *example*, not a new crate: the connection
//! credential it needs is the bootstrap/superuser account (the only role
//! able to run `CREATE ROLE` in `infra/migrations/0001_init.sql`), which is
//! an operational concern, not a `docs/architecture.md` §5 dependency edge.
//! Reads configuration directly from the environment rather than through
//! `sentinel-config` so this stays a dev-dependency, not a change to
//! `sentinel-db`'s declared production dependency edge (`core` only).

use std::env;
use std::process::ExitCode;
use std::time::Duration;

use sentinel_db::{connect_with_retry, run_migrations, ConnectOptions};

fn env_or_fail(key: &str) -> Result<String, String> {
    env::var(key).map_err(|_| format!("missing required environment variable: {key}"))
}

#[tokio::main]
async fn main() -> ExitCode {
    let result: Result<(), String> = async {
        let host = env_or_fail("SENTINEL_DB_HOST")?;
        let port: u16 = env_or_fail("SENTINEL_DB_PORT")?
            .parse()
            .map_err(|e| format!("invalid SENTINEL_DB_PORT: {e}"))?;
        let user = env_or_fail("SENTINEL_DB_USER")?;
        let password = env_or_fail("SENTINEL_DB_PASSWORD")?;
        let database = env_or_fail("SENTINEL_DB_NAME")?;

        let opts = ConnectOptions {
            host,
            port,
            user,
            password,
            database,
            max_attempts: 10,
            retry_backoff: Duration::from_secs(1),
            attempt_timeout: Duration::from_secs(3),
        };

        eprintln!(
            "connecting to postgres at {}:{} (bounded retry: {} attempts)",
            opts.host, opts.port, opts.max_attempts
        );
        let pool = connect_with_retry(&opts)
            .await
            .map_err(|e| format!("connect failed: {e}"))?;

        eprintln!("running migrations from infra/migrations");
        run_migrations(&pool)
            .await
            .map_err(|e| format!("migration failed: {e}"))?;

        eprintln!("migrations applied successfully");
        Ok(())
    }
    .await;

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("sentinel-db migrate: {msg}");
            ExitCode::FAILURE
        }
    }
}
