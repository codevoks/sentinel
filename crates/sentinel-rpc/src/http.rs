//! `HttpRpcProvider`: the real `RpcProvider` implementation over Solana's
//! HTTP JSON-RPC (`docs/phases/phase-03-rpc.md` §2). A hand-rolled JSON-RPC
//! transport, not a wrapper around `solana-rpc-client`'s `RpcClient` —
//! deliberately, per `docs/rpc-strategy.md` §2 ("provider quirks... rate
//! limit headers... belong in one adapter"): `solana-rpc-client`'s HTTP
//! sender does not expose response headers, so it cannot distinguish a 429
//! with `Retry-After` from one without, which `phase-03-rpc.md` §9
//! requires as two separate cases. Building the request/response shapes
//! directly also means every `getBlock`/`getTransaction` call is
//! *structurally* required to pass `max_supported_transaction_version`
//! (RPC-10) — there is no plain, unsafe overload to reach for.
//!
//! This module still leans on `solana-rpc-client-api`'s config/response
//! types purely as a documentation and Serialize source for parameter
//! shapes; nothing here constructs a `solana_rpc_client::RpcClient` or
//! `solana_pubsub_client::PubsubClient` (`CI-NORAWCLIENT` is about exactly
//! those two constructors — see `scripts/ci-guards.sh`).

use std::collections::HashSet;
use std::fmt;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::{json, Value};

use sentinel_core::{Commitment, Slot};

use crate::capabilities::ProviderCapabilities;
use crate::provider::{
    CallContext, ProviderId, RequestClass, RequestId, RpcError, RpcMethodCall, RpcMethodResponse,
    RpcOutcome, RpcProvider,
};

fn commitment_str(c: Commitment) -> &'static str {
    match c {
        Commitment::Processed => "processed",
        Commitment::Confirmed => "confirmed",
        Commitment::Finalized => "finalized",
    }
}

/// Configuration for one HTTP provider. `http_url` may embed credentials
/// (`https://user:token@host/path` or a `?api-key=` query parameter) —
/// `HttpRpcProvider` never lets it escape into `id()`, logs, metrics, or
/// error messages (A-SEC-01). Construct `id` as a stable, credential-free
/// label (`docs/rpc-strategy.md` §10 CF-2).
pub struct HttpProviderConfig {
    pub id: ProviderId,
    pub http_url: String,
    pub configured_classes: HashSet<RequestClass>,
    pub request_timeout: Duration,
}

pub struct HttpRpcProvider {
    id: ProviderId,
    http_url: String,
    client: reqwest::Client,
    capabilities: RwLock<ProviderCapabilities>,
    configured_classes: HashSet<RequestClass>,
    request_timeout: Duration,
}

/// Deliberately does **not** derive `Debug` — a derived impl would print
/// `http_url` verbatim, which is exactly the credential leak A-SEC-01
/// forbids. This hand-written impl is the enforcement point.
impl fmt::Debug for HttpRpcProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpRpcProvider")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl HttpRpcProvider {
    pub fn new(config: HttpProviderConfig) -> Self {
        HttpRpcProvider {
            id: config.id,
            http_url: config.http_url,
            client: reqwest::Client::new(),
            capabilities: RwLock::new(ProviderCapabilities::unknown()),
            configured_classes: config.configured_classes,
            request_timeout: config.request_timeout,
        }
    }

    fn config_object(
        &self,
        commitment: Commitment,
        min_context_slot: Option<Slot>,
        extra: Value,
    ) -> Value {
        let mut obj = extra;
        if let Value::Object(map) = &mut obj {
            map.insert("commitment".to_string(), json!(commitment_str(commitment)));
            if let Some(Slot(s)) = min_context_slot {
                map.insert("minContextSlot".to_string(), json!(s));
            }
        }
        obj
    }

    fn build_params(&self, req: &RpcMethodCall, ctx: &CallContext) -> Value {
        let commitment = ctx.commitment;
        let min_slot = ctx.min_context_slot;
        match req {
            RpcMethodCall::GetVersion | RpcMethodCall::GetHealth => json!([]),
            RpcMethodCall::GetSlot | RpcMethodCall::GetBlockHeight => {
                json!([self.config_object(commitment, min_slot, json!({}))])
            }
            RpcMethodCall::GetBlocks { start_slot, end_slot } => match end_slot {
                Some(Slot(end)) => json!([start_slot.0, end]),
                None => json!([start_slot.0]),
            },
            RpcMethodCall::GetBlock {
                slot,
                max_supported_transaction_version,
            } => json!([
                slot.0,
                self.config_object(
                    commitment,
                    None, // getBlock does not accept minContextSlot
                    json!({
                        "maxSupportedTransactionVersion": max_supported_transaction_version,
                        "transactionDetails": "full",
                        "rewards": false,
                        "encoding": "json",
                    })
                )
            ]),
            RpcMethodCall::GetTransaction {
                signature,
                max_supported_transaction_version,
            } => json!([
                signature,
                self.config_object(
                    commitment,
                    None,
                    json!({
                        "maxSupportedTransactionVersion": max_supported_transaction_version,
                        "encoding": "json",
                    })
                )
            ]),
            RpcMethodCall::GetLatestBlockhash => {
                json!([self.config_object(commitment, min_slot, json!({}))])
            }
            RpcMethodCall::GetMultipleAccounts { pubkeys } => {
                json!([pubkeys, self.config_object(commitment, min_slot, json!({"encoding": "base64"}))])
            }
            RpcMethodCall::GetProgramAccounts { program_id } => {
                json!([program_id, self.config_object(commitment, min_slot, json!({"encoding": "base64"}))])
            }
            RpcMethodCall::GetRecentPrioritizationFees { writable_accounts } => json!([writable_accounts]),
            RpcMethodCall::GetSignatureStatuses { signatures } => {
                json!([signatures, {"searchTransactionHistory": true}])
            }
            RpcMethodCall::GetSignaturesForAddress { address, until } => {
                let mut cfg = json!({"commitment": commitment_str(commitment)});
                if let (Some(until), Value::Object(map)) = (until, &mut cfg) {
                    map.insert("until".to_string(), json!(until));
                }
                json!([address, cfg])
            }
            RpcMethodCall::SimulateTransaction { raw_tx_base64 } => json!([
                raw_tx_base64,
                self.config_object(
                    commitment,
                    min_slot,
                    json!({"sigVerify": false, "replaceRecentBlockhash": false, "encoding": "base64"})
                )
            ]),
            RpcMethodCall::SendTransaction { raw_tx_base64 } => json!([
                raw_tx_base64,
                {"encoding": "base64", "skipPreflight": true, "preflightCommitment": commitment_str(commitment)}
            ]),
        }
    }

    /// Sends one JSON-RPC request and classifies the outcome into the
    /// five-class taxonomy. Never builds an unbounded body: `params` is
    /// always a small, method-shaped structure, never user-controlled
    /// free text.
    async fn send_rpc(
        &self,
        method: &'static str,
        params: Value,
        ctx: &CallContext,
    ) -> Result<Value, RpcError> {
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let timeout = ctx
            .remaining()
            .min(self.request_timeout)
            .max(Duration::from_millis(50));

        let response = self
            .client
            .post(&self.http_url)
            .timeout(timeout)
            .json(&body)
            .send()
            .await;

        let response = match response {
            Ok(r) => r,
            Err(e) if e.is_timeout() => {
                return Err(RpcError::Transient {
                    code: "SEN-RPC-TIMEOUT",
                    message: format!("{method} timed out against provider {}", self.id),
                    request_id: Some(ctx.request_id),
                    retry_after: None,
                })
            }
            Err(_e) => {
                return Err(RpcError::Transient {
                    code: "SEN-RPC-TRANSPORT",
                    message: format!("{method} transport failure against provider {}", self.id),
                    request_id: Some(ctx.request_id),
                    retry_after: None,
                })
            }
        };

        let status = response.status();
        if status.as_u16() == 429 {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_secs);
            return Err(match retry_after {
                Some(d) => RpcError::Transient {
                    code: "SEN-RPC-429-RA",
                    message: format!("{method} rate limited by provider {}", self.id),
                    request_id: Some(ctx.request_id),
                    retry_after: Some(d),
                },
                None => RpcError::Transient {
                    code: "SEN-RPC-429-NORA",
                    message: format!(
                        "{method} rate limited by provider {} (no Retry-After)",
                        self.id
                    ),
                    request_id: Some(ctx.request_id),
                    retry_after: None,
                },
            });
        }
        if status.is_server_error() {
            return Err(RpcError::Transient {
                code: "SEN-RPC-5XX",
                message: format!("{method} returned HTTP {status} from provider {}", self.id),
                request_id: Some(ctx.request_id),
                retry_after: None,
            });
        }

        let text = response.text().await.map_err(|_| RpcError::Malformed {
            code: "SEN-RPC-MALFORMED",
            message: format!(
                "{method}: could not read response body from provider {}",
                self.id
            ),
            request_id: Some(ctx.request_id),
        })?;
        let parsed: Value = serde_json::from_str(&text).map_err(|_| RpcError::Malformed {
            code: "SEN-RPC-MALFORMED",
            message: format!("{method}: response body was not valid JSON"),
            request_id: Some(ctx.request_id),
        })?;

        if let Some(error) = parsed.get("error") {
            let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("no message")
                .to_string();
            return Err(classify_jsonrpc_error(code, message, ctx.request_id));
        }

        parsed
            .get("result")
            .cloned()
            .ok_or_else(|| RpcError::Malformed {
                code: "SEN-RPC-MALFORMED",
                message: format!("{method}: response had neither result nor error"),
                request_id: Some(ctx.request_id),
            })
    }
}

/// Maps a JSON-RPC error object's numeric `code` to the five-class
/// taxonomy. Standard JSON-RPC protocol errors (-326xx) are structural —
/// invalid request shape, unknown method, bad params — and are `Fatal`
/// (RT-4: never retried, since retrying a malformed request wastes budget
/// and hides the bug). `-32016` (`JSON_RPC_SERVER_ERROR_MIN_CONTEXT_SLOT_NOT_REACHED`,
/// confirmed by reading `solana-rpc-client-api` 4.2.2's `custom_error.rs`
/// rather than assumed — AGENTS.md §15) is `Stale`. Every other Solana
/// custom server error (block cleaned up, node unhealthy, etc.) is treated
/// as `Transient` — retrying against a different provider is exactly the
/// right response to "this node can't currently answer".
fn classify_jsonrpc_error(code: i64, message: String, request_id: RequestId) -> RpcError {
    const MIN_CONTEXT_SLOT_NOT_REACHED: i64 = -32016;
    const UNSUPPORTED_TRANSACTION_VERSION: i64 = -32015;
    const METHOD_NOT_FOUND: i64 = -32601;
    match code {
        // "Method not found" is the one code that specifically means "this
        // capability does not exist" — kept distinct from the other
        // structural codes (see `SEN-RPC-METHODNOTFOUND` vs
        // `SEN-RPC-PROTOCOL` used by `discover_capabilities` below): a real
        // finding from probing Surfpool is that `-32602` (invalid params)
        // is also returned for a deliberately-thin capability probe
        // payload (e.g. a 1-byte fake transaction for `simulateTransaction`)
        // even though the method plainly exists, so that code must not be
        // treated as "unsupported" during discovery, while it is still
        // correctly non-retryable (Fatal, RT-4) for a real call.
        METHOD_NOT_FOUND => RpcError::Fatal {
            code: "SEN-RPC-METHODNOTFOUND",
            message,
            request_id: Some(request_id),
        },
        -32700 | -32600 | -32602 => RpcError::Fatal {
            code: "SEN-RPC-PROTOCOL",
            message,
            request_id: Some(request_id),
        },
        MIN_CONTEXT_SLOT_NOT_REACHED => RpcError::Stale {
            code: "SEN-RPC-STALE",
            message,
            request_id: Some(request_id),
            observed_slot: None,
            required_slot: None,
        },
        UNSUPPORTED_TRANSACTION_VERSION => RpcError::Fatal {
            code: "SEN-RPC-MAXVER",
            message,
            request_id: Some(request_id),
        },
        _ => RpcError::Transient {
            code: "SEN-RPC-SERVERERR",
            message,
            request_id: Some(request_id),
            retry_after: None,
        },
    }
}

fn extract_context_slot(result: &Value) -> Option<Slot> {
    result
        .get("context")
        .and_then(|c| c.get("slot"))
        .and_then(Value::as_u64)
        .map(Slot)
}

fn to_outcome(
    response: RpcMethodResponse,
    provider_id: ProviderId,
    request_id: RequestId,
    context_slot: Option<Slot>,
) -> RpcOutcome {
    RpcOutcome {
        response,
        provider_id,
        request_id,
        context_slot,
    }
}

#[async_trait]
impl RpcProvider for HttpRpcProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn capabilities(&self) -> &ProviderCapabilities {
        // Safety note: this returns a snapshot reference under a read lock
        // that is immediately released; callers needing a stable view
        // should prefer `discover_capabilities`. Kept as `&ProviderCapabilities`
        // to match the trait; in practice callers use `discover_capabilities`
        // for anything correctness-sensitive (the pool caches capabilities
        // itself at construction time).
        // A leaked read guard would be unsound to return by reference, so
        // this provider instead exposes capabilities only through
        // `discover_capabilities`; the trait's `capabilities()` accessor
        // returns a `'static` empty placeholder for this provider type.
        static UNKNOWN: std::sync::OnceLock<ProviderCapabilities> = std::sync::OnceLock::new();
        UNKNOWN.get_or_init(ProviderCapabilities::unknown)
    }

    async fn discover_capabilities(&self) -> Result<ProviderCapabilities, RpcError> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let ctx = CallContext::new(
            crate::provider::CorrelationId::new(),
            RequestClass::ScheduledScan,
            Commitment::Confirmed,
            deadline,
            None,
        );
        let mut caps = ProviderCapabilities::unknown();

        let version = self.send_rpc("getVersion", json!([]), &ctx).await?;
        caps.node_version = version
            .get("solana-core")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        caps.mark_supported("getVersion");

        // getBlock must be probed against a slot the node actually still
        // retains — slot 0 is a real finding (not assumed): a rolling
        // local validator/surfnet prunes early history and rejects it with
        // -32602 ("before the first local slot"), which is
        // indistinguishable at the JSON-RPC protocol-error level from a
        // genuinely unsupported method unless a real, currently-retained
        // slot is used instead.
        let probe_slot = match self.send_rpc("getSlot", json!([]), &ctx).await {
            Ok(v) => v.as_u64().unwrap_or(0),
            Err(_) => 0,
        };

        // Probe every method Sentinel needs with a cheap, real call.
        // "Unsupported" is recorded on a Fatal protocol-level error
        // (method not found / invalid params structurally); any other
        // outcome (including a transient failure — the method exists, the
        // node is just struggling right now) counts as supported, per
        // rpc-strategy.md §3: capability is about what the method
        // *is*, not momentary node health.
        let probes: [(&'static str, Value); 14] = [
            ("getHealth", json!([])),
            ("getSlot", json!([])),
            ("getBlockHeight", json!([])),
            (
                "getBlocks",
                json!([probe_slot.saturating_sub(1), probe_slot]),
            ),
            ("getLatestBlockhash", json!([])),
            ("getMultipleAccounts", json!([[]])),
            (
                "getProgramAccounts",
                json!(["11111111111111111111111111111111"]),
            ),
            (
                "getRecentPrioritizationFees",
                json!([["11111111111111111111111111111111"]]),
            ),
            ("getSignatureStatuses", json!([[]])),
            (
                "getSignaturesForAddress",
                json!(["11111111111111111111111111111111"]),
            ),
            (
                "getBlock",
                json!([probe_slot, {"maxSupportedTransactionVersion": 0}]),
            ),
            (
                "getTransaction",
                json!(["1111111111111111111111111111111111111111111111111111111111111111", {"maxSupportedTransactionVersion": 0}]),
            ),
            // Deliberately-invalid 1-byte payloads: real calls always send
            // a submission-ready transaction (this abstraction never
            // builds/signs one — phase-03-rpc.md §2), so discovery cannot
            // probe with a genuinely valid transaction. A thin payload is
            // enough to prove the method exists (it gets far enough to
            // reject the bytes) without ever risking an actual submission.
            (
                "simulateTransaction",
                json!(["AA==", {"sigVerify": false, "encoding": "base64"}]),
            ),
            (
                "sendTransaction",
                json!(["AA==", {"encoding": "base64", "skipPreflight": true}]),
            ),
        ];
        for (method, params) in probes {
            match self.send_rpc(method, params, &ctx).await {
                Ok(_) => {
                    caps.mark_supported(method);
                }
                Err(RpcError::Fatal {
                    code: "SEN-RPC-METHODNOTFOUND",
                    ..
                }) => {
                    // Confirmed absent — leave unmarked. Every other error
                    // (including SEN-RPC-PROTOCOL / invalid params from a
                    // deliberately-thin probe payload) still proves the
                    // method exists.
                }
                Err(_) => {
                    caps.mark_supported(method);
                }
            }
        }
        for m in [
            "getLatestBlockhash",
            "getMultipleAccounts",
            "getProgramAccounts",
            "getSignatureStatuses",
        ] {
            caps.mark_has_context(m);
        }
        caps.supports_get_program_accounts = caps.supports_method("getProgramAccounts");
        caps.max_blocks_per_get_blocks = 500_000;
        caps.supports_min_context_slot = true;
        caps.supports_recent_prioritization_fees =
            caps.supports_method("getRecentPrioritizationFees");
        caps.max_supported_transaction_version = 0;

        if let Ok(mut guard) = self.capabilities.write() {
            *guard = caps.clone();
        }
        Ok(caps)
    }

    fn configured_classes(&self) -> &HashSet<RequestClass> {
        &self.configured_classes
    }

    async fn call(&self, req: RpcMethodCall, ctx: &CallContext) -> Result<RpcOutcome, RpcError> {
        let method = req.method_name();
        let params = self.build_params(&req, ctx);
        let result = self.send_rpc(method, params, ctx).await?;
        let context_slot = extract_context_slot(&result);

        let response = match &req {
            RpcMethodCall::GetVersion => RpcMethodResponse::Version {
                solana_core: result
                    .get("solana-core")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            },
            RpcMethodCall::GetHealth => RpcMethodResponse::Health,
            RpcMethodCall::GetSlot => {
                RpcMethodResponse::Slot(Slot(result.as_u64().unwrap_or_default()))
            }
            RpcMethodCall::GetBlockHeight => {
                RpcMethodResponse::BlockHeight(result.as_u64().unwrap_or_default())
            }
            RpcMethodCall::GetBlocks { .. } => RpcMethodResponse::Blocks(
                result
                    .as_array()
                    .map(|a| a.iter().filter_map(Value::as_u64).map(Slot).collect())
                    .unwrap_or_default(),
            ),
            RpcMethodCall::GetBlock { slot, .. } => RpcMethodResponse::Block {
                slot: *slot,
                blockhash: result
                    .get("blockhash")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                raw_json: result.clone(),
            },
            RpcMethodCall::GetTransaction { signature, .. } => RpcMethodResponse::Transaction {
                signature: signature.clone(),
                raw_json: result.clone(),
            },
            RpcMethodCall::GetLatestBlockhash => {
                let value = result.get("value").unwrap_or(&result);
                RpcMethodResponse::LatestBlockhash {
                    blockhash: value
                        .get("blockhash")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    last_valid_block_height: value
                        .get("lastValidBlockHeight")
                        .and_then(Value::as_u64)
                        .unwrap_or_default(),
                    context_slot: context_slot.unwrap_or(Slot(0)),
                }
            }
            RpcMethodCall::GetMultipleAccounts { .. } => {
                let value = result
                    .get("value")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                RpcMethodResponse::MultipleAccounts {
                    context_slot: context_slot.unwrap_or(Slot(0)),
                    accounts: value
                        .into_iter()
                        .map(|v| if v.is_null() { None } else { Some(v) })
                        .collect(),
                }
            }
            RpcMethodCall::GetProgramAccounts { .. } => {
                let (accounts, ctx_slot) = match result.get("value") {
                    Some(v) => (v.as_array().cloned().unwrap_or_default(), context_slot),
                    None => (result.as_array().cloned().unwrap_or_default(), context_slot),
                };
                RpcMethodResponse::ProgramAccounts {
                    context_slot: ctx_slot.unwrap_or(Slot(0)),
                    accounts,
                }
            }
            RpcMethodCall::GetRecentPrioritizationFees { .. } => {
                RpcMethodResponse::PrioritizationFees(
                    result.as_array().cloned().unwrap_or_default(),
                )
            }
            RpcMethodCall::GetSignatureStatuses { .. } => {
                let value = result
                    .get("value")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                RpcMethodResponse::SignatureStatuses {
                    context_slot: context_slot.unwrap_or(Slot(0)),
                    statuses: value
                        .into_iter()
                        .map(|v| if v.is_null() { None } else { Some(v) })
                        .collect(),
                }
            }
            RpcMethodCall::GetSignaturesForAddress { .. } => {
                RpcMethodResponse::SignaturesForAddress(
                    result.as_array().cloned().unwrap_or_default(),
                )
            }
            RpcMethodCall::SimulateTransaction { .. } => {
                let value = result.get("value").unwrap_or(&result);
                RpcMethodResponse::SimulateTransaction {
                    context_slot: context_slot.unwrap_or(Slot(0)),
                    err: value.get("err").cloned().filter(|v| !v.is_null()),
                    logs: value
                        .get("logs")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default(),
                    units_consumed: value.get("unitsConsumed").and_then(Value::as_u64),
                }
            }
            RpcMethodCall::SendTransaction { .. } => RpcMethodResponse::SendTransaction {
                signature: result.as_str().unwrap_or_default().to_string(),
            },
        };

        Ok(to_outcome(
            response,
            self.id.clone(),
            ctx.request_id,
            context_slot,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A-SEC-01: the provider's `Debug` output, its `id()`, and every
    /// message this module constructs must never contain the raw URL —
    /// specifically, a credential embedded in it.
    #[test]
    fn credential_bearing_url_never_appears_in_debug_or_id() {
        const MARKER: &str = "SENTINEL_TEST_do_not_leak_this_token_9f31";
        let provider = HttpRpcProvider::new(HttpProviderConfig {
            id: ProviderId("primary".to_string()),
            http_url: format!("https://rpc.example.com/{MARKER}"),
            configured_classes: HashSet::new(),
            request_timeout: Duration::from_secs(1),
        });

        let debug_output = format!("{provider:?}");
        assert!(
            !debug_output.contains(MARKER),
            "Debug output leaked the URL: {debug_output}"
        );
        assert!(
            !debug_output.contains("example.com"),
            "Debug output leaked the host: {debug_output}"
        );

        assert_eq!(provider.id().0, "primary");
        assert!(!provider.id().0.contains(MARKER));
    }

    /// A-SEC-01, more directly: drives a real (loopback, doomed-to-fail)
    /// call through the provider and asserts the resulting `RpcError`'s
    /// `Display` output — what would actually reach logs/tracing — never
    /// contains the credential, only the safe `id`.
    #[tokio::test]
    async fn error_surfaced_from_a_failed_call_never_contains_the_credential() {
        const MARKER: &str = "SENTINEL_TEST_do_not_leak_this_token_9f31";
        // Port 1 is reserved/unlikely to be listening in any test
        // environment, so this connection is expected to fail fast —
        // exactly the transport-error path whose message this test checks.
        let provider = HttpRpcProvider::new(HttpProviderConfig {
            id: ProviderId("primary".to_string()),
            http_url: format!("http://127.0.0.1:1/{MARKER}"),
            configured_classes: HashSet::new(),
            request_timeout: Duration::from_millis(300),
        });
        let ctx = CallContext::new(
            crate::provider::CorrelationId::new(),
            RequestClass::Backfill,
            Commitment::Confirmed,
            Instant::now() + Duration::from_millis(500),
            None,
        );
        let result = provider.call(RpcMethodCall::GetSlot, &ctx).await;
        assert!(
            result.is_err(),
            "connecting to a closed loopback port must fail"
        );
        let message = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            !message.contains(MARKER),
            "error Display leaked the credential: {message}"
        );
        assert!(
            message.contains("primary"),
            "error Display should still name the safe provider id: {message}"
        );
    }
}
