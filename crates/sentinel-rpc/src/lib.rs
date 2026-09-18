//! `RpcProvider`/`RpcPool`: Sentinel's resilient RPC abstraction
//! (`docs/rpc-strategy.md`, `docs/phases/phase-03-rpc.md`).
//!
//! No call site outside this crate constructs a raw Solana RPC/PubSub
//! client (`CI-NORAWCLIENT`); every request carries an explicit commitment,
//! a request id, and a request class, and goes through [`pool::RpcPool`] so
//! budget, breaker, retry, and freshness are unbypassable.
//!
//! This crate is a **library** in Phase 3 — nothing in the workspace
//! consumes it yet (`docs/phases/phase-03-rpc.md` §2: no ingestion, no raw
//! writes, no gap detection, no transaction building/signing here).

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod breaker;
pub mod budget;
pub mod capabilities;
pub mod fault;
pub mod fixtures;
pub mod health;
pub mod http;
pub mod pool;
pub mod provider;
pub mod ws;

pub use breaker::{Breaker, BreakerConfig, BreakerState};
pub use budget::{BudgetConfig, BudgetOutcome, ProviderBudget};
pub use capabilities::ProviderCapabilities;
pub use health::{ProviderHealth, ProviderHealthTracker};
pub use http::{HttpProviderConfig, HttpRpcProvider};
pub use pool::{BroadcastOutcome, PoolConfig, ProviderSpec, ResolvedProviderSpec, RpcPool};
pub use provider::{
    CallContext, CorrelationId, ProviderId, RequestClass, RequestId, RpcError, RpcMethodCall,
    RpcMethodResponse, RpcOutcome, RpcProvider,
};
