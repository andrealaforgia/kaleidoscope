//! Slice 04 — OTel-canonical env-var precedence.
//!
//! Maps to `docs/feature/spark/slices/slice-04-env-var-precedence.md`.
//! Companion stories: US-SP-04.
//!
//! The user-centric outcome: an operator deploying a Spark-instrumented
//! service to a different region sets
//! `OTEL_EXPORTER_OTLP_ENDPOINT=http://aperture.eu-west.acme.internal:4317`
//! in the deployment manifest, the application binary is unchanged,
//! and Spark targets the EU-west Aperture without a rebuild. If the
//! application also sets `SparkConfig::with_endpoint`, the explicit
//! builder value wins.
//!
//! ## Resolution chain (DISCUSS-locked)
//!
//! 1. `SparkConfig::with_endpoint` (highest)
//! 2. `OTEL_EXPORTER_OTLP_ENDPOINT` env var
//! 3. Spark default `http://localhost:4317`
//!
//! ## serial_test
//!
//! Per `design/wave-decisions.md > Constraints established for
//! downstream waves > For DISTILL §4`: every test in this binary
//! mutates `OTEL_*` env vars via `std::env::set_var`, which is
//! process-global. Without `#[serial]`, parallel tests within this
//! binary would race their env-var assignments. The `[[test]]` per-
//! binary scheme of ADR-0015 §2 isolates Slice 04's env mutations
//! from the other slices' processes.

mod common;

use std::time::Duration;

use serial_test::serial;
use spark::{init, SparkConfig};

use crate::common::{
    capture_spark_events, expect_spark_event_with_message, spawn_aperture_with_recording_sink,
    wait_for, CANONICAL_SERVICE_NAME,
};

const ENV_OTLP_ENDPOINT: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";

/// Helper: clear the env vars Spark reads, run the body, then clear
/// again. Idempotent on entry and exit so a panicking test cannot
/// poison the next test in the same process.
fn with_clean_otel_env<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    std::env::remove_var(ENV_OTLP_ENDPOINT);
    let result = f();
    std::env::remove_var(ENV_OTLP_ENDPOINT);
    result
}

// =========================================================================
// Case A: SparkConfig::with_endpoint takes precedence over the env var
// =========================================================================

/// US-SP-04 UAT: "SparkConfig::with_endpoint takes precedence over
/// OTEL_EXPORTER_OTLP_ENDPOINT." The exporter targets the explicit
/// value; the resolved-config tracing event names the explicit value.
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn developer_sets_with_endpoint_explicitly_and_resolved_event_names_explicit_value() {
    with_clean_otel_env(|| {
        std::env::set_var(ENV_OTLP_ENDPOINT, "http://env-endpoint:4317");
    });

    let aperture = spawn_aperture_with_recording_sink().await;
    let explicit = aperture.grpc_endpoint();
    let capture = capture_spark_events();

    let _guard =
        init(SparkConfig::for_service(CANONICAL_SERVICE_NAME).with_endpoint(explicit.clone()))
            .expect("init succeeds");

    let events = capture.events();
    let evt = expect_spark_event_with_message(&events, "spark::init succeeded");
    let endpoint_field = evt
        .fields
        .get("endpoint")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    assert_eq!(
        endpoint_field.as_deref(),
        Some(explicit.as_str()),
        "resolved-config event should name the explicit endpoint; got fields: {:?}",
        evt.fields
    );

    std::env::remove_var(ENV_OTLP_ENDPOINT);
}

// =========================================================================
// Case B: env var honoured when SparkConfig::with_endpoint is absent
// =========================================================================

/// US-SP-04 UAT: "OTEL_EXPORTER_OTLP_ENDPOINT is honoured when
/// SparkConfig::with_endpoint is not called." The exporter targets
/// the env value.
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn operator_sets_env_endpoint_and_resolved_event_names_env_value() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let env_value = aperture.grpc_endpoint();

    with_clean_otel_env(|| {
        std::env::set_var(ENV_OTLP_ENDPOINT, &env_value);
    });

    let capture = capture_spark_events();

    let _guard = init(SparkConfig::for_service(CANONICAL_SERVICE_NAME))
        .expect("init succeeds with env-resolved endpoint");

    let events = capture.events();
    let evt = expect_spark_event_with_message(&events, "spark::init succeeded");
    let endpoint_field = evt
        .fields
        .get("endpoint")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    assert_eq!(
        endpoint_field.as_deref(),
        Some(env_value.as_str()),
        "resolved-config event should name the env-supplied endpoint; got fields: {:?}",
        evt.fields
    );

    std::env::remove_var(ENV_OTLP_ENDPOINT);
}

/// Round-trip witness: the env-supplied endpoint must reach
/// Aperture's listener. If the env var were ignored, the export would
/// target the default `http://localhost:4317` and the RecordingSink
/// behind the env-supplied Aperture would stay empty.
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn operator_sets_env_endpoint_and_export_reaches_env_targeted_aperture() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let env_value = aperture.grpc_endpoint();

    with_clean_otel_env(|| {
        std::env::set_var(ENV_OTLP_ENDPOINT, &env_value);
    });

    let guard = init(SparkConfig::for_service(CANONICAL_SERVICE_NAME))
        .expect("init succeeds with env-resolved endpoint");
    {
        use opentelemetry::trace::Tracer;
        let tracer = opentelemetry::global::tracer("ci-runner");
        let _span = tracer.start("op");
    }
    drop(guard);

    wait_for(|| !aperture.sink.is_empty(), Duration::from_secs(2)).await;
    assert!(
        !aperture.sink.is_empty(),
        "the export must reach the env-targeted Aperture's RecordingSink"
    );

    std::env::remove_var(ENV_OTLP_ENDPOINT);
}

// =========================================================================
// Case C: default fallback when neither is set
// =========================================================================

/// US-SP-04 UAT: "Spark defaults to http://localhost:4317 when
/// neither config nor env var is set." The resolved-config event
/// names the default.
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn developer_runs_init_with_no_endpoint_config_and_resolved_event_names_default_localhost() {
    with_clean_otel_env(|| {
        // Ensure no env var is set; we expect the default fallback.
    });

    let capture = capture_spark_events();

    // Init may fail because nothing is listening at the default; the
    // resolved-config event should still be emitted before any wire
    // attempt. If DESIGN locks "emit before exporter constructed",
    // the event lands; otherwise the test reveals a contract gap.
    let _ = init(SparkConfig::for_service(CANONICAL_SERVICE_NAME));

    // Give any tracing event a moment to land.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let events = capture.events();
    let evt = expect_spark_event_with_message(&events, "spark::init succeeded");
    let endpoint_field = evt
        .fields
        .get("endpoint")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    assert_eq!(
        endpoint_field.as_deref(),
        Some("http://localhost:4317"),
        "resolved-config event should name the default localhost endpoint; got fields: {:?}",
        evt.fields
    );
}

// =========================================================================
// Case D: resolved-config tracing event has the structured field set
// =========================================================================

/// US-SP-04 UAT: "Resolved configuration is observable on the
/// tracing facade" — the event carries service.name as a structured
/// field.
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn developer_runs_init_and_resolved_event_carries_service_name_field() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let capture = capture_spark_events();

    let _guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME).with_endpoint(aperture.grpc_endpoint()),
    )
    .expect("init succeeds");

    let events = capture.events();
    let evt = expect_spark_event_with_message(&events, "spark::init succeeded");
    let svc = evt
        .fields
        .get("service.name")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    assert_eq!(
        svc.as_deref(),
        Some(CANONICAL_SERVICE_NAME),
        "resolved-config event should name service.name as a field; got fields: {:?}",
        evt.fields
    );
}

/// US-SP-04 UAT (cont.): the event carries protocol="grpc".
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn developer_runs_init_and_resolved_event_carries_protocol_grpc_field() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let capture = capture_spark_events();

    let _guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME).with_endpoint(aperture.grpc_endpoint()),
    )
    .expect("init succeeds");

    let events = capture.events();
    let evt = expect_spark_event_with_message(&events, "spark::init succeeded");
    let protocol = evt
        .fields
        .get("protocol")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    assert_eq!(
        protocol.as_deref(),
        Some("grpc"),
        "resolved-config event should name protocol=grpc; got fields: {:?}",
        evt.fields
    );
}

/// US-SP-04 UAT (cont.): the event carries flush_timeout_ms as a
/// structured numeric field.
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn developer_runs_init_and_resolved_event_carries_flush_timeout_ms_field() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let capture = capture_spark_events();

    let _guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME).with_endpoint(aperture.grpc_endpoint()),
    )
    .expect("init succeeds");

    let events = capture.events();
    let evt = expect_spark_event_with_message(&events, "spark::init succeeded");
    let timeout_ms = evt.fields.get("flush_timeout_ms").and_then(|v| v.as_u64());
    assert!(
        timeout_ms.is_some(),
        "resolved-config event should name flush_timeout_ms as a numeric field; got fields: {:?}",
        evt.fields
    );
}
