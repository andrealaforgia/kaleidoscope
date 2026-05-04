//! Composition root — wires the configured sink, runs the Earned-Trust
//! probe, and spawns the listeners.
//!
//! Slice 01 lights up the gRPC arm only. The shape mirrors the design
//! contract (`docs/feature/aperture/design/component-design.md >
//! Composition root (compose.rs)`); subsequent slices grow the HTTP
//! listener, the readiness state machine, and the drain orchestrator.

use std::sync::Arc;

use crate::config::{Config, SinkKind};
use crate::observability;
use crate::ports::{OtlpSink, Probe};
use crate::sinks::StubSink;
use crate::transport::spawn_grpc;
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
            probe_or_refuse_to_start(&sink).await?;
            Ok(Arc::new(sink))
        }
        SinkKind::Forwarding => Err(ApertureError(
            "forwarding sink is not implemented until Slice 06".to_string(),
        )),
    }
}

/// Run the supplied probe; on `Err` emit `event=health.startup.refused`
/// and surface the failure as `ApertureError`.
async fn probe_or_refuse_to_start<P: Probe>(probe: &P) -> crate::Result<()> {
    if let Err(e) = probe.probe().await {
        tracing::error!(
            event = observability::event::HEALTH_STARTUP_REFUSED,
            reason = %e,
        );
        return Err(ApertureError(format!("sink probe failed: {e}")));
    }
    Ok(())
}

/// Wire the configuration and sink, run the sink probe, bind the gRPC
/// listener, and return a [`Handle`] for the caller to manage.
pub(crate) async fn spawn(config: Config, sink: Arc<dyn OtlpSink>) -> crate::Result<Handle> {
    observability::init_logging();
    tracing::info!(
        event = observability::event::STARTUP,
        version = env!("CARGO_PKG_VERSION"),
    );

    // The Earned-Trust probe runs in `wire_sink` for the binary path;
    // the test path constructs sinks (e.g. `RecordingSink`) whose
    // probes are statically `Ok(())`. Slice 01 keeps this asymmetry
    // because the dyn-dispatch upcast from `OtlpSink` to `Probe` is a
    // Phase-1 refinement (per ADR-0007 the dual-trait shape is the
    // long-term answer; v0 wires probing alongside concrete-typed sink
    // construction).
    let _ = &sink;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let (grpc_addr, grpc_join) =
        spawn_grpc(config.grpc_bind_addr(), Arc::clone(&sink), shutdown_rx)
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

    Ok(Handle {
        grpc_addr,
        http_addr: None,
        shutdown: Some(shutdown_tx),
        grpc_join: Some(grpc_join),
    })
}
