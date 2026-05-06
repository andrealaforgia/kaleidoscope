//! Slice 06 — Bounded flush deadline.
//!
//! Maps to `docs/feature/spark/slices/slice-06-flush-deadline.md`.
//! Companion stories: US-SP-06.
//!
//! The user-centric outcome: a developer of a short-running CLI tool,
//! a k8s pod, or any application with a process exit experiences zero
//! silent data loss; deadline-exceeded events are loud, never silent.
//!
//! ## Three sub-cases
//!
//! - Case A — clean flush within deadline. INFO event with message
//!   containing `"shutdown complete drained=unknown"`.
//! - Case B — deadline exceeded with slow downstream. WARN event with
//!   message containing `"flush deadline exceeded"` and prefix
//!   `"dropped="`. Drop completes within ~the configured deadline.
//! - Case C — downed downstream (no listener). The drop does NOT
//!   panic; the test process exits zero.
//!
//! ## Path A — drained/dropped report `unknown` at v0
//!
//! Per ADR-0014 §2 and `discuss/user-stories.md > Changed
//! Assumptions` (commit 25e3732): `opentelemetry_sdk =0.27` does NOT
//! expose drained/dropped record counts publicly. The v0 vocabulary
//! contract is the *prefix* `drained=` / `dropped=`; the *value* is
//! the literal `unknown` until the SDK exposes the counters. Tests
//! below assert the prefix, accepting `unknown` as the value.
//! Hardcoded integer assertions would lock-in a contract DESIGN
//! deliberately re-shaped via Path A.
//!
//! ## RED-on-day-one
//!
//! Every test calls `spark::init` which panics with
//! `unimplemented!()` at DISTILL.

mod common;

use std::time::Duration;
use std::time::Instant;

use spark::{init, SparkConfig};

use crate::common::{
    capture_spark_events, expect_spark_event_with_message, spawn_aperture_with_recording_sink,
    CANONICAL_SERVICE_NAME, CANONICAL_TENANT_ID,
};

// =========================================================================
// Case A — clean flush within the configured deadline
// =========================================================================

/// US-SP-06 UAT: "SparkGuard drop flushes pending exports within the
/// configured deadline." On clean flush a single INFO event with
/// message containing `"shutdown complete"` is emitted.
#[tokio::test(flavor = "multi_thread")]
async fn developer_drops_guard_with_healthy_aperture_and_observes_shutdown_complete_info_event() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let capture = capture_spark_events();

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .require_tenant_id()
            .with_tenant_id(CANONICAL_TENANT_ID)
            .with_endpoint(aperture.grpc_endpoint()),
    )
    .expect("init succeeds with healthy Aperture");

    {
        use opentelemetry::trace::Tracer;
        let tracer = opentelemetry::global::tracer("ci-runner");
        let _span = tracer.start("walking-skeleton");
    }
    drop(guard);

    let events = capture.events();
    let _ = expect_spark_event_with_message(&events, "shutdown complete");
}

/// US-SP-06 AC + Path A: the clean-flush INFO event's message
/// contains the prefix `"drained="`. The value is `unknown` at v0
/// (per ADR-0014 §2) — the prefix is the contract, the value is the
/// SDK-exposed count if available or the literal `unknown` otherwise.
#[tokio::test(flavor = "multi_thread")]
async fn developer_drops_guard_with_healthy_aperture_and_shutdown_complete_message_carries_drained_prefix(
) {
    let aperture = spawn_aperture_with_recording_sink().await;
    let capture = capture_spark_events();

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .require_tenant_id()
            .with_tenant_id(CANONICAL_TENANT_ID)
            .with_endpoint(aperture.grpc_endpoint()),
    )
    .expect("init succeeds");

    {
        use opentelemetry::trace::Tracer;
        let tracer = opentelemetry::global::tracer("ci-runner");
        let _span = tracer.start("op");
    }
    drop(guard);

    let events = capture.events();
    let evt = expect_spark_event_with_message(&events, "shutdown complete");
    assert!(
        evt.message_contains("drained="),
        "shutdown-complete message should carry the prefix 'drained='; got: {:?}",
        evt.message
    );
}

/// US-SP-06 AC: "The drop completes within the configured
/// flush_timeout_ms." Use a comfortable upper bound so a healthy
/// Aperture's flush completes well within it.
#[tokio::test(flavor = "multi_thread")]
async fn developer_drops_guard_with_healthy_aperture_and_drop_completes_within_default_timeout() {
    let aperture = spawn_aperture_with_recording_sink().await;

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .require_tenant_id()
            .with_tenant_id(CANONICAL_TENANT_ID)
            .with_endpoint(aperture.grpc_endpoint()),
    )
    .expect("init succeeds");

    {
        use opentelemetry::trace::Tracer;
        let tracer = opentelemetry::global::tracer("ci-runner");
        let _span = tracer.start("op");
    }

    let started = Instant::now();
    drop(guard);
    let elapsed = started.elapsed();

    // Default flush_timeout is 5 s. A healthy Aperture should flush
    // in well under that. Allow 6 s tolerance for CI scheduling jitter.
    assert!(
        elapsed < Duration::from_secs(6),
        "drop should complete within the default 5 s flush deadline; took {elapsed:?}"
    );
}

// =========================================================================
// Case B — deadline exceeded with a non-listening endpoint (proxy for
// "slow downstream")
// =========================================================================
//
// The Slice 06 mockup describes "Aperture configured to delay every
// accept by 10 seconds" for the slow-downstream case. At DISTILL the
// fixture for that exact behaviour is deliberately deferred — a
// non-listening endpoint produces the same observable outcome at the
// Spark boundary (the export cannot complete before the deadline; the
// WARN event is emitted; the drop completes within ~the deadline).
// DELIVER's Slice 06 implementation may upgrade this to a wiremock-
// style fixture if the SDK's connection-failure path differs from a
// genuinely-slow accept path.

/// US-SP-06 UAT: "SparkGuard drop emits a deadline-exceeded warning
/// when downstream is slow." Asserted as: a WARN-or-INFO event whose
/// message contains `"flush deadline exceeded"`.
#[tokio::test(flavor = "multi_thread")]
async fn developer_drops_guard_pointed_at_unreachable_endpoint_and_observes_flush_deadline_exceeded_event(
) {
    // Pick a port number that's unlikely to be in use (high range)
    // and configure a tight deadline so the test runs fast.
    let unreachable = "http://127.0.0.1:1";
    let capture = capture_spark_events();

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .with_endpoint(unreachable)
            .with_flush_timeout(Duration::from_millis(500)),
    )
    .expect("init succeeds even if the target is unreachable (the exporter discovers this lazily)");

    {
        use opentelemetry::trace::Tracer;
        let tracer = opentelemetry::global::tracer("ci-runner");
        let _span = tracer.start("op");
    }
    drop(guard);

    let events = capture.events();
    let _ = expect_spark_event_with_message(&events, "flush deadline exceeded");
}

/// US-SP-06 AC + Path A: the deadline-exceeded WARN event's message
/// contains the prefix `"dropped="`. The value is `unknown` at v0
/// (per ADR-0014 §2).
#[tokio::test(flavor = "multi_thread")]
async fn developer_drops_guard_pointed_at_unreachable_endpoint_and_warn_message_carries_dropped_prefix(
) {
    let unreachable = "http://127.0.0.1:1";
    let capture = capture_spark_events();

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .with_endpoint(unreachable)
            .with_flush_timeout(Duration::from_millis(500)),
    )
    .expect("init succeeds");

    {
        use opentelemetry::trace::Tracer;
        let tracer = opentelemetry::global::tracer("ci-runner");
        let _span = tracer.start("op");
    }
    drop(guard);

    let events = capture.events();
    let evt = expect_spark_event_with_message(&events, "flush deadline exceeded");
    assert!(
        evt.message_contains("dropped="),
        "deadline-exceeded message should carry the prefix 'dropped='; got: {:?}",
        evt.message
    );
}

/// US-SP-06 AC: "the WARN event names the dropped count" — Path A
/// settles this as "the prefix is `dropped=`, the value is `unknown`
/// until the SDK exposes it". Assertion accepts `unknown` as the
/// value verbatim.
#[tokio::test(flavor = "multi_thread")]
async fn developer_drops_guard_pointed_at_unreachable_endpoint_and_warn_message_dropped_value_is_unknown_or_integer(
) {
    let unreachable = "http://127.0.0.1:1";
    let capture = capture_spark_events();

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .with_endpoint(unreachable)
            .with_flush_timeout(Duration::from_millis(500)),
    )
    .expect("init succeeds");

    {
        use opentelemetry::trace::Tracer;
        let tracer = opentelemetry::global::tracer("ci-runner");
        let _span = tracer.start("op");
    }
    drop(guard);

    let events = capture.events();
    let evt = expect_spark_event_with_message(&events, "flush deadline exceeded");

    // Per Path A: the value after `dropped=` is `unknown` at v0; a
    // future SDK release that exposes the counter switches to an
    // integer without breaking the prefix contract. The test accepts
    // either shape.
    let after_prefix = evt
        .message
        .split("dropped=")
        .nth(1)
        .expect("'dropped=' prefix must appear in message");
    let value_token = after_prefix.split_whitespace().next().unwrap_or("");
    let is_unknown = value_token == "unknown";
    let is_integer = value_token.chars().all(|c| c.is_ascii_digit()) && !value_token.is_empty();
    assert!(
        is_unknown || is_integer,
        "the value after 'dropped=' should be 'unknown' (v0 per ADR-0014 §2) or an integer; got {value_token:?} in {:?}",
        evt.message
    );
}

/// US-SP-06 AC: "no Drop blocks indefinitely." With a 500 ms deadline,
/// the drop completes within ~500 ms (with tolerance for the OTel
/// SDK's internal scheduling).
#[tokio::test(flavor = "multi_thread")]
async fn developer_drops_guard_with_short_deadline_and_drop_completes_close_to_deadline() {
    let unreachable = "http://127.0.0.1:1";

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .with_endpoint(unreachable)
            .with_flush_timeout(Duration::from_millis(500)),
    )
    .expect("init succeeds");

    {
        use opentelemetry::trace::Tracer;
        let tracer = opentelemetry::global::tracer("ci-runner");
        let _span = tracer.start("op");
    }

    let started = Instant::now();
    drop(guard);
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_secs(2),
        "drop with a 500 ms deadline must not block indefinitely; took {elapsed:?}"
    );
}

// =========================================================================
// Case C — down downstream: drop does NOT panic
// =========================================================================

/// US-SP-06 UAT: "SparkGuard drop does not panic on a downed
/// downstream." The test passing (no panic, normal exit) IS the
/// assertion.
#[tokio::test(flavor = "multi_thread")]
async fn developer_drops_guard_with_no_listener_at_endpoint_and_drop_does_not_panic() {
    let unreachable = "http://127.0.0.1:1";

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .with_endpoint(unreachable)
            .with_flush_timeout(Duration::from_millis(500)),
    )
    .expect("init succeeds");

    {
        use opentelemetry::trace::Tracer;
        let tracer = opentelemetry::global::tracer("ci-runner");
        let _span = tracer.start("op");
    }

    // The act of dropping the guard MUST NOT panic. If it did, this
    // `#[test]` would propagate the panic and the test would fail.
    // The assertion is implicit in normal test exit.
    drop(guard);
}

// =========================================================================
// Idempotent drop: a second drop is a no-op
// =========================================================================

/// US-SP-06 AC: "A second drop on the same guard is a no-op." This
/// is exercised structurally: explicit `drop(guard)` followed by
/// scope-exit does not double-flush. Asserted via the captured-event
/// list — exactly one `shutdown initiated` event.
#[tokio::test(flavor = "multi_thread")]
async fn developer_calls_drop_explicitly_and_observes_exactly_one_shutdown_initiated_event() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let capture = capture_spark_events();

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .require_tenant_id()
            .with_tenant_id(CANONICAL_TENANT_ID)
            .with_endpoint(aperture.grpc_endpoint()),
    )
    .expect("init succeeds");

    drop(guard); // explicit first drop runs Drop::drop
                 // scope exit would normally drop the guard again — but the
                 // value has already been moved by the explicit drop, so the
                 // compiler will not emit a second drop. The structural
                 // assertion below confirms the observable outcome.

    let events = capture.events();
    let initiated = events
        .iter()
        .filter(|e| e.message_contains("shutdown initiated"))
        .count();
    assert_eq!(
        initiated, 1,
        "exactly one 'shutdown initiated' event should be emitted across the guard's lifetime; got {initiated} in {:?}",
        events.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

/// US-SP-06 UAT: "drop(guard) called explicitly is equivalent to
/// scope-exit drop." The flush behaviour is identical; one INFO
/// event with message containing `"shutdown complete"` is captured.
#[tokio::test(flavor = "multi_thread")]
async fn developer_calls_drop_explicitly_and_observes_shutdown_complete_event_just_like_scope_exit()
{
    let aperture = spawn_aperture_with_recording_sink().await;
    let capture = capture_spark_events();

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .require_tenant_id()
            .with_tenant_id(CANONICAL_TENANT_ID)
            .with_endpoint(aperture.grpc_endpoint()),
    )
    .expect("init succeeds");

    drop(guard);

    let events = capture.events();
    let _ = expect_spark_event_with_message(&events, "shutdown complete");
}
