//! Slice 07 — Codex schema-lint integration.
//!
//! Maps to `docs/feature/spark/slices/slice-07-codex-schema-lint.md`
//! and ADR-0025 (Codex–Spark integration).
//!
//! Spark's `init` validates the composed resource attributes against
//! Codex's `SchemaCatalogue` before any OTel SDK type is constructed.
//! Default mode emits one `tracing::warn!(target = "spark", ...)`
//! event per misconfigured init carrying the full `LintReport` via
//! its `Display` rendering. Strict mode (opt-in via
//! `SparkConfig::with_strict_schema_lint(true)`) returns
//! `Err(SparkError::SchemaValidation(report))` so CI integration
//! tests catch misconfigurations as fail-fast.
//!
//! ## Driving a violation through Spark's public API
//!
//! Spark hardcodes its blessed resource-attribute keys (`service.name`,
//! `tenant.id`, `experiment.id`, `feature_flag.{key}`). The only
//! organic path to a malformed key from the public API is an empty
//! key in `with_feature_flags`: an empty key composes to the literal
//! `feature_flag.` (the prefix with no suffix), which Codex's
//! `Prefix` matcher correctly rejects (its `name.len() > blessed.len()`
//! check refuses empty suffixes per ADR-0022 §2 / `catalogue.rs`).
//!
//! The empty-feature_flag-key case is therefore both a plausible
//! operator misconfiguration (a typo in a templating system that
//! produces empty keys) and a deterministic driver for the Codex
//! integration tests.

mod common;

use spark::{init, SparkConfig, SparkError};

use crate::common::{
    capture_spark_events, expect_no_spark_event_with_message, expect_spark_event_with_message,
    spawn_aperture_with_recording_sink, CANONICAL_SERVICE_NAME, CANONICAL_TENANT_ID,
};

// =========================================================================
// Test 1 — happy path: blessed attributes pass, no warn event
// =========================================================================

/// ADR-0025 §3 default mode happy path: when every composed resource
/// attribute is blessed by Codex's catalogue, `init` returns
/// `Ok(SparkGuard)` and emits no `schema validation failed` warn
/// event.
#[tokio::test(flavor = "multi_thread")]
async fn happy_path_with_blessed_attributes_passes_warn_mode_silently() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let capture = capture_spark_events();

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .require_tenant_id()
            .with_tenant_id(CANONICAL_TENANT_ID)
            .with_feature_flags([("checkout-v2", "on")])
            .with_endpoint(aperture.grpc_endpoint()),
    )
    .expect("init succeeds with all-blessed attributes");

    // Drop guard to flush; events captured below.
    drop(guard);

    let events = capture.events();
    expect_no_spark_event_with_message(&events, "schema validation failed");
}

// =========================================================================
// Test 2 — default mode: violation emits warn, init still succeeds
// =========================================================================

/// ADR-0025 §3 default mode: a misconfigured resource attribute
/// (here, an empty `feature_flag.` suffix produced by an empty
/// `with_feature_flags` key) emits a single
/// `tracing::warn!(target = "spark")` event whose body matches
/// `"schema validation failed:"` and contains the offending
/// attribute name. `init` still returns `Ok(SparkGuard)` so the
/// rollout is non-breaking.
#[tokio::test(flavor = "multi_thread")]
async fn empty_feature_flag_key_emits_warn_in_default_mode_and_init_succeeds() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let capture = capture_spark_events();

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .require_tenant_id()
            .with_tenant_id(CANONICAL_TENANT_ID)
            // Empty key composes to `feature_flag.` (unblessed: empty
            // suffix violates Codex's Prefix non-empty-suffix rule).
            .with_feature_flags([("", "on")])
            .with_endpoint(aperture.grpc_endpoint()),
    )
    .expect("default mode: init succeeds despite schema violation");

    drop(guard);

    let events = capture.events();
    let event = expect_spark_event_with_message(&events, "schema validation failed");
    assert_eq!(event.level, "WARN");
    assert!(
        event.message.contains("feature_flag."),
        "warn event should name the offending attribute key; got: {:?}",
        event.message
    );
}

// =========================================================================
// Test 3 — strict mode: violation returns Err, no SDK construction
// =========================================================================

/// ADR-0025 §3 strict mode: the same violation that emits a warn
/// in default mode returns `Err(SparkError::SchemaValidation(report))`
/// when `with_strict_schema_lint(true)` is set. `report`'s `Display`
/// rendering matches the warn-mode body byte-for-byte (per ADR-0025
/// §3 / §6).
#[tokio::test(flavor = "multi_thread")]
async fn empty_feature_flag_key_returns_err_in_strict_mode() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let _capture = capture_spark_events();

    let result = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .require_tenant_id()
            .with_tenant_id(CANONICAL_TENANT_ID)
            .with_feature_flags([("", "on")])
            .with_endpoint(aperture.grpc_endpoint())
            .with_strict_schema_lint(true),
    );

    match result {
        Err(SparkError::SchemaValidation(report)) => {
            let rendered = format!("{report}");
            assert!(
                rendered.starts_with("schema validation failed:"),
                "Display rendering must start with 'schema validation failed:'; got: {rendered:?}"
            );
            assert!(
                rendered.contains("feature_flag."),
                "Display rendering must name the offending attribute key; got: {rendered:?}"
            );
        }
        Err(other) => {
            panic!("expected SparkError::SchemaValidation in strict mode; got: {other:?}")
        }
        Ok(_) => {
            panic!("expected Err(SchemaValidation) in strict mode with malformed attribute; got Ok")
        }
    }
}

// =========================================================================
// Test 4 — happy path under strict mode: all blessed → Ok
// =========================================================================

/// ADR-0025 §3 strict mode + happy path: when every attribute is
/// blessed, strict mode returns `Ok(SparkGuard)` (the strict knob
/// only fires on actual violations). Pin the false-positive case so
/// strict mode does not become "every init fails".
#[tokio::test(flavor = "multi_thread")]
async fn strict_mode_with_blessed_attributes_returns_ok_guard() {
    let aperture = spawn_aperture_with_recording_sink().await;

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .require_tenant_id()
            .with_tenant_id(CANONICAL_TENANT_ID)
            .with_feature_flags([("checkout-v2", "on")])
            .with_endpoint(aperture.grpc_endpoint())
            .with_strict_schema_lint(true),
    )
    .expect("strict mode + blessed attributes returns Ok");

    drop(guard);
}

// =========================================================================
// Test 5 — internal lint runs first: missing service.name short-circuits
// =========================================================================

/// ADR-0025 §3 says the Codex lint runs after the existing internal
/// lint. Pin the order: a config that fails the internal lint
/// (missing `service.name`) returns
/// `Err(SparkError::MissingRequiredAttribute)`, NOT
/// `Err(SparkError::SchemaValidation)`. The internal lint short-
/// circuits before Codex sees the resource attributes.
#[tokio::test(flavor = "multi_thread")]
async fn internal_lint_short_circuits_before_codex_lint() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let _capture = capture_spark_events();

    let result = init(
        SparkConfig::for_service("")
            .with_endpoint(aperture.grpc_endpoint())
            .with_strict_schema_lint(true),
    );

    match result {
        Err(SparkError::MissingRequiredAttribute { name }) => {
            assert_eq!(name, "service.name");
        }
        Err(other) => panic!(
            "expected MissingRequiredAttribute (internal lint short-circuit); got: {other:?}"
        ),
        Ok(_) => panic!("empty service.name must fail the internal lint"),
    }
}
