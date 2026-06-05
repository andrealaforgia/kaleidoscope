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

use std::sync::atomic::{AtomicBool, Ordering};

use crate::app::{ingest_logs, ingest_metrics, ingest_traces, IngestOutcome, Transport};
use crate::backpressure::{refusal_message, CapTransport, ConcurrencyLimiter};
use crate::observability::event;
use crate::ports::OtlpSink;
use crate::readiness::{ReadinessPhase, SharedReadinessState};

/// Verdict a serving-loop task resolves to (ADR-0066, D1). The join
/// handle carries this so the shutdown orchestrator / run loop can fold
/// a post-bind serve death into the process exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServeOutcome {
    /// The serve future returned while a shutdown was requested: a clean
    /// drain. No event, no readiness flip — the orchestrator owns the
    /// drain narrative.
    Graceful,
    /// The serve future returned with NO shutdown requested: a post-bind
    /// death (an `Err`, or an unexpected early `Ok`). The task has
    /// already self-reacted (emitted `serve_loop_failed` and flipped
    /// readiness to `Failed`); the verdict folds to exit code 3.
    Failed,
}

/// The serve error rendered to a reason string at the failure site
/// (ADR-0066, D1). `tonic`/`axum` serve errors are not uniformly
/// `Send + 'static`, and nothing downstream of the join needs the rich
/// type — the operator reads the reason string. An early `Ok` with no
/// shutdown requested renders the "listener stopped serving" reason.
#[derive(Debug, Clone)]
pub(crate) struct ServeError(String);

impl ServeError {
    /// The reason rendered from a serve future's `Err`.
    fn from_err(reason: impl std::fmt::Display) -> Self {
        Self(reason.to_string())
    }

    /// The reason for an unexpected early `Ok` (the listener stopped
    /// serving on its own with no shutdown requested — D3, fatal at v0).
    fn from_early_ok() -> Self {
        Self("serving loop returned without a shutdown request".to_string())
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

/// Decide a serving-loop task's verdict from the serve result and the
/// shutdown-requested flag, self-reacting on a fatal return (ADR-0066,
/// D1/D3). This is the single point both transports route through, so
/// the graceful-vs-fatal discriminator and the self-reaction
/// (emit + flip) live in exactly one place.
///
/// - shutdown WAS requested -> `Graceful` (any serve return is clean).
/// - shutdown was NOT requested -> `Failed`: emit
///   `event=serve_loop_failed transport=.. error=..` at error level and
///   flip readiness to the sticky `Failed` phase.
fn resolve_serve_outcome<E: std::fmt::Display>(
    serve_result: std::result::Result<(), E>,
    shutdown_requested: &AtomicBool,
    transport: &'static str,
    readiness: &SharedReadinessState,
) -> ServeOutcome {
    if shutdown_requested.load(Ordering::Acquire) {
        return ServeOutcome::Graceful;
    }
    let error = match serve_result {
        Err(e) => ServeError::from_err(e),
        Ok(()) => ServeError::from_early_ok(),
    };
    tracing::error!(
        event = event::SERVE_LOOP_FAILED,
        transport = transport,
        error = error.as_str(),
    );
    readiness.flip_to_failed();
    ServeOutcome::Failed
}

/// Drive the production post-bind-death self-reaction (ADR-0066) for the
/// named transport against a real readiness handle, with NO shutdown
/// requested. This is the single entry point the acceptance-layer
/// injection seam (`testing::spawn_with_injected_serve_failure`) and the
/// binary's `APERTURE_TEST_INJECT_SERVE_FAILURE` trigger both route
/// through, so the injected failure exercises the EXACT production emit +
/// flip code (`resolve_serve_outcome`), never a reimplementation.
///
/// `early_ok` selects the D3 unexpected-early-`Ok` leg (the serve future
/// returned `Ok` with no shutdown requested) vs the `Err` leg; both are
/// fatal at v0 and surface identically.
pub(crate) fn inject_serve_failure(
    transport: &'static str,
    readiness: &SharedReadinessState,
    early_ok: bool,
) -> ServeOutcome {
    // A never-requested shutdown: the flag is `false`, so
    // `resolve_serve_outcome` takes the fatal leg.
    let never_requested = AtomicBool::new(false);
    let injected: std::result::Result<(), std::io::Error> = if early_ok {
        Ok(())
    } else {
        Err(std::io::Error::other(
            "injected post-bind serve failure (test seam)",
        ))
    };
    resolve_serve_outcome(injected, &never_requested, transport, readiness)
}

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
) -> Result<(SocketAddr, tokio::task::JoinHandle<ServeOutcome>), std::io::Error> {
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

    // ADR-0066 D3: the discriminator between a graceful drain and a
    // fatal post-bind death is "was shutdown requested?", NOT the
    // serve future's Ok/Err. The graceful-shutdown closure sets this
    // flag the instant the oneshot resolves, before the serve future
    // finishes draining; the task reads it after the serve future
    // returns.
    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let closure_flag = Arc::clone(&shutdown_requested);

    let server = tonic::transport::Server::builder()
        .add_service(LogsServiceServer::new(logs_service))
        .add_service(TraceServiceServer::new(trace_service))
        .add_service(MetricsServiceServer::new(metrics_service))
        .serve_with_incoming_shutdown(incoming, async move {
            let _ = shutdown.await;
            closure_flag.store(true, Ordering::Release);
        });

    let task_readiness = Arc::clone(&readiness);
    let handle = tokio::spawn(async move {
        // ADR-0066: the serve future's result is no longer swallowed.
        // On a fatal return the task self-reacts at the failure site
        // (emit serve_loop_failed + flip readiness to Failed) and the
        // typed verdict folds into the process exit code.
        let result = server.await;
        resolve_serve_outcome(result, &shutdown_requested, "grpc", &task_readiness)
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
) -> Result<(SocketAddr, tokio::task::JoinHandle<ServeOutcome>), std::io::Error> {
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

    // ADR-0066 D3: see `spawn_grpc`. The previously-SILENT HTTP arm now
    // surfaces a post-bind death identically to gRPC.
    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let closure_flag = Arc::clone(&shutdown_requested);

    let task_readiness = Arc::clone(&readiness);
    let handle = tokio::spawn(async move {
        let result = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown.await;
                closure_flag.store(true, Ordering::Release);
            })
            .await;
        resolve_serve_outcome(result, &shutdown_requested, "http", &task_readiness)
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
        // ADR-0066: a dead serving loop flips readiness to `Failed`.
        // `/readyz` returns 503 `"failed"` so an orchestrator pulls the
        // zombie from rotation; `/healthz` stays 200 (liveness).
        ReadinessPhase::Failed => (StatusCode::SERVICE_UNAVAILABLE, "failed\n"),
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

    // =====================================================================
    // ADR-0066 — serve-loop outcome resolution (D1/D3)
    // =====================================================================

    use crate::readiness::ReadinessState;

    #[test]
    fn shutdown_requested_makes_any_return_graceful() {
        // D3: when shutdown WAS requested, an Err return is still a
        // clean drain (a teardown error, not a post-bind death). No
        // readiness flip, verdict Graceful.
        let readiness = ReadinessState::new();
        readiness.mark_grpc_bound();
        readiness.mark_http_bound();
        let flag = AtomicBool::new(true);
        let outcome = resolve_serve_outcome(
            Err::<(), _>(std::io::Error::other("teardown")),
            &flag,
            "grpc",
            &readiness,
        );
        assert_eq!(outcome, ServeOutcome::Graceful);
        assert_eq!(readiness.current(), ReadinessPhase::Ready);
    }

    #[test]
    fn not_requested_err_return_is_fatal_and_flips_readiness() {
        // D1/D3: no shutdown requested + Err -> Failed, readiness flips
        // to the sticky Failed phase.
        let readiness = ReadinessState::new();
        readiness.mark_grpc_bound();
        readiness.mark_http_bound();
        let flag = AtomicBool::new(false);
        let outcome = resolve_serve_outcome(
            Err::<(), _>(std::io::Error::other("accept loop died")),
            &flag,
            "grpc",
            &readiness,
        );
        assert_eq!(outcome, ServeOutcome::Failed);
        assert_eq!(readiness.current(), ReadinessPhase::Failed);
    }

    #[test]
    fn not_requested_early_ok_return_is_fatal() {
        // D3: an unexpected early Ok with no shutdown requested is fatal
        // at v0 (the listener stopped serving on its own).
        let readiness = ReadinessState::new();
        readiness.mark_grpc_bound();
        readiness.mark_http_bound();
        let flag = AtomicBool::new(false);
        let outcome =
            resolve_serve_outcome(Ok::<(), std::io::Error>(()), &flag, "http", &readiness);
        assert_eq!(outcome, ServeOutcome::Failed);
        assert_eq!(readiness.current(), ReadinessPhase::Failed);
    }

    #[test]
    fn serve_error_renders_the_err_reason() {
        // Pins ServeError::from_err's payload against a default-string
        // mutation: the operator reads this reason on stderr.
        let err = ServeError::from_err("address in use");
        assert_eq!(err.as_str(), "address in use");
    }

    #[test]
    fn serve_error_from_early_ok_names_the_missing_shutdown() {
        // Pins the early-Ok reason string: it must say the loop returned
        // without a shutdown request, not an empty/default string.
        let err = ServeError::from_early_ok();
        assert_eq!(
            err.as_str(),
            "serving loop returned without a shutdown request"
        );
    }

    #[test]
    fn inject_serve_failure_drives_the_fatal_path() {
        // The test-seam entry point routes through the real
        // resolve_serve_outcome: it returns Failed and flips readiness.
        let readiness = ReadinessState::new();
        readiness.mark_grpc_bound();
        readiness.mark_http_bound();
        let outcome = inject_serve_failure("grpc", &readiness, false);
        assert_eq!(outcome, ServeOutcome::Failed);
        assert_eq!(readiness.current(), ReadinessPhase::Failed);
    }

    #[test]
    fn inject_serve_failure_early_ok_is_also_fatal() {
        let readiness = ReadinessState::new();
        readiness.mark_grpc_bound();
        readiness.mark_http_bound();
        let outcome = inject_serve_failure("http", &readiness, true);
        assert_eq!(outcome, ServeOutcome::Failed);
        assert_eq!(readiness.current(), ReadinessPhase::Failed);
    }
}
