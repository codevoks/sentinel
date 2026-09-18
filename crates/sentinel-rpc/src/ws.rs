//! WebSocket connection manager: the state machine from
//! `docs/ingestion-model.md` §4, with a **declarative subscription set**
//! re-applied on every connect (W-1), bounded reconnect with full jitter
//! (W-4, FI-06), and a heartbeat derived from observed slot notifications,
//! never a hardcoded interval (W-3).
//!
//! Transport is behind [`WsConnector`]/[`WsConnection`] so both the real
//! path (`TungsteniteConnector`, driven against Surfpool in integration
//! tests) and fault injection (`fault.rs`'s flapping connector, driven in
//! unit tests with no network) exercise the exact same state machine.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::sync::Notify;

use crate::pool::backoff_full_jitter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsState {
    Connecting,
    Establishing,
    Live,
    Degraded,
    Backoff,
    Failover,
    ShutDown,
}

impl WsState {
    pub fn as_str(self) -> &'static str {
        match self {
            WsState::Connecting => "connecting",
            WsState::Establishing => "establishing",
            WsState::Live => "live",
            WsState::Degraded => "degraded",
            WsState::Backoff => "backoff",
            WsState::Failover => "failover",
            WsState::ShutDown => "shutdown",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WsError {
    #[error("connect failed: {0}")]
    ConnectFailed(String),
    #[error("send failed: {0}")]
    SendFailed(String),
    #[error("connection closed")]
    Closed,
    #[error("subscribe timed out")]
    SubscribeTimeout,
}

#[async_trait]
pub trait WsConnection: Send {
    async fn send_text(&mut self, text: String) -> Result<(), WsError>;
    /// Returns `Ok(None)` on a clean close, `Err` on a transport error.
    async fn recv(&mut self, timeout: Duration) -> Result<Option<String>, WsError>;
}

#[async_trait]
pub trait WsConnector: Send + Sync {
    async fn connect(&self) -> Result<Box<dyn WsConnection>, WsError>;
}

/// A declared subscription. `id` is a caller-chosen stable logical name
/// (e.g. `"slot"`, `"account:<pubkey>"`), used only for de-duplication and
/// diagnostics — never a server-assigned subscription id, which is exactly
/// what W-1 says must never be assumed to survive a reconnect.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubscriptionSpec {
    pub id: String,
    pub method: &'static str,
    pub params: Value,
}

#[derive(Debug, Clone)]
pub enum WsEvent {
    StateChanged(WsState),
    Notification {
        subscription_id: String,
        payload: Value,
    },
    /// W-2: every transition into `Live` must trigger a gap scan. This
    /// phase does not implement ingestion, so the event is emitted for a
    /// future consumer to act on (`phase-03-rpc.md` §2 non-scope) —
    /// nothing subscribes to it yet, and nothing here performs the scan.
    GapScanDue,
    ReconnectAttempt {
        attempt: u32,
    },
}

#[derive(Debug, Clone)]
pub struct WsManagerConfig {
    pub backoff_base: Duration,
    pub backoff_cap: Duration,
    pub heartbeat_window: Duration,
    pub subscribe_ack_timeout: Duration,
    /// FI-06: a hard ceiling on reconnect attempts within
    /// `reconnect_window` before failing over — proves "no unbounded
    /// reconnect storm".
    pub max_reconnects_per_window: u32,
    pub reconnect_window: Duration,
}

impl Default for WsManagerConfig {
    fn default() -> Self {
        WsManagerConfig {
            backoff_base: Duration::from_millis(50),
            backoff_cap: Duration::from_secs(10),
            heartbeat_window: Duration::from_secs(5),
            subscribe_ack_timeout: Duration::from_secs(3),
            max_reconnects_per_window: 20,
            reconnect_window: Duration::from_secs(60),
        }
    }
}

/// Runs the WebSocket state machine for one provider until `shutdown` is
/// signalled. Emits [`WsEvent`]s on `events_tx` (state transitions,
/// notifications, gap-scan triggers) for a caller/test to observe.
pub struct WsManager {
    config: WsManagerConfig,
    subscriptions: Vec<SubscriptionSpec>,
    reconnect_count: AtomicU32,
    total_reconnects: AtomicU64,
    shutdown: Arc<Notify>,
}

impl WsManager {
    pub fn new(config: WsManagerConfig, subscriptions: Vec<SubscriptionSpec>) -> Self {
        WsManager {
            config,
            subscriptions,
            reconnect_count: AtomicU32::new(0),
            total_reconnects: AtomicU64::new(0),
            shutdown: Arc::new(Notify::new()),
        }
    }

    pub fn shutdown_handle(&self) -> Arc<Notify> {
        self.shutdown.clone()
    }

    pub fn total_reconnects(&self) -> u64 {
        self.total_reconnects.load(Ordering::Relaxed)
    }

    /// Declaratively re-applies every subscription over a freshly connected
    /// socket (W-1) and waits for each ack, bounded by
    /// `subscribe_ack_timeout`.
    async fn establish_subscriptions(
        &self,
        conn: &mut Box<dyn WsConnection>,
    ) -> Result<(), WsError> {
        for (idx, spec) in self.subscriptions.iter().enumerate() {
            let request = serde_json::json!({
                "jsonrpc": "2.0",
                "id": idx,
                "method": spec.method,
                "params": spec.params,
            });
            conn.send_text(request.to_string()).await?;
            let ack = conn
                .recv(self.config.subscribe_ack_timeout)
                .await?
                .ok_or(WsError::Closed)?;
            let parsed: Value =
                serde_json::from_str(&ack).map_err(|e| WsError::ConnectFailed(e.to_string()))?;
            if parsed.get("result").is_none() {
                return Err(WsError::SubscribeTimeout);
            }
        }
        Ok(())
    }

    /// Runs the full state machine loop. Returns when `shutdown` fires.
    /// `health` receives reconnect counts (W-5); `events_tx` receives every
    /// state transition and notification.
    pub async fn run(
        &self,
        connector: Arc<dyn WsConnector>,
        health: Arc<crate::health::ProviderHealthTracker>,
        events_tx: mpsc::UnboundedSender<WsEvent>,
    ) {
        let mut state = WsState::Connecting;
        let mut attempt: u32 = 0;
        let mut window_start = tokio::time::Instant::now();

        loop {
            match state {
                WsState::Connecting => {
                    let _ = events_tx.send(WsEvent::StateChanged(WsState::Connecting));
                    match connector.connect().await {
                        Ok(mut conn) => {
                            let _ = events_tx.send(WsEvent::StateChanged(WsState::Establishing));
                            match self.establish_subscriptions(&mut conn).await {
                                Ok(()) => {
                                    attempt = 0;
                                    let _ = events_tx.send(WsEvent::StateChanged(WsState::Live));
                                    let _ = events_tx.send(WsEvent::GapScanDue); // W-2
                                    state = self.live_loop(&mut conn, &events_tx).await;
                                }
                                Err(_) => {
                                    state = WsState::Backoff;
                                }
                            }
                        }
                        Err(_) => {
                            state = WsState::Backoff;
                        }
                    }
                }
                WsState::Backoff => {
                    let _ = events_tx.send(WsEvent::StateChanged(WsState::Backoff));
                    // FI-06: bounded reconnect. If we have already
                    // reconnected `max_reconnects_per_window` times inside
                    // `reconnect_window`, fail over instead of continuing
                    // to hammer this provider — this is the concrete
                    // "no unbounded reconnect storm" guarantee.
                    if window_start.elapsed() > self.config.reconnect_window {
                        window_start = tokio::time::Instant::now();
                        self.reconnect_count.store(0, Ordering::Relaxed);
                    }
                    let count = self.reconnect_count.fetch_add(1, Ordering::Relaxed) + 1;
                    self.total_reconnects.fetch_add(1, Ordering::Relaxed);
                    health.record_reconnect();
                    let _ = events_tx.send(WsEvent::ReconnectAttempt { attempt: count });
                    if count > self.config.max_reconnects_per_window {
                        state = WsState::Failover;
                        continue;
                    }
                    let delay = backoff_full_jitter(
                        attempt,
                        self.config.backoff_base,
                        self.config.backoff_cap,
                    );
                    attempt = attempt.saturating_add(1);
                    tokio::select! {
                        _ = tokio::time::sleep(delay) => {}
                        _ = self.shutdown.notified() => {
                            let _ = events_tx.send(WsEvent::StateChanged(WsState::ShutDown));
                            return;
                        }
                    }
                    state = WsState::Connecting;
                }
                WsState::Failover => {
                    let _ = events_tx.send(WsEvent::StateChanged(WsState::Failover));
                    // The pool (not this manager) is responsible for
                    // choosing another provider's WS independently of this
                    // provider's HTTP health — separate health domains
                    // (rpc-strategy.md §8). This manager stops here.
                    return;
                }
                WsState::Degraded | WsState::Live | WsState::Establishing | WsState::ShutDown => {
                    // Reached only via live_loop's return value below, or
                    // not at all as an entry state.
                    unreachable!("live_loop must resolve to Connecting, Backoff, or Failover")
                }
            }
        }
    }

    /// Handles the `Live`/`Degraded` sub-states: reads notifications,
    /// tracks a heartbeat derived from actual message arrival (never a
    /// hardcoded 400ms — W-3), and returns the next top-level state once it
    /// exits (`Connecting` after a clean/expected close, `Backoff` after
    /// heartbeat deadline exceeded or a transport error).
    async fn live_loop(
        &self,
        conn: &mut Box<dyn WsConnection>,
        events_tx: &mpsc::UnboundedSender<WsEvent>,
    ) -> WsState {
        let mut degraded = false;
        loop {
            match conn.recv(self.config.heartbeat_window).await {
                Ok(Some(text)) => {
                    if degraded {
                        degraded = false;
                        let _ = events_tx.send(WsEvent::StateChanged(WsState::Live));
                    }
                    if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
                        if let Some(method) = parsed.get("method").and_then(|m| m.as_str()) {
                            let sub_id = method.trim_end_matches("Notification").to_string();
                            let _ = events_tx.send(WsEvent::Notification {
                                subscription_id: sub_id,
                                payload: parsed,
                            });
                        }
                    }
                }
                Ok(None) => return WsState::Connecting, // socket closed cleanly -> reconnect
                Err(_) if !degraded => {
                    // No message within the heartbeat window: Live -> Degraded
                    // (per the state diagram), not yet a reconnect.
                    degraded = true;
                    let _ = events_tx.send(WsEvent::StateChanged(WsState::Degraded));
                    continue;
                }
                Err(_) => {
                    // Already degraded and the heartbeat deadline elapsed
                    // again: Degraded -> Backoff.
                    return WsState::Backoff;
                }
            }
        }
    }
}

/// The real transport: one `tokio-tungstenite` connection per provider.
pub struct TungsteniteConnector {
    pub url: String,
}

#[async_trait]
impl WsConnector for TungsteniteConnector {
    async fn connect(&self) -> Result<Box<dyn WsConnection>, WsError> {
        let (stream, _resp) = tokio::time::timeout(
            Duration::from_secs(5),
            tokio_tungstenite::connect_async(&self.url),
        )
        .await
        .map_err(|_| WsError::ConnectFailed("connect timed out".into()))?
        .map_err(|e| WsError::ConnectFailed(e.to_string()))?;
        Ok(Box::new(TungsteniteConnection { stream }))
    }
}

struct TungsteniteConnection {
    stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
}

#[async_trait]
impl WsConnection for TungsteniteConnection {
    async fn send_text(&mut self, text: String) -> Result<(), WsError> {
        use futures_util::SinkExt;
        self.stream
            .send(tokio_tungstenite::tungstenite::Message::Text(text.into()))
            .await
            .map_err(|e| WsError::SendFailed(e.to_string()))
    }

    async fn recv(&mut self, timeout: Duration) -> Result<Option<String>, WsError> {
        use futures_util::StreamExt;
        match tokio::time::timeout(timeout, self.stream.next()).await {
            Ok(Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text)))) => {
                Ok(Some(text.to_string()))
            }
            Ok(Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_)))) => Ok(None),
            Ok(Some(Ok(_other))) => Ok(Some(String::new())), // non-text control frame; ignored upstream
            Ok(Some(Err(e))) => Err(WsError::SendFailed(e.to_string())),
            Ok(None) => Ok(None),
            Err(_elapsed) => Err(WsError::Closed), // heartbeat timeout, handled by live_loop
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Mutex as TokioMutex;

    /// A scripted fake connection: each `recv` pops the next scripted
    /// event. Deterministic — no randomness, no real sockets
    /// (`phase-03-rpc.md` §18: fault injection must be deterministic).
    /// `Close` models a clean server-side close (goes straight back to
    /// `Connecting`, no backoff); `HeartbeatTimeout` models a stalled
    /// connection (goes through `Degraded` then `Backoff`) and is what the
    /// reconnect test below actually exercises.
    enum ScriptEvent {
        Ack,
        Notify(&'static str),
        HeartbeatTimeout,
    }

    struct ScriptedConnection {
        script: std::collections::VecDeque<ScriptEvent>,
    }

    #[async_trait]
    impl WsConnection for ScriptedConnection {
        async fn send_text(&mut self, _text: String) -> Result<(), WsError> {
            Ok(())
        }
        async fn recv(&mut self, _timeout: Duration) -> Result<Option<String>, WsError> {
            match self.script.pop_front() {
                Some(ScriptEvent::Ack) => {
                    Ok(Some(serde_json::json!({"result": 1, "id": 0}).to_string()))
                }
                Some(ScriptEvent::Notify(method)) => Ok(Some(
                    serde_json::json!({"method": method, "params": {}}).to_string(),
                )),
                Some(ScriptEvent::HeartbeatTimeout) => Err(WsError::Closed),
                None => Ok(None), // clean close: script exhausted
            }
        }
    }

    struct AlwaysFailConnector;
    #[async_trait]
    impl WsConnector for AlwaysFailConnector {
        async fn connect(&self) -> Result<Box<dyn WsConnection>, WsError> {
            Err(WsError::ConnectFailed("simulated flap".into()))
        }
    }

    struct ScriptedConnector {
        scripts: TokioMutex<std::collections::VecDeque<Vec<ScriptEvent>>>,
    }

    #[async_trait]
    impl WsConnector for ScriptedConnector {
        async fn connect(&self) -> Result<Box<dyn WsConnection>, WsError> {
            let mut scripts = self.scripts.lock().await;
            let script = scripts.pop_front().unwrap_or_default();
            Ok(Box::new(ScriptedConnection {
                script: script.into(),
            }))
        }
    }

    fn health() -> Arc<crate::health::ProviderHealthTracker> {
        Arc::new(crate::health::ProviderHealthTracker::new(
            crate::provider::ProviderId("ws-test".into()),
            crate::breaker::BreakerConfig::default(),
        ))
    }

    #[tokio::test]
    async fn subscribe_receive_and_reconnect_resubscribe_cycle() {
        let subs = vec![SubscriptionSpec {
            id: "slot".into(),
            method: "slotSubscribe",
            params: serde_json::json!([]),
        }];
        let manager = Arc::new(WsManager::new(
            WsManagerConfig {
                backoff_base: Duration::from_millis(1),
                backoff_cap: Duration::from_millis(5),
                heartbeat_window: Duration::from_millis(50),
                ..Default::default()
            },
            subs,
        ));
        let connector = Arc::new(ScriptedConnector {
            scripts: TokioMutex::new(
                vec![
                    // First connection: ack, one notification, then two
                    // consecutive heartbeat timeouts — Live -> Degraded ->
                    // Backoff — genuinely exercising the reconnect path
                    // (a clean `Close` instead would skip Backoff entirely
                    // and go straight back to Connecting with no
                    // `ReconnectAttempt` event, which is correct behavior
                    // but not what this test wants to exercise).
                    vec![
                        ScriptEvent::Ack,
                        ScriptEvent::Notify("slotNotification"),
                        ScriptEvent::HeartbeatTimeout,
                        ScriptEvent::HeartbeatTimeout,
                    ],
                    // Second connection (post-reconnect): ack again (proves
                    // resubscribe), then a second notification.
                    vec![ScriptEvent::Ack, ScriptEvent::Notify("slotNotification")],
                ]
                .into(),
            ),
        });
        let (tx, mut rx) = mpsc::unbounded_channel();
        let h = health();
        let shutdown = manager.shutdown_handle();
        let m2 = manager.clone();
        let handle = tokio::spawn(async move { m2.run(connector, h, tx).await });

        let mut notifications = 0;
        let mut saw_reconnect = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while tokio::time::Instant::now() < deadline && notifications < 2 {
            if let Ok(Some(event)) =
                tokio::time::timeout(Duration::from_millis(200), rx.recv()).await
            {
                match event {
                    WsEvent::Notification { .. } => notifications += 1,
                    WsEvent::ReconnectAttempt { .. } => saw_reconnect = true,
                    _ => {}
                }
            }
        }
        shutdown.notify_one();
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;

        assert_eq!(
            notifications, 2,
            "must receive a notification both before and after reconnect"
        );
        assert!(
            saw_reconnect,
            "must have actually reconnected between the two notifications"
        );
    }

    #[tokio::test]
    async fn flapping_connector_produces_bounded_reconnects_fi06() {
        let manager = Arc::new(WsManager::new(
            WsManagerConfig {
                backoff_base: Duration::from_millis(1),
                backoff_cap: Duration::from_millis(2),
                heartbeat_window: Duration::from_millis(20),
                max_reconnects_per_window: 5,
                reconnect_window: Duration::from_secs(60),
                ..Default::default()
            },
            vec![],
        ));
        let connector = Arc::new(AlwaysFailConnector);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let h = health();
        let handle = tokio::spawn({
            let m = manager.clone();
            async move { m.run(connector, h, tx).await }
        });

        let mut max_attempt = 0u32;
        let mut saw_failover = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(300), rx.recv()).await {
                Ok(Some(WsEvent::ReconnectAttempt { attempt })) => {
                    max_attempt = max_attempt.max(attempt)
                }
                Ok(Some(WsEvent::StateChanged(WsState::Failover))) => {
                    saw_failover = true;
                    break;
                }
                Ok(Some(_)) => {}
                _ => break,
            }
        }
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;

        assert!(
            saw_failover,
            "an always-failing connector must eventually reach Failover, not reconnect forever"
        );
        assert!(
            max_attempt <= 6,
            "reconnect attempts must be bounded near max_reconnects_per_window (5), got {max_attempt}"
        );
    }
}
