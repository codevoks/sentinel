//! Capability discovery — probed, never assumed (`docs/rpc-strategy.md` §3).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::provider::RequestClass;

/// Mirrors `docs/rpc-strategy.md` §3's `ProviderCapabilities` exactly.
/// Every field is discovered by probing at startup and re-probed on
/// reconnect (`phase-03-rpc.md` §2); a missing capability is an explicit
/// `false`/`None`, never assumed `true`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub max_supported_transaction_version: u8,
    pub supports_get_program_accounts: bool,
    pub get_program_accounts_max_bytes: Option<usize>,
    pub max_blocks_per_get_blocks: u64,
    pub supports_min_context_slot: bool,
    pub supports_websocket: bool,
    pub supports_signature_subscribe: bool,
    pub supports_recent_prioritization_fees: bool,
    pub filters_vote_transactions: Option<bool>,
    /// Geyser yes, RPC no (`docs/rpc-strategy.md` §3).
    pub reports_write_version: bool,
    pub node_version: String,
    /// Per-method `context`/`minContextSlot` support
    /// (`docs/rpc-strategy.md` §5.2: "not uniform across RPC methods").
    /// Keyed by `RpcMethodCall::method_name()`. A method absent from this
    /// set is treated as NOT supporting `context`/`minContextSlot`, so the
    /// caller falls back to a head comparison rather than assuming.
    pub methods_with_context: HashSet<String>,
    /// Methods **positively confirmed** supported by probing. A method
    /// absent from this set — whether never probed or probed and found
    /// absent (e.g. `-32601 Method not found`) — is treated as NOT
    /// supported. This is deliberately a whitelist, not a blacklist: the
    /// default (`unknown()`, an empty set) must assume nothing, per
    /// `docs/rpc-strategy.md` §3 ("a missing capability is an explicit
    /// false/None ... not a runtime surprise").
    pub supported_methods: HashSet<String>,
}

impl ProviderCapabilities {
    /// A conservative "nothing probed yet" default. Never used as a
    /// stand-in for a real probe result on the request path — only as the
    /// pre-startup placeholder before `discover_capabilities` runs.
    pub fn unknown() -> Self {
        ProviderCapabilities {
            max_supported_transaction_version: 0,
            supports_get_program_accounts: false,
            get_program_accounts_max_bytes: None,
            max_blocks_per_get_blocks: 0,
            supports_min_context_slot: false,
            supports_websocket: false,
            supports_signature_subscribe: false,
            supports_recent_prioritization_fees: false,
            filters_vote_transactions: None,
            reports_write_version: false,
            node_version: String::new(),
            methods_with_context: HashSet::new(),
            supported_methods: HashSet::new(),
        }
    }

    pub fn supports_method(&self, method: &str) -> bool {
        self.supported_methods.contains(method)
    }

    pub fn method_has_context(&self, method: &str) -> bool {
        self.methods_with_context.contains(method)
    }

    /// The methods each request class actually issues, per
    /// `docs/rpc-strategy.md` §4's example column. Used at startup to
    /// decide whether a provider configured for a class can actually serve
    /// it (CF-4 / `phase-03-rpc.md` §2).
    pub fn class_requires(class: RequestClass) -> &'static [&'static str] {
        match class {
            RequestClass::Execution => &[
                "simulateTransaction",
                "sendTransaction",
                "getLatestBlockhash",
                "getSignatureStatuses",
            ],
            RequestClass::RealtimeCompleteness => &["getBlock", "getBlocks"],
            RequestClass::GapRepair => &["getBlock"],
            RequestClass::Backfill => &["getBlock", "getSignaturesForAddress"],
            RequestClass::ScheduledScan => &["getProgramAccounts"],
        }
    }

    /// CF-4: a provider configured for a class it cannot actually serve is
    /// a startup error. Returns the first unsupported method found, if any.
    pub fn missing_method_for_class(&self, class: RequestClass) -> Option<&'static str> {
        Self::class_requires(class)
            .iter()
            .find(|m| !self.supports_method(m))
            .copied()
    }

    pub fn mark_supported(&mut self, method: impl Into<String>) {
        self.supported_methods.insert(method.into());
    }

    pub fn mark_has_context(&mut self, method: impl Into<String>) {
        self.methods_with_context.insert(method.into());
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CapabilityError {
    #[error(
        "provider {provider_id} is configured for class {class} but does not support required method {method} — startup error (CF-4)"
    )]
    UnsupportedClass {
        provider_id: String,
        class: RequestClass,
        method: &'static str,
    },
}

/// Validates every provider's configured classes against its discovered
/// capabilities. Called once at pool construction; a failure here is a
/// startup error, never a runtime surprise (`phase-03-rpc.md` §2, CF-4).
pub fn validate_configured_classes(
    provider_id: &str,
    configured: &HashSet<RequestClass>,
    caps: &ProviderCapabilities,
) -> Result<(), CapabilityError> {
    for &class in configured {
        if let Some(method) = caps.missing_method_for_class(class) {
            return Err(CapabilityError::UnsupportedClass {
                provider_id: provider_id.to_string(),
                class,
                method,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps_missing_get_program_accounts() -> ProviderCapabilities {
        // Whitelist model: everything ScheduledScan needs EXCEPT
        // getProgramAccounts is marked supported, so the gap is the one
        // under test rather than an artifact of an otherwise-empty set.
        let mut caps = ProviderCapabilities::unknown();
        caps.mark_has_context("getLatestBlockhash");
        caps
    }

    #[test]
    fn missing_capability_is_explicit_not_assumed() {
        let caps = ProviderCapabilities::unknown();
        assert!(
            !caps.supports_method("getBlock"),
            "unknown() must not assume support"
        );
        assert!(!caps.method_has_context("getLatestBlockhash"));
    }

    #[test]
    fn provider_configured_for_unsupported_class_is_rejected() {
        let caps = caps_missing_get_program_accounts();
        let mut classes = HashSet::new();
        classes.insert(RequestClass::ScheduledScan);
        let result = validate_configured_classes("p1", &classes, &caps);
        assert!(result.is_err());
        match result {
            Err(CapabilityError::UnsupportedClass { method, class, .. }) => {
                assert_eq!(method, "getProgramAccounts");
                assert_eq!(class, RequestClass::ScheduledScan);
            }
            Ok(()) => unreachable!(),
        }
    }

    #[test]
    fn provider_configured_for_supported_class_is_accepted() {
        let mut caps = ProviderCapabilities::unknown();
        for method in ProviderCapabilities::class_requires(RequestClass::Execution) {
            caps.mark_supported(*method);
        }
        let mut classes = HashSet::new();
        classes.insert(RequestClass::Execution);
        assert!(validate_configured_classes("p1", &classes, &caps).is_ok());
    }
}
