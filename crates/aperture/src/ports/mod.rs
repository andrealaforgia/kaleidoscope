//! Output ports — the seams Aperture's application core writes
//! through.
//!
//! The full contract lives in
//! `docs/feature/aperture/design/component-design.md > ports::OtlpSink`.
//!
//! ## Trait shape
//!
//! `OtlpSink` and `Probe` are hand-rolled `Pin<Box<dyn Future>>`
//! returning traits. The `async-trait` macro is on the workspace dep
//! list, and ADR-0007 permits a swap to `#[async_trait]` as a
//! non-breaking change, but the hand-rolled shape is retained because
//! the on-the-wire contract is identical and it keeps stack traces
//! free of macro indirection.

use std::future::Future;
use std::pin::Pin;

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;

/// Re-export the authenticated-tenant newtype (aegis-ingest-auth-v0,
/// ADR-0068) so downstream `OtlpSink` implementors (`sieve`,
/// `aperture-storage-sink`, `spark`) can name the tenant on a
/// [`TenantScoped`] without taking their own direct `aegis` dependency.
pub use aegis::TenantId;

/// A signal payload paired with the authenticated tenant it was ingested
/// under (aegis-ingest-auth-v0, ADR-0068 DD3).
///
/// Every `SinkRecord` carries one of these, so "an accepted record is
/// tagged with the tenant that authenticated it" is a **type-level
/// guarantee**: there is no way to construct a `SinkRecord` without a
/// `TenantId`. The tenant comes from the validated `aegis::TenantContext`
/// the handler obtained before `ingest_*` ran; it rides INSIDE the record
/// so `OtlpSink::accept(record)`'s signature is unchanged (no sink
/// implementor breaks).
#[derive(Debug)]
pub struct TenantScoped<T> {
    pub tenant: TenantId,
    pub inner: T,
}

impl<T> TenantScoped<T> {
    /// Pair a payload with the authenticated tenant it was ingested under.
    pub fn new(tenant: TenantId, inner: T) -> Self {
        Self { tenant, inner }
    }

    /// The authenticated tenant this payload was ingested under.
    pub fn tenant(&self) -> &TenantId {
        &self.tenant
    }

    /// Consume the wrapper, returning the inner payload (dropping the
    /// tenant tag). Sinks that have already recorded / routed by tenant
    /// use this to reach the raw OTLP request.
    pub fn into_inner(self) -> T {
        self.inner
    }
}

/// `Deref` to the inner payload so a `TenantScoped<ExportLogsServiceRequest>`
/// reads its `resource_logs` (and every other field/method) transparently.
/// This keeps the brownfield sink consumers (`aperture-storage-sink`,
/// `sieve`, `spark`) a one-token change: a destructured `req` that was an
/// `ExportLogsServiceRequest` is now a `TenantScoped<…>` whose fields stay
/// reachable through auto-deref. The tenant tag is read explicitly via
/// [`TenantScoped::tenant`].
impl<T> std::ops::Deref for TenantScoped<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

/// The three OTLP-stable signals at v0. Each variant pairs the upstream
/// `opentelemetry_proto` request with the authenticated tenant via
/// [`TenantScoped`] (ADR-0068 DD3): the tenant tag is structural, not a
/// runtime convention.
///
/// `#[non_exhaustive]` so future-additive evolution is non-breaking.
#[derive(Debug)]
#[non_exhaustive]
pub enum SinkRecord {
    Logs(TenantScoped<ExportLogsServiceRequest>),
    Traces(TenantScoped<ExportTraceServiceRequest>),
    Metrics(TenantScoped<ExportMetricsServiceRequest>),
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

        let tenant = || TenantId("acme-prod".to_string());
        let logs = SinkRecord::Logs(TenantScoped::new(
            tenant(),
            ExportLogsServiceRequest {
                resource_logs: vec![],
            },
        ));
        let traces = SinkRecord::Traces(TenantScoped::new(
            tenant(),
            ExportTraceServiceRequest {
                resource_spans: vec![],
            },
        ));
        let metrics = SinkRecord::Metrics(TenantScoped::new(
            tenant(),
            ExportMetricsServiceRequest {
                resource_metrics: vec![],
            },
        ));
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
