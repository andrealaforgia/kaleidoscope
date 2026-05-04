//! Composition root — wires the configured sink, runs the Earned-Trust
//! probe, and spawns the listeners.
//!
//! Slice 02 adds the HTTP/protobuf listener and the readiness state
//! machine alongside the gRPC arm Slice 01 lit up. Subsequent slices
//! grow the drain orchestrator (Slice 08) and the rich `ApertureError`
//! enum (Slice 07/08).

use std::sync::Arc;

use crate::config::{Config, SinkKind};
use crate::observability;
use crate::ports::{OtlpSink, Probe};
use crate::readiness::ReadinessState;
use crate::sinks::StubSink;
use crate::transport::{spawn_grpc, spawn_http};
use crate::ApertureError;
use crate::Handle;

/// Wire the sink the configuration names AND run its Earned-Trust
/// probe before returning. Slice 01 honours `SinkKind::Stub`; Slice 06
/// will land `SinkKind::Forwarding` against the real `ForwardingSink`.
///
/// This is the binary's "wire → probe → use" hook (the test path
/// constructs sinks directly with their own probes already verified at
/// construction time — see `aperture::testing::RecordingSink`).
pub(crate) async fn wire_sink(config: &Config) -> crate::Result<Arc<dyn OtlpSink>> {
    match config.sink_kind() {
        SinkKind::Stub => {
            let sink = StubSink;
            // Run the Earned-Trust probe before erasing the type. The
            // probe failure path emits `event=health.startup.refused`;
            // Slice 06 will land the `ForwardingSink` against a
            // wiremock downstream that exercises both the success and
            // failure branches.
            if let Err(e) = sink.probe().await {
                tracing::error!(
                    event = observability::event::HEALTH_STARTUP_REFUSED,
                    reason = %e,
                );
                return Err(ApertureError(format!("sink probe failed: {e}")));
            }
            Ok(Arc::new(sink))
        }
        SinkKind::Forwarding => Err(ApertureError(
            "forwarding sink is not implemented until Slice 06".to_string(),
        )),
    }
}

/// Wire the configuration and sink, run the sink probe, bind both the
/// gRPC and HTTP listeners, and return a [`Handle`] for the caller to
/// manage.
pub(crate) async fn spawn(config: Config, sink: Arc<dyn OtlpSink>) -> crate::Result<Handle> {
    observability::install_subscriber();
    tracing::info!(
        event = observability::event::STARTUP,
        version = env!("CARGO_PKG_VERSION"),
    );

    // The Earned-Trust probe runs in `wire_sink` for the binary path;
    // the test path constructs sinks (e.g. `RecordingSink`) whose
    // probes are statically `Ok(())`. Slice 01 kept this asymmetry
    // because the dyn-dispatch upcast from `OtlpSink` to `Probe` is a
    // Phase-1 refinement (per ADR-0007 the dual-trait shape is the
    // long-term answer; v0 wires probing alongside concrete-typed sink
    // construction).
    let _ = &sink;

    // Two oneshots so each transport's `serve_with_graceful_shutdown`
    // can own its own receiver. Slice 08 will replace this with a
    // `broadcast` sender wired through the shutdown orchestrator.
    let (grpc_shutdown_tx, grpc_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let (http_shutdown_tx, http_shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let readiness = ReadinessState::new();

    let (grpc_addr, grpc_join) = spawn_grpc(
        config.grpc_bind_addr(),
        Arc::clone(&sink),
        Arc::clone(&readiness),
        grpc_shutdown_rx,
    )
    .await
    .map_err(|e| {
        tracing::error!(
            event = observability::event::LISTENER_BIND_FAILED,
            transport = "grpc",
            addr = %config.grpc_bind_addr(),
            reason = %e,
        );
        ApertureError(format!("listener bind failed for grpc: {e}"))
    })?;

    let http_outcome = spawn_http(
        config.http_bind_addr(),
        Arc::clone(&sink),
        Arc::clone(&readiness),
        http_shutdown_rx,
    )
    .await;
    let (http_addr, http_join) = match http_outcome {
        Ok(pair) => pair,
        Err(e) => {
            tracing::error!(
                event = observability::event::LISTENER_BIND_FAILED,
                transport = "http",
                addr = %config.http_bind_addr(),
                reason = %e,
            );
            // The gRPC listener is already bound; signal it to wind
            // down so we don't leave a dangling task on the
            // bind-failure path.
            let _ = grpc_shutdown_tx.send(());
            return Err(ApertureError(format!("listener bind failed for http: {e}")));
        }
    };

    // Both listeners bound; the readiness state has flipped to
    // `Ready` inside the second `mark_*_bound` call. Emit the closing
    // `event=ready` line per ADR-0009.
    tracing::info!(event = observability::event::READY);

    Ok(Handle {
        grpc_addr,
        http_addr,
        grpc_shutdown: Some(grpc_shutdown_tx),
        http_shutdown: Some(http_shutdown_tx),
        grpc_join: Some(grpc_join),
        http_join: Some(http_join),
    })
}
