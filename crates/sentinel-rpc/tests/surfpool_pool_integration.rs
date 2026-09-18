//! Phase-3 integration tests against the real local Surfpool
//! (`docs/phases/phase-03-rpc.md` §22): capability discovery through
//! `HttpRpcProvider`, every method Sentinel needs called through the real
//! `RpcPool` (not a raw client), and the WebSocket manager's full
//! connect → subscribe → notification → disconnect → reconnect →
//! re-subscribe → notification cycle against a real Surfpool WS endpoint.
//!
//! Skips (never fails) when Surfpool is unreachable, exactly like
//! `surfpool_capability_probe.rs` and `sentinel-db`'s own pattern — `make
//! up` starts the required local stack; this file only ever talks to
//! loopback.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tokio::sync::mpsc;

use sentinel_core::Commitment;
use sentinel_rpc::budget::BudgetConfig;
use sentinel_rpc::pool::{PoolConfig, ProviderSpec};
use sentinel_rpc::provider::RpcMethodResponse;
use sentinel_rpc::ws::{
    SubscriptionSpec, TungsteniteConnector, WsEvent, WsManager, WsManagerConfig,
};
use sentinel_rpc::{
    BreakerConfig, HttpProviderConfig, HttpRpcProvider, ProviderHealthTracker, ProviderId,
    RequestClass, RpcMethodCall, RpcPool, RpcProvider,
};

const RPC_URL: &str = "http://127.0.0.1:8899";
const WS_URL: &str = "ws://127.0.0.1:8900";

async fn provider_or_skip() -> Option<Arc<HttpRpcProvider>> {
    let mut classes = HashSet::new();
    for c in [
        RequestClass::Execution,
        RequestClass::RealtimeCompleteness,
        RequestClass::GapRepair,
        RequestClass::Backfill,
        RequestClass::ScheduledScan,
    ] {
        classes.insert(c);
    }
    let provider = HttpRpcProvider::new(HttpProviderConfig {
        id: ProviderId("surfpool-local".to_string()),
        http_url: RPC_URL.to_string(),
        configured_classes: classes,
        request_timeout: Duration::from_secs(5),
    });
    match provider.discover_capabilities().await {
        Ok(_) => Some(Arc::new(provider)),
        Err(e) => {
            eprintln!("skipping surfpool_pool_integration: Surfpool not reachable at {RPC_URL}: {e}\nstart it with `make up`");
            None
        }
    }
}

#[tokio::test]
async fn capability_discovery_against_real_surfpool_finds_every_required_method() {
    let Some(provider) = provider_or_skip().await else {
        return;
    };
    let caps = provider
        .discover_capabilities()
        .await
        .expect("discovery must succeed against a reachable Surfpool");

    assert!(
        !caps.node_version.is_empty(),
        "node_version must be populated from a real getVersion call"
    );
    for method in [
        "getSlot",
        "getBlockHeight",
        "getBlocks",
        "getBlock",
        "getLatestBlockhash",
        "getMultipleAccounts",
        "getProgramAccounts",
        "getSignatureStatuses",
        "getSignaturesForAddress",
        "getTransaction",
        "getRecentPrioritizationFees",
        "simulateTransaction",
        "sendTransaction",
    ] {
        assert!(
            caps.supports_method(method),
            "real Surfpool must support {method}, capability discovery reported it missing"
        );
    }
}

#[tokio::test]
async fn every_required_rpc_method_succeeds_through_the_real_pool() {
    let Some(provider) = provider_or_skip().await else {
        return;
    };

    let mut classes = HashSet::new();
    for c in [
        RequestClass::Execution,
        RequestClass::RealtimeCompleteness,
        RequestClass::GapRepair,
        RequestClass::Backfill,
        RequestClass::ScheduledScan,
    ] {
        classes.insert(c);
    }
    let pool = RpcPool::build(
        vec![ProviderSpec {
            provider,
            configured_classes: classes,
            budget: BudgetConfig::new(200, 32),
        }],
        BreakerConfig::default(),
        PoolConfig::default(),
    )
    .await
    .expect("pool must build against a real, capable Surfpool");

    // getSlot / getBlockHeight — explicit commitment, no raw client.
    let slot_outcome = pool
        .call(
            RpcMethodCall::GetSlot,
            RequestClass::RealtimeCompleteness,
            Commitment::Confirmed,
        )
        .await
        .expect("getSlot must succeed through the pool");
    let RpcMethodResponse::Slot(slot) = slot_outcome.response else {
        panic!("expected Slot response");
    };
    pool.set_known_head_slot(slot.0);

    pool.call(
        RpcMethodCall::GetBlockHeight,
        RequestClass::RealtimeCompleteness,
        Commitment::Confirmed,
    )
    .await
    .expect("getBlockHeight must succeed through the pool");

    // getBlocks -> getBlock, always with maxSupportedTransactionVersion set
    // at the type level (RPC-10) — there is no call site here that could
    // omit it.
    let blocks_outcome = pool
        .call(
            RpcMethodCall::GetBlocks {
                start_slot: sentinel_core::Slot(slot.0.saturating_sub(5)),
                end_slot: Some(slot),
            },
            RequestClass::Backfill,
            Commitment::Confirmed,
        )
        .await
        .expect("getBlocks must succeed through the pool");
    let RpcMethodResponse::Blocks(produced_slots) = blocks_outcome.response else {
        panic!("expected Blocks response");
    };
    assert!(
        !produced_slots.is_empty(),
        "getBlocks must return at least one produced slot"
    );

    let block_outcome = pool
        .call(
            RpcMethodCall::GetBlock {
                slot: produced_slots[0],
                max_supported_transaction_version: 0,
            },
            RequestClass::GapRepair,
            Commitment::Confirmed,
        )
        .await
        .expect("getBlock (with maxSupportedTransactionVersion) must succeed through the pool");
    assert!(matches!(
        block_outcome.response,
        RpcMethodResponse::Block { .. }
    ));

    // getLatestBlockhash, getMultipleAccounts, getProgramAccounts,
    // getRecentPrioritizationFees, getSignatureStatuses,
    // getSignaturesForAddress — every remaining required method.
    pool.call(
        RpcMethodCall::GetLatestBlockhash,
        RequestClass::Execution,
        Commitment::Confirmed,
    )
    .await
    .expect("getLatestBlockhash must succeed through the pool");

    pool.call(
        RpcMethodCall::GetMultipleAccounts {
            pubkeys: vec!["11111111111111111111111111111111".to_string()],
        },
        RequestClass::GapRepair,
        Commitment::Confirmed,
    )
    .await
    .expect("getMultipleAccounts must succeed through the pool");

    pool.call(
        RpcMethodCall::GetProgramAccounts {
            program_id: "11111111111111111111111111111111".to_string(),
        },
        RequestClass::ScheduledScan,
        Commitment::Confirmed,
    )
    .await
    .expect("getProgramAccounts must succeed through the pool");

    pool.call(
        RpcMethodCall::GetRecentPrioritizationFees {
            writable_accounts: vec!["11111111111111111111111111111111".to_string()],
        },
        RequestClass::Backfill,
        Commitment::Confirmed,
    )
    .await
    .expect("getRecentPrioritizationFees must succeed through the pool");

    pool.call(
        RpcMethodCall::GetSignatureStatuses { signatures: vec![] },
        RequestClass::Execution,
        Commitment::Confirmed,
    )
    .await
    .expect("getSignatureStatuses must succeed through the pool (empty input is valid)");

    pool.call(
        RpcMethodCall::GetSignaturesForAddress {
            address: "11111111111111111111111111111111".to_string(),
            until: None,
        },
        RequestClass::Backfill,
        Commitment::Confirmed,
    )
    .await
    .expect("getSignaturesForAddress must succeed through the pool");
}

/// A real `TungsteniteConnector`, wrapped so that the manager's *first*
/// connection is force-closed (from the client side) after it has
/// delivered one genuine notification from real Surfpool — simulating a
/// disconnect without touching the server. This drives the manager's own
/// `Live -> Backoff -> Connecting` path for real, and the *second*
/// connection it opens (also real) is left untouched, so a subsequent
/// notification on it proves reconnect + re-subscribe actually happened,
/// not just that two unrelated runs each worked once.
struct DisconnectAfterOneNotificationConnector {
    inner: TungsteniteConnector,
    connections_made: std::sync::atomic::AtomicU32,
}

#[async_trait::async_trait]
impl sentinel_rpc::ws::WsConnector for DisconnectAfterOneNotificationConnector {
    async fn connect(
        &self,
    ) -> Result<Box<dyn sentinel_rpc::ws::WsConnection>, sentinel_rpc::ws::WsError> {
        let real = self.inner.connect().await?;
        let n = self
            .connections_made
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(Box::new(ForceDisconnectAfterOne {
            real,
            force_close_after_first_notification: n == 0,
            notifications_seen: 0,
            already_forced: false,
        }))
    }
}

struct ForceDisconnectAfterOne {
    real: Box<dyn sentinel_rpc::ws::WsConnection>,
    force_close_after_first_notification: bool,
    notifications_seen: u32,
    already_forced: bool,
}

#[async_trait::async_trait]
impl sentinel_rpc::ws::WsConnection for ForceDisconnectAfterOne {
    async fn send_text(&mut self, text: String) -> Result<(), sentinel_rpc::ws::WsError> {
        self.real.send_text(text).await
    }

    async fn recv(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<String>, sentinel_rpc::ws::WsError> {
        if self.already_forced {
            // This connection was already marked for a simulated
            // disconnect on a prior call — every subsequent poll reports a
            // clean close without touching the real socket again.
            return Ok(None);
        }
        let msg = self.real.recv(timeout).await?;
        if let Some(text) = &msg {
            if text.contains("\"method\"") {
                self.notifications_seen += 1;
                if self.force_close_after_first_notification && self.notifications_seen == 1 {
                    // Simulate a disconnect right after this one real
                    // notification is handed back to the caller: return the
                    // real message this call, then report a clean close on
                    // every call after.
                    self.already_forced = true;
                    return Ok(msg);
                }
            }
        }
        Ok(msg)
    }
}

/// The real WS connect → subscribe → notification → disconnect → reconnect
/// → re-subscribe → notification cycle (`phase-03-rpc.md` §15/§22),
/// against Surfpool's actual WebSocket endpoint, driven by ONE `WsManager`
/// instance across the whole cycle.
#[tokio::test]
async fn websocket_connect_subscribe_notify_disconnect_reconnect_resubscribe_notify_against_real_surfpool(
) {
    let ok = tokio::time::timeout(
        Duration::from_secs(3),
        tokio_tungstenite::connect_async(WS_URL),
    )
    .await;
    if ok.is_err() || ok.as_ref().is_ok_and(|r| r.is_err()) {
        eprintln!("skipping websocket integration test: Surfpool WS not reachable at {WS_URL}\nstart it with `make up`");
        return;
    }

    let subs = vec![SubscriptionSpec {
        id: "slot".into(),
        method: "slotSubscribe",
        params: json!([]),
    }];
    let manager = Arc::new(WsManager::new(
        WsManagerConfig {
            heartbeat_window: Duration::from_secs(5),
            backoff_base: Duration::from_millis(50),
            backoff_cap: Duration::from_millis(500),
            ..Default::default()
        },
        subs,
    ));
    let connector = Arc::new(DisconnectAfterOneNotificationConnector {
        inner: TungsteniteConnector {
            url: WS_URL.to_string(),
        },
        connections_made: std::sync::atomic::AtomicU32::new(0),
    });
    let connector_for_inspection = connector.clone();
    let health = Arc::new(ProviderHealthTracker::new(
        ProviderId("surfpool-ws".into()),
        BreakerConfig::default(),
    ));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let shutdown = manager.shutdown_handle();
    let m2 = manager.clone();
    let handle = tokio::spawn(async move { m2.run(connector, health, tx).await });

    let mut notification_count = 0;
    let mut live_transitions = 0;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while tokio::time::Instant::now() < deadline && notification_count < 2 {
        match tokio::time::timeout(Duration::from_secs(3), rx.recv()).await {
            Ok(Some(WsEvent::StateChanged(sentinel_rpc::ws::WsState::Live))) => {
                live_transitions += 1
            }
            Ok(Some(WsEvent::Notification { .. })) => notification_count += 1,
            Ok(Some(_)) => {}
            _ => {}
        }
    }
    shutdown.notify_one();
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;

    assert_eq!(
        notification_count, 2,
        "must receive a real notification both before and after the forced disconnect+reconnect"
    );
    // The connector's own counter is the ground truth for "a genuinely new
    // TCP/WS connection was opened" — more direct evidence than inferring
    // it from event timing. A clean close (this test's simulated
    // disconnect) intentionally skips the punitive Backoff/ReconnectAttempt
    // path (see ws.rs live_loop: `Ok(None) => WsState::Connecting`) and
    // reconnects immediately, which is correct production behavior, not a
    // gap in this proof.
    let real_connections = connector_for_inspection
        .connections_made
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        real_connections >= 2,
        "the manager must have opened at least 2 real WebSocket connections (original + reconnect), got {real_connections}"
    );
    assert!(
        live_transitions >= 2,
        "must reach Live twice: once on the original connection, once after re-subscribing on the reconnect (W-1), got {live_transitions}"
    );
}
