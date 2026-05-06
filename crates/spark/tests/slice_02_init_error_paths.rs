//! Slice 02 — Init error paths.
//!
//! Maps to `docs/feature/spark/slices/slice-02-init-error-paths.md`.
//! Companion stories: US-SP-02.
//!
//! The user-centric outcome: a developer who introduces a
//! misconfiguration into a Spark-instrumented service receives a
//! precise, named diagnostic at `spark::init` time, before any
//! telemetry is emitted. Three of the four `SparkError` variants are
//! covered here:
//! - `MissingRequiredAttribute { name }` — for missing or empty
//!   `service.name` and missing or empty `tenant.id` after
//!   `require_tenant_id()`.
//! - `InvalidEndpoint { endpoint, reason }` — for unparseable URIs
//!   and non-http(s) schemes.
//! - `ExporterInitFailed { reason, source }` — declared on the public
//!   surface; reachability at v0 is via test scaffolding only (the
//!   variant exists for forward-compat).
//!
//! The fourth variant (`GlobalAlreadyInitialised`) lives in its own
//! `[[test]]`-declared binary (`tests/invariant_single_init.rs`) per
//! ADR-0015 §2 — touching the OTel global state would race with the
//! other tests in this binary if it lived here.
//!
//! ## Substring assertions on Display
//!
//! Per `design/wave-decisions.md > Constraints established for
//! downstream waves > For DISTILL §5`: error tests assert
//! `error.to_string().contains("...")` rather than matching the
//! entire `Display` line. The exact `Display` strings are locked in
//! ADR-0012; renames are version-bump.
//!
//! ## RED-on-day-one
//!
//! `spark::init` panics with `unimplemented!()` at DISTILL. Every
//! test below panics at the first `init` call. DELIVER replaces the
//! panic with the lint pass when Slice 02 lands.

mod common;

use std::time::Duration;

use spark::{init, SparkConfig, SparkError};

use crate::common::{
    capture_spark_events, expect_no_spark_event_with_message, spawn_aperture_with_recording_sink,
    CANONICAL_SERVICE_NAME,
};

// =========================================================================
// MissingRequiredAttribute — tenant.id absent
// =========================================================================

/// US-SP-02 UAT: "spark::init refuses missing required tenant.id with
/// a precise error."
#[tokio::test(flavor = "multi_thread")]
async fn developer_calls_init_with_require_tenant_id_but_no_tenant_id_and_receives_missing_required_attribute_error(
) {
    let result = init(SparkConfig::for_service(CANONICAL_SERVICE_NAME).require_tenant_id());

    assert!(
        matches!(
            result,
            Err(SparkError::MissingRequiredAttribute { ref name }) if name == "tenant.id"
        ),
        "expected MissingRequiredAttribute {{ name: \"tenant.id\" }}; got {:?}",
        result.err()
    );
}

/// The Display message for the missing-tenant.id variant must name
/// the attribute key. Per ADR-0012, the substring "tenant.id" is
/// the contract.
#[tokio::test(flavor = "multi_thread")]
async fn developer_reads_missing_tenant_id_error_display_and_finds_tenant_id_substring() {
    let err = init(SparkConfig::for_service(CANONICAL_SERVICE_NAME).require_tenant_id())
        .expect_err("init must reject missing tenant.id");

    let display = err.to_string();
    assert!(
        display.contains("tenant.id"),
        "Display output must contain the attribute name 'tenant.id'; got: {display:?}"
    );
}

// =========================================================================
// MissingRequiredAttribute — tenant.id empty string
// =========================================================================

/// US-SP-02 UAT: "spark::init refuses empty-string tenant.id with
/// the same error as missing." Empty strings are treated identically
/// to absence.
#[tokio::test(flavor = "multi_thread")]
async fn developer_calls_init_with_empty_string_tenant_id_and_receives_missing_required_attribute_error(
) {
    let result = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .require_tenant_id()
            .with_tenant_id(""),
    );

    assert!(
        matches!(
            result,
            Err(SparkError::MissingRequiredAttribute { ref name }) if name == "tenant.id"
        ),
        "expected MissingRequiredAttribute {{ name: \"tenant.id\" }}; got {:?}",
        result.err()
    );
}

// =========================================================================
// MissingRequiredAttribute — service.name empty (defence-in-depth)
// =========================================================================

/// US-SP-02 AC: "spark::init returns Err(SparkError::
/// MissingRequiredAttribute { name: \"service.name\" }) if the
/// SparkConfig was somehow constructed with an empty service.name."
/// The constructor takes a non-Option String, so this path is
/// defence-in-depth.
#[tokio::test(flavor = "multi_thread")]
async fn developer_calls_init_with_empty_service_name_and_receives_missing_required_attribute_error(
) {
    let result = init(SparkConfig::for_service(""));

    assert!(
        matches!(
            result,
            Err(SparkError::MissingRequiredAttribute { ref name }) if name == "service.name"
        ),
        "expected MissingRequiredAttribute {{ name: \"service.name\" }}; got {:?}",
        result.err()
    );
}

// =========================================================================
// InvalidEndpoint — unparseable URI
// =========================================================================

/// US-SP-02 UAT: "spark::init refuses an invalid endpoint with a
/// precise diagnostic." A typo in the scheme produces a parse-failure
/// the `reason` field names.
#[tokio::test(flavor = "multi_thread")]
async fn developer_calls_init_with_typo_in_endpoint_scheme_and_receives_invalid_endpoint_error() {
    let result =
        init(SparkConfig::for_service(CANONICAL_SERVICE_NAME).with_endpoint("htp://typo:4317"));

    assert!(
        matches!(result, Err(SparkError::InvalidEndpoint { .. })),
        "expected InvalidEndpoint variant; got {:?}",
        result.err()
    );
}

/// The InvalidEndpoint variant's `endpoint` field carries the literal
/// value Spark attempted to use, so the application can log the
/// problematic input verbatim.
#[tokio::test(flavor = "multi_thread")]
async fn developer_calls_init_with_invalid_endpoint_and_error_carries_literal_endpoint() {
    let bad_endpoint = "htp://typo:4317";
    let err = init(SparkConfig::for_service(CANONICAL_SERVICE_NAME).with_endpoint(bad_endpoint))
        .expect_err("init must reject the invalid endpoint");

    match err {
        SparkError::InvalidEndpoint { endpoint, .. } => {
            assert_eq!(
                endpoint, bad_endpoint,
                "InvalidEndpoint.endpoint should hold the literal input"
            );
        }
        other => panic!("expected InvalidEndpoint; got {other:?}"),
    }
}

/// The InvalidEndpoint variant's `reason` field names the parse
/// failure. Substring assertion on a non-empty reason; the exact
/// wording is DESIGN-locked but the field must not be empty.
#[tokio::test(flavor = "multi_thread")]
async fn developer_calls_init_with_invalid_endpoint_and_reason_field_is_non_empty() {
    let err =
        init(SparkConfig::for_service(CANONICAL_SERVICE_NAME).with_endpoint("htp://typo:4317"))
            .expect_err("init must reject the invalid endpoint");

    match err {
        SparkError::InvalidEndpoint { reason, .. } => {
            assert!(
                !reason.is_empty(),
                "InvalidEndpoint.reason should describe the parse failure; got empty"
            );
        }
        other => panic!("expected InvalidEndpoint; got {other:?}"),
    }
}

/// The Display output for InvalidEndpoint mentions the literal
/// endpoint value (locked in ADR-0012 — `write!(f, "spark: invalid
/// endpoint {endpoint:?}: {reason}")`).
#[tokio::test(flavor = "multi_thread")]
async fn developer_reads_invalid_endpoint_display_and_finds_endpoint_substring() {
    let err =
        init(SparkConfig::for_service(CANONICAL_SERVICE_NAME).with_endpoint("htp://typo:4317"))
            .expect_err("init must reject the invalid endpoint");

    let display = err.to_string();
    assert!(
        display.contains("typo"),
        "Display should reference the bad endpoint substring 'typo'; got: {display:?}"
    );
}

// =========================================================================
// Negative case: SparkConfig without require_tenant_id() succeeds
// =========================================================================

/// US-SP-02 UAT: "spark::init accepts a SparkConfig without
/// require_tenant_id() and no with_tenant_id." The opt-in tenant.id
/// posture preserves single-tenant adopters.
#[tokio::test(flavor = "multi_thread")]
async fn developer_calls_init_without_require_tenant_id_and_receives_ok_without_tenant_id() {
    let aperture = spawn_aperture_with_recording_sink().await;

    let result = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME).with_endpoint(aperture.grpc_endpoint()),
    );

    assert!(
        result.is_ok(),
        "spark::init must succeed for a SparkConfig without require_tenant_id; got: {:?}",
        result.as_ref().err()
    );
}

// =========================================================================
// No side effects on Err paths
// =========================================================================

/// US-SP-02 AC: "On any Err return, no OTLP exporter is constructed,
/// no global provider is set, no telemetry reaches any backend." The
/// 'spark::init succeeded' tracing event is the structural witness;
/// its absence on Err paths confirms no SDK construction happened.
#[tokio::test(flavor = "multi_thread")]
async fn developer_calls_init_with_invalid_config_and_no_init_succeeded_event_is_emitted() {
    let capture = capture_spark_events();

    // Trigger the missing-tenant.id error.
    let _ = init(SparkConfig::for_service(CANONICAL_SERVICE_NAME).require_tenant_id());

    // Give any errant tracing event a moment to be emitted.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let events = capture.events();
    expect_no_spark_event_with_message(&events, "spark::init succeeded");
}

/// US-SP-02 AC: "On any Err return ... no telemetry reaches any
/// backend." The RecordingSink behind a real Aperture must remain
/// empty after a failed init.
#[tokio::test(flavor = "multi_thread")]
async fn developer_calls_init_with_invalid_endpoint_and_no_export_reaches_recording_sink() {
    let aperture = spawn_aperture_with_recording_sink().await;

    let _ = init(SparkConfig::for_service(CANONICAL_SERVICE_NAME).with_endpoint("htp://typo:4317"));

    // Give any errant export a moment to be flushed.
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert!(
        aperture.sink.is_empty(),
        "no OTLP export must reach the RecordingSink after a failed init"
    );
}
