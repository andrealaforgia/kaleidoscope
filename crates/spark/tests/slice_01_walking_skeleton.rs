//! Slice 01 — Walking skeleton.
//!
//! Maps to `docs/feature/spark/slices/slice-01-walking-skeleton.md`.
//! Companion stories: US-SP-01.
//!
//! The user-centric outcome: a Rust developer adds Spark to their
//! service, calls `spark::init` with a canonical configuration, records
//! one span via the standard OTel API, lets the returned `SparkGuard`
//! drop, and the recording sink behind a real Aperture instance
//! captures one `ExportTraceServiceRequest` whose `Resource` carries
//! `service.name="payments-api"` and `tenant.id="acme-prod"`.
//!
//! ## Strategy C "real local"
//!
//! Per `discuss/wave-decisions.md > Slice 01`: real Aperture instances
//! at ephemeral loopback ports, `RecordingSink` to capture export
//! traffic, no InMemory transports. The wire is exercised end to end:
//! Spark composes the OTel Resource, the OTel SDK + OTLP/gRPC exporter
//! + tonic transport produces wire bytes, the bytes reach Aperture's
//!   gRPC listener, Aperture's harness validates them, and the
//!   `RecordingSink` captures the typed `SinkRecord`.
//!
//! ## Hexagonal boundary
//!
//! Every test invokes the public `spark::init` driving port — never
//! the private `init.rs::init` function or any other internal module.
//! The OTel SDK calls reach into Spark's substrate via
//! `opentelemetry::global::tracer(...)`, which Spark configures during
//! `init`.
//!
//! ## RED-on-day-one
//!
//! `spark::init` panics with `unimplemented!()` at DISTILL. Every
//! test below therefore panics at the first `init` call. DELIVER
//! replaces the panic with the real orchestration when Slice 01
//! lands. After Slice 01 is GREEN, every assertion in this file holds.
//!
//! ## Mandate Single-Then-Per-Fact
//!
//! Each user-observable claim from US-SP-01's UAT is its own `#[test]`
//! function, so a mutation in `init.rs` or `guard.rs` can only kill
//! one assertion at a time. Mutation testing (Gate 5 of ADR-0011)
//! demands per-fact granularity.
//!
//! ## Single-init reset
//!
//! Slice 02 lands the AtomicBool single-init flag (per ADR-0015 §1).
//! Cargo runs the seven tests in this binary as concurrent threads in
//! one process, so each test needs:
//!
//! 1. `#[serial_test::serial]` to serialise the OTel global tracer
//!    provider replacement (the global's `set_*_provider` is silent-
//!    overwrite at `=0.27`).
//! 2. A pre-test `spark::__reset_for_testing()` call to release the
//!    AtomicBool flag the previous test's `init` consumed.
//!
//! This pattern is the test-side bookend to the production-side
//! invariant: production sets the flag once per process; the test
//! binary resets it between serialised tests.

mod common;

use std::time::Duration;

use aperture::ports::SinkRecord;
use spark::{init, SparkConfig};

use crate::common::{
    capture_spark_events, expect_spark_event_with_message, spawn_aperture_with_recording_sink,
    wait_for, CANONICAL_SERVICE_NAME, CANONICAL_TENANT_ID,
};

// =========================================================================
// Walking skeleton: spark::init returns Ok(SparkGuard) for the canonical
// configuration
// =========================================================================

/// US-SP-01 AC: "spark::init(config) returns Ok(SparkGuard) for the
/// canonical SparkConfig (service.name + tenant.id set, valid
/// endpoint, single-init)."
#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
async fn developer_runs_init_with_canonical_config_and_receives_ok_guard() {
    spark::__reset_for_testing();
    let aperture = spawn_aperture_with_recording_sink().await;

    let result = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .require_tenant_id()
            .with_tenant_id(CANONICAL_TENANT_ID)
            .with_endpoint(aperture.grpc_endpoint()),
    );

    assert!(
        result.is_ok(),
        "spark::init must succeed for the canonical Slice-01 config; got: {:?}",
        result.as_ref().err()
    );
}

// =========================================================================
// One span round-trips end-to-end: emit -> drop -> RecordingSink
// =========================================================================

/// US-SP-01 AC: "An ExportTraceServiceRequest emitted via
/// opentelemetry::global::tracer(...).in_span(...) reaches Aperture's
/// listener and Aperture's RecordingSink records the request."
#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
async fn developer_records_one_span_and_recording_sink_captures_a_traces_export() {
    spark::__reset_for_testing();
    let aperture = spawn_aperture_with_recording_sink().await;

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .require_tenant_id()
            .with_tenant_id(CANONICAL_TENANT_ID)
            .with_endpoint(aperture.grpc_endpoint()),
    )
    .expect("init succeeds for the canonical config");

    // Emit one span via the standard OTel global API.
    {
        let tracer = opentelemetry::global::tracer("ci-runner");
        use opentelemetry::trace::Tracer;
        let _span = tracer.start("walking-skeleton");
    }

    // Drop the guard explicitly to force the synchronous flush.
    drop(guard);

    wait_for(|| !aperture.sink.is_empty(), Duration::from_secs(2)).await;

    let recorded = aperture.sink.drain();
    assert!(
        matches!(recorded.first(), Some(SinkRecord::Traces(_))),
        "RecordingSink should hold one Traces SinkRecord after the span emission; got: {recorded:?}"
    );
}

/// US-SP-01 AC: "The recorded request's
/// ResourceSpans.resource.attributes contains service.name."
#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
async fn developer_records_one_span_and_recording_sink_resource_includes_service_name() {
    spark::__reset_for_testing();
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
        let _span = tracer.start("walking-skeleton");
    }

    drop(guard);

    wait_for(|| !aperture.sink.is_empty(), Duration::from_secs(2)).await;

    let recorded = aperture.sink.drain();
    let traces = match recorded.into_iter().next() {
        Some(SinkRecord::Traces(req)) => req,
        other => panic!("expected one Traces SinkRecord; got {other:?}"),
    };

    let attrs: Vec<(String, String)> = traces
        .resource_spans
        .iter()
        .filter_map(|rs| rs.resource.as_ref())
        .flat_map(|r| r.attributes.iter())
        .filter_map(|kv| {
            let v = kv.value.as_ref()?;
            let s = match &v.value {
                Some(opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(s)) => {
                    s.clone()
                }
                _ => return None,
            };
            Some((kv.key.clone(), s))
        })
        .collect();

    assert!(
        attrs
            .iter()
            .any(|(k, v)| k == "service.name" && v == CANONICAL_SERVICE_NAME),
        "Resource.attributes should contain service.name={CANONICAL_SERVICE_NAME:?}; got {attrs:?}"
    );
}

/// US-SP-01 AC: "The Resource includes tenant.id exactly as set on
/// the SparkConfig when with_tenant_id was called."
#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
async fn developer_records_one_span_and_recording_sink_resource_includes_tenant_id() {
    spark::__reset_for_testing();
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
        let _span = tracer.start("walking-skeleton");
    }

    drop(guard);

    wait_for(|| !aperture.sink.is_empty(), Duration::from_secs(2)).await;

    let recorded = aperture.sink.drain();
    let traces = match recorded.into_iter().next() {
        Some(SinkRecord::Traces(req)) => req,
        other => panic!("expected one Traces SinkRecord; got {other:?}"),
    };

    let attrs: Vec<(String, String)> = traces
        .resource_spans
        .iter()
        .filter_map(|rs| rs.resource.as_ref())
        .flat_map(|r| r.attributes.iter())
        .filter_map(|kv| {
            let v = kv.value.as_ref()?;
            let s = match &v.value {
                Some(opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(s)) => {
                    s.clone()
                }
                _ => return None,
            };
            Some((kv.key.clone(), s))
        })
        .collect();

    assert!(
        attrs
            .iter()
            .any(|(k, v)| k == "tenant.id" && v == CANONICAL_TENANT_ID),
        "Resource.attributes should contain tenant.id={CANONICAL_TENANT_ID:?}; got {attrs:?}"
    );
}

/// US-SP-01 AC: "The recorded request's first ResourceSpans contains
/// exactly one span." The walking skeleton emits one span; the
/// RecordingSink should receive exactly that one.
#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
async fn developer_records_one_span_and_recording_sink_holds_exactly_one_span() {
    spark::__reset_for_testing();
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
        let _span = tracer.start("walking-skeleton");
    }

    drop(guard);

    wait_for(|| !aperture.sink.is_empty(), Duration::from_secs(2)).await;

    let recorded = aperture.sink.drain();
    let traces = match recorded.into_iter().next() {
        Some(SinkRecord::Traces(req)) => req,
        other => panic!("expected one Traces SinkRecord; got {other:?}"),
    };

    let span_count: usize = traces
        .resource_spans
        .iter()
        .flat_map(|rs| rs.scope_spans.iter())
        .map(|ss| ss.spans.len())
        .sum();

    assert_eq!(
        span_count, 1,
        "exactly one span should reach the RecordingSink; got {span_count}"
    );
}

// =========================================================================
// Spark's own diagnostic event reaches the application's tracing facade
// =========================================================================

/// US-SP-01 AC: "A single tracing INFO event with target=\"spark\"
/// and message containing \"spark::init succeeded\" is captured by a
/// subscriber the application configured."
#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
async fn developer_runs_init_and_observes_spark_init_succeeded_event_on_tracing_facade() {
    spark::__reset_for_testing();
    let aperture = spawn_aperture_with_recording_sink().await;
    let capture = capture_spark_events();

    let _guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .require_tenant_id()
            .with_tenant_id(CANONICAL_TENANT_ID)
            .with_endpoint(aperture.grpc_endpoint()),
    )
    .expect("init succeeds");

    let events = capture.events();
    let _ = expect_spark_event_with_message(&events, "spark::init succeeded");
}

// =========================================================================
// SparkConfig is plain data (no I/O before init)
// =========================================================================

/// Journey-spark.feature scenario: "SparkConfig is plain data with
/// no I/O." Building a SparkConfig must not write to any channel
/// (stdout, stderr, tracing) and must not produce any OTLP export.
#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
async fn developer_builds_a_spark_config_and_emits_no_telemetry_before_init() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let capture = capture_spark_events();

    let _config = SparkConfig::for_service(CANONICAL_SERVICE_NAME)
        .require_tenant_id()
        .with_tenant_id(CANONICAL_TENANT_ID)
        .with_endpoint(aperture.grpc_endpoint());

    // Give any errant I/O a moment to manifest.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let events = capture.events();
    assert!(
        events.is_empty(),
        "building a SparkConfig must not emit any tracing events; got {events:?}"
    );
    assert!(
        aperture.sink.is_empty(),
        "building a SparkConfig must not produce any OTLP export; \
         the RecordingSink must be empty"
    );
}
