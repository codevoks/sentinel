//! The `RpcProvider` abstraction (`docs/rpc-strategy.md` §1): one endpoint,
//! knowing nothing about pools, failover, or policy.
//!
//! `rpc-strategy.md` §1 sketches the trait with a generic `call<R:
//! RpcRequest>` method. A generic trait method is not object-safe, and
//! `RpcPool` must hold a heterogeneous set of providers (`HttpRpcProvider`,
//! `FixtureProvider`, `FaultInjectingProvider`) behind one dynamic type. This
//! is a routine implementation adaptation (AGENTS.md §13 — not an ADR-worthy
//! deviation): `RpcRequest`/`RpcResponse` become a closed enum of exactly the
//! methods `rpc-strategy.md` §4 and §7 name as ones Sentinel needs, and
//! `RpcProvider::call` takes/returns those enums directly. Every documented
//! property is preserved: no call site builds its own raw request, every
//! call still carries commitment/request-id/class, and the set of methods is
//! exactly the frozen one — nothing invented "for later" (AGENTS.md §6).

use std::collections::HashSet;
use std::fmt;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use sentinel_core::{Commitment, Slot};

use crate::capabilities::ProviderCapabilities;

/// Identifies one logical `RpcPool::call`, stable across every retry attempt
/// for that call. Never persisted directly; exists so diagnostics can tie
/// several attempts (several `RequestId`s) together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CorrelationId(pub Uuid);

impl CorrelationId {
    pub fn new() -> Self {
        CorrelationId(Uuid::new_v4())
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identifies exactly one `RpcProvider::call` attempt. A fresh `RequestId`
/// is generated for every attempt (`docs/phases/phase-03-rpc.md` §6) —
/// retrying the same logical call against a different provider produces a
/// new `RequestId`, never a reused one, because Phase 4's raw observation
/// row keys on "the call and provider that produced it"
/// (`docs/rpc-strategy.md` §9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequestId(pub Uuid);

impl RequestId {
    pub fn new() -> Self {
        RequestId(Uuid::new_v4())
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A Sentinel-owned, credential-free provider identifier
/// (`docs/rpc-strategy.md` §10 CF-2, `docs/threat-model.md` A-SEC-01).
///
/// Constructed once from configuration; every other component (health,
/// breaker, metrics labels, logs, `provider_health.provider_id`) uses this
/// and never sees the underlying URL again.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderId(pub String);

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Request classes and their priority ordering (`docs/rpc-strategy.md` §4).
/// Declaration order is priority order — `Ord`/`PartialOrd` are derived and
/// relied on by `pool.rs` selection and by `budget.rs` isolation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RequestClass {
    /// Priority 1 (highest). Never starved; dedicated budget slice.
    Execution,
    /// Priority 2. Protected.
    RealtimeCompleteness,
    /// Priority 3. Yields to 1-2.
    GapRepair,
    /// Priority 4. Yields, and is expected to.
    Backfill,
    /// Priority 5 (lowest). Starves first, by design.
    ScheduledScan,
}

impl RequestClass {
    pub const ALL: [RequestClass; 5] = [
        RequestClass::Execution,
        RequestClass::RealtimeCompleteness,
        RequestClass::GapRepair,
        RequestClass::Backfill,
        RequestClass::ScheduledScan,
    ];

    /// 1 = highest priority, per `docs/rpc-strategy.md` §4's table.
    pub fn priority(self) -> u8 {
        match self {
            RequestClass::Execution => 1,
            RequestClass::RealtimeCompleteness => 2,
            RequestClass::GapRepair => 3,
            RequestClass::Backfill => 4,
            RequestClass::ScheduledScan => 5,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            RequestClass::Execution => "execution",
            RequestClass::RealtimeCompleteness => "realtime_completeness",
            RequestClass::GapRepair => "gap_repair",
            RequestClass::Backfill => "backfill",
            RequestClass::ScheduledScan => "scheduled_scan",
        }
    }
}

impl fmt::Display for RequestClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// `CallContext` (`docs/rpc-strategy.md` §1): correlation id, per-attempt
/// request id, deadline, request class, explicit commitment, and the
/// `minContextSlot` freshness requirement. There is no "default commitment"
/// path — every construction requires one explicitly (`phase-03-rpc.md` §6).
#[derive(Debug, Clone)]
pub struct CallContext {
    pub correlation_id: CorrelationId,
    pub request_id: RequestId,
    pub class: RequestClass,
    pub commitment: Commitment,
    pub deadline: Instant,
    pub min_context_slot: Option<Slot>,
}

impl CallContext {
    pub fn new(
        correlation_id: CorrelationId,
        class: RequestClass,
        commitment: Commitment,
        deadline: Instant,
        min_context_slot: Option<Slot>,
    ) -> Self {
        CallContext {
            correlation_id,
            request_id: RequestId::new(),
            class,
            commitment,
            deadline,
            min_context_slot,
        }
    }

    /// Builds the `CallContext` for the next retry attempt: same logical
    /// call (same `correlation_id`), fresh `request_id`, same deadline
    /// (retries share one deadline — RT-1).
    pub fn next_attempt(&self) -> Self {
        CallContext {
            correlation_id: self.correlation_id,
            request_id: RequestId::new(),
            class: self.class,
            commitment: self.commitment,
            deadline: self.deadline,
            min_context_slot: self.min_context_slot,
        }
    }

    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
}

/// The closed set of RPC methods Sentinel's architecture uses
/// (`docs/rpc-strategy.md` §4, §7). Each variant carries exactly the
/// parameters the method needs; every `getBlock`/`getTransaction` variant
/// requires `max_supported_transaction_version` at the type level so a call
/// site cannot omit it (RPC-10, `CI-NOMAXVER`).
#[derive(Debug, Clone)]
pub enum RpcMethodCall {
    GetVersion,
    GetHealth,
    GetSlot,
    GetBlockHeight,
    GetBlocks {
        start_slot: Slot,
        end_slot: Option<Slot>,
    },
    GetBlock {
        slot: Slot,
        max_supported_transaction_version: u8,
    },
    GetTransaction {
        signature: String,
        max_supported_transaction_version: u8,
    },
    GetLatestBlockhash,
    GetMultipleAccounts {
        pubkeys: Vec<String>,
    },
    GetProgramAccounts {
        program_id: String,
    },
    GetRecentPrioritizationFees {
        writable_accounts: Vec<String>,
    },
    GetSignatureStatuses {
        signatures: Vec<String>,
    },
    GetSignaturesForAddress {
        address: String,
        until: Option<String>,
    },
    SimulateTransaction {
        raw_tx_base64: String,
    },
    /// Half-open probes use `GetSlot`/`GetHealth` only (CB-3) — never this
    /// variant, and never `SimulateTransaction`/`SendTransaction`.
    SendTransaction {
        raw_tx_base64: String,
    },
}

impl RpcMethodCall {
    /// The bare JSON-RPC method name, used for capability lookups and
    /// metrics labels (bounded cardinality — never a request id).
    pub fn method_name(&self) -> &'static str {
        match self {
            RpcMethodCall::GetVersion => "getVersion",
            RpcMethodCall::GetHealth => "getHealth",
            RpcMethodCall::GetSlot => "getSlot",
            RpcMethodCall::GetBlockHeight => "getBlockHeight",
            RpcMethodCall::GetBlocks { .. } => "getBlocks",
            RpcMethodCall::GetBlock { .. } => "getBlock",
            RpcMethodCall::GetTransaction { .. } => "getTransaction",
            RpcMethodCall::GetLatestBlockhash => "getLatestBlockhash",
            RpcMethodCall::GetMultipleAccounts { .. } => "getMultipleAccounts",
            RpcMethodCall::GetProgramAccounts { .. } => "getProgramAccounts",
            RpcMethodCall::GetRecentPrioritizationFees { .. } => "getRecentPrioritizationFees",
            RpcMethodCall::GetSignatureStatuses { .. } => "getSignatureStatuses",
            RpcMethodCall::GetSignaturesForAddress { .. } => "getSignaturesForAddress",
            RpcMethodCall::SimulateTransaction { .. } => "simulateTransaction",
            RpcMethodCall::SendTransaction { .. } => "sendTransaction",
        }
    }

    /// Whether this method's response is expected to carry an RPC
    /// `context.slot` that freshness checking can validate
    /// (`docs/rpc-strategy.md` §5.2).
    pub fn has_context(&self) -> bool {
        matches!(
            self,
            RpcMethodCall::GetMultipleAccounts { .. }
                | RpcMethodCall::GetProgramAccounts { .. }
                | RpcMethodCall::SimulateTransaction { .. }
                | RpcMethodCall::GetLatestBlockhash
                | RpcMethodCall::GetSignatureStatuses { .. }
        )
    }
}

/// The response counterpart to [`RpcMethodCall`]. Kept intentionally thin —
/// this phase does not decode beyond what freshness/divergence checking and
/// the demo CLI need (`docs/phases/phase-03-rpc.md` §2 non-scope).
#[derive(Debug, Clone, PartialEq)]
pub enum RpcMethodResponse {
    Version {
        solana_core: String,
    },
    Health,
    Slot(Slot),
    BlockHeight(u64),
    Blocks(Vec<Slot>),
    /// A block, identified by its blockhash (used for FI-10/FI-11 content
    /// divergence: two providers' `Block` values differing for one slot, or
    /// differing for one already-agreed blockhash, is exactly what CB-4 and
    /// the divergence tests key on). `raw_json` is the full decoded payload
    /// for anything the demo/tests need beyond the identity fields.
    Block {
        slot: Slot,
        blockhash: String,
        raw_json: serde_json::Value,
    },
    Transaction {
        signature: String,
        raw_json: serde_json::Value,
    },
    LatestBlockhash {
        blockhash: String,
        last_valid_block_height: u64,
        context_slot: Slot,
    },
    MultipleAccounts {
        context_slot: Slot,
        accounts: Vec<Option<serde_json::Value>>,
    },
    ProgramAccounts {
        context_slot: Slot,
        accounts: Vec<serde_json::Value>,
    },
    PrioritizationFees(Vec<serde_json::Value>),
    SignatureStatuses {
        context_slot: Slot,
        statuses: Vec<Option<serde_json::Value>>,
    },
    SignaturesForAddress(Vec<serde_json::Value>),
    SimulateTransaction {
        context_slot: Slot,
        err: Option<serde_json::Value>,
        logs: Vec<String>,
        units_consumed: Option<u64>,
    },
    SendTransaction {
        signature: String,
    },
}

/// The frozen five-class error taxonomy (`docs/architecture.md` §9),
/// specialised to what an RPC call can produce. Every `RpcProvider::call`
/// failure maps to exactly one of these (`phase-03-rpc.md` §9).
#[derive(Debug, Clone)]
pub enum RpcError {
    /// Provider timeout, 429, 5xx, connection reset. Bounded retry with
    /// jittered backoff; counts toward the breaker.
    Transient {
        code: &'static str,
        message: String,
        request_id: Option<RequestId>,
        /// Present exactly for a 429 that carried a `Retry-After` header
        /// (`phase-03-rpc.md` §9's "429 with Retry-After" case).
        retry_after: Option<Duration>,
    },
    /// Response older than required: `minContextSlot` unmet, or
    /// `context.slot` behind the known head beyond tolerance (FI-09).
    /// Never used as canonical (RPC-04).
    Stale {
        code: &'static str,
        message: String,
        request_id: Option<RequestId>,
        observed_slot: Option<Slot>,
        required_slot: Option<Slot>,
    },
    /// Payload failed validation or decoding. No panic; error surfaced.
    Malformed {
        code: &'static str,
        message: String,
        request_id: Option<RequestId>,
    },
    /// Two providers (or a provider and its own prior answer) returned
    /// different content for the same natural key (FI-10/FI-11). Distinct
    /// from `Malformed` and from ordinary `Transient` failure because CB-4
    /// requires it trip the breaker immediately, with no minimum sample.
    /// Boxed (`clippy::result_large_err`): this is by far `RpcError`'s
    /// largest variant (two `ProviderId`s plus a message) and divergence is
    /// the rare path, so boxing it keeps the common `Result<_, RpcError>`
    /// small everywhere else.
    ContentDivergence {
        code: &'static str,
        detail: Box<ContentDivergenceDetail>,
    },
    /// Configuration wrong, unsupported capability/method, invariant
    /// breach. Never retried — retrying a bug wastes budget and hides it
    /// (RT-4).
    Fatal {
        code: &'static str,
        message: String,
        request_id: Option<RequestId>,
    },
}

#[derive(Debug, Clone)]
pub struct ContentDivergenceDetail {
    pub message: String,
    pub slot: Option<Slot>,
    pub blockhash: Option<String>,
    pub provider_a: ProviderId,
    pub provider_b: ProviderId,
}

impl RpcError {
    /// RT-4: a non-retryable error fails fast. Only `Transient` is
    /// retryable; `Stale` is handled by re-issuing against a different
    /// provider at the pool level (not a raw retry of the same failure),
    /// and everything else fails fast.
    pub fn is_retryable(&self) -> bool {
        matches!(self, RpcError::Transient { .. })
    }

    pub fn code(&self) -> &'static str {
        match self {
            RpcError::Transient { code, .. } => code,
            RpcError::Stale { code, .. } => code,
            RpcError::Malformed { code, .. } => code,
            RpcError::ContentDivergence { code, .. } => code,
            RpcError::Fatal { code, .. } => code,
        }
    }

    pub fn class_name(&self) -> &'static str {
        match self {
            RpcError::Transient { .. } => "transient",
            RpcError::Stale { .. } => "stale",
            RpcError::Malformed { .. } => "malformed",
            RpcError::ContentDivergence { .. } => "content_divergence",
            RpcError::Fatal { .. } => "fatal",
        }
    }

    pub fn request_id(&self) -> Option<RequestId> {
        match self {
            RpcError::Transient { request_id, .. } => *request_id,
            RpcError::Stale { request_id, .. } => *request_id,
            RpcError::Malformed { request_id, .. } => *request_id,
            RpcError::ContentDivergence { .. } => None,
            RpcError::Fatal { request_id, .. } => *request_id,
        }
    }

    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            RpcError::Transient { retry_after, .. } => *retry_after,
            _ => None,
        }
    }
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RpcError::Transient { code, message, .. } => write!(f, "[{code}] transient: {message}"),
            RpcError::Stale { code, message, .. } => write!(f, "[{code}] stale: {message}"),
            RpcError::Malformed { code, message, .. } => write!(f, "[{code}] malformed: {message}"),
            RpcError::ContentDivergence { code, detail } => write!(
                f,
                "[{code}] content divergence between {} and {}: {}",
                detail.provider_a, detail.provider_b, detail.message
            ),
            RpcError::Fatal { code, message, .. } => write!(f, "[{code}] fatal: {message}"),
        }
    }
}

impl std::error::Error for RpcError {}

/// The outcome of a single successful provider attempt: the response, the
/// provider that served it, and the request id that produced it (surfaced
/// so a later Phase-4 raw observation can persist it — `phase-03-rpc.md` §6).
#[derive(Debug, Clone)]
pub struct RpcOutcome {
    pub response: RpcMethodResponse,
    pub provider_id: ProviderId,
    pub request_id: RequestId,
    pub context_slot: Option<Slot>,
}

/// One endpoint. Knows nothing about pools, failover, or policy
/// (`docs/rpc-strategy.md` §1).
#[async_trait]
pub trait RpcProvider: Send + Sync {
    fn id(&self) -> &ProviderId;
    fn capabilities(&self) -> &ProviderCapabilities;

    /// Probes (or re-probes, on reconnect) this provider's capabilities.
    /// Implementations must not assume from name/version alone
    /// (`docs/rpc-strategy.md` §3) — each uncertain capability is
    /// exercised with a bounded, cheap call.
    async fn discover_capabilities(&self) -> Result<ProviderCapabilities, RpcError>;

    async fn call(&self, req: RpcMethodCall, ctx: &CallContext) -> Result<RpcOutcome, RpcError>;

    /// The classes this provider is configured to serve
    /// (`docs/rpc-strategy.md` §10 CF-3).
    fn configured_classes(&self) -> &HashSet<RequestClass>;
}
