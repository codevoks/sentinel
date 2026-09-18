//! Typed configuration loading for Sentinel.
//!
//! Two properties are non-negotiable (`AGENTS.md` §7, `docs/phases/phase-01-foundation.md` §6):
//!
//! 1. **No default configuration value points at a non-loopback host.** This
//!    crate has no defaults at all — every field is a required environment
//!    variable, and a missing one fails startup with a clear error rather
//!    than falling back to anything.
//! 2. **Secrets are a wrapper type from the first line of this crate.**
//!    [`Secret`]'s `Debug`/`Display`/`Serialize` implementations print only a
//!    placeholder. Retrofitting redaction is how secrets leak.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize, Serializer};

/// A secret value whose `Debug`, `Display`, and `Serialize` implementations
/// never reveal the wrapped value.
///
/// Access the real value only via [`Secret::expose`], and only at the point
/// it is actually needed (e.g. building a database connection string) —
/// never store the exposed value somewhere that gets logged or serialized.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

const REDACTED: &str = "[REDACTED]";

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Secret(value.into())
    }

    /// Returns the real value. Callers must not log, serialize, or otherwise
    /// persist the returned string.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Secret").field(&REDACTED).finish()
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{REDACTED}")
    }
}

impl Serialize for Secret {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(REDACTED)
    }
}

/// The deployment profile. `Local` is subject to the loopback-only rule;
/// `Production` is not (a production database is never loopback).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    Local,
    Production,
}

impl Profile {
    fn parse(raw: &str) -> Result<Self, ConfigError> {
        match raw {
            "local" => Ok(Profile::Local),
            "production" => Ok(Profile::Production),
            other => Err(ConfigError::InvalidValue {
                field: "SENTINEL_PROFILE".into(),
                reason: format!("expected \"local\" or \"production\", got {other:?}"),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Secret,
    pub database: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TelemetryConfig {
    pub otlp_endpoint_host: String,
    pub otlp_endpoint_port: u16,
    pub service_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppConfig {
    pub profile: Profile,
    pub database: DatabaseConfig,
    pub telemetry: TelemetryConfig,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required configuration: {0}")]
    MissingEnv(String),

    #[error("invalid value for {field}: {reason}")]
    InvalidValue { field: String, reason: String },

    #[error(
        "{field} resolves to non-loopback host {value:?}, which is not permitted in the local profile"
    )]
    NonLoopbackEndpoint { field: String, value: String },
}

fn required<'a>(vars: &'a HashMap<String, String>, key: &str) -> Result<&'a str, ConfigError> {
    vars.get(key)
        .map(String::as_str)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| ConfigError::MissingEnv(key.to_string()))
}

fn required_port(vars: &HashMap<String, String>, key: &str) -> Result<u16, ConfigError> {
    required(vars, key)?
        .parse::<u16>()
        .map_err(|e| ConfigError::InvalidValue {
            field: key.to_string(),
            reason: e.to_string(),
        })
}

/// Returns true if `host` can only ever resolve within the local machine.
///
/// Intentionally conservative: anything not obviously loopback is rejected.
/// A DNS name that merely *might* resolve to loopback on some machines is
/// not accepted — the local profile must be able to run correctly with the
/// network interface down except loopback (ADR-0013), so ambiguity is
/// treated as non-loopback.
fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return ip.is_loopback();
    }
    false
}

impl AppConfig {
    /// Loads configuration from a pre-collected map of environment
    /// variables. Prefer [`AppConfig::from_env`] outside of tests; this
    /// entry point exists so tests never mutate real process environment
    /// (which would race across parallel tests).
    pub fn from_map(vars: &HashMap<String, String>) -> Result<AppConfig, ConfigError> {
        let profile = Profile::parse(required(vars, "SENTINEL_PROFILE")?)?;

        let database = DatabaseConfig {
            host: required(vars, "SENTINEL_DB_HOST")?.to_string(),
            port: required_port(vars, "SENTINEL_DB_PORT")?,
            user: required(vars, "SENTINEL_DB_USER")?.to_string(),
            password: Secret::new(required(vars, "SENTINEL_DB_PASSWORD")?),
            database: required(vars, "SENTINEL_DB_NAME")?.to_string(),
        };

        let telemetry = TelemetryConfig {
            otlp_endpoint_host: required(vars, "SENTINEL_OTLP_HOST")?.to_string(),
            otlp_endpoint_port: required_port(vars, "SENTINEL_OTLP_PORT")?,
            service_name: required(vars, "SENTINEL_SERVICE_NAME")?.to_string(),
        };

        let config = AppConfig {
            profile,
            database,
            telemetry,
        };
        config.validate()?;
        Ok(config)
    }

    /// Loads configuration from the real process environment.
    pub fn from_env() -> Result<AppConfig, ConfigError> {
        let vars: HashMap<String, String> = std::env::vars().collect();
        Self::from_map(&vars)
    }

    /// Enforces the local-profile loopback rule (`AGENTS.md` §7,
    /// `docs/phases/phase-01-foundation.md` §6/§8). Startup must fail rather
    /// than silently accept a non-loopback endpoint while claiming to be
    /// local.
    fn validate(&self) -> Result<(), ConfigError> {
        if self.profile != Profile::Local {
            return Ok(());
        }
        if !is_loopback_host(&self.database.host) {
            return Err(ConfigError::NonLoopbackEndpoint {
                field: "SENTINEL_DB_HOST".into(),
                value: self.database.host.clone(),
            });
        }
        if !is_loopback_host(&self.telemetry.otlp_endpoint_host) {
            return Err(ConfigError::NonLoopbackEndpoint {
                field: "SENTINEL_OTLP_HOST".into(),
                value: self.telemetry.otlp_endpoint_host.clone(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_local_vars() -> HashMap<String, String> {
        HashMap::from([
            ("SENTINEL_PROFILE".into(), "local".into()),
            ("SENTINEL_DB_HOST".into(), "127.0.0.1".into()),
            ("SENTINEL_DB_PORT".into(), "5432".into()),
            ("SENTINEL_DB_USER".into(), "sentinel_app".into()),
            (
                "SENTINEL_DB_PASSWORD".into(),
                "SENTINEL_TEST_SENTINEL_VALUE_do-not-leak-me".into(),
            ),
            ("SENTINEL_DB_NAME".into(), "sentinel".into()),
            ("SENTINEL_OTLP_HOST".into(), "127.0.0.1".into()),
            ("SENTINEL_OTLP_PORT".into(), "4317".into()),
            ("SENTINEL_SERVICE_NAME".into(), "sentinel-indexer".into()),
        ])
    }

    #[test]
    fn valid_configuration_loads() {
        let cfg = AppConfig::from_map(&valid_local_vars()).expect("valid config must load");
        assert_eq!(cfg.profile, Profile::Local);
        assert_eq!(cfg.database.host, "127.0.0.1");
        assert_eq!(cfg.database.port, 5432);
    }

    #[test]
    fn missing_required_configuration_fails_clearly() {
        let mut vars = valid_local_vars();
        vars.remove("SENTINEL_DB_PASSWORD");
        let err = AppConfig::from_map(&vars).unwrap_err();
        match err {
            ConfigError::MissingEnv(field) => assert_eq!(field, "SENTINEL_DB_PASSWORD"),
            other => panic!("expected MissingEnv, got {other:?}"),
        }
    }

    #[test]
    fn non_loopback_host_is_rejected_in_local_profile() {
        let mut vars = valid_local_vars();
        vars.insert("SENTINEL_DB_HOST".into(), "db.example.com".into());
        let err = AppConfig::from_map(&vars).unwrap_err();
        match err {
            ConfigError::NonLoopbackEndpoint { field, value } => {
                assert_eq!(field, "SENTINEL_DB_HOST");
                assert_eq!(value, "db.example.com");
            }
            other => panic!("expected NonLoopbackEndpoint, got {other:?}"),
        }
    }

    #[test]
    fn production_profile_permits_non_loopback_host() {
        let mut vars = valid_local_vars();
        vars.insert("SENTINEL_PROFILE".into(), "production".into());
        vars.insert("SENTINEL_DB_HOST".into(), "db.internal.example.com".into());
        let cfg = AppConfig::from_map(&vars).expect("production profile must allow a real host");
        assert_eq!(cfg.database.host, "db.internal.example.com");
    }

    /// The redaction test required by `docs/phases/phase-01-foundation.md`
    /// §7: serialize a full config and assert the recognizable secret value
    /// does not appear anywhere in the output, while proving the
    /// serialization actually ran (other fields are present).
    #[test]
    fn full_config_serialization_never_leaks_the_secret() {
        const SENTINEL_SECRET_MARKER: &str = "SENTINEL_TEST_SENTINEL_VALUE_do-not-leak-me";
        let vars = valid_local_vars();
        assert_eq!(
            vars.get("SENTINEL_DB_PASSWORD").map(String::as_str),
            Some(SENTINEL_SECRET_MARKER),
            "test fixture sanity check"
        );

        let cfg = AppConfig::from_map(&vars).expect("valid config must load");

        let json = serde_json::to_string_pretty(&cfg).expect("config must serialize");
        assert!(
            !json.contains(SENTINEL_SECRET_MARKER),
            "serialized config leaked the secret: {json}"
        );
        assert!(
            json.contains(REDACTED),
            "serialized config must show the redaction placeholder"
        );

        let debug_output = format!("{cfg:?}");
        assert!(!debug_output.contains(SENTINEL_SECRET_MARKER));

        // Prove the serialization is not simply empty/failed: real fields
        // must still be present.
        assert!(json.contains("127.0.0.1"));
        assert!(json.contains("sentinel_app"));
    }

    #[test]
    fn secret_debug_and_display_are_redacted() {
        let secret = Secret::new("hunter2-should-never-appear");
        assert_eq!(format!("{secret:?}"), "Secret(\"[REDACTED]\")");
        assert_eq!(format!("{secret}"), "[REDACTED]");
        assert_eq!(secret.expose(), "hunter2-should-never-appear");
    }
}
