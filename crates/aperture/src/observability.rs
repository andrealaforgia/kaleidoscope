//! Observability — closed v0 event-name vocabulary plus a
//! `tracing-subscriber` JSON layer and a test-side capture seam.
//!
//! See `docs/feature/aperture/design/component-design.md > Observability
//! design` and ADR-0009 for the contract.
//!
//! Slice 01 lights up: `LISTENER_BOUND`, `REQUEST_RECEIVED`,
//! `SINK_ACCEPTED`. Subsequent slices grow the call sites against the
//! same closed vocabulary.

use std::sync::{Mutex, OnceLock};

use serde_json::{Map, Value};
use tracing::{
    field::{Field, Visit},
    Event, Subscriber,
};
use tracing_subscriber::{layer::Context, prelude::*, registry::LookupSpan, EnvFilter, Layer};

/// Closed v0 event-name set (DISCUSS D1 + four DESIGN-derived names).
///
/// The `xtask single-validator-per-signal` and the integration tests
/// match against these literal strings; renames are version-bump-able,
/// additions are non-breaking.
///
/// Constants not yet referenced by Slice 01's call sites are kept here
/// (the closed vocabulary is the design contract; Slice 02–08 light up
/// the rest) under `#[allow(dead_code)]`.
#[allow(dead_code)]
pub mod event {
    pub const STARTUP: &str = "startup";
    pub const LISTENER_BOUND: &str = "listener_bound";
    pub const LISTENER_CLOSING: &str = "listener_closing";
    pub const LISTENER_BIND_FAILED: &str = "listener_bind_failed";
    pub const READY: &str = "ready";
    pub const READINESS_CHANGED: &str = "readiness_changed";
    pub const REQUEST_RECEIVED: &str = "request_received";
    pub const SINK_ACCEPTED: &str = "sink_accepted";
    pub const SINK_FAILED: &str = "sink_failed";
    pub const SHUTDOWN_INITIATED: &str = "shutdown_initiated";
    pub const SHUTDOWN_COMPLETE: &str = "shutdown_complete";
    pub const IN_FLIGHT_DRAINED: &str = "in_flight_drained";
    pub const DRAIN_DEADLINE_EXCEEDED: &str = "drain_deadline_exceeded";
    pub const UNSUPPORTED_MEDIA_TYPE: &str = "unsupported_media_type";
    pub const BODY_TOO_LARGE: &str = "body_too_large";
    pub const CONCURRENCY_CAP_HIT: &str = "concurrency_cap_hit";
    pub const TLS_NOT_SUPPORTED_IN_V0: &str = "tls_not_supported_in_v0";
    pub const HEALTH_STARTUP_REFUSED: &str = "health.startup.refused";
    pub const CONFIG_VALIDATION_FAILED: &str = "config_validation_failed";
    pub const INTERNAL_INVARIANT_VIOLATION: &str = "internal_invariant_violation";
}

/// A captured structured-log line.
///
/// Mirrors the shape `tests/common::StderrEvent` declares. The capture
/// layer parses each tracing `Event` into this form so integration tests
/// can interrogate fields without parsing JSON-on-the-wire.
#[derive(Debug, Clone)]
pub struct CapturedEvent {
    pub level: String,
    pub event: String,
    pub fields: Value,
}

/// Init the production tracing subscriber: JSON layer to stderr plus
/// the capture layer. Idempotent: subsequent calls are no-ops.
///
/// The capture layer is always present in the registry; it only
/// records events when [`begin_capture`] has installed a buffer. This
/// means the production subscriber and the test capture share one
/// `tracing` registry — initialising twice (which `try_init()` would
/// silently no-op) does not race or shadow either path.
pub fn init_logging() {
    install_subscriber();
}

// =========================================================================
// Capture layer — used by `aperture::testing::stderr_capture`
// =========================================================================
//
// The capture seam is a `tracing-subscriber::Layer` that records every
// event emitted while a buffer is installed. Tests subscribe a fresh
// buffer for the duration of an async closure and then drain the
// recorded events.

/// Shared capture sink. Holds zero-or-more event vectors; multiple
/// concurrent captures are not supported in v0 (the integration tests
/// run sequentially under `RUST_TEST_THREADS=1`).
type CaptureSink = Mutex<Option<Vec<CapturedEvent>>>;

fn capture_sink() -> &'static CaptureSink {
    static SINK: OnceLock<CaptureSink> = OnceLock::new();
    SINK.get_or_init(|| Mutex::new(None))
}

/// Tracing layer that records every event into the global capture
/// sink, when one is installed.
pub(crate) struct CaptureLayer;

impl<S> Layer<S> for CaptureLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut guard = match capture_sink().lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let buffer = match guard.as_mut() {
            Some(b) => b,
            None => return,
        };
        let mut visitor = JsonVisitor::default();
        event.record(&mut visitor);
        let event_name = visitor
            .fields
            .get("event")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        buffer.push(CapturedEvent {
            level: event.metadata().level().to_string().to_lowercase(),
            event: event_name.to_string(),
            fields: Value::Object(visitor.fields),
        });
    }
}

/// Begin capturing events. Returns once the buffer is installed.
///
/// Tests typically run an async closure between `begin_capture()` and
/// `end_capture()` — see `aperture::testing::stderr_capture`.
pub(crate) fn begin_capture() {
    install_subscriber();
    if let Ok(mut guard) = capture_sink().lock() {
        *guard = Some(Vec::new());
    }
}

/// Stop capturing and return whatever events have been recorded.
pub(crate) fn end_capture() -> Vec<CapturedEvent> {
    capture_sink()
        .lock()
        .ok()
        .and_then(|mut g| g.take())
        .unwrap_or_default()
}

/// Install the global subscriber. Idempotent: the registry holds the
/// JSON-stderr layer AND the capture layer; either a production call
/// to [`init_logging`] or a test call to [`begin_capture`] is enough
/// to install both.
fn install_subscriber() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let filter =
            EnvFilter::try_from_env("APERTURE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_writer(std::io::stderr)
                    .flatten_event(true)
                    .with_current_span(false)
                    .with_span_list(false)
                    .with_target(false),
            )
            .with(CaptureLayer)
            .try_init();
    });
}

#[derive(Default)]
struct JsonVisitor {
    fields: Map<String, Value>,
}

impl Visit for JsonVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .insert(field.name().to_string(), Value::String(value.to_string()));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields
            .insert(field.name().to_string(), Value::Number(value.into()));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .insert(field.name().to_string(), Value::Number(value.into()));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .insert(field.name().to_string(), Value::Bool(value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        if let Some(n) = serde_json::Number::from_f64(value) {
            self.fields
                .insert(field.name().to_string(), Value::Number(n));
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields.insert(
            field.name().to_string(),
            Value::String(format!("{value:?}")),
        );
    }
}
