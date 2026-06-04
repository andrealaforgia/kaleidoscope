//! Composition root — wires the configured sink, runs the Earned-Trust
//! probe, and spawns the listeners.
//!
//! It wires both listeners (the gRPC arm and the HTTP/protobuf arm),
//! the readiness state machine, and the drain orchestrator that
//! graceful shutdown uses.

use std::sync::Arc;

use crate::backpressure::{CapTransport, ConcurrencyLimiter};
use crate::config::{Config, SinkKind};
use crate::observability;
use crate::ports::{OtlpSink, Probe};
use crate::readiness::ReadinessState;
use crate::shutdown::ShutdownBundle;
use crate::sinks::{ForwardingSink, StubSink};
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
            // Run the Earned-Trust probe before erasing the type.
            probe_or_refuse(&sink).await?;
            Ok(Arc::new(sink))
        }
        SinkKind::Forwarding => {
            let sink = ForwardingSink::new(
                config.forwarding_endpoint().to_string(),
                config.forwarding_timeout(),
            );
            probe_or_refuse(&sink).await?;
            Ok(Arc::new(sink))
        }
    }
}

/// Run a sink's Earned-Trust probe; on failure emit
/// `event=health.startup.refused` and surface a startup error.
///
/// The composition root invokes this for every concrete sink it wires.
/// Per ADR-0007 Probe is a separate trait from OtlpSink, so the
/// structural-layer xtask check can verify every concrete `OtlpSink`
/// also has a `Probe` impl by AST inspection. The behavioural-layer
/// gold-test (`tests/probe_gold_runner.rs`) verifies the `Probe` impl
/// genuinely exercises its dependency.
async fn probe_or_refuse<P: Probe + ?Sized>(sink: &P) -> crate::Result<()> {
    if let Err(e) = sink.probe().await {
        tracing::error!(
            event = observability::event::HEALTH_STARTUP_REFUSED,
            reason = %e,
        );
        return Err(ApertureError(format!("sink probe failed: {e}")));
    }
    Ok(())
}

/// Wire the configuration and sink, run the sink probe, bind both the
/// gRPC and HTTP listeners, and return a [`Handle`] for the caller to
/// manage.
///
/// Sink selection: when `config.sink_kind == Stub` the passed `sink` is
/// used as-is (the test path injects `RecordingSink`; the probe is
/// statically `Ok(())`). When `config.sink_kind == Forwarding` the
/// passed sink is **replaced** by a freshly-constructed `ForwardingSink`
/// against the configured downstream endpoint, and the Earned-Trust
/// probe runs against that real client. A failed probe surfaces as
/// `Err(ApertureError)` and emits `event=health.startup.refused` per
/// ADR-0007 / ADR-0009.
pub(crate) async fn spawn(config: Config, sink: Arc<dyn OtlpSink>) -> crate::Result<Handle> {
    observability::install_subscriber();
    tracing::info!(
        event = observability::event::STARTUP,
        version = env!("CARGO_PKG_VERSION"),
    );

    // Forward-compat TLS / SPIFFE knobs (ADR-0008 schema / ADR-0061
    // runtime reaction). The schema still accepts the keys at v0 so
    // configs roll forward into Phase 2 (Aegis) without a schema break,
    // but a config that sets `tls.enabled=true` or
    // `auth.spiffe.enabled=true` never reaches this point: config
    // validation (`RawConfig::into_config`, ADR-0061) refuses to
    // construct a `Config` and aperture exits 2 with
    // `event=config_validation_failed` before any listener binds. By the
    // time `spawn` runs, both knobs are necessarily off — there is no
    // warn-and-bind-plaintext path.

    // Sink selection: configuration is the source of truth for which
    // sink the runtime uses. The test path constructs `RecordingSink`
    // and passes it through; the production path with
    // `sink_kind=forwarding` swaps in a real `ForwardingSink` against
    // the configured downstream and runs its probe before any listener
    // binds.
    let sink: Arc<dyn OtlpSink> = match config.sink_kind() {
        SinkKind::Stub => sink,
        SinkKind::Forwarding => {
            let forwarding = ForwardingSink::new(
                config.forwarding_endpoint().to_string(),
                config.forwarding_timeout(),
            );
            probe_or_refuse(&forwarding).await?;
            Arc::new(forwarding)
        }
    };

    // Two oneshots so each transport's `serve_with_graceful_shutdown`
    // can own its own receiver. Slice 08 will replace this with a
    // `broadcast` sender wired through the shutdown orchestrator.
    let (grpc_shutdown_tx, grpc_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let (http_shutdown_tx, http_shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let readiness = ReadinessState::new();

    // Per-transport concurrency limiters. ADR-0010: independent
    // semaphores per transport so a saturated gRPC fleet does not
    // starve the HTTP listener and vice versa. The cap is shared by
    // both transports at v0 (single configuration knob).
    let grpc_limiter =
        ConcurrencyLimiter::new(config.max_concurrent_requests(), CapTransport::Grpc);
    let http_limiter =
        ConcurrencyLimiter::new(config.max_concurrent_requests(), CapTransport::HttpProtobuf);

    let (grpc_addr, grpc_join) = spawn_grpc(
        config.grpc_bind_addr(),
        Arc::clone(&sink),
        Arc::clone(&readiness),
        grpc_limiter.clone(),
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
        http_limiter.clone(),
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

    let bundle = ShutdownBundle {
        readiness: Arc::clone(&readiness),
        grpc_limiter,
        http_limiter,
        grpc_shutdown: grpc_shutdown_tx,
        http_shutdown: http_shutdown_tx,
        grpc_join,
        http_join,
        drain_deadline: config.drain_deadline(),
    };

    Ok(Handle {
        grpc_addr,
        http_addr,
        bundle: Some(bundle),
    })
}
