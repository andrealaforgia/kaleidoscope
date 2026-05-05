//! Output ports — the seams Aperture's application core writes
//! through.
//!
//! The full contract lives in
//! `docs/feature/aperture/design/component-design.md > ports::OtlpSink`.
//!
//! ## Slice 01 status
//!
//! `OtlpSink` and `Probe` are hand-rolled `Pin<Box<dyn Future>>`
//! returning traits — the `async-trait` macro is on the workspace dep
//! list (Slice 01 added it) but Slice 01 keeps the hand-rolled shape
//! here because the on-the-wire contract is identical and the
//! hand-rolled form has zero macro indirection in stack traces. ADR-0007
//! permits the swap to `#[async_trait]` as a non-breaking change in a
//! later slice.

use std::future::Future;
use std::pin::Pin;

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;

/// The three OTLP-stable signals at v0. Carries the upstream
/// `opentelemetry_proto` type unwrapped per DISCUSS D2 (no harness-local
/// wrapper, no Aperture-local wrapper).
///
/// `#[non_exhaustive]` so future-additive evolution is non-breaking.
#[derive(Debug)]
#[non_exhaustive]
pub enum SinkRecord {
    Logs(ExportLogsServiceRequest),
    Traces(ExportTraceServiceRequest),
    Metrics(ExportMetricsServiceRequest),
}

/// Reasons a sink can refuse a record. DELIVER replaces this with the
/// full `thiserror`-derived enum from the design contract.
#[derive(Debug)]
#[non_exhaustive]
pub enum SinkError {
    /// Downstream returned a non-2xx status, refused the connection,
    /// or DNS resolution failed.
    DownstreamUnavailable { reason: String },
    /// Downstream did not respond within the configured timeout.
    DownstreamTimeout { elapsed_ms: u64 },
    /// Catch-all for sink-internal failures.
    Internal { message: String },
}

impl std::fmt::Display for SinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SinkError::DownstreamUnavailable { reason } => {
                write!(f, "downstream unavailable: {reason}")
            }
            SinkError::DownstreamTimeout { elapsed_ms } => {
                write!(f, "downstream timeout after {elapsed_ms} ms")
            }
            SinkError::Internal { message } => write!(f, "sink internal error: {message}"),
        }
    }
}

impl std::error::Error for SinkError {}

/// Aperture's hand-off boundary with the next pipeline stage.
///
/// At DISTILL the trait is shaped without the `async-trait` macro to
/// keep the dependency surface of the stub small; DELIVER swaps the
/// hand-rolled `Future` shape for `#[async_trait]` per the design
/// contract. The signature DELIVER sees is the same: `async fn accept`
/// returning `Result<(), SinkError>`.
pub trait OtlpSink: Send + Sync + 'static {
    /// Hand the typed record to the next stage. Returns when the next
    /// stage has acknowledged (`Ok`) or refused (`Err`). Aperture
    /// awaits this before responding to the upstream SDK.
    fn accept<'a>(
        &'a self,
        record: SinkRecord,
    ) -> Pin<Box<dyn Future<Output = std::result::Result<(), SinkError>> + Send + 'a>>;
}

/// Probe error. DELIVER replaces with the full `thiserror`-derived
/// enum from the design contract.
#[derive(Debug)]
#[non_exhaustive]
pub enum ProbeError {
    Unreachable { endpoint: String, reason: String },
    Refused { endpoint: String, status: u16 },
    Timeout { endpoint: String, elapsed_ms: u64 },
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeError::Unreachable { endpoint, reason } => {
                write!(f, "downstream unreachable at {endpoint}: {reason}")
            }
            ProbeError::Refused { endpoint, status } => {
                write!(
                    f,
                    "downstream rejected probe at {endpoint}: status {status}"
                )
            }
            ProbeError::Timeout {
                endpoint,
                elapsed_ms,
            } => {
                write!(
                    f,
                    "probe timed out after {elapsed_ms} ms against {endpoint}"
                )
            }
        }
    }
}

impl std::error::Error for ProbeError {}

/// Earned-Trust probe contract. Every `OtlpSink` MUST also implement
/// `Probe`; DELIVER's composition root invokes `wire_then_probe_then_use`
/// which refuses to start if any probe returns `Err`.
pub trait Probe: Send + Sync + 'static {
    fn probe<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = std::result::Result<(), ProbeError>> + Send + 'a>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the v0 OTLP-stable signal set: `SinkRecord` carries
    /// exactly three variants — `Logs`, `Traces`, `Metrics` — one per
    /// stable OTLP signal. Slice 04 closes the three-signal contract;
    /// any addition or removal of a variant is a public-API surface
    /// change that must be deliberate. The `#[non_exhaustive]`
    /// attribute keeps downstream-crate matches future-proof, but
    /// this in-crate match is exhaustive precisely because we, as the
    /// defining crate, control the variant set.
    #[test]
    fn sink_record_has_exactly_three_variants_one_per_otlp_stable_signal() {
        fn classify(record: &SinkRecord) -> &'static str {
            match record {
                SinkRecord::Logs(_) => "logs",
                SinkRecord::Traces(_) => "traces",
                SinkRecord::Metrics(_) => "metrics",
            }
        }

        let logs = SinkRecord::Logs(ExportLogsServiceRequest {
            resource_logs: vec![],
        });
        let traces = SinkRecord::Traces(ExportTraceServiceRequest {
            resource_spans: vec![],
        });
        let metrics = SinkRecord::Metrics(ExportMetricsServiceRequest {
            resource_metrics: vec![],
        });
        assert_eq!(classify(&logs), "logs");
        assert_eq!(classify(&traces), "traces");
        assert_eq!(classify(&metrics), "metrics");
    }

    // -------------------------------------------------------------------------
    // SinkError::Display and ProbeError::Display — pin the operator-facing
    // strings against an `Ok(Default::default())` mutation that would render
    // every refusal / probe failure as the empty string. The harness reject
    // path (`OtlpViolation::Display`) round-trips verbatim through the
    // transport layer (Slice 03 / 04); these errors carry the symmetric
    // contract for sink and probe failures.
    // -------------------------------------------------------------------------

    #[test]
    fn sink_error_downstream_unavailable_display_names_reason() {
        let err = SinkError::DownstreamUnavailable {
            reason: "connection refused".to_string(),
        };
        let s = err.to_string();
        assert!(s.contains("downstream unavailable"));
        assert!(s.contains("connection refused"));
    }

    #[test]
    fn sink_error_downstream_timeout_display_names_elapsed_ms() {
        let err = SinkError::DownstreamTimeout { elapsed_ms: 1500 };
        let s = err.to_string();
        assert!(s.contains("downstream timeout"));
        assert!(s.contains("1500"));
    }

    #[test]
    fn sink_error_internal_display_names_message() {
        let err = SinkError::Internal {
            message: "queue overflow".to_string(),
        };
        let s = err.to_string();
        assert!(s.contains("sink internal error"));
        assert!(s.contains("queue overflow"));
    }

    #[test]
    fn probe_error_unreachable_display_names_endpoint_and_reason() {
        let err = ProbeError::Unreachable {
            endpoint: "https://downstream.example/v1".to_string(),
            reason: "dns failure".to_string(),
        };
        let s = err.to_string();
        assert!(s.contains("downstream unreachable"));
        assert!(s.contains("https://downstream.example/v1"));
        assert!(s.contains("dns failure"));
    }

    #[test]
    fn probe_error_refused_display_names_endpoint_and_status() {
        let err = ProbeError::Refused {
            endpoint: "https://downstream.example/v1".to_string(),
            status: 401,
        };
        let s = err.to_string();
        assert!(s.contains("downstream rejected probe"));
        assert!(s.contains("https://downstream.example/v1"));
        assert!(s.contains("401"));
    }

    #[test]
    fn probe_error_timeout_display_names_endpoint_and_elapsed_ms() {
        let err = ProbeError::Timeout {
            endpoint: "https://downstream.example/v1".to_string(),
            elapsed_ms: 2500,
        };
        let s = err.to_string();
        assert!(s.contains("probe timed out"));
        assert!(s.contains("2500"));
        assert!(s.contains("https://downstream.example/v1"));
    }
}
