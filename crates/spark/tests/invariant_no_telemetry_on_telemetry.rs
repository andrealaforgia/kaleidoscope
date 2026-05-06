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
    capture_spark_events, expect_spark_event_with_message, install_bridge_against_logger_provider,
    spawn_aperture_with_recording_sink, wait_for, CANONICAL_SERVICE_NAME, CANONICAL_TENANT_ID,
};

// =========================================================================
// Spark's own diagnostics reach the tracing facade
// =========================================================================

/// D5 + US-SP-01 AC: Spark's own init-success event is captured by
/// the application's tracing subscriber.
#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
async fn spark_emits_init_succeeded_event_to_tracing_facade() {
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
// No Spark-internal telemetry reaches the OTel pipeline
// =========================================================================

/// D5: "no record carries service.name=\"spark\" or any other
/// Spark-internal identifier." Walk every captured Resource on every
/// signal type and confirm none of them masquerade as Spark's own
/// telemetry.
#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
async fn no_export_reaches_recording_sink_with_spark_as_service_name() {
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
#[serial_test::serial]
async fn no_export_reaches_recording_sink_with_spark_prefixed_resource_attribute() {
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
// ADR-0017 §3 — the bridge filter excludes target = "spark"
// =========================================================================
//
// ADR-0017 adopts `opentelemetry-appender-tracing` as Spark v0's
// logs-emission seam. The bridge forwards `tracing::*!` events as
// OTel `LogRecord`s through Spark's configured `LoggerProvider`.
// Without a target filter, Spark's own `tracing::info!(target:
// "spark", ...)` events would feed back into the OTel pipeline Spark
// configured — a feedback loop violating D5.
//
// ADR-0017 §3 specifies that the bridge MUST exclude `target =
// "spark"` events. The implementation is in
// `crate::init::non_spark_target` and (mirrored on the test side) in
// `crate::common::BridgeWithTargetFilter`. The two tests below
// defend the invariant: a `tracing::info!(target: "spark", "marker")`
// event must NOT produce a `LogRecord` at Aperture's RecordingSink.

/// ADR-0017 §3: a `tracing::info!(target: "spark", "marker")` event
/// emitted AFTER the bridge is wired must NOT produce a `LogRecord`
/// at Aperture's RecordingSink. The filter is the load-bearing
/// implementation detail this test guards.
#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
async fn target_spark_tracing_event_does_not_produce_log_record_at_recording_sink() {
    spark::__reset_for_testing();
    let aperture = spawn_aperture_with_recording_sink().await;

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .require_tenant_id()
            .with_tenant_id(CANONICAL_TENANT_ID)
            .with_endpoint(aperture.grpc_endpoint()),
    )
    .expect("init succeeds");
    let logger_provider =
        spark::__test_logger_provider().expect("logger provider available after init");
    install_bridge_against_logger_provider(logger_provider);

    // The load-bearing event: target = "spark". If the filter is
    // absent or inverted, this event becomes a `LogRecord` and the
    // assertion below fails (which is the test's purpose).
    tracing::info!(target: "spark", "spark-internal marker for the no-telemetry-on-telemetry invariant");

    // Emit a non-spark event so we can tell "no logs at all" apart
    // from "the bridge is up and routing non-spark events". The
    // non-spark event is the positive control.
    tracing::info!(target: "checkout-service", "positive control marker");

    drop(guard);

    wait_for(|| !aperture.sink.is_empty(), Duration::from_secs(3)).await;
    let recorded = aperture.sink.drain();

    let logs: Vec<_> = recorded
        .iter()
        .filter_map(|r| match r {
            SinkRecord::Logs(req) => Some(req),
            _ => None,
        })
        .collect();
    // Positive control: at least one Logs record reached the sink
    // (proving the bridge is wired and would have forwarded the
    // `target = "spark"` event if the filter were absent).
    assert!(
        !logs.is_empty(),
        "expected at least one LogRecord from the positive-control non-spark event; \
         the bridge wiring may not be active"
    );

    // The invariant: no Logs record's body or scope or attribute set
    // shows the spark-internal marker text. The simplest structural
    // check walks the log records' bodies; if Spark's marker leaked
    // into the OTel pipeline, the marker text would appear here.
    for log_req in &logs {
        for resource_logs in &log_req.resource_logs {
            for scope_logs in &resource_logs.scope_logs {
                for record in &scope_logs.log_records {
                    let body_text = record
                        .body
                        .as_ref()
                        .and_then(|b| b.value.as_ref())
                        .map(|v| {
                            match v {
                            opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(
                                s,
                            ) => s.clone(),
                            other => format!("{other:?}"),
                        }
                        })
                        .unwrap_or_default();
                    assert!(
                        !body_text.contains("spark-internal marker"),
                        "no LogRecord may carry Spark's `target=\"spark\"` marker text; \
                         found body {body_text:?} — the bridge filter excluding target=\"spark\" \
                         (per ADR-0017 §3) is broken"
                    );
                }
            }
        }
    }
}

/// Companion to the body-text assertion: the spark-internal event
/// also must not appear as a `target` attribute on any LogRecord.
/// The appender records `metadata.target()` on each LogRecord (per
/// `opentelemetry-appender-tracing` 0.27 source); if a `target =
/// "spark"` event leaks through the filter, the LogRecord's target
/// attribute is `"spark"`. This test reads target back off the wire.
#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
async fn target_spark_tracing_event_does_not_produce_log_record_with_spark_target() {
    spark::__reset_for_testing();
    let aperture = spawn_aperture_with_recording_sink().await;

    let guard = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .require_tenant_id()
            .with_tenant_id(CANONICAL_TENANT_ID)
            .with_endpoint(aperture.grpc_endpoint()),
    )
    .expect("init succeeds");
    let logger_provider =
        spark::__test_logger_provider().expect("logger provider available after init");
    install_bridge_against_logger_provider(logger_provider);

    tracing::info!(target: "spark", "another spark-internal marker");
    tracing::info!(target: "checkout-service", "another positive control");

    drop(guard);

    wait_for(|| !aperture.sink.is_empty(), Duration::from_secs(3)).await;
    let recorded = aperture.sink.drain();

    let logs: Vec<_> = recorded
        .iter()
        .filter_map(|r| match r {
            SinkRecord::Logs(req) => Some(req),
            _ => None,
        })
        .collect();
    assert!(
        !logs.is_empty(),
        "expected at least one LogRecord from the positive-control non-spark event"
    );

    // Walk every LogRecord's instrumentation scope and the LogRecord
    // attributes/target slot. Per `opentelemetry-appender-tracing
    // 0.27`'s `layer.rs`, `log_record.set_target(meta.target())` is
    // called on every emitted record — so a leaked `target="spark"`
    // event would carry `spark` as the LogRecord's target.
    for log_req in &logs {
        for resource_logs in &log_req.resource_logs {
            for scope_logs in &resource_logs.scope_logs {
                // The scope_logs.scope.name carries the appender's
                // own scope (e.g. "opentelemetry-appender-tracing");
                // the per-record target is on each LogRecord. The
                // proto's LogRecord at 0.27 represents the target
                // via a flagged attribute. Walk the body & attrs.
                for record in &scope_logs.log_records {
                    for kv in &record.attributes {
                        if let Some(v) = kv.value.as_ref() {
                            if let Some(
                                opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(s),
                            ) = &v.value
                            {
                                assert_ne!(
                                    s, "spark",
                                    "no LogRecord may carry an attribute valued \"spark\"; \
                                     the bridge filter excluding target=\"spark\" is broken"
                                );
                            }
                        }
                    }
                }
            }
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
