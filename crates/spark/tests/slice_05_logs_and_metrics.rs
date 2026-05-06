//! Slice 05 — Logs and metrics symmetry.
//!
//! Maps to `docs/feature/spark/slices/slice-05-logs-and-metrics.md`.
//! Companion stories: US-SP-05.
//!
//! The user-centric outcome: a developer using all three OTLP signal
//! types (traces, logs, metrics) emits each one with the same Resource
//! shape — same names, same values. The operator's unified query
//! `tenant.id="acme-prod" AND service.name="payments-api"` returns
//! spans, logs, and metrics in one filter.
//!
//! ## Logs-emission contract gap (see distill/back-propagation.md)
//!
//! DISTILL discovered while writing this file that the OpenTelemetry
//! Rust SDK at the family-pinned version `=0.27` has no
//! `opentelemetry::global::logger_provider()` getter (analogous to
//! `tracer_provider()` and `meter_provider()`). The DISCUSS BDD
//! scenarios for US-SP-05 reference that exact API, which does not
//! compile at this version. See
//! `docs/feature/spark/distill/back-propagation.md > Issue 1` for the
//! full analysis and the recommended Path A (rephrase the contract;
//! DESIGN extends the public surface or chooses an emission seam).
//!
//! Until Bea routes Path A through Luna, this Slice 05 test file
//! asserts cross-signal symmetry across the **traces** and
//! **metrics** signal types only — both of which DO have a canonical
//! `opentelemetry::global::*` API at 0.27. The dedicated logs
//! assertion is captured as `#[ignore]`d test stubs that will be
//! enabled when the contract resolution lands.
//!
//! ## What this slice asserts at DISTILL state
//!
//! - LoggerProvider, MeterProvider, and TracerProvider are configured
//!   with the same Resource (asserted indirectly via the wire bytes
//!   of the two API-accessible signals).
//! - An emitted metric data point reaches Aperture as
//!   `ExportMetricsServiceRequest` whose Resource carries every set
//!   house attribute.
//! - Traces and metrics signals carry an identical Resource attribute
//!   set (same names, same values).
//!
//! ## Counter accumulation timing
//!
//! Per `slice-mapping.md > Slice 05 implementation pointers` and
//! `design/wave-decisions.md > For DISTILL §7`: a metric `add(1, &[])`
//! does NOT produce a wire export immediately. The integration test
//! must increment a counter, drop the guard, and only THEN assert the
//! `ExportMetricsServiceRequest` reached the sink.
//!
//! ## RED-on-day-one
//!
//! Every test calls `spark::init` which panics with
//! `unimplemented!()` at DISTILL.

mod common;

use std::time::Duration;

use aperture::ports::SinkRecord;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use spark::{init, SparkConfig};

use crate::common::{
    install_bridge_against_logger_provider, spawn_aperture_with_recording_sink, wait_for,
    CANONICAL_EXPERIMENT_ID, CANONICAL_FEATURE_FLAG_KEY, CANONICAL_FEATURE_FLAG_VALUE,
    CANONICAL_SERVICE_NAME, CANONICAL_TENANT_ID,
};

// =========================================================================
// Helpers local to this slice
// =========================================================================

fn traces_resource_attrs(req: &ExportTraceServiceRequest) -> Vec<(String, String)> {
    req.resource_spans
        .iter()
        .filter_map(|rs| rs.resource.as_ref())
        .flat_map(|r| r.attributes.iter())
        .filter_map(string_kv)
        .collect()
}

#[allow(dead_code)] // Used by the #[ignore]'d logs tests below.
fn logs_resource_attrs(req: &ExportLogsServiceRequest) -> Vec<(String, String)> {
    req.resource_logs
        .iter()
        .filter_map(|rl| rl.resource.as_ref())
        .flat_map(|r| r.attributes.iter())
        .filter_map(string_kv)
        .collect()
}

fn metrics_resource_attrs(req: &ExportMetricsServiceRequest) -> Vec<(String, String)> {
    req.resource_metrics
        .iter()
        .filter_map(|rm| rm.resource.as_ref())
        .flat_map(|r| r.attributes.iter())
        .filter_map(string_kv)
        .collect()
}

fn string_kv(kv: &opentelemetry_proto::tonic::common::v1::KeyValue) -> Option<(String, String)> {
    let v = kv.value.as_ref()?;
    let s = match &v.value {
        Some(opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(s)) => s.clone(),
        _ => return None,
    };
    Some((kv.key.clone(), s))
}

fn canonical_config(endpoint: String) -> SparkConfig {
    SparkConfig::for_service(CANONICAL_SERVICE_NAME)
        .require_tenant_id()
        .with_tenant_id(CANONICAL_TENANT_ID)
        .with_feature_flags([(CANONICAL_FEATURE_FLAG_KEY, CANONICAL_FEATURE_FLAG_VALUE)])
        .with_experiment_id(CANONICAL_EXPERIMENT_ID)
        .with_endpoint(endpoint)
}

// =========================================================================
// Metrics export carries the four house attributes on the Resource
// =========================================================================

/// US-SP-05 UAT: "A metrics export carries the same four house
/// attributes on the Resource."
#[tokio::test(flavor = "multi_thread")]
async fn developer_increments_one_counter_and_metrics_export_carries_service_name_on_resource() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let guard = init(canonical_config(aperture.grpc_endpoint())).expect("init succeeds");

    {
        let meter = opentelemetry::global::meter("checkout-service");
        let counter = meter.u64_counter("orders.processed").build();
        counter.add(1, &[]);
    }
    drop(guard);

    wait_for(|| !aperture.sink.is_empty(), Duration::from_secs(2)).await;
    let recorded = aperture.sink.drain();
    let metrics = recorded
        .into_iter()
        .find_map(|r| match r {
            SinkRecord::Metrics(req) => Some(req),
            _ => None,
        })
        .expect("expected a Metrics SinkRecord");

    let attrs = metrics_resource_attrs(&metrics);
    assert!(
        attrs
            .iter()
            .any(|(k, v)| k == "service.name" && v == CANONICAL_SERVICE_NAME),
        "metrics Resource should carry service.name; got {attrs:?}"
    );
}

/// US-SP-05 UAT (cont.): tenant.id on the metrics Resource.
#[tokio::test(flavor = "multi_thread")]
async fn developer_increments_one_counter_and_metrics_export_carries_tenant_id_on_resource() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let guard = init(canonical_config(aperture.grpc_endpoint())).expect("init succeeds");

    {
        let meter = opentelemetry::global::meter("checkout-service");
        let counter = meter.u64_counter("orders.processed").build();
        counter.add(1, &[]);
    }
    drop(guard);

    wait_for(|| !aperture.sink.is_empty(), Duration::from_secs(2)).await;
    let recorded = aperture.sink.drain();
    let metrics = recorded
        .into_iter()
        .find_map(|r| match r {
            SinkRecord::Metrics(req) => Some(req),
            _ => None,
        })
        .expect("expected a Metrics SinkRecord");

    let attrs = metrics_resource_attrs(&metrics);
    assert!(
        attrs
            .iter()
            .any(|(k, v)| k == "tenant.id" && v == CANONICAL_TENANT_ID),
        "metrics Resource should carry tenant.id; got {attrs:?}"
    );
}

/// US-SP-05 UAT (cont.): feature_flag.checkout-v2 on the metrics
/// Resource.
#[tokio::test(flavor = "multi_thread")]
async fn developer_increments_one_counter_and_metrics_export_carries_feature_flag_on_resource() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let guard = init(canonical_config(aperture.grpc_endpoint())).expect("init succeeds");

    {
        let meter = opentelemetry::global::meter("checkout-service");
        let counter = meter.u64_counter("orders.processed").build();
        counter.add(1, &[]);
    }
    drop(guard);

    wait_for(|| !aperture.sink.is_empty(), Duration::from_secs(2)).await;
    let recorded = aperture.sink.drain();
    let metrics = recorded
        .into_iter()
        .find_map(|r| match r {
            SinkRecord::Metrics(req) => Some(req),
            _ => None,
        })
        .expect("expected a Metrics SinkRecord");

    let attrs = metrics_resource_attrs(&metrics);
    assert!(
        attrs
            .iter()
            .any(|(k, v)| k == "feature_flag.checkout-v2" && v == "on"),
        "metrics Resource should carry feature_flag.checkout-v2; got {attrs:?}"
    );
}

/// US-SP-05 UAT (cont.): experiment.id on the metrics Resource.
#[tokio::test(flavor = "multi_thread")]
async fn developer_increments_one_counter_and_metrics_export_carries_experiment_id_on_resource() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let guard = init(canonical_config(aperture.grpc_endpoint())).expect("init succeeds");

    {
        let meter = opentelemetry::global::meter("checkout-service");
        let counter = meter.u64_counter("orders.processed").build();
        counter.add(1, &[]);
    }
    drop(guard);

    wait_for(|| !aperture.sink.is_empty(), Duration::from_secs(2)).await;
    let recorded = aperture.sink.drain();
    let metrics = recorded
        .into_iter()
        .find_map(|r| match r {
            SinkRecord::Metrics(req) => Some(req),
            _ => None,
        })
        .expect("expected a Metrics SinkRecord");

    let attrs = metrics_resource_attrs(&metrics);
    assert!(
        attrs
            .iter()
            .any(|(k, v)| k == "experiment.id" && v == CANONICAL_EXPERIMENT_ID),
        "metrics Resource should carry experiment.id; got {attrs:?}"
    );
}

// =========================================================================
// Symmetry: traces and metrics carry identical Resource attribute sets
// =========================================================================
//
// US-SP-05 UAT "All three signals share the same Resource shape" is
// asserted across traces and metrics at DISTILL state. The third
// signal (logs) is deferred per the back-propagation note. When the
// logs-emission contract resolves, an additional symmetry test
// extends this assertion to all three signals.

/// US-SP-05 UAT (cont.): the four house attributes appear identically
/// on the Resource of both Traces and Metrics requests.
#[tokio::test(flavor = "multi_thread")]
async fn developer_emits_trace_and_metric_and_resource_attributes_match_across_two_signals() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let guard = init(canonical_config(aperture.grpc_endpoint())).expect("init succeeds");

    {
        use opentelemetry::trace::Tracer;
        let tracer = opentelemetry::global::tracer("checkout-service");
        let _span = tracer.start("op");
    }
    {
        let meter = opentelemetry::global::meter("checkout-service");
        let counter = meter.u64_counter("orders.processed").build();
        counter.add(1, &[]);
    }
    drop(guard);

    wait_for(|| aperture.sink.len() >= 2, Duration::from_secs(3)).await;

    let recorded = aperture.sink.drain();
    let traces = recorded
        .iter()
        .find_map(|r| match r {
            SinkRecord::Traces(req) => Some(traces_resource_attrs(req)),
            _ => None,
        })
        .expect("traces export expected");
    let metrics = recorded
        .iter()
        .find_map(|r| match r {
            SinkRecord::Metrics(req) => Some(metrics_resource_attrs(req)),
            _ => None,
        })
        .expect("metrics export expected");

    let canon: Vec<(String, String)> = vec![
        ("service.name".into(), CANONICAL_SERVICE_NAME.into()),
        ("tenant.id".into(), CANONICAL_TENANT_ID.into()),
        ("feature_flag.checkout-v2".into(), "on".into()),
        ("experiment.id".into(), CANONICAL_EXPERIMENT_ID.into()),
    ];

    for (key, value) in &canon {
        assert!(
            traces.iter().any(|(k, v)| k == key && v == value),
            "traces Resource missing {key}={value:?}; got {traces:?}"
        );
        assert!(
            metrics.iter().any(|(k, v)| k == key && v == value),
            "metrics Resource missing {key}={value:?}; got {metrics:?}"
        );
    }
}

// =========================================================================
// Logs symmetry — Path A3 (ADR-0017): tracing::info! via the appender
// =========================================================================
//
// DESIGN ADR-0017 picks Option A3 from the DISTILL back-propagation
// menu: Spark adopts `opentelemetry-appender-tracing =0.27` as a
// runtime dependency and wires
// `OpenTelemetryTracingBridge::new(&logger_provider)` as a
// `tracing_subscriber::Layer`, filtered to non-`spark` targets per
// ADR-0017 §3 / D5. Applications emit log records via the standard
// `tracing::*!` macros they already use.
//
// At DELIVER time these three tests are un-ignored and the bodies are
// rewritten to use `tracing::info!(target: "checkout-service", ...)`
// per the `journey-spark.feature` US-SP-05 logs scenario. The
// assertions match the contract from the function names verbatim:
// the `LogRecord` reaches Aperture's RecordingSink with the four
// house attributes on its Resource.
//
// The bridge is installed against Spark's `LoggerProvider` (retrieved
// via the doc-hidden `__test_logger_provider` test seam) AFTER
// `init` returns and BEFORE the application emits the
// `tracing::info!` event. The fixture's
// `install_bridge_against_logger_provider` helper centralises that
// dance so each test reads as a clean three-step sequence: spawn
// Aperture, init Spark + install bridge, emit + drop guard + assert.

/// US-SP-05 UAT: "A logs export carries the same four house
/// attributes on the Resource" — service.name.
#[tokio::test(flavor = "multi_thread")]
async fn developer_emits_one_log_record_and_logs_export_carries_service_name_on_resource() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let guard = init(canonical_config(aperture.grpc_endpoint())).expect("init succeeds");
    let logger_provider =
        spark::__test_logger_provider().expect("logger provider available after init");
    install_bridge_against_logger_provider(logger_provider);

    tracing::info!(target: "checkout-service", order_id = "ord-42", "order processed");
    drop(guard);

    wait_for(|| !aperture.sink.is_empty(), Duration::from_secs(3)).await;
    let recorded = aperture.sink.drain();
    let logs = recorded
        .into_iter()
        .find_map(|r| match r {
            SinkRecord::Logs(req) => Some(req),
            _ => None,
        })
        .expect("expected a Logs SinkRecord");

    let attrs = logs_resource_attrs(&logs);
    assert!(
        attrs
            .iter()
            .any(|(k, v)| k == "service.name" && v == CANONICAL_SERVICE_NAME),
        "logs Resource should carry service.name; got {attrs:?}"
    );
}

/// US-SP-05 UAT: "A logs export carries the same four house
/// attributes on the Resource" — tenant.id.
#[tokio::test(flavor = "multi_thread")]
async fn developer_emits_one_log_record_and_logs_export_carries_tenant_id_on_resource() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let guard = init(canonical_config(aperture.grpc_endpoint())).expect("init succeeds");
    let logger_provider =
        spark::__test_logger_provider().expect("logger provider available after init");
    install_bridge_against_logger_provider(logger_provider);

    tracing::info!(target: "checkout-service", order_id = "ord-42", "order processed");
    drop(guard);

    wait_for(|| !aperture.sink.is_empty(), Duration::from_secs(3)).await;
    let recorded = aperture.sink.drain();
    let logs = recorded
        .into_iter()
        .find_map(|r| match r {
            SinkRecord::Logs(req) => Some(req),
            _ => None,
        })
        .expect("expected a Logs SinkRecord");

    let attrs = logs_resource_attrs(&logs);
    assert!(
        attrs
            .iter()
            .any(|(k, v)| k == "tenant.id" && v == CANONICAL_TENANT_ID),
        "logs Resource should carry tenant.id; got {attrs:?}"
    );
}

/// US-SP-05 UAT: "All three signals share the same Resource shape" —
/// extended to logs.
#[tokio::test(flavor = "multi_thread")]
async fn developer_emits_all_three_signals_and_resource_attributes_match_across_signals() {
    let aperture = spawn_aperture_with_recording_sink().await;
    let guard = init(canonical_config(aperture.grpc_endpoint())).expect("init succeeds");
    let logger_provider =
        spark::__test_logger_provider().expect("logger provider available after init");
    install_bridge_against_logger_provider(logger_provider);

    {
        use opentelemetry::trace::Tracer;
        let tracer = opentelemetry::global::tracer("checkout-service");
        let _span = tracer.start("op");
    }
    tracing::info!(target: "checkout-service", order_id = "ord-42", "order processed");
    {
        let meter = opentelemetry::global::meter("checkout-service");
        let counter = meter.u64_counter("orders.processed").build();
        counter.add(1, &[]);
    }
    drop(guard);

    wait_for(|| aperture.sink.len() >= 3, Duration::from_secs(5)).await;

    let recorded = aperture.sink.drain();
    let traces = recorded
        .iter()
        .find_map(|r| match r {
            SinkRecord::Traces(req) => Some(traces_resource_attrs(req)),
            _ => None,
        })
        .expect("traces export expected");
    let logs = recorded
        .iter()
        .find_map(|r| match r {
            SinkRecord::Logs(req) => Some(logs_resource_attrs(req)),
            _ => None,
        })
        .expect("logs export expected");
    let metrics = recorded
        .iter()
        .find_map(|r| match r {
            SinkRecord::Metrics(req) => Some(metrics_resource_attrs(req)),
            _ => None,
        })
        .expect("metrics export expected");

    let canon: Vec<(String, String)> = vec![
        ("service.name".into(), CANONICAL_SERVICE_NAME.into()),
        ("tenant.id".into(), CANONICAL_TENANT_ID.into()),
        ("feature_flag.checkout-v2".into(), "on".into()),
        ("experiment.id".into(), CANONICAL_EXPERIMENT_ID.into()),
    ];

    for (key, value) in &canon {
        assert!(
            traces.iter().any(|(k, v)| k == key && v == value),
            "traces Resource missing {key}={value:?}; got {traces:?}"
        );
        assert!(
            logs.iter().any(|(k, v)| k == key && v == value),
            "logs Resource missing {key}={value:?}; got {logs:?}"
        );
        assert!(
            metrics.iter().any(|(k, v)| k == key && v == value),
            "metrics Resource missing {key}={value:?}; got {metrics:?}"
        );
    }
}
