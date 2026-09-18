//! `FixtureProvider`: an `RpcProvider` implementation backed entirely by
//! canned, deterministic responses (`docs/phases/phase-03-rpc.md` §17).
//! Drives every unit/property test that does not need a real Surfpool
//! instance, through exactly the same trait every production code path
//! uses — so a failure-injection test exercises real pool/breaker/budget
//! code, not a parallel mock universe.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;

use sentinel_core::Slot;

use crate::capabilities::ProviderCapabilities;
use crate::provider::{
    CallContext, ProviderId, RequestClass, RpcError, RpcMethodCall, RpcMethodResponse, RpcOutcome,
    RpcProvider,
};

/// A scripted answer for one method. `FixtureProvider` looks up by method
/// name; if the request's parameters need to influence the answer (e.g.
/// per-slot blocks), use [`FixtureProvider::with_block`] instead of the
/// generic map.
#[derive(Clone)]
pub enum ScriptedResponse {
    Ok(RpcMethodResponse),
    Err(RpcError),
}

pub struct FixtureProvider {
    id: ProviderId,
    capabilities: ProviderCapabilities,
    configured_classes: HashSet<RequestClass>,
    by_method: Mutex<HashMap<&'static str, ScriptedResponse>>,
    blocks_by_slot: Mutex<HashMap<u64, (String, serde_json::Value)>>,
    call_count: AtomicUsize,
    calls_seen: Mutex<Vec<(String, u64)>>, // (method, request_id_bits) for uniqueness tests
}

impl FixtureProvider {
    pub fn new(
        id: impl Into<String>,
        capabilities: ProviderCapabilities,
        configured_classes: HashSet<RequestClass>,
    ) -> Self {
        FixtureProvider {
            id: ProviderId(id.into()),
            capabilities,
            configured_classes,
            by_method: Mutex::new(HashMap::new()),
            blocks_by_slot: Mutex::new(HashMap::new()),
            call_count: AtomicUsize::new(0),
            calls_seen: Mutex::new(Vec::new()),
        }
    }

    pub fn with_response(self, method: &'static str, response: ScriptedResponse) -> Self {
        self.by_method
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(method, response);
        self
    }

    /// Registers a canned block for `slot` — used by the divergence tests
    /// (FI-10/FI-11) to make two `FixtureProvider`s disagree deliberately.
    pub fn with_block(
        self,
        slot: u64,
        blockhash: impl Into<String>,
        raw_json: serde_json::Value,
    ) -> Self {
        self.blocks_by_slot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(slot, (blockhash.into(), raw_json));
        self
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::Relaxed)
    }

    pub fn calls_seen(&self) -> Vec<(String, u64)> {
        self.calls_seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

#[async_trait]
impl RpcProvider for FixtureProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }

    async fn discover_capabilities(&self) -> Result<ProviderCapabilities, RpcError> {
        Ok(self.capabilities.clone())
    }

    fn configured_classes(&self) -> &HashSet<RequestClass> {
        &self.configured_classes
    }

    async fn call(&self, req: RpcMethodCall, ctx: &CallContext) -> Result<RpcOutcome, RpcError> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        self.calls_seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((
                req.method_name().to_string(),
                ctx.request_id.0.as_u128() as u64,
            ));

        let method = req.method_name();
        if !self.capabilities.supports_method(method) {
            return Err(RpcError::Fatal {
                code: "SEN-RPC-UNSUPPORTED",
                message: format!("{method} is not supported by fixture provider {}", self.id),
                request_id: Some(ctx.request_id),
            });
        }

        if let RpcMethodCall::GetBlock { slot, .. } = &req {
            if let Some((blockhash, raw_json)) = self
                .blocks_by_slot
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&slot.0)
            {
                return Ok(RpcOutcome {
                    response: RpcMethodResponse::Block {
                        slot: *slot,
                        blockhash: blockhash.clone(),
                        raw_json: raw_json.clone(),
                    },
                    provider_id: self.id.clone(),
                    request_id: ctx.request_id,
                    context_slot: Some(*slot),
                });
            }
        }

        let scripted = self
            .by_method
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(method)
            .cloned();
        match scripted {
            Some(ScriptedResponse::Ok(response)) => {
                let context_slot = response_context_slot(&response);
                Ok(RpcOutcome {
                    response,
                    provider_id: self.id.clone(),
                    request_id: ctx.request_id,
                    context_slot,
                })
            }
            Some(ScriptedResponse::Err(mut e)) => {
                attach_request_id(&mut e, ctx.request_id);
                Err(e)
            }
            None => Err(RpcError::Fatal {
                code: "SEN-RPC-NOFIXTURE",
                message: format!(
                    "no scripted response for {method} on fixture provider {}",
                    self.id
                ),
                request_id: Some(ctx.request_id),
            }),
        }
    }
}

fn response_context_slot(response: &RpcMethodResponse) -> Option<Slot> {
    match response {
        RpcMethodResponse::LatestBlockhash { context_slot, .. }
        | RpcMethodResponse::MultipleAccounts { context_slot, .. }
        | RpcMethodResponse::ProgramAccounts { context_slot, .. }
        | RpcMethodResponse::SignatureStatuses { context_slot, .. }
        | RpcMethodResponse::SimulateTransaction { context_slot, .. } => Some(*context_slot),
        RpcMethodResponse::Block { slot, .. } => Some(*slot),
        RpcMethodResponse::Slot(s) => Some(*s),
        _ => None,
    }
}

fn attach_request_id(err: &mut RpcError, request_id: crate::provider::RequestId) {
    match err {
        RpcError::Transient { request_id: r, .. }
        | RpcError::Stale { request_id: r, .. }
        | RpcError::Malformed { request_id: r, .. }
        | RpcError::Fatal { request_id: r, .. } => {
            if r.is_none() {
                *r = Some(request_id);
            }
        }
        RpcError::ContentDivergence { .. } => {}
    }
}

/// Convenience builders for the classes every fixture test needs.
pub fn all_classes() -> HashSet<RequestClass> {
    RequestClass::ALL.into_iter().collect()
}

pub fn full_capabilities() -> ProviderCapabilities {
    let mut caps = ProviderCapabilities::unknown();
    caps.max_supported_transaction_version = 0;
    caps.supports_get_program_accounts = true;
    caps.max_blocks_per_get_blocks = 500_000;
    caps.supports_min_context_slot = true;
    caps.supports_websocket = true;
    caps.supports_recent_prioritization_fees = true;
    caps.node_version = "fixture-1.0.0".to_string();
    for m in [
        "getVersion",
        "getHealth",
        "getSlot",
        "getBlockHeight",
        "getBlocks",
        "getBlock",
        "getTransaction",
        "getLatestBlockhash",
        "getMultipleAccounts",
        "getProgramAccounts",
        "getRecentPrioritizationFees",
        "getSignatureStatuses",
        "getSignaturesForAddress",
        "simulateTransaction",
        "sendTransaction",
    ] {
        caps.mark_has_context(m);
        caps.mark_supported(m);
    }
    caps
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_core::Commitment;
    use std::time::{Duration, Instant};

    /// Test-only helper standing in for `.expect()`/`.unwrap()`, which this
    /// crate's `#![deny(clippy::unwrap_used, clippy::expect_used)]` also
    /// forbids inside `#[cfg(test)]` modules (they compile into the same
    /// crate as `src/`, unlike files under `tests/`).
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

    fn ctx() -> CallContext {
        CallContext::new(
            crate::provider::CorrelationId::new(),
            RequestClass::Backfill,
            Commitment::Confirmed,
            Instant::now() + Duration::from_secs(5),
            None,
        )
    }

    #[tokio::test]
    async fn missing_capability_returns_fatal_not_a_panic() {
        let mut caps = full_capabilities();
        caps.supported_methods.remove("getProgramAccounts");
        let provider = FixtureProvider::new("f1", caps, all_classes());
        let result = provider
            .call(
                RpcMethodCall::GetProgramAccounts {
                    program_id: "11111111111111111111111111111111".into(),
                },
                &ctx(),
            )
            .await;
        let err = assert_err(result);
        assert!(matches!(
            err,
            RpcError::Fatal {
                code: "SEN-RPC-UNSUPPORTED",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn request_ids_are_generated_and_attached_to_outcome() {
        let provider = FixtureProvider::new("f1", full_capabilities(), all_classes())
            .with_response(
                "getSlot",
                ScriptedResponse::Ok(RpcMethodResponse::Slot(Slot(42))),
            );
        let c = ctx();
        let outcome = assert_ok(provider.call(RpcMethodCall::GetSlot, &c).await);
        assert_eq!(outcome.request_id, c.request_id);
    }

    #[tokio::test]
    async fn each_attempt_gets_a_distinct_request_id() {
        let provider = FixtureProvider::new("f1", full_capabilities(), all_classes())
            .with_response(
                "getSlot",
                ScriptedResponse::Ok(RpcMethodResponse::Slot(Slot(1))),
            );
        let base = ctx();
        let c1 = base.next_attempt();
        let c2 = base.next_attempt();
        assert_ne!(
            c1.request_id, c2.request_id,
            "retries must not reuse a request id"
        );
        assert_ok(provider.call(RpcMethodCall::GetSlot, &c1).await);
        assert_ok(provider.call(RpcMethodCall::GetSlot, &c2).await);
        let seen = provider.calls_seen();
        let ids: std::collections::HashSet<_> = seen.iter().map(|(_, id)| *id).collect();
        assert_eq!(ids.len(), 2, "every attempt's request id must be unique");
    }
}
