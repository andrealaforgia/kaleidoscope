//! Driven adapters — concrete `OtlpSink` implementations.
//!
//! See `docs/feature/aperture/design/component-design.md > Sinks` for
//! the design contract.
//!
//! Slice 01 lights up `StubSink`. Slice 06 lands `ForwardingSink`.

use std::pin::Pin;
use std::time::Duration;

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use prost::Message;

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
    /// `accept` (POST target) and `probe` (Earned-Trust check).
    endpoint: String,
    /// Per-request timeout, configured by `[forwarding_sink] timeout_ms`.
    /// Threaded into the `reqwest::Client` builder; the probe path uses
    /// its own per-stage budget independent of this value. Held as a
    /// field so a `Debug` impl reveals the configured value to
    /// operators reading panic dumps.
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
    fn url_for(&self, signal: &'static str) -> String {
        format!("{}/v1/{signal}", self.endpoint.trim_end_matches('/'))
    }
}

impl OtlpSink for ForwardingSink {
    fn accept<'a>(
        &'a self,
        record: SinkRecord,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), SinkError>> + Send + 'a>> {
        Box::pin(async move {
            let signal = signal_name_for(&record);
            let body = encode_for_forwarding(&record);
            let url = self.url_for(signal);
            let started = std::time::Instant::now();
            let result = self
                .client
                .post(&url)
                .header(reqwest::header::CONTENT_TYPE, "application/x-protobuf")
                .body(body)
                .send()
                .await;
            let elapsed_ms = started.elapsed().as_millis() as u64;
            match result {
                Ok(response) if response.status().is_success() => {
                    self.emit_sink_accepted_for_forwarding(&record, elapsed_ms);
                    Ok(())
                }
                Ok(response) => {
                    let status = response.status().as_u16();
                    let reason = format!("downstream returned {status}");
                    self.emit_sink_failed(&reason);
                    Err(SinkError::DownstreamUnavailable { reason })
                }
                Err(e) => Err(self.classify_accept_error(e, elapsed_ms)),
            }
        })
    }
}

impl ForwardingSink {
    /// Emit the forwarding-sink-flavoured `event=sink_accepted` line.
    /// Mirror of `emit_sink_accepted` above but with the
    /// forwarding-only fields (`downstream`, `downstream_latency_ms`)
    /// alongside the per-signal count field. `tracing::info!` fixes
    /// field names at compile time, so the per-signal call sites are
    /// the natural shape.
    fn emit_sink_accepted_for_forwarding(&self, record: &SinkRecord, downstream_latency_ms: u64) {
        let summary = summarise_record(record);
        let service_name = summary.resource_service_name.unwrap_or("");
        let count = summary.count as u64;
        match record {
            SinkRecord::Logs(_) => tracing::info!(
                event = event::SINK_ACCEPTED,
                sink = "forwarding",
                downstream = %self.endpoint,
                signal = summary.signal,
                record_count = count,
                downstream_latency_ms = downstream_latency_ms,
                "resource.service.name" = service_name,
            ),
            SinkRecord::Traces(_) => tracing::info!(
                event = event::SINK_ACCEPTED,
                sink = "forwarding",
                downstream = %self.endpoint,
                signal = summary.signal,
                span_count = count,
                downstream_latency_ms = downstream_latency_ms,
                "resource.service.name" = service_name,
            ),
            SinkRecord::Metrics(_) => tracing::info!(
                event = event::SINK_ACCEPTED,
                sink = "forwarding",
                downstream = %self.endpoint,
                signal = summary.signal,
                data_point_count = count,
                downstream_latency_ms = downstream_latency_ms,
                "resource.service.name" = service_name,
            ),
        }
    }

    /// Emit the `event=sink_failed` error line. The downstream's
    /// specific error becomes the stderr `reason` field; the upstream
    /// SDK sees only `gRPC UNAVAILABLE` / `HTTP 503`.
    fn emit_sink_failed(&self, reason: &str) {
        tracing::error!(
            event = event::SINK_FAILED,
            sink = "forwarding",
            downstream = %self.endpoint,
            reason = reason,
        );
    }

    /// Translate a `reqwest::Error` from the accept-path POST into a
    /// `SinkError`. Timeouts surface as `DownstreamTimeout`; everything
    /// else (connect refused, DNS, TLS) collapses to
    /// `DownstreamUnavailable` carrying the underlying error message.
    fn classify_accept_error(&self, err: reqwest::Error, elapsed_ms: u64) -> SinkError {
        if err.is_timeout() {
            self.emit_sink_failed("downstream timeout");
            SinkError::DownstreamTimeout { elapsed_ms }
        } else {
            let reason = err.to_string();
            self.emit_sink_failed(&reason);
            SinkError::DownstreamUnavailable { reason }
        }
    }
}

/// Static signal name for the wire-format URL path. The harness's
/// type-path identity guarantee (US-04 AC 2 in
/// `otlp-conformance-harness-v0`) makes this safe — the variant the
/// harness produced is the variant the typed `Export*ServiceRequest`
/// belongs to.
fn signal_name_for(record: &SinkRecord) -> &'static str {
    match record {
        SinkRecord::Logs(_) => "logs",
        SinkRecord::Traces(_) => "traces",
        SinkRecord::Metrics(_) => "metrics",
    }
}

/// Serialise a typed `SinkRecord` to OTLP/HTTP/protobuf bytes. The
/// `prost::Message::encode_to_vec` call mirrors what the upstream SDK
/// puts on the wire; the harness's type-path identity guarantee means
/// the round-trip is byte-for-byte equivalent to the SDK's original
/// encoding (modulo non-canonical proto encodings, which OTLP
/// receivers must accept).
fn encode_for_forwarding(record: &SinkRecord) -> Vec<u8> {
    match record {
        SinkRecord::Logs(req) => req.encode_to_vec(),
        SinkRecord::Traces(req) => req.encode_to_vec(),
        SinkRecord::Metrics(req) => req.encode_to_vec(),
    }
}

/// Per-stage probe deadline. Independent of the per-request `accept`
/// timeout: the probe gives the downstream a fixed budget so that a
/// misconfigured `[forwarding_sink] timeout_ms = 50` does not produce
/// a probe timeout that hides a slower-but-correct startup.
const PROBE_STAGE_TIMEOUT: Duration = Duration::from_secs(2);

impl Probe for ForwardingSink {
    fn probe<'a>(
        &'a self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), ProbeError>> + Send + 'a>> {
        // Two-stage probe (ADR-0007 / ADR-0010 layer 3):
        //
        //   Stage 1 — OPTIONS preflight against `<endpoint>/v1/logs`.
        //     2xx or 204                 → success.
        //     404 / 405                  → downstream may be OTLP-shaped
        //                                  without OPTIONS support; fall
        //                                  through to the degraded POST.
        //     other 4xx/5xx              → `Refused { status }`.
        //     timeout                    → `Timeout { elapsed_ms = 2000 }`.
        //     transport error            → `Unreachable { reason }`.
        //
        //   Stage 2 — degraded POST of a zero-records
        //     `ExportLogsServiceRequest`. The catalogued substrate lie is
        //     `200 OPTIONS / 503 POST`; this stage is the only line of
        //     defence against that lie. The behavioural-layer gold-test
        //     (`tests/probe_gold_runner.rs`) drives this scenario.
        //     2xx                        → success.
        //     other status               → `Refused { status }`.
        //     timeout                    → `Timeout { elapsed_ms = 2000 }`.
        //     transport error            → `Unreachable { reason }`.
        Box::pin(async move {
            match self.probe_options().await {
                ProbeStageOutcome::Succeeded => return Ok(()),
                ProbeStageOutcome::FallThrough => {}
                ProbeStageOutcome::Failed(err) => return Err(err),
            }
            self.probe_degraded_post().await
        })
    }
}

/// Outcome of a single probe stage. `FallThrough` says "this stage did
/// not refuse; continue to the next stage" — the only case is OPTIONS
/// returning 404 / 405.
enum ProbeStageOutcome {
    Succeeded,
    FallThrough,
    Failed(ProbeError),
}

impl ForwardingSink {
    /// Stage 1 of the two-stage probe: OPTIONS preflight. Returns
    /// `FallThrough` only when the downstream responded with 404/405,
    /// which is the documented "OTLP-compatible without OPTIONS support"
    /// shape; every other path either succeeds or fails the probe.
    async fn probe_options(&self) -> ProbeStageOutcome {
        let url = self.url_for("logs");
        let request = self
            .client
            .request(reqwest::Method::OPTIONS, &url)
            .timeout(PROBE_STAGE_TIMEOUT);
        match request.send().await {
            Ok(response) => self.classify_options_response(response.status()),
            Err(e) => ProbeStageOutcome::Failed(self.classify_transport_error(e)),
        }
    }

    /// Map an OPTIONS status to a probe outcome. Pure, total: every
    /// status code lands in exactly one branch.
    ///
    /// **204 is the only status that short-circuits the probe.** RFC
    /// 9110 specifies 204 as the canonical "preflight semantically
    /// successful, no body" response for OPTIONS; an OTel-compatible
    /// downstream that genuinely supports OTLP/HTTP returns 204 here.
    /// Any other 2xx is treated as "downstream may answer OK without
    /// understanding the preflight question" and falls through to the
    /// degraded POST so the catalogued `200 OPTIONS / 503 POST`
    /// substrate lie is caught.
    ///
    /// 404 / 405 fall through too: the downstream accepts OTLP traffic
    /// but does not implement OPTIONS, which is allowed.
    fn classify_options_response(&self, status: reqwest::StatusCode) -> ProbeStageOutcome {
        if status.as_u16() == 204 {
            return ProbeStageOutcome::Succeeded;
        }
        if status.is_success() || matches!(status.as_u16(), 404 | 405) {
            return ProbeStageOutcome::FallThrough;
        }
        ProbeStageOutcome::Failed(ProbeError::Refused {
            endpoint: self.endpoint.clone(),
            status: status.as_u16(),
        })
    }

    /// Stage 2 of the two-stage probe: send a zero-records
    /// `ExportLogsServiceRequest` and require a 2xx response. The only
    /// path through which the catalogued `200 OPTIONS / 503 POST`
    /// substrate lie is caught.
    async fn probe_degraded_post(&self) -> Result<(), ProbeError> {
        let url = self.url_for("logs");
        let body = empty_export_logs_service_request_bytes();
        let request = self
            .client
            .post(&url)
            .header(reqwest::header::CONTENT_TYPE, "application/x-protobuf")
            .body(body)
            .timeout(PROBE_STAGE_TIMEOUT);
        let response = request
            .send()
            .await
            .map_err(|e| self.classify_transport_error(e))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(ProbeError::Refused {
                endpoint: self.endpoint.clone(),
                status: response.status().as_u16(),
            })
        }
    }

    /// Translate a `reqwest::Error` into a `ProbeError`. Timeouts get
    /// the dedicated `Timeout` variant; everything else (connect refused,
    /// DNS failure, TLS handshake) collapses to `Unreachable` carrying
    /// the underlying error string.
    fn classify_transport_error(&self, err: reqwest::Error) -> ProbeError {
        if err.is_timeout() {
            ProbeError::Timeout {
                endpoint: self.endpoint.clone(),
                elapsed_ms: PROBE_STAGE_TIMEOUT.as_millis() as u64,
            }
        } else {
            ProbeError::Unreachable {
                endpoint: self.endpoint.clone(),
                reason: err.to_string(),
            }
        }
    }
}

/// Encode the zero-records `ExportLogsServiceRequest` the degraded
/// probe stage POSTs. Pure; the bytes are constant per encoding (the
/// `prost`-generated empty message serialises to zero bytes).
fn empty_export_logs_service_request_bytes() -> Vec<u8> {
    ExportLogsServiceRequest {
        resource_logs: vec![],
    }
    .encode_to_vec()
}
