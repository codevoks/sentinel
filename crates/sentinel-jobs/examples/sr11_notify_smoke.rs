//! SR-11 (`docs/project-status.md`): Phase 2's stated obligation is a
//! **smoke-level** `LISTEN/NOTIFY` latency measurement (not Phase 14's full
//! throughput campaign). This program does exactly that against the real
//! local Postgres: it opens a `PgListener` on `sentinel_jobs::NOTIFY_CHANNEL`,
//! then enqueues N jobs one at a time (each enqueue's `NOTIFY` happens
//! inside `sentinel_jobs::enqueue`, in the same statement/transaction as
//! the insert), and measures the wall-clock time from just before each
//! enqueue call to the corresponding notification's arrival.
//!
//! Run: `cargo run -p sentinel-jobs --example sr11_notify_smoke` against a
//! running local stack (`make up`). Output is real, not fabricated —
//! recorded verbatim in `docs/project-status.md`.

use std::time::{Duration, Instant};

use sqlx::postgres::{PgListener, PgPoolOptions};

use sentinel_db::enums::JobKind;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("SENTINEL_DB_URL").unwrap_or_else(|_| {
        "postgres://sentinel_rust:sentinel_local_dev_only_rust@127.0.0.1:5432/sentinel".to_string()
    });

    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await?;
    let mut listener = PgListener::connect(&url).await?;
    listener.listen(sentinel_jobs::NOTIFY_CHANNEL).await?;

    const ROUNDS: usize = 50;
    let mut latencies = Vec::with_capacity(ROUNDS);
    let run_id = uuid::Uuid::new_v4();

    for i in 0..ROUNDS {
        let dedupe_key = format!("sr11-smoke-{run_id}-{i}");
        let started = Instant::now();
        sentinel_jobs::enqueue(
            &pool,
            JobKind::BackfillRange,
            &dedupe_key,
            serde_json::json!({}),
            0,
            1,
        )
        .await?;

        let recv_result = tokio::time::timeout(Duration::from_secs(5), listener.recv()).await;
        match recv_result {
            Ok(Ok(_notification)) => {
                latencies.push(started.elapsed());
            }
            Ok(Err(e)) => {
                eprintln!("round {i}: listener error: {e}");
            }
            Err(_) => {
                eprintln!("round {i}: TIMED OUT waiting for NOTIFY after 5s");
            }
        }
    }

    latencies.sort();
    let n = latencies.len();
    if n == 0 {
        eprintln!("no notifications received — SR-11 smoke measurement FAILED to produce data");
        std::process::exit(1);
    }
    let sum: Duration = latencies.iter().sum();
    let mean = sum / n as u32;
    let p50 = latencies[n / 2];
    let p95 = latencies[(n * 95 / 100).min(n - 1)];
    let max = latencies[n - 1];

    println!("SR-11 LISTEN/NOTIFY smoke measurement");
    println!("rounds attempted: {ROUNDS}, rounds with a received notification: {n}");
    println!("mean: {mean:?}");
    println!("p50:  {p50:?}");
    println!("p95:  {p95:?}");
    println!("max:  {max:?}");

    Ok(())
}
