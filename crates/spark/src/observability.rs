//! `observability` — the centralised `target="spark"` tracing
//! vocabulary.
//!
//! Per ADR-0011 §"Internal layout" + the C4 Component diagram: every
//! `tracing::info!` and `tracing::warn!` invocation Spark makes flows
//! through this module. Centralising matters for two reasons:
//!
//! 1. Test substring assertions (Slice 02: "the error message
//!    contains 'tenant.id'", Slice 06: "shutdown complete drained=N")
//!    need a single source of truth for the literals.
//! 2. Future renames are one-file edits.
//!
//! ## Vocabulary (locked by `shared-artifacts-registry.md >
//! spark_log_event_vocabulary`)
//!
//! - `INFO  target="spark"  message="spark::init succeeded"  service.name=... endpoint=... protocol=grpc flush_timeout_ms=...`
//! - `INFO  target="spark"  message="spark: shutdown initiated"  flush_timeout_ms=...`
//! - `INFO  target="spark"  message="spark: shutdown complete drained=unknown"`
//! - `WARN  target="spark"  message="spark: flush deadline exceeded dropped=unknown flush_timeout_ms=..."`
//! - `ERROR target="spark"  message="spark: exporter initialisation failed: ..."`
//!
//! Per ADR-0014 §2 (Path A applied to DISCUSS): `drained=unknown` /
//! `dropped=unknown` at v0 because `opentelemetry_sdk =0.27` does not
//! expose the counters publicly. The `drained=` / `dropped=` *prefix*
//! is the contract; the *value* is `unknown` until the SDK exposes it.
//!
//! Slice 01 lands [`emit_init_succeeded`]. Slice 06 lands the
//! shutdown / flush-deadline events.

#![allow(dead_code)]

use std::time::Duration;

/// The closed prefix every `tracing` event Spark emits carries on its
/// target field. Asserted verbatim by the integration tests.
pub(crate) const TARGET: &str = "spark";

/// The `feature_flag.` resource-attribute namespace prefix. Single
/// source of truth (per `slice-mapping.md > Slice 03 implementation
/// pointers`). The literal does not appear elsewhere in the crate.
pub(crate) const FEATURE_FLAG_PREFIX: &str = "feature_flag.";

/// Emit the `spark::init succeeded` INFO event with the resolved
/// configuration's structured fields.
///
/// Per Slice 04 UAT "Resolved configuration is observable on the
/// tracing facade": the event carries `service.name`, `endpoint`,
/// `protocol`, `flush_timeout_ms`. Slice 01 emits the message and
/// the four fields; Slice 04 asserts the structured-field content.
pub(crate) fn emit_init_succeeded(
    service_name: &str,
    endpoint: &str,
    protocol: &str,
    flush_timeout: Duration,
) {
    let flush_timeout_ms = flush_timeout.as_millis() as u64;
    tracing::info!(
        target: TARGET,
        service_name = service_name,
        endpoint = endpoint,
        protocol = protocol,
        flush_timeout_ms = flush_timeout_ms,
        "spark::init succeeded",
    );
}

/// Emit the `spark: shutdown initiated` INFO event at the start of
/// `SparkGuard::Drop`. Names the configured `flush_timeout_ms`.
pub(crate) fn emit_shutdown_initiated(_flush_timeout: Duration) {
    unimplemented!("emit_shutdown_initiated — DELIVER fills in the tracing::info! invocation when Slice 06 lands")
}

/// Emit the `spark: shutdown complete drained=unknown` INFO event
/// when the per-provider sequential flush completes within the
/// deadline. Per ADR-0014 §2: at v0 the literal value is `unknown`.
pub(crate) fn emit_shutdown_complete() {
    unimplemented!("emit_shutdown_complete — DELIVER fills in the tracing::info! invocation when Slice 06 lands")
}

/// Emit the `spark: flush deadline exceeded dropped=unknown
/// flush_timeout_ms=...` WARN event when any provider hits the
/// deadline or returns a non-Ok flush outcome. Per ADR-0014 §2: at
/// v0 the literal value is `unknown`.
pub(crate) fn emit_flush_deadline_exceeded(_flush_timeout: Duration) {
    unimplemented!("emit_flush_deadline_exceeded — DELIVER fills in the tracing::warn! invocation when Slice 06 lands")
}
