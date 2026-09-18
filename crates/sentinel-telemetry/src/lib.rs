//! OpenTelemetry setup for Sentinel: a metric registry (exported in
//! Prometheus text format for the Compose Prometheus service to scrape) and
//! structured JSON logging.
//!
//! Phase 1 scope only (`docs/phases/phase-01-foundation.md` §6): metrics can
//! register and export, and a log line carries the expected structured
//! fields. OTLP export to the collector, trace propagation, and dashboards
//! are Phase 12 work.

use std::io;
use std::sync::{Arc, Mutex};

use opentelemetry::metrics::{Meter, MeterProvider as _};
use opentelemetry_sdk::metrics::SdkMeterProvider;
use prometheus::{Encoder, Registry, TextEncoder};

/// A running metric registry: an OpenTelemetry `MeterProvider` backed by a
/// Prometheus registry, so `export_text` produces the exact text format
/// `infra/compose`'s Prometheus service scrapes.
pub struct Metrics {
    registry: Registry,
    provider: SdkMeterProvider,
}

#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    #[error("failed to build the Prometheus exporter: {0}")]
    ExporterInit(#[from] opentelemetry_sdk::error::OTelSdkError),

    #[error("failed to encode metrics: {0}")]
    Encode(#[from] prometheus::Error),

    #[error("metrics text was not valid UTF-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}

impl Metrics {
    pub fn init() -> Result<Self, TelemetryError> {
        let registry = Registry::new();
        let exporter = opentelemetry_prometheus::exporter()
            .with_registry(registry.clone())
            .build()?;
        let provider = SdkMeterProvider::builder().with_reader(exporter).build();
        Ok(Metrics { registry, provider })
    }

    /// Registers (or retrieves) a named meter, the unit through which
    /// counters/gauges/histograms are created.
    pub fn meter(&self, name: &'static str) -> Meter {
        self.provider.meter(name)
    }

    /// Exports every registered metric in Prometheus text exposition
    /// format.
    pub fn export_text(&self) -> Result<String, TelemetryError> {
        let metric_families = self.registry.gather();
        let mut buf = Vec::new();
        TextEncoder::new().encode(&metric_families, &mut buf)?;
        Ok(String::from_utf8(buf)?)
    }
}

/// Builds a JSON-formatted `tracing` subscriber writing to `make_writer`.
///
/// Split out from [`init_logging`] so tests can capture output into a
/// buffer instead of racing on the process-global subscriber.
pub fn json_subscriber<W>(make_writer: W) -> impl tracing::Subscriber + Send + Sync
where
    W: for<'writer> tracing_subscriber::fmt::MakeWriter<'writer> + Send + Sync + 'static,
{
    tracing_subscriber::fmt()
        .json()
        .with_current_span(false)
        .with_target(true)
        .with_writer(make_writer)
        .finish()
}

/// Installs the JSON subscriber as the process-global default. Call exactly
/// once, at process startup.
pub fn init_logging() {
    tracing::subscriber::set_global_default(json_subscriber(io::stdout))
        .expect("init_logging must be called at most once per process");
}

/// An in-memory `MakeWriter` used by tests to capture log output without
/// touching stdout or the global subscriber.
#[derive(Clone, Default)]
pub struct CapturingWriter(Arc<Mutex<Vec<u8>>>);

impl CapturingWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn contents(&self) -> String {
        String::from_utf8(self.0.lock().expect("capture buffer poisoned").clone())
            .expect("captured log output must be valid UTF-8")
    }
}

impl io::Write for CapturingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .expect("capture buffer poisoned")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturingWriter {
    type Writer = CapturingWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::KeyValue;

    #[test]
    fn metrics_can_register_and_export() {
        let metrics = Metrics::init().expect("metrics init must succeed with no network");
        let meter = metrics.meter("sentinel_test");
        let counter = meter.u64_counter("sentinel_phase1_test_total").build();
        counter.add(1, &[KeyValue::new("phase", "1")]);

        let exported = metrics.export_text().expect("export must succeed");
        assert!(
            exported.contains("sentinel_phase1_test_total"),
            "exported text did not contain the registered metric: {exported}"
        );
    }

    #[test]
    fn log_line_contains_required_structured_fields() {
        let writer = CapturingWriter::new();
        let subscriber = json_subscriber(writer.clone());

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(
                service = "sentinel-test",
                slot = 42u64,
                "phase 1 telemetry test"
            );
        });

        let output = writer.contents();
        let line = output
            .lines()
            .next()
            .expect("exactly one log line must be written");
        let json: serde_json::Value =
            serde_json::from_str(line).expect("log line must be valid JSON");

        assert_eq!(json["fields"]["message"], "phase 1 telemetry test");
        assert_eq!(json["fields"]["service"], "sentinel-test");
        assert_eq!(json["fields"]["slot"], 42);
        assert!(
            json.get("timestamp").is_some(),
            "log line must carry a timestamp: {json}"
        );
        assert_eq!(json["level"], "INFO");
        assert!(
            json.get("target").is_some(),
            "log line must carry a target: {json}"
        );
    }
}
