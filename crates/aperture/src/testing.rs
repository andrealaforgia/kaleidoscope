//! Test doubles for integration tests.
//!
//! Per `docs/feature/aperture/design/component-design.md > Test
//! doubles`, [`RecordingSink`] is the seam US-AP-03's "custom OtlpSink
//! plugs in without crate-level changes" UAT writes against. It is the
//! smallest possible witness that the trait IS the integration surface.
//!
//! The [`stderr_capture`] helper subscribes a layer to the production
//! `tracing-subscriber` registry that records every event emitted while
//! the supplied closure runs. Integration tests use it to assert
//! against the closed v0 event vocabulary without parsing JSON out of
//! file descriptors.

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::observability::CapturedEvent;
use crate::ports::{OtlpSink, Probe, ProbeError, SinkError, SinkRecord};
use crate::sinks::ForwardingSink;

/// In-memory sink: every accepted record is appended to a vector.
///
/// `RecordingSink` is the test double the integration tests use to
/// observe what Aperture's application core hands off through the
/// `OtlpSink` trait. It records the records — nothing else.
pub struct RecordingSink {
    inner: Mutex<Vec<SinkRecord>>,
}

impl Default for RecordingSink {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordingSink {
    /// Construct an empty recorder.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Vec::new()),
        }
    }

    /// Snapshot of every record accepted so far. Clones the records
    /// out from under the mutex; tests assert against the snapshot.
    pub fn drain(&self) -> Vec<SinkRecord> {
        let mut g = self.inner.lock().expect("recording-sink mutex poisoned");
        std::mem::take(&mut *g)
    }

    /// Number of records accepted so far without removing them.
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("recording-sink mutex poisoned")
            .len()
    }

    /// Convenience predicate; equivalent to `len() == 0`.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl OtlpSink for RecordingSink {
    fn accept<'a>(
        &'a self,
        record: SinkRecord,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), SinkError>> + Send + 'a>> {
        Box::pin(async move {
            // A `RecordingSink` substitutes for `StubSink` at the
            // hexagonal seam (DISTILL D2). The integration tests assert
            // against the production-bound stderr line
            // `event=sink_accepted sink=stub` — so the recording sink
            // emits the same line shape on accept (via the shared
            // `emit_sink_accepted` helper). This keeps the hexagonal
            // substitution observable across all three signals.
            crate::sinks::emit_sink_accepted("stub", &record);
            self.inner
                .lock()
                .expect("recording-sink mutex poisoned")
                .push(record);
            Ok(())
        })
    }
}

impl Probe for RecordingSink {
    fn probe<'a>(
        &'a self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), ProbeError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

// =========================================================================
// stderr capture seam
// =========================================================================

/// A captured tracing event observed during a [`stderr_capture`] call.
#[derive(Debug, Clone)]
pub struct StderrEvent {
    pub level: String,
    pub event: String,
    pub fields: serde_json::Value,
}

impl From<CapturedEvent> for StderrEvent {
    fn from(c: CapturedEvent) -> Self {
        Self {
            level: c.level,
            event: c.event,
            fields: c.fields,
        }
    }
}

// =========================================================================
// Probe gold-test factory
// =========================================================================
//
// The Earned-Trust probe contract has three semantically-orthogonal
// enforcement layers (ADR-0007). Layer 3 — behavioural enforcement —
// is the gold-test (`tests/probe_gold_runner.rs`) that drives a real
// `ForwardingSink::probe()` against a lying wiremock fixture. The
// factory below is the seam the gold-test enters through; it returns
// the concrete sink as an `Arc<dyn Probe>` so the test can call
// `probe()` without going through `aperture::spawn` (the shorter the
// driving-port-to-driven-port path, the harder the gold-test is to
// fake).

/// Construct a real `ForwardingSink` against the given downstream
/// endpoint and return its `Probe` view.
///
/// The gold-test (`tests/probe_gold_runner.rs`) calls this and then
/// invokes `probe()` directly against a wiremock fixture. The whole
/// point of the gold-test is to verify the probe genuinely exercises
/// the network — a `probe { Ok(()) }` placeholder would not produce
/// any HTTP traffic against the fixture and the assertion would fail.
pub fn forwarding_sink_probe_for_gold_test(endpoint: String, timeout: Duration) -> Arc<dyn Probe> {
    Arc::new(ForwardingSink::new(endpoint, timeout))
}

// =========================================================================
// Serve-failure injection seam (DISTILL scaffold; DELIVER implements)
// =========================================================================
//
// `aperture-serve-loop-error-surfacing-v0` makes a post-bind serving-loop
// death surface instead of being swallowed (`let _ = server.await;` at
// `transport.rs:93` for gRPC; `let _ = axum::serve(...).await` at
// `:153-157` for HTTP). To make that failure FALSIFIABLE in-suite, the
// acceptance tests need to drive a real spawned transport whose serve
// future resolves to `Err` (or an unexpected early `Ok`) POST-BIND, with
// NO shutdown requested — the aperture analogue of cinder's
// `FailingFsyncBackend`.
//
// No such seam exists on the public surface today: `spawn_grpc` /
// `spawn_http` build the real `tonic` / `axum` serve future internally,
// and `ServeOutcome` / `ReadinessPhase` / `ShutdownBundle` are
// `pub(crate)` and unreachable from the `tests/` crate. This stub is the
// MINIMAL seam DELIVER must implement (ADR-0066 "Test seam" (ii),
// brief.md "For Acceptance Designer"). It is a `#[cfg(...)]`-free public
// test helper kept beside `RecordingSink` and `stderr_capture` so the
// integration tests can name it; DELIVER replaces the `unimplemented!`
// body with the real injection (e.g. a spawn helper that, behind the
// already-bound listener, resolves the serve future to `Err` without
// setting the `shutdown_requested` flag, so the task self-reacts:
// emit `serve_loop_failed`, `flip_to_failed`, fold `ServeOutcome::Failed`
// → exit code 3).

/// Which transport's serving loop the injected failure should kill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectServeFailure {
    /// Make the gRPC serving loop resolve to `Err` post-bind.
    Grpc,
    /// Make the HTTP serving loop resolve to `Err` post-bind.
    Http,
    /// Make a serving loop return `Ok` early, with NO shutdown
    /// requested (the unexpected-early-`Ok` D3 case; fatal at v0).
    GrpcEarlyOk,
}

/// Spawn a real Aperture instance whose named serving loop is made to
/// die POST-BIND (resolve `Err`, or early `Ok`) with NO shutdown
/// requested, so the production self-reaction fires: one
/// `event=serve_loop_failed` line on stderr, `/readyz` → 503 `"failed"`,
/// `/healthz` stays 200, and the process verdict folds to exit code 3.
///
/// This is the acceptance-layer falsifiability seam (ADR-0066 Test seam
/// (ii)). The listeners bind first (so `/readyz`/`/healthz` are
/// probeable over the wire), then the named transport's serve future is
/// forced to fail.
///
/// DELIVER implements this. DISTILL only declares it so the
/// serve-failure acceptance tests can name it and run RED (a captured
/// `serve_loop_failed`, a 503 `/readyz`, an exit-3 verdict) against the
/// present swallow, which surfaces nothing.
pub async fn spawn_with_injected_serve_failure(
    config: crate::config::Config,
    sink: Arc<dyn OtlpSink>,
    which: InjectServeFailure,
) -> Result<crate::Handle, crate::ApertureError> {
    // Bind REAL listeners through the production spawn path, capturing
    // the same readiness handle the running listeners hold so the
    // injected death flips the real `/readyz` an over-the-wire probe
    // reads.
    let (handle, readiness) = crate::compose::spawn_with_readiness(config, sink).await?;

    // Drive the PRODUCTION self-reaction (`resolve_serve_outcome`) for
    // the named transport, with NO shutdown requested. This emits the
    // real `serve_loop_failed` line and flips readiness to the sticky
    // `Failed` phase — exactly the code a genuine post-bind death runs.
    // The seam fakes only the trigger; everything downstream is real.
    let (transport, early_ok) = match which {
        InjectServeFailure::Grpc => ("grpc", false),
        InjectServeFailure::Http => ("http", false),
        InjectServeFailure::GrpcEarlyOk => ("grpc", true),
    };
    let _outcome = crate::transport::inject_serve_failure(transport, &readiness, early_ok);

    Ok(handle)
}

/// Run the supplied async closure with a fresh capture layer
/// subscribed to the tracing registry. Returns the closure's value
/// alongside every event the closure emitted.
///
/// The capture layer is process-global; concurrent captures are not
/// supported (the integration test harness runs sequentially under
/// `RUST_TEST_THREADS=1`).
pub async fn stderr_capture<F, Fut, R>(f: F) -> (R, Vec<StderrEvent>)
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = R>,
{
    crate::observability::begin_capture();
    let value = f().await;
    let captured = crate::observability::end_capture();
    let events = captured.into_iter().map(StderrEvent::from).collect();
    (value, events)
}
