//! Driving adapters — gRPC server (`tonic`) and (later, Slice 02) HTTP
//! server (`axum`).
//!
//! See `docs/feature/aperture/design/component-design.md > Module
//! structure :: transport/grpc.rs and transport/http.rs` for the
//! design contract; ADR-0006 for the library choices.
//!
//! Slice 01 lights up the gRPC arm only: a tonic Server with a
//! `LogsService` impl that delegates to `app::ingest_logs`. The HTTP
//! arm and the Traces/Metrics services land in subsequent slices.

use std::net::SocketAddr;
use std::sync::Arc;

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

/// Spawn the gRPC listener on the given address. Returns the bound
/// socket address (so callers binding `127.0.0.1:0` can discover the
/// ephemeral port) and a join handle for the serving task.
///
/// Emits `event=listener_bound transport=grpc addr=...` on stderr the
/// moment the listener has bound the socket.
pub async fn spawn_grpc(
    bind_addr: SocketAddr,
    sink: Arc<dyn OtlpSink>,
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
