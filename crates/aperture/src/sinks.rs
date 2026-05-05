//! Driven adapters — concrete `OtlpSink` implementations.
//!
//! See `docs/feature/aperture/design/component-design.md > Sinks` for
//! the design contract.
//!
//! Slice 01 lights up `StubSink`. Slice 06 lands `ForwardingSink`.

use std::pin::Pin;
use std::time::Duration;

use crate::app::summarise_record;
use crate::observability::event;
use crate::ports::{OtlpSink, Probe, ProbeError, SinkError, SinkRecord};

/// `StubSink` — writes one structured stderr line per accepted record
/// (`event=sink_accepted sink=stub`) and returns `Ok(())`. Useful for
/// smoke-testing fixtures and CI; the v0 default sink kind.
#[derive(Debug, Default)]
pub struct StubSink;

impl OtlpSink for StubSink {
    fn accept<'a>(
        &'a self,
        record: SinkRecord,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), SinkError>> + Send + 'a>> {
        Box::pin(async move {
            emit_sink_accepted("stub", &record);
            Ok(())
        })
    }
}

/// Emit the `event=sink_accepted` line with the per-signal count field
/// name. The closed v0 vocabulary uses signal-specific count fields:
/// `record_count` for logs (Slice 01), `span_count` for traces (Slice
/// 03), `data_point_count` for metrics (Slice 04). `tracing::info!`
/// fixes field names at compile time, so the per-signal call sites are
/// the natural shape.
pub(crate) fn emit_sink_accepted(sink: &'static str, record: &SinkRecord) {
    let summary = summarise_record(record);
    let service_name = summary.resource_service_name.unwrap_or("");
    let count = summary.count as u64;
    match record {
        SinkRecord::Logs(_) => tracing::info!(
            event = event::SINK_ACCEPTED,
            sink = sink,
            signal = summary.signal,
            record_count = count,
            "resource.service.name" = service_name,
        ),
        SinkRecord::Traces(_) => tracing::info!(
            event = event::SINK_ACCEPTED,
            sink = sink,
            signal = summary.signal,
            span_count = count,
            "resource.service.name" = service_name,
        ),
        SinkRecord::Metrics(_) => tracing::info!(
            event = event::SINK_ACCEPTED,
            sink = sink,
            signal = summary.signal,
            data_point_count = count,
            "resource.service.name" = service_name,
        ),
    }
}

impl Probe for StubSink {
    fn probe<'a>(
        &'a self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), ProbeError>> + Send + 'a>> {
        // No external dependency. Probe is trivially Ok.
        Box::pin(async { Ok(()) })
    }
}

// =========================================================================
// ForwardingSink — Slice 06
// =========================================================================
//
// Real `reqwest` client posting accepted records to a configured
// downstream OTLP-compatible HTTP endpoint. Implements both `OtlpSink`
// (the runtime acceptance contract) and `Probe` (the Earned-Trust
// startup contract). ADR-0007 locks the dual-trait shape so the
// structural-layer enforcement (xtask AST walk) and the behavioural-
// layer enforcement (`tests/probe_gold_runner.rs`) can both reach the
// `Probe` impl independently.
//
// Plaintext at v0; `tls.enabled=true` is reserved by Slice 07 and the
// config validator rejects it ahead of this sink. Authentication and
// retries land with Aegis (Phase 2). The SDK retries upstream;
// Aperture refuses fast.

/// `ForwardingSink` — POSTs each accepted `SinkRecord` to
/// `<endpoint>/v1/{logs,traces,metrics}` as `application/x-protobuf`.
///
/// The struct holds a single `reqwest::Client` configured with the
/// per-request timeout from `[forwarding_sink] timeout_ms`. Cloning is
/// cheap because the client is internally reference-counted.
#[derive(Debug)]
pub struct ForwardingSink {
    /// Downstream endpoint URL (no trailing slash). The signal-specific
    /// suffix (`/v1/logs`, etc.) is appended at request time. Read by
    /// `accept` (POST target) and `probe` (Earned-Trust check) — both
    /// landed across Slice 06's cycles 2-3.
    #[allow(dead_code)]
    endpoint: String,
    /// Per-request timeout, configured by `[forwarding_sink] timeout_ms`.
    /// Threaded into the `reqwest::Client` builder; subsequent cycles
    /// also use it for elapsed-vs-deadline reporting.
    #[allow(dead_code)]
    timeout: Duration,
    client: reqwest::Client,
}

impl ForwardingSink {
    /// Build a `ForwardingSink` against the given downstream endpoint.
    ///
    /// `endpoint` is the base URL of the OTel-compatible backend, e.g.
    /// `http://otel-collector:4318`. Trailing slashes are tolerated.
    /// `timeout` becomes the per-request budget on the underlying
    /// `reqwest::Client`; the probe path uses a separate 2 s budget per
    /// the design contract.
    pub fn new(endpoint: String, timeout: Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent(format!("aperture/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest::Client::build is infallible with these options");
        Self {
            endpoint,
            timeout,
            client,
        }
    }

    /// The full URL Aperture POSTs to for a given OTLP signal name. The
    /// trailing-slash trim keeps `http://host/` and `http://host`
    /// indistinguishable from the SDK perspective.
    #[allow(dead_code)] // wired in by Slice 06 cycle 2
    fn url_for(&self, signal: &'static str) -> String {
        format!("{}/v1/{signal}", self.endpoint.trim_end_matches('/'))
    }
}

impl OtlpSink for ForwardingSink {
    fn accept<'a>(
        &'a self,
        record: SinkRecord,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), SinkError>> + Send + 'a>> {
        // Slice 06 cycle 1: scaffold only. Subsequent cycles light up
        // the encode-and-POST path and the failure-mode mapping.
        let _ = record;
        let _ = &self.client;
        let _ = self.timeout;
        Box::pin(async move {
            Err(SinkError::Internal {
                message: "forwarding sink accept not yet implemented".to_string(),
            })
        })
    }
}

impl Probe for ForwardingSink {
    fn probe<'a>(
        &'a self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), ProbeError>> + Send + 'a>> {
        // Slice 06 cycle 1: scaffold only — accept any wiring without
        // touching the network. Cycle 2 implements the two-stage probe.
        Box::pin(async move { Ok(()) })
    }
}
