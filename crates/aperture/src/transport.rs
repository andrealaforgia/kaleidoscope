//! Driving adapters — gRPC server (`tonic`) and HTTP server (`axum`).
//!
//! See `docs/feature/aperture/design/component-design.md > Module
//! structure :: transport/grpc.rs and transport/http.rs` for the
//! design contract; ADR-0006 for the library choices.
//!
//! Slice 01 lit up the gRPC arm; Slice 02 adds the HTTP/protobuf arm
//! plus the multiplexed `/healthz` and `/readyz` operator probes. The
//! Traces and Metrics signals on either arm land in Slices 03 and 04.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use opentelemetry_proto::tonic::collector::logs::v1::{
    logs_service_server::{LogsService, LogsServiceServer},
    ExportLogsPartialSuccess, ExportLogsServiceRequest, ExportLogsServiceResponse,
};
use opentelemetry_proto::tonic::collector::metrics::v1::{
    metrics_service_server::{MetricsService, MetricsServiceServer},
    ExportMetricsPartialSuccess, ExportMetricsServiceRequest, ExportMetricsServiceResponse,
};
use opentelemetry_proto::tonic::collector::trace::v1::{
    trace_service_server::{TraceService, TraceServiceServer},
    ExportTracePartialSuccess, ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use prost::Message;
use tokio::net::TcpListener;
use tonic::transport::server::TcpIncoming;
use tonic::{Request, Response, Status};

use crate::app::{ingest_logs, ingest_metrics, ingest_traces, IngestOutcome, Transport};
use crate::backpressure::{refusal_message, CapTransport, ConcurrencyLimiter};
use crate::observability::event;
use crate::ports::OtlpSink;
use crate::readiness::{ReadinessPhase, SharedReadinessState};

/// Spawn the gRPC listener on the given address. Returns the bound
/// socket address (so callers binding `127.0.0.1:0` can discover the
/// ephemeral port) and a join handle for the serving task.
///
/// Emits `event=listener_bound transport=grpc addr=...` on stderr the
/// moment the listener has bound the socket and flips the readiness
/// state's `grpc_bound` flag.
pub async fn spawn_grpc(
    bind_addr: SocketAddr,
    sink: Arc<dyn OtlpSink>,
    readiness: SharedReadinessState,
    limiter: ConcurrencyLimiter,
    shutdown: tokio::sync::oneshot::Receiver<()>,
) -> Result<(SocketAddr, tokio::task::JoinHandle<()>), std::io::Error> {
    let listener = TcpListener::bind(bind_addr).await?;
    let bound = listener.local_addr()?;
    let incoming = TcpIncoming::from_listener(listener, true, None)
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    tracing::info!(
        event = event::LISTENER_BOUND,
        transport = "grpc",
        addr = %bound,
    );
    readiness.mark_grpc_bound();

    let logs_service = LogsServiceImpl {
        sink: Arc::clone(&sink),
        limiter: limiter.clone(),
    };
    let trace_service = TraceServiceImpl {
        sink: Arc::clone(&sink),
        limiter: limiter.clone(),
    };
    let metrics_service = MetricsServiceImpl {
        sink: Arc::clone(&sink),
        limiter: limiter.clone(),
    };
    let server = tonic::transport::Server::builder()
        .add_service(LogsServiceServer::new(logs_service))
        .add_service(TraceServiceServer::new(trace_service))
        .add_service(MetricsServiceServer::new(metrics_service))
        .serve_with_incoming_shutdown(incoming, async move {
            let _ = shutdown.await;
        });

    let handle = tokio::spawn(async move {
        // The serve future's error is swallowed at v0; binding errors
        // surface synchronously above. Slice 08 will surface drain
        // outcomes through the shutdown orchestrator.
        let _ = server.await;
    });

    Ok((bound, handle))
}

// =========================================================================
// HTTP listener — axum Router for /v1/{logs} + /healthz + /readyz
// =========================================================================

/// Application state passed to every axum handler.
#[derive(Clone)]
struct HttpState {
    sink: Arc<dyn OtlpSink>,
    readiness: SharedReadinessState,
    limiter: ConcurrencyLimiter,
}

/// Spawn the HTTP listener. Returns the bound socket address and a
/// join handle for the serving task.
///
/// Emits `event=listener_bound transport=http addr=...` on stderr the
/// moment the listener has bound the socket and flips the readiness
/// state's `http_bound` flag.
pub async fn spawn_http(
    bind_addr: SocketAddr,
    sink: Arc<dyn OtlpSink>,
    readiness: SharedReadinessState,
    limiter: ConcurrencyLimiter,
    shutdown: tokio::sync::oneshot::Receiver<()>,
) -> Result<(SocketAddr, tokio::task::JoinHandle<()>), std::io::Error> {
    let listener = TcpListener::bind(bind_addr).await?;
    let bound = listener.local_addr()?;

    tracing::info!(
        event = event::LISTENER_BOUND,
        transport = "http",
        addr = %bound,
    );
    readiness.mark_http_bound();

    let state = HttpState {
        sink,
        readiness: Arc::clone(&readiness),
        limiter,
    };

    // Slices 02, 03, and 04 ship the logs, traces, and metrics signals
    // on the HTTP arm; the OTLP three-signal contract is now complete
    // for the HTTP transport. Any other `/v1/*` POST falls through to
    // axum's 404.
    let router: Router = Router::new()
        .route("/healthz", get(handle_healthz))
        .route("/readyz", get(handle_readyz))
        .route("/v1/logs", post(handle_logs))
        .route("/v1/traces", post(handle_traces))
        .route("/v1/metrics", post(handle_metrics))
        .with_state(state);

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown.await;
            })
            .await;
    });

    Ok((bound, handle))
}

// -------------------------------------------------------------------------
// Liveness — `/healthz` always returns 200 while the process is up.
// -------------------------------------------------------------------------

async fn handle_healthz() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "ok\n",
    )
}

// -------------------------------------------------------------------------
// Readiness — `/readyz` reflects the ReadinessState phase.
// -------------------------------------------------------------------------

async fn handle_readyz(State(state): State<HttpState>) -> impl IntoResponse {
    // Slice 02 ships `Starting → Ready`; Slice 08 lands the `Draining`
    // arm. The body shape is `<phase>\n`, matching the slice tests:
    // `"starting\n"`, `"ready\n"`, `"draining\n"`. The 503-vs-200 split
    // is what an orchestrator's readiness probe acts on.
    let (status, body) = match state.readiness.current() {
        ReadinessPhase::Starting => (StatusCode::SERVICE_UNAVAILABLE, "starting\n"),
        ReadinessPhase::Ready => (StatusCode::OK, "ready\n"),
        ReadinessPhase::Draining => (StatusCode::SERVICE_UNAVAILABLE, "draining\n"),
    };
    (
        status,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
}

// -------------------------------------------------------------------------
// `/v1/logs` — HTTP/protobuf logs accept path.
// -------------------------------------------------------------------------

/// The OTLP/HTTP content type for protobuf bodies. The OTel spec is
/// strict on this exact string; the Slice 02 acceptance test rejects
/// `application/json` with 415.
const CONTENT_TYPE_PROTOBUF: &str = "application/x-protobuf";

/// Handle `POST /v1/logs`. Returns:
///
/// - 200 OK on a body the harness accepts (record forwarded to sink),
/// - 400 Bad Request with the `OtlpViolation::Display` string verbatim
///   in the body when the harness rejects,
/// - 415 Unsupported Media Type when the Content-Type header is not
///   `application/x-protobuf`,
/// - 503 Service Unavailable on a sink refusal (Slice 06 territory;
///   StubSink/RecordingSink never refuse).
async fn handle_logs(
    State(state): State<HttpState>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    // ADR-0010: per-transport concurrency cap. Permit acquired before
    // the harness sees the body; dropped on response sent. The
    // content-type check is intentionally inside the cap — a 415
    // response still consumes a permit because the request was
    // accepted by the listener and the slot is occupied for the
    // duration of the handler. Slice 05 acceptance tests focus on the
    // saturation case; the 415 path is a no-op for the cap.
    let _permit = match state.limiter.try_acquire() {
        Ok(p) => p,
        Err(()) => {
            return refuse_http(state.limiter.cap());
        }
    };

    if !is_protobuf_content_type(&headers) {
        tracing::warn!(
            event = event::UNSUPPORTED_MEDIA_TYPE,
            transport = "http",
            signal = "logs",
            content_type = headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or(""),
        );
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "unsupported media type; expected application/x-protobuf\n",
        )
            .into_response();
    }

    tracing::info!(
        event = event::REQUEST_RECEIVED,
        transport = "http_protobuf",
        signal = "logs",
        bytes = body.len() as u64,
    );

    let outcome = ingest_logs(&body, Transport::HttpProtobuf, &state.sink).await;
    match outcome {
        IngestOutcome::Accepted => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/x-protobuf")],
            // Aperture v0 does not synthesise partial-success
            // diagnostics; an accepted batch is fully accepted, so the
            // success body is the empty serialised
            // `ExportLogsServiceResponse` (one zero byte for the
            // `partial_success` field tag would be larger; an empty
            // body is also a conformant response).
            Vec::<u8>::new(),
        )
            .into_response(),
        IngestOutcome::Rejected(violation) => (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            // The acceptance test asserts the harness's
            // `OtlpViolation::Display` round-trips verbatim — operators
            // see exactly the rule, signal, framing, locus, expected
            // and observed substrings the harness produced.
            violation.to_string(),
        )
            .into_response(),
        IngestOutcome::SinkRefused(err) => (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            err.to_string(),
        )
            .into_response(),
    }
}

// -------------------------------------------------------------------------
// `/v1/traces` — HTTP/protobuf traces accept path.
// -------------------------------------------------------------------------

/// Handle `POST /v1/traces`. Mirror of [`handle_logs`] for the traces
/// signal. Returns:
///
/// - 200 OK on a body the harness accepts (record forwarded to sink),
/// - 400 Bad Request with the `OtlpViolation::Display` string verbatim
///   in the body when the harness rejects (the `WireType::SignalMismatch`
///   reject path, exercised by sending logs bytes to `/v1/traces`,
///   surfaces here),
/// - 415 Unsupported Media Type when the Content-Type header is not
///   `application/x-protobuf`,
/// - 503 Service Unavailable on a sink refusal.
async fn handle_traces(
    State(state): State<HttpState>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    // ADR-0010: per-transport concurrency cap. See `handle_logs` for
    // rationale; the shape is identical for every signal arm.
    let _permit = match state.limiter.try_acquire() {
        Ok(p) => p,
        Err(()) => {
            return refuse_http(state.limiter.cap());
        }
    };

    if !is_protobuf_content_type(&headers) {
        tracing::warn!(
            event = event::UNSUPPORTED_MEDIA_TYPE,
            transport = "http",
            signal = "traces",
            content_type = headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or(""),
        );
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "unsupported media type; expected application/x-protobuf\n",
        )
            .into_response();
    }

    tracing::info!(
        event = event::REQUEST_RECEIVED,
        transport = "http_protobuf",
        signal = "traces",
        bytes = body.len() as u64,
    );

    let outcome = ingest_traces(&body, Transport::HttpProtobuf, &state.sink).await;
    match outcome {
        IngestOutcome::Accepted => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/x-protobuf")],
            // Aperture v0 does not synthesise partial-success
            // diagnostics; an accepted batch is fully accepted, so the
            // success body is the empty serialised
            // `ExportTraceServiceResponse`.
            Vec::<u8>::new(),
        )
            .into_response(),
        IngestOutcome::Rejected(violation) => (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            // Verbatim harness `OtlpViolation::Display` — operators see
            // the rule, signal, framing, locus, expected and observed
            // substrings the harness produced. The signal-mismatch
            // reject path (logs bytes posted to `/v1/traces`) carries
            // `rule=WireType::SignalMismatch{observed=Logs,
            // asserted=Traces}` here.
            violation.to_string(),
        )
            .into_response(),
        IngestOutcome::SinkRefused(err) => (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            err.to_string(),
        )
            .into_response(),
    }
}

// -------------------------------------------------------------------------
// `/v1/metrics` — HTTP/protobuf metrics accept path.
// -------------------------------------------------------------------------

/// Handle `POST /v1/metrics`. Mirror of [`handle_logs`] and
/// [`handle_traces`] for the metrics signal. Returns:
///
/// - 200 OK on a body the harness accepts (record forwarded to sink),
/// - 400 Bad Request with the `OtlpViolation::Display` string verbatim
///   in the body when the harness rejects (the `WireType::SignalMismatch`
///   reject path, exercised by sending traces bytes to `/v1/metrics`,
///   surfaces here),
/// - 415 Unsupported Media Type when the Content-Type header is not
///   `application/x-protobuf`,
/// - 503 Service Unavailable on a sink refusal.
async fn handle_metrics(
    State(state): State<HttpState>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    // ADR-0010: per-transport concurrency cap. See `handle_logs` for
    // rationale; the shape is identical for every signal arm.
    let _permit = match state.limiter.try_acquire() {
        Ok(p) => p,
        Err(()) => {
            return refuse_http(state.limiter.cap());
        }
    };

    if !is_protobuf_content_type(&headers) {
        tracing::warn!(
            event = event::UNSUPPORTED_MEDIA_TYPE,
            transport = "http",
            signal = "metrics",
            content_type = headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or(""),
        );
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "unsupported media type; expected application/x-protobuf\n",
        )
            .into_response();
    }

    tracing::info!(
        event = event::REQUEST_RECEIVED,
        transport = "http_protobuf",
        signal = "metrics",
        bytes = body.len() as u64,
    );

    let outcome = ingest_metrics(&body, Transport::HttpProtobuf, &state.sink).await;
    match outcome {
        IngestOutcome::Accepted => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/x-protobuf")],
            // Aperture v0 does not synthesise partial-success
            // diagnostics; an accepted batch is fully accepted, so the
            // success body is the empty serialised
            // `ExportMetricsServiceResponse`.
            Vec::<u8>::new(),
        )
            .into_response(),
        IngestOutcome::Rejected(violation) => (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            // Verbatim harness `OtlpViolation::Display` — operators see
            // the rule, signal, framing, locus, expected and observed
            // substrings the harness produced. The signal-mismatch
            // reject path (traces bytes posted to `/v1/metrics`) carries
            // `rule=WireType::SignalMismatch{observed=Traces,
            // asserted=Metrics}` here.
            violation.to_string(),
        )
            .into_response(),
        IngestOutcome::SinkRefused(err) => (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            err.to_string(),
        )
            .into_response(),
    }
}

/// Build the HTTP refusal response shape locked by ADR-0010 / DISCUSS
/// US-AP-07: status 503, `Retry-After: 1`, body names the cap. The
/// OTel SDK retry policy reads the `Retry-After` header verbatim; the
/// body is operator-readable but not protocol-load-bearing.
fn refuse_http(cap: u32) -> axum::response::Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::RETRY_AFTER, "1"),
        ],
        format!("{}\n", refusal_message(CapTransport::HttpProtobuf, cap)),
    )
        .into_response()
}

/// True when the `Content-Type` header value starts with
/// `application/x-protobuf`. The OTLP/HTTP spec is strict on the type
/// but tolerant of optional parameters (`; charset=...`).
fn is_protobuf_content_type(headers: &HeaderMap) -> bool {
    let Some(raw) = headers.get(header::CONTENT_TYPE) else {
        return false;
    };
    let Ok(text) = raw.to_str() else {
        return false;
    };
    let media_type = text.split(';').next().unwrap_or("").trim();
    media_type.eq_ignore_ascii_case(CONTENT_TYPE_PROTOBUF)
}

/// gRPC `LogsService` implementation.
///
/// Per ADR-0006 the service holds an `Arc<dyn OtlpSink>` cloned from
/// the composition root; per the `app::ingest_logs` contract this is
/// the only call site that re-encodes the request into bytes for the
/// harness.
struct LogsServiceImpl {
    sink: Arc<dyn OtlpSink>,
    limiter: ConcurrencyLimiter,
}

#[tonic::async_trait]
impl LogsService for LogsServiceImpl {
    async fn export(
        &self,
        request: Request<ExportLogsServiceRequest>,
    ) -> Result<Response<ExportLogsServiceResponse>, Status> {
        // ADR-0010: per-transport concurrency cap. Acquire a permit
        // BEFORE the harness sees the body. On saturation the limiter
        // emits the `event=concurrency_cap_hit` warn line and we
        // surface `RESOURCE_EXHAUSTED` immediately. The permit binding
        // is held for the lifetime of this method — its drop at
        // end-of-scope is the contract-specified "released on response
        // sent" point.
        let _permit = match self.limiter.try_acquire() {
            Ok(p) => p,
            Err(()) => {
                return Err(Status::resource_exhausted(refusal_message(
                    CapTransport::Grpc,
                    self.limiter.cap(),
                )));
            }
        };

        let req = request.into_inner();
        // Re-encode the typed request into bytes so the validator sees
        // the same shape the SDK put on the wire. tonic decoded the
        // gRPC frame for us; the harness validates the protobuf body.
        let bytes = req.encode_to_vec();

        tracing::info!(
            event = event::REQUEST_RECEIVED,
            transport = "grpc",
            signal = "logs",
            bytes = bytes.len() as u64,
        );

        let outcome = ingest_logs(&bytes, Transport::Grpc, &self.sink).await;
        match outcome {
            IngestOutcome::Accepted => {
                // Aperture v0 does not synthesise partial-success
                // diagnostics; an accepted batch is fully accepted.
                let response = ExportLogsServiceResponse {
                    partial_success: Some(ExportLogsPartialSuccess::default()),
                };
                Ok(Response::new(response))
            }
            IngestOutcome::Rejected(violation) => {
                Err(Status::invalid_argument(violation.to_string()))
            }
            IngestOutcome::SinkRefused(err) => {
                // Slice 01's StubSink and RecordingSink never refuse a
                // record; the production-bound code path here is
                // unreachable until Slice 06 lands `ForwardingSink`.
                // Slice 06 will distinguish `SinkError::Internal`
                // (gRPC INTERNAL) from the rest (gRPC UNAVAILABLE)
                // — see `app::responses::sink_error_to_grpc` in the
                // design contract. For Slice 01 we map every refusal
                // to UNAVAILABLE because that is the contract's
                // default.
                Err(Status::unavailable(err.to_string()))
            }
        }
    }
}

/// gRPC `TraceService` implementation. Mirror of [`LogsServiceImpl`]
/// for the traces signal.
///
/// Per ADR-0006 the service holds an `Arc<dyn OtlpSink>` cloned from
/// the composition root; per the `app::ingest_traces` contract this is
/// the only call site that re-encodes the request into bytes for the
/// harness.
struct TraceServiceImpl {
    sink: Arc<dyn OtlpSink>,
    limiter: ConcurrencyLimiter,
}

#[tonic::async_trait]
impl TraceService for TraceServiceImpl {
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        // ADR-0010: per-transport concurrency cap (see `LogsServiceImpl`
        // for the full rationale). Permit acquired before the harness
        // sees the body; dropped on response sent.
        let _permit = match self.limiter.try_acquire() {
            Ok(p) => p,
            Err(()) => {
                return Err(Status::resource_exhausted(refusal_message(
                    CapTransport::Grpc,
                    self.limiter.cap(),
                )));
            }
        };

        let req = request.into_inner();
        // Re-encode the typed request into bytes so the validator sees
        // the same shape the SDK put on the wire. tonic decoded the
        // gRPC frame for us; the harness validates the protobuf body.
        let bytes = req.encode_to_vec();

        tracing::info!(
            event = event::REQUEST_RECEIVED,
            transport = "grpc",
            signal = "traces",
            bytes = bytes.len() as u64,
        );

        let outcome = ingest_traces(&bytes, Transport::Grpc, &self.sink).await;
        match outcome {
            IngestOutcome::Accepted => {
                let response = ExportTraceServiceResponse {
                    partial_success: Some(ExportTracePartialSuccess::default()),
                };
                Ok(Response::new(response))
            }
            IngestOutcome::Rejected(violation) => {
                Err(Status::invalid_argument(violation.to_string()))
            }
            IngestOutcome::SinkRefused(err) => {
                // Same rationale as the logs path — Slice 06 will
                // distinguish `SinkError::Internal` (gRPC INTERNAL) from
                // the rest (gRPC UNAVAILABLE). Slice 03 maps every
                // refusal to UNAVAILABLE.
                Err(Status::unavailable(err.to_string()))
            }
        }
    }
}

/// gRPC `MetricsService` implementation. Mirror of [`LogsServiceImpl`]
/// and [`TraceServiceImpl`] for the metrics signal.
///
/// Per ADR-0006 the service holds an `Arc<dyn OtlpSink>` cloned from
/// the composition root; per the `app::ingest_metrics` contract this
/// is the only call site that re-encodes the request into bytes for
/// the harness.
struct MetricsServiceImpl {
    sink: Arc<dyn OtlpSink>,
    limiter: ConcurrencyLimiter,
}

#[tonic::async_trait]
impl MetricsService for MetricsServiceImpl {
    async fn export(
        &self,
        request: Request<ExportMetricsServiceRequest>,
    ) -> Result<Response<ExportMetricsServiceResponse>, Status> {
        // ADR-0010: per-transport concurrency cap (see `LogsServiceImpl`
        // for the full rationale). Permit acquired before the harness
        // sees the body; dropped on response sent.
        let _permit = match self.limiter.try_acquire() {
            Ok(p) => p,
            Err(()) => {
                return Err(Status::resource_exhausted(refusal_message(
                    CapTransport::Grpc,
                    self.limiter.cap(),
                )));
            }
        };

        let req = request.into_inner();
        // Re-encode the typed request into bytes so the validator sees
        // the same shape the SDK put on the wire. tonic decoded the
        // gRPC frame for us; the harness validates the protobuf body.
        let bytes = req.encode_to_vec();

        tracing::info!(
            event = event::REQUEST_RECEIVED,
            transport = "grpc",
            signal = "metrics",
            bytes = bytes.len() as u64,
        );

        let outcome = ingest_metrics(&bytes, Transport::Grpc, &self.sink).await;
        match outcome {
            IngestOutcome::Accepted => {
                let response = ExportMetricsServiceResponse {
                    partial_success: Some(ExportMetricsPartialSuccess::default()),
                };
                Ok(Response::new(response))
            }
            IngestOutcome::Rejected(violation) => {
                Err(Status::invalid_argument(violation.to_string()))
            }
            IngestOutcome::SinkRefused(err) => {
                // Same rationale as the logs / traces paths — Slice 06
                // will distinguish `SinkError::Internal` (gRPC INTERNAL)
                // from the rest (gRPC UNAVAILABLE). Slice 04 maps every
                // refusal to UNAVAILABLE.
                Err(Status::unavailable(err.to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Focused unit tests for the pure helpers extracted from the HTTP
    //! handler. The integration tests in
    //! `tests/slice_02_http_protobuf_and_readiness.rs` exercise the
    //! end-to-end accept / reject paths over the wire; the tests here
    //! pin the content-type-classification rules so a mutation-test
    //! flip of any branch surfaces against a single assertion.
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with_content_type(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_str(value).expect("test content-type value parses"),
        );
        headers
    }

    #[test]
    fn protobuf_content_type_is_accepted() {
        assert!(is_protobuf_content_type(&headers_with_content_type(
            "application/x-protobuf"
        )));
    }

    #[test]
    fn protobuf_content_type_with_charset_parameter_is_accepted() {
        // The OTLP/HTTP spec is strict on the media type but tolerant
        // of optional parameters; SDKs commonly attach `; charset=...`
        // even though protobuf is binary.
        assert!(is_protobuf_content_type(&headers_with_content_type(
            "application/x-protobuf; charset=utf-8"
        )));
    }

    #[test]
    fn protobuf_content_type_is_case_insensitive() {
        // RFC 9110: media type comparisons are case-insensitive.
        assert!(is_protobuf_content_type(&headers_with_content_type(
            "Application/X-Protobuf"
        )));
    }

    #[test]
    fn json_content_type_is_rejected() {
        assert!(!is_protobuf_content_type(&headers_with_content_type(
            "application/json"
        )));
    }

    #[test]
    fn empty_content_type_is_rejected() {
        assert!(!is_protobuf_content_type(&headers_with_content_type("")));
    }

    #[test]
    fn missing_content_type_header_is_rejected() {
        assert!(!is_protobuf_content_type(&HeaderMap::new()));
    }

    #[test]
    fn protobuf_prefix_with_extra_path_segment_is_rejected() {
        // `application/x-protobuf-foo` is not the OTLP media type and
        // must not be conflated with `application/x-protobuf`. This
        // pins the eq-vs-starts_with mutation: a `starts_with` flip
        // would accept this string.
        assert!(!is_protobuf_content_type(&headers_with_content_type(
            "application/x-protobuf-foo"
        )));
    }
}
