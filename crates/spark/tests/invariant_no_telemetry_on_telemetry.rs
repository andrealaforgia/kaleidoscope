//! Invariant — no telemetry on telemetry (D5).
//!
//! Maps to D5 in `discuss/wave-decisions.md`,
//! `shared-artifacts-registry.md > ci_invariants > no_telemetry_on_telemetry`,
//! and is reiterated in
//! `journey-spark.feature > @property` cross-cutting scenarios.
//!
//! The contract: Spark's own diagnostic events (`spark::init
//! succeeded`, `spark: shutdown initiated`, `spark: shutdown
//! complete`, `spark: flush deadline exceeded`, `spark: exporter
//! initialisation failed`) flow through the application's `tracing`
//! facade — NEVER through the OTel pipeline Spark itself configured.
//!
//! ## Mechanism
//!
//! The integration test subscribes to the application's `tracing`
//! facade AND plugs a `RecordingSink` behind Aperture. Spark
//! instruments a normal application flow (init → emit one span →
//! drop guard). Two assertions:
//!
//! 1. Exactly one INFO event with `target="spark"` and message
//!    containing `"spark::init succeeded"` is captured by the
//!    application's subscriber. (Plus the shutdown vocabulary; the
//!    structural witness is the init-succeeded event.)
//! 2. No `ExportTraceServiceRequest`, `ExportLogsServiceRequest`, or
//!    `ExportMetricsServiceRequest` reaches the `RecordingSink`
//!    carrying `service.name="spark"` or any other Spark-internal
//!    identifier. Spark's traffic on the wire is the application's
//!    traffic; Spark's traffic about itself is on the tracing facade.
//!
//! ## RED-on-day-one
//!
//! `spark::init` panics with `unimplemented!()` at DISTILL. The two
//! assertions below cannot be reached until DELIVER lands the init
//! flow.

mod common;

use std::time::Duration;

use aperture::ports::SinkRecord;
use spark::{init, SparkConfig};

use crate::common::{
    capture_spark_events, expect_spark_event_with_message, spawn_aperture_with_recording_sink,
    wait_for, CANONICAL_SERVICE_NAME, CANONICAL_TENANT_ID,
};

// =========================================================================
// Spark's own diagnostics reach the tracing facade
// =========================================================================

/// D5 + US-SP-01 AC: Spark's own init-success event is captured by
/// the application's tracing subscriber.
#[tokio::test(flavor = "multi_thread")]
async fn spark_emits_init_succeeded_event_to_tracing_facade() {
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
// No Spark-internal telemetry reaches the OTel pipeline
// =========================================================================

/// D5: "no record carries service.name=\"spark\" or any other
/// Spark-internal identifier." Walk every captured Resource on every
/// signal type and confirm none of them masquerade as Spark's own
/// telemetry.
#[tokio::test(flavor = "multi_thread")]
async fn no_export_reaches_recording_sink_with_spark_as_service_name() {
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
        let tracer = opentelemetry::global::tracer("checkout-service");
        let _span = tracer.start("op");
    }

    drop(guard);

    wait_for(|| !aperture.sink.is_empty(), Duration::from_secs(2)).await;

    let recorded = aperture.sink.drain();
    for record in &recorded {
        for (key, value) in resource_attribute_strings(record) {
            assert_ne!(
                (key.as_str(), value.as_str()),
                ("service.name", "spark"),
                "no recorded export may carry service.name=\"spark\" — that would be \
                 Spark emitting telemetry-on-telemetry through the OTel pipeline"
            );
        }
    }
}

/// D5 elaborated: not only `service.name="spark"` but any `spark.*`
/// resource attribute would be evidence of telemetry-on-telemetry.
/// Spark's resource composition adds `feature_flag.{key}`, `tenant.id`,
/// `experiment.id`, `service.name` — never a `spark.*` key.
#[tokio::test(flavor = "multi_thread")]
async fn no_export_reaches_recording_sink_with_spark_prefixed_resource_attribute() {
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
        let tracer = opentelemetry::global::tracer("checkout-service");
        let _span = tracer.start("op");
    }

    drop(guard);

    wait_for(|| !aperture.sink.is_empty(), Duration::from_secs(2)).await;

    let recorded = aperture.sink.drain();
    for record in &recorded {
        for (key, _value) in resource_attribute_strings(record) {
            assert!(
                !key.starts_with("spark."),
                "no recorded export may carry a 'spark.'-prefixed resource attribute; \
                 found {key:?} — Spark must not contribute resource attributes naming itself"
            );
        }
    }
}

// =========================================================================
// Helpers
// =========================================================================

fn resource_attribute_strings(record: &SinkRecord) -> Vec<(String, String)> {
    // SinkRecord is `#[non_exhaustive]` (per Aperture's ports module),
    // so the wildcard arm is required by the compiler. v0 has three
    // variants; if Aperture adds a fourth in a future release this
    // test stays well-defined (an empty attrs vector is the sane
    // fallback for an unknown signal type).
    let attrs = match record {
        SinkRecord::Traces(req) => req
            .resource_spans
            .iter()
            .filter_map(|rs| rs.resource.as_ref())
            .flat_map(|r| r.attributes.iter())
            .collect::<Vec<_>>(),
        SinkRecord::Logs(req) => req
            .resource_logs
            .iter()
            .filter_map(|rl| rl.resource.as_ref())
            .flat_map(|r| r.attributes.iter())
            .collect::<Vec<_>>(),
        SinkRecord::Metrics(req) => req
            .resource_metrics
            .iter()
            .filter_map(|rm| rm.resource.as_ref())
            .flat_map(|r| r.attributes.iter())
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    attrs
        .into_iter()
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
