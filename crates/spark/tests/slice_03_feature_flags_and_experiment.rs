//! Slice 03 — Feature flags and experiment.id on the Resource.
//!
//! Maps to `docs/feature/spark/slices/slice-03-feature-flags-and-experiment.md`.
//! Companion stories: US-SP-03.
//!
//! The user-centric outcome: a developer building a tenant-aware,
//! feature-flag-aware, or experiment-aware Rust service emits
//! telemetry whose Resource carries every set house attribute, so the
//! data team's "show me errors for tenant X with feature
//! checkout-v2 enabled in experiment exp-2026-Q2-pricing" query is
//! answerable.
//!
//! ## What this slice asserts
//!
//! - `with_feature_flags` accepts `IntoIterator<Item = (impl Into<String>, impl Into<String>)>`.
//! - The Resource carries `feature_flag.{key}` (with the `feature_flag.`
//!   prefix) for each non-empty pair.
//! - The Resource carries `experiment.id` for non-empty values.
//! - Empty-string optional attributes are SKIPPED, not emitted as
//!   empty-string Resource attributes.
//! - A `SparkConfig` without optional house attributes produces a
//!   Resource containing only `service.name`.
//!
//! ## RED-on-day-one
//!
//! Every test calls `spark::init` which panics with
//! `unimplemented!()` at DISTILL.

mod common;

use std::time::Duration;

use aperture::ports::SinkRecord;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use spark::{init, SparkConfig};

use crate::common::{
    spawn_aperture_with_recording_sink, wait_for, CANONICAL_EXPERIMENT_ID,
    CANONICAL_FEATURE_FLAG_KEY, CANONICAL_FEATURE_FLAG_VALUE, CANONICAL_SERVICE_NAME,
    CANONICAL_TENANT_ID,
};

// =========================================================================
// Helpers local to this slice
// =========================================================================

/// Extract the (key, value) string-attribute pairs from the first
/// ResourceSpans of a captured Traces export.
fn resource_attributes(req: &ExportTraceServiceRequest) -> Vec<(String, String)> {
    req.resource_spans
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
        .collect()
}

/// Drive Spark with the given config, emit one span, drop the guard,
/// and return the captured Traces export the RecordingSink received.
async fn drive_one_span_and_capture_traces(
    config: SparkConfig,
    aperture: &crate::common::ApertureFixture,
) -> ExportTraceServiceRequest {
    let guard = init(config).expect("init succeeds");
    {
        use opentelemetry::trace::Tracer;
        let tracer = opentelemetry::global::tracer("ci-runner");
        let _span = tracer.start("op");
    }
    drop(guard);

    wait_for(|| !aperture.sink.is_empty(), Duration::from_secs(2)).await;
    let recorded = aperture.sink.drain();
    match recorded.into_iter().next() {
        Some(SinkRecord::Traces(req)) => req,
        other => panic!("expected one Traces SinkRecord; got {other:?}"),
    }
}

// =========================================================================
// All four house attributes land on the Resource
// =========================================================================

/// US-SP-03 UAT: "A traces export carries all four house attributes
/// when all are configured" — service.name asserted.
#[tokio::test(flavor = "multi_thread")]
async fn developer_sets_all_four_house_attrs_and_resource_includes_service_name() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let config = SparkConfig::for_service(CANONICAL_SERVICE_NAME)
        .require_tenant_id()
        .with_tenant_id(CANONICAL_TENANT_ID)
        .with_feature_flags([(CANONICAL_FEATURE_FLAG_KEY, CANONICAL_FEATURE_FLAG_VALUE)])
        .with_experiment_id(CANONICAL_EXPERIMENT_ID)
        .with_endpoint(aperture.grpc_endpoint());

    let req = drive_one_span_and_capture_traces(config, &aperture).await;
    let attrs = resource_attributes(&req);

    assert!(
        attrs
            .iter()
            .any(|(k, v)| k == "service.name" && v == CANONICAL_SERVICE_NAME),
        "Resource should carry service.name={CANONICAL_SERVICE_NAME:?}; got {attrs:?}"
    );
}

/// US-SP-03 UAT (cont.): tenant.id asserted.
#[tokio::test(flavor = "multi_thread")]
async fn developer_sets_all_four_house_attrs_and_resource_includes_tenant_id() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let config = SparkConfig::for_service(CANONICAL_SERVICE_NAME)
        .require_tenant_id()
        .with_tenant_id(CANONICAL_TENANT_ID)
        .with_feature_flags([(CANONICAL_FEATURE_FLAG_KEY, CANONICAL_FEATURE_FLAG_VALUE)])
        .with_experiment_id(CANONICAL_EXPERIMENT_ID)
        .with_endpoint(aperture.grpc_endpoint());

    let req = drive_one_span_and_capture_traces(config, &aperture).await;
    let attrs = resource_attributes(&req);

    assert!(
        attrs
            .iter()
            .any(|(k, v)| k == "tenant.id" && v == CANONICAL_TENANT_ID),
        "Resource should carry tenant.id={CANONICAL_TENANT_ID:?}; got {attrs:?}"
    );
}

/// US-SP-03 UAT (cont.): the feature_flag.checkout-v2 attribute
/// uses the `feature_flag.` prefix verbatim.
#[tokio::test(flavor = "multi_thread")]
async fn developer_sets_feature_flag_checkout_v2_and_resource_includes_prefixed_attribute() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let config = SparkConfig::for_service(CANONICAL_SERVICE_NAME)
        .require_tenant_id()
        .with_tenant_id(CANONICAL_TENANT_ID)
        .with_feature_flags([(CANONICAL_FEATURE_FLAG_KEY, CANONICAL_FEATURE_FLAG_VALUE)])
        .with_experiment_id(CANONICAL_EXPERIMENT_ID)
        .with_endpoint(aperture.grpc_endpoint());

    let req = drive_one_span_and_capture_traces(config, &aperture).await;
    let attrs = resource_attributes(&req);

    assert!(
        attrs
            .iter()
            .any(|(k, v)| k == "feature_flag.checkout-v2" && v == "on"),
        "Resource should carry feature_flag.checkout-v2=\"on\"; got {attrs:?}"
    );
}

/// US-SP-03 UAT (cont.): experiment.id asserted with the canonical
/// realistic value.
#[tokio::test(flavor = "multi_thread")]
async fn developer_sets_experiment_id_and_resource_includes_experiment_id_attribute() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let config = SparkConfig::for_service(CANONICAL_SERVICE_NAME)
        .require_tenant_id()
        .with_tenant_id(CANONICAL_TENANT_ID)
        .with_feature_flags([(CANONICAL_FEATURE_FLAG_KEY, CANONICAL_FEATURE_FLAG_VALUE)])
        .with_experiment_id(CANONICAL_EXPERIMENT_ID)
        .with_endpoint(aperture.grpc_endpoint());

    let req = drive_one_span_and_capture_traces(config, &aperture).await;
    let attrs = resource_attributes(&req);

    assert!(
        attrs
            .iter()
            .any(|(k, v)| k == "experiment.id" && v == CANONICAL_EXPERIMENT_ID),
        "Resource should carry experiment.id={CANONICAL_EXPERIMENT_ID:?}; got {attrs:?}"
    );
}

// =========================================================================
// Multiple feature_flag pairs each land as their own attribute
// =========================================================================

/// US-SP-03 UAT: "feature_flag attributes are namespace-prefixed with
/// feature_flag." — covers two pairs at once.
#[tokio::test(flavor = "multi_thread")]
async fn developer_sets_two_feature_flags_and_resource_includes_both_prefixed_attributes() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let config = SparkConfig::for_service(CANONICAL_SERVICE_NAME)
        .require_tenant_id()
        .with_tenant_id(CANONICAL_TENANT_ID)
        .with_feature_flags([("checkout-v2", "on"), ("dark-mode", "off")])
        .with_endpoint(aperture.grpc_endpoint());

    let req = drive_one_span_and_capture_traces(config, &aperture).await;
    let attrs = resource_attributes(&req);

    assert!(
        attrs
            .iter()
            .any(|(k, v)| k == "feature_flag.checkout-v2" && v == "on"),
        "Resource should carry feature_flag.checkout-v2=\"on\"; got {attrs:?}"
    );
    assert!(
        attrs
            .iter()
            .any(|(k, v)| k == "feature_flag.dark-mode" && v == "off"),
        "Resource should carry feature_flag.dark-mode=\"off\"; got {attrs:?}"
    );
}

/// US-SP-03 UAT: "neither attribute appears WITHOUT the feature_flag.
/// prefix."
#[tokio::test(flavor = "multi_thread")]
async fn developer_sets_feature_flags_and_unprefixed_attribute_does_not_appear_on_resource() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let config = SparkConfig::for_service(CANONICAL_SERVICE_NAME)
        .with_feature_flags([("checkout-v2", "on")])
        .with_endpoint(aperture.grpc_endpoint());

    let req = drive_one_span_and_capture_traces(config, &aperture).await;
    let attrs = resource_attributes(&req);

    assert!(
        !attrs.iter().any(|(k, _)| k == "checkout-v2"),
        "the unprefixed key 'checkout-v2' must NOT appear on the Resource; got {attrs:?}"
    );
}

// =========================================================================
// Empty-value entries are skipped
// =========================================================================

/// US-SP-03 UAT: "Empty-string optional attributes are skipped, not
/// emitted." Empty experiment.id must not produce an attribute with
/// an empty value.
#[tokio::test(flavor = "multi_thread")]
async fn developer_sets_empty_experiment_id_and_resource_does_not_include_experiment_id() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let config = SparkConfig::for_service(CANONICAL_SERVICE_NAME)
        .with_experiment_id("")
        .with_endpoint(aperture.grpc_endpoint());

    let req = drive_one_span_and_capture_traces(config, &aperture).await;
    let attrs = resource_attributes(&req);

    assert!(
        !attrs.iter().any(|(k, _)| k == "experiment.id"),
        "an empty experiment.id must be skipped, not emitted; got {attrs:?}"
    );
}

// =========================================================================
// Minimum-viable Spark integration: only service.name on the Resource
// =========================================================================

/// US-SP-03 UAT: "A SparkConfig without optional attributes produces
/// a minimal Resource."
#[tokio::test(flavor = "multi_thread")]
async fn developer_uses_only_for_service_and_resource_does_not_include_tenant_id() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let config =
        SparkConfig::for_service(CANONICAL_SERVICE_NAME).with_endpoint(aperture.grpc_endpoint());

    let req = drive_one_span_and_capture_traces(config, &aperture).await;
    let attrs = resource_attributes(&req);

    assert!(
        !attrs.iter().any(|(k, _)| k == "tenant.id"),
        "tenant.id must NOT appear on a minimum-viable Resource; got {attrs:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn developer_uses_only_for_service_and_resource_does_not_include_feature_flag_attribute() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let config =
        SparkConfig::for_service(CANONICAL_SERVICE_NAME).with_endpoint(aperture.grpc_endpoint());

    let req = drive_one_span_and_capture_traces(config, &aperture).await;
    let attrs = resource_attributes(&req);

    assert!(
        !attrs.iter().any(|(k, _)| k.starts_with("feature_flag.")),
        "no feature_flag.* attribute must appear on a minimum-viable Resource; got {attrs:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn developer_uses_only_for_service_and_resource_does_not_include_experiment_id() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let config =
        SparkConfig::for_service(CANONICAL_SERVICE_NAME).with_endpoint(aperture.grpc_endpoint());

    let req = drive_one_span_and_capture_traces(config, &aperture).await;
    let attrs = resource_attributes(&req);

    assert!(
        !attrs.iter().any(|(k, _)| k == "experiment.id"),
        "experiment.id must NOT appear on a minimum-viable Resource; got {attrs:?}"
    );
}
