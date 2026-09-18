//! `FaultInjectingProvider`: deterministically produces every required
//! failure mode against the real `RpcProvider`/`RpcPool` code path
//! (`docs/phases/phase-03-rpc.md` §18). Faults are drawn from an explicit,
//! ordered script — never randomness — so CI is reliable
//! (`docs/testing-strategy.md`, AGENTS.md §9: "a mocked happy path is not
//! distributed-systems evidence; failure-injection results are").

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;

use sentinel_core::Slot;

use crate::capabilities::ProviderCapabilities;
use crate::provider::{
    CallContext, ProviderId, RequestClass, RpcError, RpcMethodCall, RpcOutcome, RpcProvider,
};
use crate::ws::{WsConnection, WsConnector, WsError};

/// One scripted fault. Every variant here maps to exactly one required
/// case from `phase-03-rpc.md` §18/§8.
#[derive(Debug, Clone)]
pub enum InjectedFault {
    Timeout,
    RateLimitedWithRetryAfter(Duration),
    RateLimitedNoRetryAfter,
    ServerError(u16),
    StaleContext {
        observed: Slot,
        required: Slot,
    },
    MalformedResponse,
    /// A generic "this provider is broken" transient failure — the
    /// building block for driving a breaker to `Open` (used for FI-22's
    /// all-providers-open scenario).
    ProviderFailure,
}

impl InjectedFault {
    fn into_error(self, request_id: crate::provider::RequestId) -> RpcError {
        match self {
            InjectedFault::Timeout => RpcError::Transient {
                code: "SEN-RPC-TIMEOUT",
                message: "simulated provider timeout".into(),
                request_id: Some(request_id),
                retry_after: None,
            },
            InjectedFault::RateLimitedWithRetryAfter(d) => RpcError::Transient {
                code: "SEN-RPC-429-RA",
                message: format!("simulated 429 with Retry-After: {d:?}"),
                request_id: Some(request_id),
                retry_after: Some(d),
            },
            InjectedFault::RateLimitedNoRetryAfter => RpcError::Transient {
                code: "SEN-RPC-429-NORA",
                message: "simulated 429 without Retry-After".into(),
                request_id: Some(request_id),
                retry_after: None,
            },
            InjectedFault::ServerError(status) => RpcError::Transient {
                code: "SEN-RPC-5XX",
                message: format!("simulated HTTP {status}"),
                request_id: Some(request_id),
                retry_after: None,
            },
            InjectedFault::StaleContext { observed, required } => RpcError::Stale {
                code: "SEN-RPC-STALE",
                message: format!(
                    "simulated stale context: observed {observed} < required {required}"
                ),
                request_id: Some(request_id),
                observed_slot: Some(observed),
                required_slot: Some(required),
            },
            InjectedFault::MalformedResponse => RpcError::Malformed {
                code: "SEN-RPC-MALFORMED",
                message: "simulated malformed response body".into(),
                request_id: Some(request_id),
            },
            InjectedFault::ProviderFailure => RpcError::Transient {
                code: "SEN-RPC-PROVIDER-FAILURE",
                message: "simulated configured provider failure".into(),
                request_id: Some(request_id),
                retry_after: None,
            },
        }
    }
}

/// Wraps an inner `RpcProvider` (typically a `FixtureProvider`) and injects
/// faults from a deterministic, per-method FIFO script. Once a method's
/// script is exhausted, calls fall through to the inner provider unchanged.
pub struct FaultInjectingProvider {
    id: ProviderId,
    inner: Arc<dyn RpcProvider>,
    scripts: Mutex<std::collections::HashMap<&'static str, VecDeque<InjectedFault>>>,
    /// When set, every call regardless of method returns this fault,
    /// ignoring per-method scripts — the "configured provider failure"
    /// case, and the building block for FI-21/FI-22.
    always: Mutex<Option<InjectedFault>>,
}

impl FaultInjectingProvider {
    pub fn wrapping(inner: Arc<dyn RpcProvider>) -> Self {
        FaultInjectingProvider {
            id: inner.id().clone(),
            inner,
            scripts: Mutex::new(std::collections::HashMap::new()),
            always: Mutex::new(None),
        }
    }

    pub fn script(&self, method: &'static str, faults: Vec<InjectedFault>) -> &Self {
        self.scripts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(method, faults.into());
        self
    }

    pub fn always_fail(&self, fault: InjectedFault) -> &Self {
        *self.always.lock().unwrap_or_else(|e| e.into_inner()) = Some(fault);
        self
    }

    pub fn clear_always_fail(&self) -> &Self {
        *self.always.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self
    }
}

#[async_trait]
impl RpcProvider for FaultInjectingProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn capabilities(&self) -> &ProviderCapabilities {
        self.inner.capabilities()
    }

    async fn discover_capabilities(&self) -> Result<ProviderCapabilities, RpcError> {
        self.inner.discover_capabilities().await
    }

    fn configured_classes(&self) -> &HashSet<RequestClass> {
        self.inner.configured_classes()
    }

    async fn call(&self, req: RpcMethodCall, ctx: &CallContext) -> Result<RpcOutcome, RpcError> {
        if let Some(fault) = self
            .always
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            return Err(fault.into_error(ctx.request_id));
        }
        let next = {
            let mut scripts = self.scripts.lock().unwrap_or_else(|e| e.into_inner());
            scripts
                .get_mut(req.method_name())
                .and_then(|q| q.pop_front())
        };
        match next {
            Some(fault) => Err(fault.into_error(ctx.request_id)),
            None => self.inner.call(req, ctx).await,
        }
    }
}

/// A WS connector that fails to connect a fixed, bounded number of times
/// before succeeding — deterministic flapping for FI-06, delegating the
/// eventual successful connection to `then`.
pub struct FlappingWsConnector {
    failures_remaining: AtomicU32,
    then: Arc<dyn WsConnector>,
}

impl FlappingWsConnector {
    pub fn new(failures_before_success: u32, then: Arc<dyn WsConnector>) -> Self {
        FlappingWsConnector {
            failures_remaining: AtomicU32::new(failures_before_success),
            then,
        }
    }
}

#[async_trait]
impl WsConnector for FlappingWsConnector {
    async fn connect(&self) -> Result<Box<dyn WsConnection>, WsError> {
        let remaining = self.failures_remaining.load(Ordering::Relaxed);
        if remaining > 0 {
            self.failures_remaining.fetch_sub(1, Ordering::Relaxed);
            return Err(WsError::ConnectFailed(format!(
                "simulated flap ({remaining} failures remaining)"
            )));
        }
        self.then.connect().await
    }
}

/// A connector that always fails — the pure "provider is down" WS case,
/// used to prove reconnect storms are bounded (FI-06) independent of any
/// eventual recovery.
pub struct AlwaysFailWsConnector;

#[async_trait]
impl WsConnector for AlwaysFailWsConnector {
    async fn connect(&self) -> Result<Box<dyn WsConnection>, WsError> {
        Err(WsError::ConnectFailed("simulated permanent failure".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{all_classes, full_capabilities, FixtureProvider, ScriptedResponse};
    use crate::provider::{CorrelationId, RpcMethodResponse};
    use sentinel_core::Commitment;
    use std::time::Instant;

    fn ctx() -> CallContext {
        CallContext::new(
            CorrelationId::new(),
            RequestClass::Backfill,
            Commitment::Confirmed,
            Instant::now() + Duration::from_secs(5),
            None,
        )
    }

    /// Test-only stand-ins for `.unwrap()`/`.unwrap_err()`/`.expect()`,
    /// which this crate's `#![deny(clippy::unwrap_used, clippy::expect_used)]`
    /// also forbids inside `#[cfg(test)]` modules.
    fn assert_ok<T>(r: Result<T, RpcError>) -> T {
        assert!(
            r.is_ok(),
            "expected Ok, got Err: {:?}",
            r.as_ref().err().map(|e| e.to_string())
        );
        match r {
            Ok(v) => v,
            Err(_) => unreachable!(),
        }
    }

    fn assert_err<T>(r: Result<T, RpcError>) -> RpcError {
        assert!(r.is_err(), "expected Err, got Ok");
        match r {
            Err(e) => e,
            Ok(_) => unreachable!(),
        }
    }

    fn inner_ok_slot() -> Arc<dyn RpcProvider> {
        Arc::new(
            FixtureProvider::new("f1", full_capabilities(), all_classes()).with_response(
                "getSlot",
                ScriptedResponse::Ok(RpcMethodResponse::Slot(Slot(7))),
            ),
        )
    }

    #[tokio::test]
    async fn every_required_fault_class_is_producible_and_classified_correctly() {
        let provider = FaultInjectingProvider::wrapping(inner_ok_slot());
        provider.script(
            "getSlot",
            vec![
                InjectedFault::Timeout,
                InjectedFault::RateLimitedWithRetryAfter(Duration::from_millis(50)),
                InjectedFault::RateLimitedNoRetryAfter,
                InjectedFault::ServerError(503),
                InjectedFault::MalformedResponse,
                InjectedFault::StaleContext {
                    observed: Slot(1),
                    required: Slot(10),
                },
            ],
        );

        let e1 = assert_err(provider.call(RpcMethodCall::GetSlot, &ctx()).await);
        assert!(matches!(
            e1,
            RpcError::Transient {
                code: "SEN-RPC-TIMEOUT",
                ..
            }
        ));
        assert!(e1.is_retryable());

        let e2 = assert_err(provider.call(RpcMethodCall::GetSlot, &ctx()).await);
        assert!(matches!(
            e2,
            RpcError::Transient {
                code: "SEN-RPC-429-RA",
                ..
            }
        ));
        assert_eq!(e2.retry_after(), Some(Duration::from_millis(50)));

        let e3 = assert_err(provider.call(RpcMethodCall::GetSlot, &ctx()).await);
        assert!(matches!(
            e3,
            RpcError::Transient {
                code: "SEN-RPC-429-NORA",
                ..
            }
        ));
        assert_eq!(e3.retry_after(), None);

        let e4 = assert_err(provider.call(RpcMethodCall::GetSlot, &ctx()).await);
        assert!(matches!(
            e4,
            RpcError::Transient {
                code: "SEN-RPC-5XX",
                ..
            }
        ));

        let e5 = assert_err(provider.call(RpcMethodCall::GetSlot, &ctx()).await);
        assert!(matches!(e5, RpcError::Malformed { .. }));
        assert!(!e5.is_retryable(), "malformed must fail fast, not retry");

        let e6 = assert_err(provider.call(RpcMethodCall::GetSlot, &ctx()).await);
        assert!(matches!(e6, RpcError::Stale { .. }));
        assert!(!e6.is_retryable());

        // Script exhausted: falls through to the inner fixture's real answer.
        let ok = assert_ok(provider.call(RpcMethodCall::GetSlot, &ctx()).await);
        assert_eq!(ok.response, RpcMethodResponse::Slot(Slot(7)));
    }

    #[tokio::test]
    async fn always_fail_overrides_every_method() {
        let provider = FaultInjectingProvider::wrapping(inner_ok_slot());
        provider.always_fail(InjectedFault::ProviderFailure);
        let err = assert_err(provider.call(RpcMethodCall::GetSlot, &ctx()).await);
        assert!(err.is_retryable());
        provider.clear_always_fail();
        let ok = provider.call(RpcMethodCall::GetSlot, &ctx()).await;
        assert!(ok.is_ok());
    }

    #[tokio::test]
    async fn flapping_ws_connector_fails_a_bounded_number_of_times_then_succeeds() {
        struct FakeOkConnector;
        #[async_trait]
        impl WsConnector for FakeOkConnector {
            async fn connect(&self) -> Result<Box<dyn WsConnection>, WsError> {
                Err(WsError::ConnectFailed(
                    "stub — never reached in this test".into(),
                ))
            }
        }
        let flapper = FlappingWsConnector::new(3, Arc::new(FakeOkConnector));
        for _ in 0..3 {
            assert!(flapper.connect().await.is_err());
        }
        // The 4th call reaches `then`, which itself errors in this stub —
        // proving control actually passed through after exactly 3 flaps.
        let final_result = flapper.connect().await;
        let is_the_stub_error =
            matches!(&final_result, Err(WsError::ConnectFailed(msg)) if msg.contains("stub"));
        assert!(
            is_the_stub_error,
            "the 4th call must reach `then` and surface its stub error"
        );
    }
}
