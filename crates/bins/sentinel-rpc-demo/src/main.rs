//! Phase 3 demo (`docs/phases/phase-03-rpc.md` §26): fetches blocks/slots
//! through the real `RpcPool` against local Surfpool while a fault is
//! injected live on a second provider — demonstrating breaker open,
//! selection routing away, half-open probing, and recovery, with metrics
//! exported in Prometheus text format for the local Grafana/Prometheus
//! stack (`infra/compose`) to visualize.
//!
//! Zero-cost local (AGENTS.md §11): no paid RPC, no API key, no hosted
//! dependency — both providers point at the same local Surfpool instance
//! (`make up`); "provider failure" is simulated with `FaultInjectingProvider`
//! wrapping the second one, exactly as `phase-03-rpc.md` §17/§18 intends
//! fixtures/fault injection to be usable standalone or layered over a real
//! provider.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use opentelemetry::KeyValue;
use sentinel_core::Commitment;
use sentinel_rpc::budget::BudgetConfig;
use sentinel_rpc::fault::{FaultInjectingProvider, InjectedFault};
use sentinel_rpc::pool::{PoolConfig, ProviderSpec};
use sentinel_rpc::{
    BreakerConfig, HttpProviderConfig, HttpRpcProvider, ProviderId, RequestClass, RpcMethodCall,
    RpcPool, RpcProvider,
};
use sentinel_telemetry::Metrics;

const RPC_URL: &str = "http://127.0.0.1:8899";
const METRICS_ADDR: &str = "0.0.0.0:9464";

fn breaker_state_to_number(s: &str) -> f64 {
    match s {
        "closed" => 0.0,
        "degraded" => 1.0,
        "open" => 2.0,
        "half_open" => 3.0,
        _ => -1.0,
    }
}

async fn serve_metrics(metrics: Arc<Metrics>) {
    let listener = match TcpListener::bind(METRICS_ADDR).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!(
                "metrics server: failed to bind {METRICS_ADDR}: {e} (continuing without /metrics)"
            );
            return;
        }
    };
    println!("metrics: http://{METRICS_ADDR}/metrics (scraped by infra/compose/prometheus/prometheus.yml)");
    loop {
        let Ok((mut socket, _)) = listener.accept().await else {
            continue;
        };
        let metrics = metrics.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            // A minimal, single-request read is enough for a Prometheus
            // scrape — this is a demo endpoint, not a general HTTP server.
            let _ = socket.read(&mut buf).await;
            let body = metrics
                .export_text()
                .unwrap_or_else(|e| format!("# export error: {e}\n"));
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        });
    }
}

#[tokio::main]
async fn main() {
    println!("=== Sentinel Phase 3 RPC demo ===");
    println!("Requires: `make up` (Postgres/Surfpool/OTel/Prometheus/Grafana) — no paid RPC, no API key.\n");

    // Confirm Surfpool is actually reachable before doing anything else —
    // fail loudly and immediately rather than a confusing timeout later.
    if reqwest::Client::new()
        .post(RPC_URL)
        .json(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":"getVersion"}))
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .is_err()
    {
        eprintln!("Surfpool is not reachable at {RPC_URL}. Run `make up` first.");
        std::process::exit(1);
    }

    let metrics =
        Arc::new(Metrics::init().expect("telemetry metrics init must succeed with no network"));
    let meter = metrics.meter("sentinel_rpc_demo");
    let requests_counter = meter
        .u64_counter("sentinel_rpc_demo_requests_total")
        .build();
    let breaker_gauge = meter.f64_gauge("sentinel_rpc_demo_breaker_state").build();
    let p95_gauge = meter.f64_gauge("sentinel_rpc_demo_p95_latency_ms").build();
    tokio::spawn(serve_metrics(metrics.clone()));

    let mut classes = HashSet::new();
    classes.insert(RequestClass::RealtimeCompleteness);
    classes.insert(RequestClass::GapRepair);

    let primary = Arc::new(HttpRpcProvider::new(HttpProviderConfig {
        id: ProviderId("primary".to_string()),
        http_url: RPC_URL.to_string(),
        configured_classes: classes.clone(),
        request_timeout: Duration::from_secs(5),
    }));
    let secondary_inner = Arc::new(HttpRpcProvider::new(HttpProviderConfig {
        id: ProviderId("secondary".to_string()),
        http_url: RPC_URL.to_string(),
        configured_classes: classes.clone(),
        request_timeout: Duration::from_secs(5),
    }));
    // discover_capabilities populates the inner provider's node_version
    // etc.; FaultInjectingProvider delegates capabilities() to it.
    let _ = secondary_inner.discover_capabilities().await;
    let _ = primary.discover_capabilities().await;
    let secondary = Arc::new(FaultInjectingProvider::wrapping(secondary_inner));

    let pool = RpcPool::build(
        vec![
            ProviderSpec {
                provider: primary,
                configured_classes: classes.clone(),
                budget: BudgetConfig::new(200, 32),
            },
            ProviderSpec {
                provider: secondary.clone(),
                configured_classes: classes,
                budget: BudgetConfig::new(200, 32),
            },
        ],
        // Demo-tuned breaker: short cooldown and small sample size so the
        // full Closed -> Open -> HalfOpen -> Closed cycle is observable in
        // well under a minute, live. Production defaults
        // (`BreakerConfig::default()`) use a 30s cooldown and a 10-sample
        // minimum — correct for real traffic volume, too slow for a
        // person watching a terminal.
        BreakerConfig {
            // A small rolling window and low minimum sample size so the
            // failure ratio is dominated by *recent* attempts rather than
            // diluted by pre-fault history — needed for the fault's effect
            // to be visible within a live demo's timescale. Production
            // defaults (`BreakerConfig::default()`: window_size=50,
            // min_sample_size=10, open_cooldown=30s) are correct for real
            // traffic volume and deliberately not used here.
            window_size: 5,
            min_sample_size: 3,
            failure_ratio_threshold: 0.5,
            open_cooldown: Duration::from_secs(6),
            half_open_success_threshold: 2,
            ..Default::default()
        },
        PoolConfig::default(),
    )
    .await
    .expect("pool must build against a reachable, capable Surfpool");

    let iteration = AtomicU8::new(0);
    let last_provider_used: Mutex<String> = Mutex::new(String::new());

    println!("Phase 1/6: normal operation — both providers healthy.\n");

    for step in 0..40u32 {
        // Inject the fault window in the middle of the run.
        if step == 8 {
            println!(
                "\n>>> Phase 2/6: killing provider `secondary` (simulated provider failure) <<<\n"
            );
            secondary.always_fail(InjectedFault::ProviderFailure);
            // A quick burst of extra calls right after the fault starts:
            // health-weighted selection is probabilistic, so a handful of
            // calls at the normal cadence is not guaranteed to land on the
            // now-faulty provider enough times to trip its breaker within
            // a live demo's timescale. This burst exists only to make the
            // Open transition observable promptly; the breaker's own logic
            // (breaker.rs) is exactly what production traffic exercises.
            println!(
                "      (issuing a quick burst of calls so the breaker trip is observable promptly)"
            );
            for _ in 0..12 {
                let _ = pool
                    .call(
                        RpcMethodCall::GetSlot,
                        RequestClass::RealtimeCompleteness,
                        Commitment::Confirmed,
                    )
                    .await;
            }
        }
        if step == 20 {
            println!("\n>>> Phase 5/6: restoring provider `secondary` — watch it recover via half-open probes <<<\n");
            secondary.clear_always_fail();
        }

        let result = pool
            .call(
                RpcMethodCall::GetSlot,
                RequestClass::RealtimeCompleteness,
                Commitment::Confirmed,
            )
            .await;
        let n = iteration.fetch_add(1, Ordering::Relaxed);
        match &result {
            Ok(outcome) => {
                *last_provider_used.lock().unwrap_or_else(|e| e.into_inner()) =
                    outcome.provider_id.0.clone();
                println!("[{n:02}] getSlot OK via provider={}", outcome.provider_id);
                requests_counter.add(
                    1,
                    &[
                        KeyValue::new("outcome", "ok"),
                        KeyValue::new("provider", outcome.provider_id.0.clone()),
                    ],
                );
            }
            Err(e) => {
                println!("[{n:02}] getSlot FAILED: {e}");
                requests_counter.add(
                    1,
                    &[
                        KeyValue::new("outcome", "error"),
                        KeyValue::new("error_class", e.class_name().to_string()),
                    ],
                );
            }
        }

        // Exercise the half-open probe path directly and report it,
        // rather than only relying on it happening implicitly inside the
        // next `call()` — this makes "half-open probe occurs" an
        // observable, printed event (§26 acceptance).
        if let Some((probed_id, ok)) = pool.run_half_open_probe().await {
            println!(
                "      half-open probe on {probed_id}: {}",
                if ok { "SUCCESS" } else { "failed" }
            );
        }

        for h in pool.health() {
            breaker_gauge.record(
                breaker_state_to_number(&h.breaker_state),
                &[KeyValue::new("provider", h.provider_id.clone())],
            );
            if let Some(p95) = h.p95_ms {
                p95_gauge.record(
                    p95 as f64,
                    &[KeyValue::new("provider", h.provider_id.clone())],
                );
            }
            println!(
                "      health: provider={:<10} breaker={:<9} requests={:<4} errors={:<4} p95_ms={:?}",
                h.provider_id, h.breaker_state, h.requests, h.errors, h.p95_ms
            );
        }

        tokio::time::sleep(Duration::from_millis(600)).await;
    }

    println!("\n>>> Phase 6/6: final state <<<");
    for h in pool.health() {
        println!(
            "provider={:<10} breaker={:<9} requests={:<4} errors={:<4} divergence_events={}",
            h.provider_id, h.breaker_state, h.requests, h.errors, h.divergence_events
        );
    }
    println!("\nDemo complete. Metrics were exported at http://{METRICS_ADDR}/metrics throughout the run.");
    println!("With `make up` running, these were scraped by Prometheus (infra/compose/prometheus/prometheus.yml, job \"sentinel-rpc-demo\") and visible in Grafana at http://127.0.0.1:3001.");
}
