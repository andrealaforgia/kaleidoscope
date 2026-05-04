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
use prost::Message;
use tokio::net::TcpListener;
use tonic::transport::server::TcpIncoming;
use tonic::{Request, Response, Status};

use crate::app::{ingest_logs, IngestOutcome, Transport};
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

    let service = LogsServiceImpl { sink };
    let server = tonic::transport::Server::builder()
        .add_service(LogsServiceServer::new(service))
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
    };

    // Slice 02 ships the logs signal on the HTTP arm only. Slices 03
    // and 04 grow the `/v1/traces` and `/v1/metrics` routes against the
    // same dispatch shape; until then any other `/v1/*` POST falls
    // through to axum's 404.
    let router: Router = Router::new()
        .route("/healthz", get(handle_healthz))
        .route("/readyz", get(handle_readyz))
        .route("/v1/logs", post(handle_logs))
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
}

#[tonic::async_trait]
impl LogsService for LogsServiceImpl {
    async fn export(
        &self,
        request: Request<ExportLogsServiceRequest>,
    ) -> Result<Response<ExportLogsServiceResponse>, Status> {
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
