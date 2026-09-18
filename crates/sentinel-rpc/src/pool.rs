//! `RpcPool`: the set of providers with health, policy, and selection —
//! what every caller actually uses (`docs/rpc-strategy.md` §1). No call
//! site outside this module ever talks to a provider directly
//! (`CI-NORAWCLIENT`); budget, breaker, retry, and freshness are enforced
//! here, unbypassably.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::Rng;

use sentinel_core::{Commitment, Slot};

use crate::breaker::BreakerConfig;
use crate::budget::{BudgetConfig, BudgetOutcome, ProviderBudget};
use crate::capabilities::{self, ProviderCapabilities};
use crate::health::{ProviderHealth, ProviderHealthTracker};
use crate::provider::{
    CallContext, CorrelationId, ProviderId, RequestClass, RpcError, RpcMethodCall,
    RpcMethodResponse, RpcOutcome, RpcProvider,
};

/// RT-1: bounded attempts per class. Never unbounded.
fn max_attempts(class: RequestClass) -> u32 {
    match class {
        RequestClass::Execution => 5,
        RequestClass::RealtimeCompleteness => 4,
        RequestClass::GapRepair => 4,
        RequestClass::Backfill => 3,
        RequestClass::ScheduledScan => 2,
    }
}

/// RT-2: exponential backoff with full jitter (bounded, no thundering
/// herd). `attempt` is zero-indexed.
pub fn backoff_full_jitter(attempt: u32, base: Duration, cap: Duration) -> Duration {
    let exp_ms = (base.as_millis() as u64).saturating_mul(1u64 << attempt.min(20));
    let capped_ms = exp_ms.min(cap.as_millis() as u64).max(1);
    // "Full jitter": sleep = random_between(0, capped). AWS's documented
    // algorithm — avoids synchronized retries becoming an outage (RT-2).
    let jittered_ms = rand::thread_rng().gen_range(0..=capped_ms);
    Duration::from_millis(jittered_ms)
}

#[derive(Debug, Clone)]
pub struct FreshnessConfig {
    /// Slots of allowed lag for methods without native `context.slot`
    /// support, compared against the pool's tracked head
    /// (`docs/rpc-strategy.md` §5.2).
    pub max_slot_lag: u64,
}

impl Default for FreshnessConfig {
    fn default() -> Self {
        FreshnessConfig { max_slot_lag: 150 }
    }
}

#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub backoff_base: Duration,
    pub backoff_cap: Duration,
    pub call_deadline: Duration,
    pub freshness: FreshnessConfig,
}

impl Default for PoolConfig {
    fn default() -> Self {
        PoolConfig {
            backoff_base: Duration::from_millis(50),
            backoff_cap: Duration::from_secs(5),
            call_deadline: Duration::from_secs(10),
            freshness: FreshnessConfig::default(),
        }
    }
}

/// One provider's static configuration, as passed to
/// [`RpcPool::build`]/[`RpcPool::from_providers`].
pub struct ProviderSpec {
    pub provider: Arc<dyn RpcProvider>,
    pub configured_classes: HashSet<RequestClass>,
    pub budget: BudgetConfig,
}

/// [`ProviderSpec`] plus its already-discovered capabilities, for
/// [`RpcPool::from_providers`] (used directly by tests that construct
/// capabilities themselves instead of probing).
pub struct ResolvedProviderSpec {
    pub provider: Arc<dyn RpcProvider>,
    pub capabilities: ProviderCapabilities,
    pub configured_classes: HashSet<RequestClass>,
    pub budget: BudgetConfig,
}

struct ProviderEntry {
    provider: Arc<dyn RpcProvider>,
    budget: ProviderBudget,
    health: ProviderHealthTracker,
    configured_classes: HashSet<RequestClass>,
}

/// Result of a `broadcast()` fan-out call (`docs/rpc-strategy.md` §1, RT-5).
#[derive(Debug, Clone)]
pub struct BroadcastOutcome {
    pub per_provider: Vec<(ProviderId, Result<RpcOutcome, RpcError>)>,
}

impl BroadcastOutcome {
    pub fn any_success(&self) -> bool {
        self.per_provider.iter().any(|(_, r)| r.is_ok())
    }

    pub fn all_failed(&self) -> bool {
        !self.any_success() && !self.per_provider.is_empty()
    }

    pub fn signatures(&self) -> Vec<String> {
        self.per_provider
            .iter()
            .filter_map(|(_, r)| match r {
                Ok(RpcOutcome {
                    response: RpcMethodResponse::SendTransaction { signature },
                    ..
                }) => Some(signature.clone()),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PoolBuildError {
    #[error(transparent)]
    Capability(#[from] capabilities::CapabilityError),
}

/// The resilient pool. Construction probes every provider's capabilities
/// and validates them against configured classes (CF-4) — a provider
/// configured for a class it cannot serve fails **at construction**, never
/// silently at request time.
pub struct RpcPool {
    entries: Vec<ProviderEntry>,
    config: PoolConfig,
    /// The best-known chain head, used for freshness comparison on methods
    /// without native `context.slot` (`docs/rpc-strategy.md` §5.2). Updated
    /// externally (typically by the WS manager's slot subscription).
    known_head_slot: AtomicU64,
}

impl RpcPool {
    /// Constructs a pool from already-discovered capabilities (used by
    /// tests and by `build` below, which does the discovery itself).
    pub fn from_providers(
        providers: Vec<ResolvedProviderSpec>,
        breaker_config: BreakerConfig,
        pool_config: PoolConfig,
    ) -> Result<Self, PoolBuildError> {
        let mut entries = Vec::new();
        for spec in providers {
            capabilities::validate_configured_classes(
                &spec.provider.id().0,
                &spec.configured_classes,
                &spec.capabilities,
            )?;
            entries.push(ProviderEntry {
                health: ProviderHealthTracker::new(spec.provider.id().clone(), breaker_config),
                budget: ProviderBudget::new(spec.budget),
                provider: spec.provider,
                configured_classes: spec.configured_classes,
            });
        }
        Ok(RpcPool {
            entries,
            config: pool_config,
            known_head_slot: AtomicU64::new(0),
        })
    }

    /// Probes every provider's capabilities live and builds the pool,
    /// failing loudly (CF-4 / `phase-03-rpc.md` §2) if any configured
    /// class cannot actually be served.
    pub async fn build(
        providers: Vec<ProviderSpec>,
        breaker_config: BreakerConfig,
        pool_config: PoolConfig,
    ) -> Result<Self, PoolBuildError> {
        let mut resolved = Vec::new();
        for spec in providers {
            let caps = spec
                .provider
                .discover_capabilities()
                .await
                .unwrap_or_else(|_| ProviderCapabilities::unknown());
            resolved.push(ResolvedProviderSpec {
                provider: spec.provider,
                capabilities: caps,
                configured_classes: spec.configured_classes,
                budget: spec.budget,
            });
        }
        Self::from_providers(resolved, breaker_config, pool_config)
    }

    pub fn set_known_head_slot(&self, slot: u64) {
        self.known_head_slot.fetch_max(slot, Ordering::Relaxed);
    }

    pub fn known_head_slot(&self) -> u64 {
        self.known_head_slot.load(Ordering::Relaxed)
    }

    fn budget_headroom(&self, entry: &ProviderEntry, class: RequestClass) -> f64 {
        let granted = entry.budget.granted(class) as f64;
        let rejected = entry.budget.rejected(class) as f64;
        let total = granted + rejected;
        if total == 0.0 {
            1.0
        } else {
            (granted / total).clamp(0.0, 1.0)
        }
    }

    /// Health-weighted selection (`docs/rpc-strategy.md` §5.1): every
    /// eligible provider is scored, then chosen by weighted-random pick
    /// (a small random tiebreak, not pure argmax) so equally healthy
    /// providers are not always hit in the same order — avoiding a herd on
    /// whichever provider sorts first.
    fn select(&self, class: RequestClass, exclude: &HashSet<ProviderId>) -> Option<&ProviderEntry> {
        for entry in &self.entries {
            entry.health.tick_breaker();
        }
        let head = self.known_head_slot();
        let mut scored: Vec<(&ProviderEntry, f64)> = self
            .entries
            .iter()
            .filter(|e| e.configured_classes.contains(&class))
            .filter(|e| e.health.eligible_for(class))
            .filter(|e| !exclude.contains(e.provider.id()))
            .map(|e| {
                (
                    e,
                    e.health
                        .score(self.budget_headroom(e, class), head)
                        .max(0.0001),
                )
            })
            .collect();
        if scored.is_empty() {
            return None;
        }
        let total: f64 = scored.iter().map(|(_, s)| s).sum();
        let mut pick = rand::thread_rng().gen_range(0.0..total);
        for (entry, score) in &scored {
            if pick < *score {
                return Some(entry);
            }
            pick -= score;
        }
        scored.pop().map(|(e, _)| e)
    }

    /// A provider currently `HalfOpen` (cool-down elapsed, awaiting a cheap
    /// read-only probe — CB-3). Distinct from ordinary selection because
    /// `Breaker::eligible_for` deliberately excludes `HalfOpen` from normal
    /// traffic; only the probe path is allowed to use it.
    fn select_half_open(&self) -> Option<&ProviderEntry> {
        for entry in &self.entries {
            entry.health.tick_breaker();
        }
        self.entries
            .iter()
            .find(|e| e.health.breaker_state() == crate::breaker::BreakerState::HalfOpen)
    }

    /// CB-3: issues the cheap, read-only, bounded half-open probe
    /// (`getSlot`) — never a submission, never a scan.
    pub async fn run_half_open_probe(&self) -> Option<(ProviderId, bool)> {
        let entry = self.select_half_open()?;
        let ctx = CallContext::new(
            CorrelationId::new(),
            RequestClass::GapRepair, // any non-Execution class; probes bypass class budget entirely
            Commitment::Confirmed,
            Instant::now() + Duration::from_secs(2),
            None,
        );
        let started = Instant::now();
        let result = entry.provider.call(RpcMethodCall::GetSlot, &ctx).await;
        let ok = result.is_ok();
        if ok {
            entry
                .health
                .record_half_open_probe_success(started.elapsed().as_millis() as u64);
        } else {
            entry.health.record_half_open_probe_failure();
        }
        Some((entry.provider.id().clone(), ok))
    }

    fn satisfies_freshness(
        &self,
        ctx: &CallContext,
        method: &RpcMethodCall,
        outcome: &RpcOutcome,
    ) -> Result<(), ()> {
        if let Some(required) = ctx.min_context_slot {
            match outcome.context_slot {
                Some(observed) if observed >= required => return Ok(()),
                Some(_) => return Err(()),
                None => {
                    // Method claims no context slot but a minimum was
                    // required — cannot prove freshness; fail closed.
                    return Err(());
                }
            }
        }
        // No explicit minContextSlot requested. For methods that DO report
        // a context slot, compare it to the tracked head within tolerance
        // (compensating check for methods/providers without native
        // minContextSlot support — rpc-strategy.md §5.2).
        if method.has_context() {
            let head = self.known_head_slot();
            if head > 0 {
                if let Some(Slot(observed)) = outcome.context_slot {
                    if head.saturating_sub(observed) > self.config.freshness.max_slot_lag {
                        return Err(());
                    }
                }
            }
        }
        Ok(())
    }

    /// The retry loop from `docs/rpc-strategy.md` §5, implemented exactly:
    /// select → call → accept/retry/fail-fast, bounded in count (RT-1) and
    /// in total time via `ctx.deadline`. A retry may land on a different
    /// provider (RT-3).
    pub async fn call(
        &self,
        method: RpcMethodCall,
        class: RequestClass,
        commitment: Commitment,
    ) -> Result<RpcOutcome, RpcError> {
        self.call_with_min_slot(method, class, commitment, None)
            .await
    }

    pub async fn call_with_min_slot(
        &self,
        method: RpcMethodCall,
        class: RequestClass,
        commitment: Commitment,
        min_context_slot: Option<Slot>,
    ) -> Result<RpcOutcome, RpcError> {
        let correlation_id = CorrelationId::new();
        let deadline = Instant::now() + self.config.call_deadline;
        let attempts = max_attempts(class);
        let mut excluded = HashSet::new();
        let mut last_err: Option<RpcError> = None;

        for attempt in 0..attempts {
            if Instant::now() >= deadline {
                break;
            }
            let Some(entry) = self.select(class, &excluded) else {
                return Err(RpcError::Fatal {
                    code: "SEN-RPC-000",
                    message: "no healthy provider available for this class (CB-6)".to_string(),
                    request_id: None,
                });
            };

            let permit = match entry.budget.try_acquire(class) {
                Ok(p) => p,
                Err(BudgetOutcome::RateExhausted) | Err(BudgetOutcome::ConcurrencyExhausted) => {
                    // Budget exhausted client-side: the provider is never
                    // called (phase-03-rpc.md §8). Treat like a transient
                    // condition on THIS provider and try another.
                    excluded.insert(entry.provider.id().clone());
                    last_err = Some(RpcError::Transient {
                        code: "SEN-RPC-BUDGET",
                        message: format!(
                            "client-side budget exhausted for {}",
                            entry.provider.id()
                        ),
                        request_id: None,
                        retry_after: None,
                    });
                    continue;
                }
            };

            let ctx = CallContext::new(
                correlation_id,
                class,
                commitment,
                deadline,
                min_context_slot,
            );
            let started = Instant::now();
            let result = entry.provider.call(method.clone(), &ctx).await;
            drop(permit);

            match result {
                Ok(outcome) => {
                    if self.satisfies_freshness(&ctx, &method, &outcome).is_ok() {
                        entry
                            .health
                            .record_success(started.elapsed().as_millis() as u64);
                        if let Some(Slot(s)) = outcome.context_slot {
                            self.set_known_head_slot(s);
                        }
                        return Ok(outcome);
                    } else {
                        entry.health.record_stale();
                        last_err = Some(RpcError::Stale {
                            code: "SEN-RPC-STALE",
                            message: format!(
                                "{} response failed freshness check",
                                method.method_name()
                            ),
                            request_id: Some(outcome.request_id),
                            observed_slot: outcome.context_slot,
                            required_slot: ctx.min_context_slot,
                        });
                        excluded.insert(entry.provider.id().clone());
                        continue;
                    }
                }
                Err(e) if e.is_retryable() => {
                    let is_timeout = matches!(&e, RpcError::Transient { code, .. } if *code == "SEN-RPC-TIMEOUT");
                    let is_rl = matches!(&e, RpcError::Transient { code, .. } if code.starts_with("SEN-RPC-429"));
                    entry.health.record_failure(e.code(), is_timeout, is_rl);
                    let sleep_for = e.retry_after().unwrap_or_else(|| {
                        backoff_full_jitter(
                            attempt,
                            self.config.backoff_base,
                            self.config.backoff_cap,
                        )
                    });
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    last_err = Some(e);
                    if remaining.is_zero() {
                        break;
                    }
                    tokio::time::sleep(sleep_for.min(remaining)).await;
                    continue;
                }
                Err(e) => {
                    entry.health.record_failure(e.code(), false, false);
                    return Err(e);
                }
            }
        }

        Err(last_err.unwrap_or(RpcError::Fatal {
            code: "SEN-RPC-RETRIES-EXHAUSTED",
            message: "retries exhausted without a specific last error".to_string(),
            request_id: None,
        }))
    }

    /// RT-5: fan-out submission, never sequential failover. Every eligible
    /// `Closed`-breaker provider configured for `Execution` is called
    /// concurrently; results are all surfaced.
    pub async fn broadcast(&self, raw_tx_base64: &str) -> BroadcastOutcome {
        for entry in &self.entries {
            entry.health.tick_breaker();
        }
        let eligible: Vec<&ProviderEntry> = self
            .entries
            .iter()
            .filter(|e| e.configured_classes.contains(&RequestClass::Execution))
            .filter(|e| e.health.eligible_for(RequestClass::Execution))
            .collect();

        let correlation_id = CorrelationId::new();
        let deadline = Instant::now() + self.config.call_deadline;

        let futures = eligible.into_iter().map(|entry| {
            let raw_tx_base64 = raw_tx_base64.to_string();
            async move {
                let permit = entry.budget.try_acquire(RequestClass::Execution);
                let id = entry.provider.id().clone();
                let Ok(_permit) = permit else {
                    return (
                        id,
                        Err(RpcError::Transient {
                            code: "SEN-RPC-BUDGET",
                            message: "broadcast budget exhausted".to_string(),
                            request_id: None,
                            retry_after: None,
                        }),
                    );
                };
                let ctx = CallContext::new(
                    correlation_id,
                    RequestClass::Execution,
                    Commitment::Processed,
                    deadline,
                    None,
                );
                let started = Instant::now();
                let result = entry
                    .provider
                    .call(RpcMethodCall::SendTransaction { raw_tx_base64 }, &ctx)
                    .await;
                match &result {
                    Ok(_) => entry
                        .health
                        .record_success(started.elapsed().as_millis() as u64),
                    Err(e) => entry.health.record_failure(e.code(), false, false),
                }
                (id, result)
            }
        });

        let per_provider = futures_util::future::join_all(futures).await;
        BroadcastOutcome { per_provider }
    }

    /// FI-10/FI-11: queries the two named providers directly for
    /// `getBlock(slot)` and compares content — surfacing both responses to
    /// the caller and tripping both providers' breakers on any mismatch
    /// (CB-4), rather than silently choosing one as canonical.
    pub async fn cross_check_block(
        &self,
        slot: Slot,
        provider_a: &ProviderId,
        provider_b: &ProviderId,
    ) -> Result<(RpcOutcome, RpcOutcome), RpcError> {
        let entry_a = self
            .entries
            .iter()
            .find(|e| e.provider.id() == provider_a)
            .ok_or_else(|| fatal_unknown_provider(provider_a))?;
        let entry_b = self
            .entries
            .iter()
            .find(|e| e.provider.id() == provider_b)
            .ok_or_else(|| fatal_unknown_provider(provider_b))?;

        let deadline = Instant::now() + self.config.call_deadline;
        let correlation_id = CorrelationId::new();
        let ctx_a = CallContext::new(
            correlation_id,
            RequestClass::RealtimeCompleteness,
            Commitment::Confirmed,
            deadline,
            None,
        );
        let ctx_b = ctx_a.next_attempt();

        let method = RpcMethodCall::GetBlock {
            slot,
            max_supported_transaction_version: 0,
        };
        let (res_a, res_b) = tokio::join!(
            entry_a.provider.call(method.clone(), &ctx_a),
            entry_b.provider.call(method.clone(), &ctx_b)
        );
        let (outcome_a, outcome_b) = (res_a?, res_b?);

        if let (
            RpcMethodResponse::Block {
                blockhash: hash_a,
                raw_json: json_a,
                ..
            },
            RpcMethodResponse::Block {
                blockhash: hash_b,
                raw_json: json_b,
                ..
            },
        ) = (&outcome_a.response, &outcome_b.response)
        {
            let diverges = if hash_a != hash_b {
                true // FI-10: different blocks for the same slot
            } else {
                json_a != json_b // FI-11: same blockhash, different content
            };
            if diverges {
                entry_a.health.record_content_divergence();
                entry_b.health.record_content_divergence();
                return Err(RpcError::ContentDivergence {
                    code: "SEN-RPC-DIVERGE",
                    detail: Box::new(crate::provider::ContentDivergenceDetail {
                        message: format!("providers disagree on the content of slot {slot}"),
                        slot: Some(slot),
                        blockhash: if hash_a == hash_b {
                            Some(hash_a.clone())
                        } else {
                            None
                        },
                        provider_a: provider_a.clone(),
                        provider_b: provider_b.clone(),
                    }),
                });
            }
        }
        Ok((outcome_a, outcome_b))
    }

    pub fn health(&self) -> Vec<ProviderHealth> {
        self.entries.iter().map(|e| e.health.snapshot()).collect()
    }

    /// Per-(provider, class) budget accounting — `(class, granted,
    /// rejected)` — for metrics dimensions §21 requires ("budget
    /// rejection") that a bare `ProviderHealth` snapshot does not carry.
    pub fn budget_stats(&self, id: &ProviderId) -> Vec<(RequestClass, i64, i64)> {
        let Some(entry) = self.entries.iter().find(|e| e.provider.id() == id) else {
            return Vec::new();
        };
        RequestClass::ALL
            .into_iter()
            .map(|c| (c, entry.budget.granted(c), entry.budget.rejected(c)))
            .collect()
    }

    pub fn provider_ids(&self) -> Vec<ProviderId> {
        self.entries
            .iter()
            .map(|e| e.provider.id().clone())
            .collect()
    }

    pub fn provider_health_tracker(&self, id: &ProviderId) -> Option<&ProviderHealthTracker> {
        self.entries
            .iter()
            .find(|e| e.provider.id() == id)
            .map(|e| &e.health)
    }

    /// Persists every provider's current health snapshot
    /// (`phase-03-rpc.md` §20).
    pub async fn persist_health(
        &self,
        pool: &sqlx::PgPool,
    ) -> Result<(), sentinel_db::queries::QueryError> {
        for entry in &self.entries {
            entry.health.persist(pool).await?;
        }
        Ok(())
    }
}

fn fatal_unknown_provider(id: &ProviderId) -> RpcError {
    RpcError::Fatal {
        code: "SEN-RPC-UNKNOWN-PROVIDER",
        message: format!("no such provider in this pool: {id}"),
        request_id: None,
    }
}
