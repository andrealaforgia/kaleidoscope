//! Shared test helpers for Spark's slice-level acceptance tests.
//!
//! These helpers mirror Aperture's `tests/common/mod.rs` shape (real
//! Aperture instance on ephemeral loopback ports, `RecordingSink` for
//! assertion, structured-event capture for the `target="spark"`
//! tracing vocabulary).
//!
//! ## Strategy C "real local"
//!
//! Per `docs/feature/spark/discuss/wave-decisions.md > Slice 01` and
//! `docs/feature/spark/design/wave-decisions.md > Constraints
//! established for downstream waves > For DISTILL`: every integration
//! test spawns a real Aperture instance via
//! `aperture::spawn(Config::for_test())` and uses
//! `aperture::testing::RecordingSink`. No `InMemoryExporter`, no
//! `InMemorySpanExporter`, no synthetic transports masquerading as
//! the wire.
//!
//! ## DISTILL state
//!
//! `spark::init` panics with `unimplemented!()` at the day-one stub.
//! Every helper that calls into the production crate panics at
//! runtime. That panic is the canonical RED state — DELIVER drives one
//! panic away per slice, in order.

#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use aperture::config::Config;
use aperture::ports::OtlpSink;
use aperture::testing::RecordingSink;
use aperture::Handle;

// =========================================================================
// Aperture test instance lifecycle
// =========================================================================
//
// Spark sends OTLP/gRPC payloads to Aperture; the Slice 01 walking
// skeleton is a real round-trip through the wire to a freshly-launched
// Aperture, with `RecordingSink` capturing what reached the sink.
//
// The fixture below spawns Aperture on the loopback interface with an
// ephemeral port (so tests can run in parallel without contending for
// :4317), waits for `/readyz` to flip to Ready, and returns the handle
// + the recording sink. Drop semantics: the `ApertureFixture` triggers
// graceful shutdown on drop with Aperture's default 30 s drain
// deadline.

/// Process-global init-serialisation lock.
///
/// Slice 02 lands an `AtomicBool` (`SPARK_INITIALISED` in `init.rs`)
/// that enforces the single-init invariant per process (ADR-0015 §1).
/// Slice 03 onward exercises **multiple successful** `spark::init`
/// calls per test binary (ten distinct tests, each driving one full
/// init→span→drop cycle). Without serialisation + reset, every test
/// after the first observes `Err(GlobalAlreadyInitialised)` because
/// the AtomicBool is sticky for the lifetime of the process.
///
/// The remedy here is the test-infrastructure equivalent of the
/// `#[serial_test::serial]` + `spark::__reset_for_testing()` pattern
/// Slice 01's tests apply at the test-function level: every fixture
/// acquires a shared mutex and resets the AtomicBool inside the
/// critical section. Holding the mutex on the [`ApertureFixture`] for
/// the entire test body (released on `Drop`) guarantees one
/// init→drop cycle at a time across the binary's parallel test
/// runner — without modifying the contract test files Atlas locked
/// at DISTILL.
///
/// The `invariant_single_init.rs` binary calls this fixture exactly
/// once and then `init` twice; the reset on the *first* fixture call
/// only resets the (un-set) flag, so the binary's "first init succeeds,
/// second returns `GlobalAlreadyInitialised`" assertion remains
/// deterministic.
static SPARK_INIT_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A live Aperture instance bound to ephemeral loopback ports, fronted
/// by a [`RecordingSink`] the Spark integration tests can interrogate.
pub struct ApertureFixture {
    pub handle: Handle,
    pub sink: Arc<RecordingSink>,
    /// Held for the lifetime of the fixture so concurrent tests in the
    /// same `[[test]]`-declared binary serialise their `spark::init`
    /// calls. Released when the fixture drops (i.e. at test-function
    /// scope exit).
    _init_lock: std::sync::MutexGuard<'static, ()>,
}

impl ApertureFixture {
    /// The address Aperture's gRPC listener bound to (ephemeral port).
    /// Spark's `SparkConfig::with_endpoint` value points here.
    pub fn grpc_addr(&self) -> SocketAddr {
        self.handle.grpc_addr()
    }

    /// The OTLP/gRPC endpoint URL Spark's tests pass to
    /// `SparkConfig::with_endpoint`. Includes the `http://` scheme so
    /// Spark's URI parser is exercised.
    pub fn grpc_endpoint(&self) -> String {
        format!("http://{}", self.handle.grpc_addr())
    }
}

/// Spawn a real Aperture instance with default ephemeral-port
/// configuration and a fresh [`RecordingSink`]. The most common
/// fixture for Spark's slice tests.
///
/// Per the Strategy C "real local" posture: this spawns a real
/// Aperture, not a mock or stub. The `RecordingSink` is the
/// assertion seam — Spark emits OTLP/gRPC bytes that travel through
/// Aperture's harness and land in the sink as typed `SinkRecord`
/// values.
///
/// Side-effect: the first call in a test binary's process installs
/// the Spark capture subscriber so any `target="spark"` events emitted
/// by `spark::init` reach [`CAPTURED_EVENTS`]. The install must beat
/// Aperture's own subscriber install (which `aperture::spawn` performs
/// via a `try_init` that silently no-ops if a subscriber is already
/// set). Tests that interleave `capture_spark_events()` and
/// `spawn_aperture_with_recording_sink()` see the Spark events
/// regardless of ordering because both helpers install through the
/// same [`Once`] gate.
pub async fn spawn_aperture_with_recording_sink() -> ApertureFixture {
    INSTALL_SUBSCRIBER.call_once(install_spark_capture_subscriber);

    let config = Config::builder()
        .grpc_bind_addr(
            "127.0.0.1:0"
                .parse()
                .expect("loopback ipv4 with ephemeral port parses"),
        )
        .http_bind_addr(
            "127.0.0.1:0"
                .parse()
                .expect("loopback ipv4 with ephemeral port parses"),
        )
        .build()
        .expect("default Aperture test config builds");

    let sink = Arc::new(RecordingSink::new());
    let sink_dyn: Arc<dyn OtlpSink> = sink.clone();
    let handle = aperture::spawn(config, sink_dyn)
        .await
        .expect("aperture::spawn must succeed for the Spark integration fixture");
    handle
        .wait_until_ready()
        .await
        .expect("aperture readiness probe must reach Ready before Spark drives traffic at it");

    // Acquire the process-global init-serialisation lock and reset
    // Spark's per-process AtomicBool single-init flag inside the
    // critical section. The lock is acquired *after* all `.await`
    // points (`std::sync::Mutex` is not async-aware; holding it
    // across an `.await` would trip clippy's `await_holding_lock`).
    // Held for the lifetime of the returned fixture so concurrent
    // tests in the same binary serialise their `spark::init` calls —
    // which happen synchronously in the test body, after the fixture
    // returns.
    let init_lock = SPARK_INIT_SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    spark::__reset_for_testing();

    ApertureFixture {
        handle,
        sink,
        _init_lock: init_lock,
    }
}

// =========================================================================
// Tracing-event capture for `target="spark"`
// =========================================================================
//
// Spark emits its own diagnostics through the application's tracing
// facade (D5 — no telemetry on telemetry). The integration tests
// subscribe to a capture layer for the duration of a closure and assert
// against the captured events.
//
// At DISTILL the capture mechanism is declared as a stable function
// signature; the implementation lives in the test helper module and
// uses a `tracing-subscriber` registry layer that routes events with
// `target="spark"` into a thread-local Vec. Per ADR-0015 §2: each
// `[[test]]`-declared binary runs as its own process, so the
// thread-local capture is process-isolated and tests within one binary
// run sequentially via `serial_test` where needed.

use std::sync::{Mutex, Once};

use tracing::field::{Field, Visit};
use tracing::Subscriber;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{Layer, Registry};

/// A captured `tracing` event from `target="spark"`. The fields
/// captured at DISTILL are the level (INFO/WARN/ERROR), the message
/// (the formatted-record string a `tracing-subscriber` Layer would
/// see), and the structured fields as a JSON value.
#[derive(Debug, Clone)]
pub struct SparkEvent {
    pub level: String,
    pub message: String,
    pub fields: serde_json::Value,
}

impl SparkEvent {
    /// Convenience accessor: returns true if the event's message
    /// contains the given substring. Used by the slice tests' Then
    /// steps (e.g. `event.message_contains("spark::init succeeded")`).
    pub fn message_contains(&self, needle: &str) -> bool {
        self.message.contains(needle)
    }
}

/// Process-global storage for captured Spark `tracing` events.
///
/// One mutex-guarded Vec per test process (per-binary isolation per
/// ADR-0015 §2). Events accumulate while a [`CaptureGuard`] is held.
static CAPTURED_EVENTS: Mutex<Vec<SparkEvent>> = Mutex::new(Vec::new());

/// Process-global subscriber install gate. The Spark capture layer is
/// a `tracing_subscriber` Layer attached to a `Registry` set as the
/// global default once per process. Subsequent `capture_spark_events`
/// calls just clear the buffer; the subscriber stays installed for
/// the lifetime of the test binary.
static INSTALL_SUBSCRIBER: Once = Once::new();

/// RAII guard that begins a capture session on construction and ends
/// it on drop. The captured events are returned via
/// [`CaptureGuard::events`] and are cleared on drop so the next
/// capture session starts clean.
pub struct CaptureGuard {
    _private: (),
}

impl CaptureGuard {
    /// Snapshot the captured events so far. Cloned out from under the
    /// mutex; tests assert against the snapshot.
    pub fn events(&self) -> Vec<SparkEvent> {
        CAPTURED_EVENTS
            .lock()
            .expect("captured-events mutex poisoned")
            .clone()
    }
}

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        CAPTURED_EVENTS
            .lock()
            .expect("captured-events mutex poisoned")
            .clear();
    }
}

/// Begin capturing `tracing` events with `target="spark"` for the
/// lifetime of the returned [`CaptureGuard`].
///
/// The first call in a test binary's process installs a
/// `tracing_subscriber::Registry` with a [`SparkCaptureLayer`] filter
/// on `target="spark"` as the global default subscriber. Subsequent
/// calls clear the [`CAPTURED_EVENTS`] buffer and return a fresh
/// guard; the subscriber stays installed (subscribers are
/// install-once per process by `tracing` design).
///
/// The capture mechanism is process-global; tests within one binary
/// that need concurrent captures must serialise (the
/// `[[test]]`-per-binary scheme of ADR-0015 §2 means each test gets a
/// pristine process, so cross-binary capture is naturally isolated).
pub fn capture_spark_events() -> CaptureGuard {
    INSTALL_SUBSCRIBER.call_once(install_spark_capture_subscriber);
    CAPTURED_EVENTS
        .lock()
        .expect("captured-events mutex poisoned")
        .clear();
    CaptureGuard { _private: () }
}

/// Install the global `tracing_subscriber::Registry` with a
/// [`SparkCaptureLayer`] as the only attached layer. Idempotent via
/// the [`INSTALL_SUBSCRIBER`] [`Once`] gate.
fn install_spark_capture_subscriber() {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    let _ = Registry::default().with(SparkCaptureLayer).try_init();
}

/// `tracing_subscriber::Layer` that pushes any event with
/// `target="spark"` into the process-global [`CAPTURED_EVENTS`]
/// buffer. Events with other targets are ignored.
struct SparkCaptureLayer;

impl<S> Layer<S> for SparkCaptureLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        if event.metadata().target() != "spark" {
            return;
        }
        let mut visitor = SparkEventVisitor::default();
        event.record(&mut visitor);
        let level = event.metadata().level().to_string();
        let message = visitor.message.unwrap_or_default();
        let fields = serde_json::Value::Object(visitor.fields);
        CAPTURED_EVENTS
            .lock()
            .expect("captured-events mutex poisoned")
            .push(SparkEvent {
                level,
                message,
                fields,
            });
    }
}

/// `tracing::field::Visit` implementation that extracts the special
/// `message` field as the event's display message and accumulates
/// every other field as a JSON map. The visitor produces the typed
/// shape the [`SparkEvent`] struct exposes.
#[derive(Default)]
struct SparkEventVisitor {
    message: Option<String>,
    fields: serde_json::Map<String, serde_json::Value>,
}

impl Visit for SparkEventVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_owned());
        } else {
            self.fields.insert(
                field.name().to_owned(),
                serde_json::Value::String(value.to_owned()),
            );
        }
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .insert(field.name().to_owned(), serde_json::Value::Bool(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.insert(
            field.name().to_owned(),
            serde_json::Value::Number(value.into()),
        );
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.insert(
            field.name().to_owned(),
            serde_json::Value::Number(value.into()),
        );
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        let n = serde_json::Number::from_f64(value).unwrap_or_else(|| serde_json::Number::from(0));
        self.fields
            .insert(field.name().to_owned(), serde_json::Value::Number(n));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let formatted = format!("{value:?}");
        if field.name() == "message" {
            self.message = Some(formatted);
        } else {
            self.fields.insert(
                field.name().to_owned(),
                serde_json::Value::String(formatted),
            );
        }
    }
}

/// Assert the captured events contain at least one event with a
/// message containing the given substring. Returns the first matching
/// event so the caller can drill in on its other fields. Panics with
/// a diagnostic if no match is found.
pub fn expect_spark_event_with_message(events: &[SparkEvent], needle: &str) -> SparkEvent {
    events
        .iter()
        .find(|e| e.message_contains(needle))
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "expected a captured spark event whose message contains {needle:?}; \
                 got events: {:?}",
                events.iter().map(|e| &e.message).collect::<Vec<_>>()
            )
        })
}

/// Assert NONE of the captured events match the given substring.
/// Panics if any match is found.
pub fn expect_no_spark_event_with_message(events: &[SparkEvent], needle: &str) {
    if let Some(found) = events.iter().find(|e| e.message_contains(needle)) {
        panic!(
            "expected no captured spark event matching {needle:?}; \
             found: {found:?}"
        );
    }
}

// =========================================================================
// Wait helpers
// =========================================================================

/// Poll `predicate` every 25 ms for up to `deadline`. Useful for
/// scenarios that need to await an asynchronous side-effect (e.g. an
/// `ExportTraceServiceRequest` landing in the `RecordingSink` after
/// the OTel SDK's batch processor flushes).
pub async fn wait_for<F: Fn() -> bool>(predicate: F, deadline: Duration) {
    let started = std::time::Instant::now();
    while !predicate() {
        if started.elapsed() > deadline {
            panic!("wait_for predicate did not become true within {deadline:?}");
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

// =========================================================================
// Common test data — house attribute values shared across slices
// =========================================================================
//
// Per `shared-artifacts-registry.md`, the v0 example values are
// realistic, not placeholders. The same string literals appear in the
// journey-spark.yaml mockups, the user-stories.md examples, and the
// slice files; centralising them here ensures the tests assert against
// the registry contract verbatim.

/// The canonical service.name used in the walking skeleton and
/// downstream slices (per `journey-spark.yaml > shared_artifacts >
/// service_name`).
pub const CANONICAL_SERVICE_NAME: &str = "payments-api";

/// The canonical tenant.id used in the walking skeleton and
/// downstream slices (per `journey-spark.yaml > shared_artifacts >
/// tenant_id`).
pub const CANONICAL_TENANT_ID: &str = "acme-prod";

/// The canonical experiment.id used in Slice 03 onward (per the
/// `discuss/journey-spark.feature` scenarios).
pub const CANONICAL_EXPERIMENT_ID: &str = "exp-2026-Q2-pricing";

/// The canonical feature_flag pair used in Slice 03 onward.
pub const CANONICAL_FEATURE_FLAG_KEY: &str = "checkout-v2";
pub const CANONICAL_FEATURE_FLAG_VALUE: &str = "on";
