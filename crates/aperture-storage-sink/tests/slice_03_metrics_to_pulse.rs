// Kaleidoscope aperture-storage-sink — slice 03 acceptance test
// Copyright (C) 2026 The Kaleidoscope authors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Slice 03 — metrics persist to pulse end to end.
//!
//! Maps to `docs/feature/aperture-storage-sink-v0/slices/slice-03-metrics-to-pulse.md`.
//! Story: US-03. Decisions: DD3 (tenant resolution tenant.id ->
//! default_tenant -> refuse), DD4 (sink holds `Arc<FileBacked*Store>`),
//! DD5 (probe is an active write check), DD7 (atomic translation,
//! accepted => persisted, refused => writes nothing), DD8 (unsupported
//! metric point types are SKIPPED with an observable event, not fatal),
//! DD11 (value source: prefer `as_double`, fall back to `as_int` as
//! exact `f64`). ADR-0041 Decisions 1, 2 and 3. Outcome KPIs: KPI-3
//! (round-trip fidelity + durability), KPI-4, KPI-5.
//!
//! These tests enter through the real aperture driving port
//! `OtlpSink::accept` and the `Probe::probe` contract. The observable
//! outcome is what an operator can later query out of pulse
//! (`MetricStore::query`). Nothing internal to the translator is invoked
//! directly: the OTLP `ExportMetricsServiceRequest` goes in at the port,
//! and the persisted `pulse::Metric` / `MetricPoint`s come out at the
//! store.
//!
//! ## RED-gate boundary
//!
//! Slices 01 / 02 delivered `StorageSink` / `StorageSinkConfig` with
//! logs-only (`with_log_store`) and traces-only (`with_trace_store`)
//! constructors, plus a combined `with_log_and_trace_stores`. The
//! `SinkRecord::Metrics` arm is still an honest accepted-but-not-persisted
//! no-op. This slice imports a not-yet-existing metrics-only constructor
//! `StorageSink::with_metric_store`: the compile error against that symbol
//! is the RED state for the classic Rust outside-in loop. DELIVER adds the
//! `metric_store: Option<Arc<pulse::FileBackedMetricStore>>` field plus
//! this constructor and turns the `SinkRecord::Metrics` arm from a no-op
//! into real translation + ingest into pulse; these tests then go GREEN,
//! committed atomic with the slice.
//!
//! ## Assumed StorageSink metrics surface (DELIVER must match)
//!
//! Mirroring the slice-01 / slice-02 single-signal constructors (DD4),
//! the smallest honest slice-03 shape is a metrics-only constructor:
//!
//! - `StorageSink::with_metric_store(Arc<pulse::FileBackedMetricStore>, StorageSinkConfig)`
//!   constructs a metrics-only sink. DELIVER adds the
//!   `metric_store: Option<Arc<...>>` field non-breakingly; the logs
//!   path (slice 01) and traces path (slice 02) keep working with just
//!   their own store wired, and the metrics path works with just the
//!   metric store wired.
//! - `StorageSinkConfig` is unchanged from slices 01 / 02
//!   (`with_default_tenant` / `no_default_tenant`).
//!
//! If DELIVER chooses a combined builder taking all three handles, it
//! must keep an equivalent metrics-only entry so this slice stays
//! independently shippable.
//!
//! ## Skip-not-refuse (DD8 / ADR-0041 Decision 3)
//!
//! pulse v0 persists gauge + sum number data points only. Histogram /
//! ExponentialHistogram / Summary are SKIPPED with an observable event,
//! NOT refused: `accept` returns Ok, any supported points in the same
//! request still persist, and a request of only-unsupported types
//! persists nothing (still Ok). This supersedes the original DISCUSS AC
//! ("refused"); reconciled in `distill/upstream-issues.md`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use pulse::{FileBackedMetricStore, MetricKind, MetricName, MetricStore, NoopRecorder, TimeRange};

use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::metrics::v1::{
    metric as proto_metric, number_data_point, Gauge, Histogram, HistogramDataPoint, Metric,
    NumberDataPoint, ResourceMetrics, ScopeMetrics, Sum,
};
use opentelemetry_proto::tonic::resource::v1::Resource;

use aperture::ports::{OtlpSink, Probe, SinkRecord};

use aperture_storage_sink::{StorageSink, StorageSinkConfig};

// =========================================================================
// Tempdir helper — mirrors the slice-01 / slice-02 shape (temp_base +
// cleanup), pointing at a "pulse" pillar root.
// =========================================================================

fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("aperture-storage-sink-{test_name}-{pid}-{nanos}"));
    fs::create_dir_all(&path).expect("mkdir");
    path.push("pulse");
    path
}

fn cleanup(base: &Path) {
    if let Some(dir) = base.parent() {
        let _ = fs::remove_dir_all(dir);
    }
}

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn metric_name(name: &str) -> MetricName {
    MetricName::new(name)
}

fn open_metric_store(base: &Path) -> Arc<FileBackedMetricStore> {
    Arc::new(FileBackedMetricStore::open(base, Box::new(NoopRecorder)).expect("open pulse store"))
}

// =========================================================================
// OTLP ExportMetricsServiceRequest builder — hand-crafted from the real
// upstream opentelemetry-proto types, matching the shape an OTel SDK
// emits (one ResourceMetrics, one ScopeMetrics, N Metrics). The resource
// carries service.name and optionally tenant.id.
//
// Wire shapes the translator must handle (asserted across the scenarios
// below):
//   - each proto `Metric` has a `data: Option<metric::Data>` oneof:
//     Gauge / Sum carry `NumberDataPoint`s; Histogram /
//     ExponentialHistogram / Summary are unsupported at pulse v0.
//   - a `NumberDataPoint.value` is itself a oneof:
//     `as_double(f64)` or `as_int(i64)`; an unset value oneof is an
//     invalid (value-less) point.
//   - a `Sum` additionally carries aggregation_temporality + is_monotonic.
// =========================================================================

fn string_kv(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_string())),
        }),
    }
}

/// One `NumberDataPoint` carrying an `as_double` value, with the given
/// point-level attributes folded in.
fn double_point(time: u64, value: f64, attrs: Vec<KeyValue>) -> NumberDataPoint {
    NumberDataPoint {
        attributes: attrs,
        start_time_unix_nano: 0,
        time_unix_nano: time,
        exemplars: vec![],
        flags: 0,
        value: Some(number_data_point::Value::AsDouble(value)),
    }
}

/// One `NumberDataPoint` carrying an `as_int` value (DD11: maps to exact
/// `f64`).
fn int_point(time: u64, value: i64, attrs: Vec<KeyValue>) -> NumberDataPoint {
    NumberDataPoint {
        attributes: attrs,
        start_time_unix_nano: 0,
        time_unix_nano: time,
        exemplars: vec![],
        flags: 0,
        value: Some(number_data_point::Value::AsInt(value)),
    }
}

/// One `NumberDataPoint` whose value oneof is UNSET — the OTLP-defined
/// "invalid" point ("A point is considered invalid when one of the
/// recognized value fields is not present inside this oneof").
fn value_less_point(time: u64) -> NumberDataPoint {
    NumberDataPoint {
        attributes: vec![],
        start_time_unix_nano: 0,
        time_unix_nano: time,
        exemplars: vec![],
        flags: 0,
        value: None,
    }
}

/// A proto `Metric` whose data is a Gauge of the supplied number points.
fn gauge_metric(name: &str, unit: &str, points: Vec<NumberDataPoint>) -> Metric {
    Metric {
        name: name.to_string(),
        description: String::new(),
        unit: unit.to_string(),
        metadata: vec![],
        data: Some(proto_metric::Data::Gauge(Gauge {
            data_points: points,
        })),
    }
}

/// A proto `Metric` whose data is a (cumulative, monotonic) Sum of the
/// supplied number points.
fn sum_metric(name: &str, unit: &str, points: Vec<NumberDataPoint>) -> Metric {
    Metric {
        name: name.to_string(),
        description: String::new(),
        unit: unit.to_string(),
        metadata: vec![],
        data: Some(proto_metric::Data::Sum(Sum {
            data_points: points,
            // AggregationTemporality::Cumulative = 2 on the wire.
            aggregation_temporality: 2,
            is_monotonic: true,
        })),
    }
}

/// A proto `Metric` whose data is a Histogram — unsupported at pulse v0
/// (DD8). One histogram data point, enough to make it a real histogram.
fn histogram_metric(name: &str, unit: &str) -> Metric {
    Metric {
        name: name.to_string(),
        description: String::new(),
        unit: unit.to_string(),
        metadata: vec![],
        data: Some(proto_metric::Data::Histogram(Histogram {
            data_points: vec![HistogramDataPoint {
                attributes: vec![],
                start_time_unix_nano: 0,
                time_unix_nano: 1_716_240_000_000_000_000,
                count: 3,
                sum: Some(12.0),
                bucket_counts: vec![1, 2],
                explicit_bounds: vec![5.0],
                exemplars: vec![],
                flags: 0,
                min: Some(2.0),
                max: Some(7.0),
            }],
            aggregation_temporality: 2,
        })),
    }
}

/// Build an `ExportMetricsServiceRequest` for one service, with the given
/// resource attributes folded in and the supplied proto metrics on one
/// ScopeMetrics.
fn metrics_request(
    service_name: &str,
    extra_resource_attrs: Vec<KeyValue>,
    metrics: Vec<Metric>,
) -> ExportMetricsServiceRequest {
    let mut resource_attrs = vec![string_kv("service.name", service_name)];
    resource_attrs.extend(extra_resource_attrs);

    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: resource_attrs,
                dropped_attributes_count: 0,
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: Some(InstrumentationScope {
                    name: "aperture-storage-sink.test".to_string(),
                    version: "0.0.0".to_string(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                metrics,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

// Canonical metric names reused across scenarios.
const CPU: &str = "process.cpu.utilization";
const REQUESTS: &str = "http.server.request.count";
const LATENCY: &str = "http.server.duration";

/// The canonical slice-03 gauge payload: `process.cpu.utilization`
/// (unit "1") for checkout-api with one point value 0.42 at
/// 1716240000000000000 ns (US-03 domain example 1).
fn cpu_gauge_request() -> ExportMetricsServiceRequest {
    metrics_request(
        "checkout-api",
        vec![],
        vec![gauge_metric(
            CPU,
            "1",
            vec![double_point(1_716_240_000_000_000_000, 0.42, vec![])],
        )],
    )
}

// =========================================================================
// Walking skeleton — the operator sends a metric and finds it in pulse
// =========================================================================

// @walking_skeleton @driving_port @real-io @adapter-integration @US-03
//
// Strategy: real local filesystem adapter (FileBackedMetricStore over a
// tmp dir). If the real adapter were deleted this skeleton could not
// pass, so it proves wiring, not an in-memory double. The user goal:
// Priya exports a gauge and later queries the point back, field-faithful.
#[tokio::test]
async fn operator_exports_a_metric_and_finds_it_in_pulse() {
    let base = temp_base("ws_export_and_find");
    let store = open_metric_store(&base);

    let sink = StorageSink::with_metric_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    sink.accept(SinkRecord::Metrics(cpu_gauge_request()))
        .await
        .expect("the gateway accepts the metric");

    let found = store
        .query(&tenant("acme"), &metric_name(CPU), TimeRange::all())
        .expect("query pulse for acme");

    assert_eq!(
        found.len(),
        1,
        "exactly the one exported point is queryable"
    );
    let (metric, point) = &found[0];
    assert_eq!(point.value, 0.42);
    assert_eq!(metric.kind, MetricKind::Gauge);
    assert_eq!(
        metric
            .resource_attributes
            .get("service.name")
            .map(String::as_str),
        Some("checkout-api"),
    );

    cleanup(&base);
}

// =========================================================================
// Faithful translation — gauge: every mapped field round-trips
// =========================================================================

// @driving_port @US-03
//
// Asserts the field-by-field gauge translation contract (arch section
// 6.3): metric name, unit, kind (OTLP Gauge -> pulse Gauge), point time,
// value (as_double -> f64), and resource service.name fold.
#[tokio::test]
async fn persisted_gauge_faithfully_reflects_the_translated_fields() {
    let base = temp_base("gauge_faithful");
    let store = open_metric_store(&base);
    let sink = StorageSink::with_metric_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let req = metrics_request(
        "checkout-api",
        vec![],
        vec![gauge_metric(
            CPU,
            "1",
            vec![double_point(1_716_240_000_000_000_000, 0.42, vec![])],
        )],
    );
    sink.accept(SinkRecord::Metrics(req))
        .await
        .expect("accept the checkout-api gauge");

    let found = store
        .query(&tenant("acme"), &metric_name(CPU), TimeRange::all())
        .expect("query");
    assert_eq!(found.len(), 1);
    let (metric, point) = &found[0];

    assert_eq!(metric.name, metric_name(CPU));
    assert_eq!(metric.unit, "1", "unit is folded through");
    assert_eq!(
        metric.kind,
        MetricKind::Gauge,
        "OTLP Gauge maps to pulse Gauge"
    );
    assert_eq!(point.time_unix_nano, 1_716_240_000_000_000_000);
    assert_eq!(point.value, 0.42);
    assert_eq!(
        metric
            .resource_attributes
            .get("service.name")
            .map(String::as_str),
        Some("checkout-api"),
        "resource service.name is folded through",
    );

    cleanup(&base);
}

// =========================================================================
// Faithful translation — sum, including as_int -> f64 (DD11)
// =========================================================================

// @driving_port @US-03
//
// A Sum metric with a point carrying a point-level attribute persists
// with MetricKind::Sum, the attribute folded through, and the value
// faithful (US-03 domain example 2).
#[tokio::test]
async fn persisted_sum_reflects_kind_value_and_point_attribute() {
    let base = temp_base("sum_faithful");
    let store = open_metric_store(&base);
    let sink = StorageSink::with_metric_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let req = metrics_request(
        "billing-worker",
        vec![],
        vec![sum_metric(
            REQUESTS,
            "1",
            vec![double_point(
                1_716_240_000_000_000_000,
                7.0,
                vec![string_kv("http.route", "/charge")],
            )],
        )],
    );
    sink.accept(SinkRecord::Metrics(req))
        .await
        .expect("accept the billing-worker sum");

    let found = store
        .query(&tenant("acme"), &metric_name(REQUESTS), TimeRange::all())
        .expect("query");
    assert_eq!(found.len(), 1);
    let (metric, point) = &found[0];

    assert_eq!(metric.kind, MetricKind::Sum, "OTLP Sum maps to pulse Sum");
    assert_eq!(point.value, 7.0);
    assert_eq!(
        point.attributes.get("http.route").map(String::as_str),
        Some("/charge"),
        "point-level attribute is folded through",
    );

    cleanup(&base);
}

// @driving_port @US-03
//
// A Sum point carrying an `as_int` value maps to its exact `f64`
// representation (DD11). Guards the integer half of the value oneof: a
// translator that only ever reads `as_double` would drop the value.
#[tokio::test]
async fn a_sum_with_an_integer_value_maps_to_exact_f64() {
    let base = temp_base("sum_as_int");
    let store = open_metric_store(&base);
    let sink = StorageSink::with_metric_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let req = metrics_request(
        "billing-worker",
        vec![],
        vec![sum_metric(
            REQUESTS,
            "1",
            vec![int_point(1_716_240_000_000_000_000, 42, vec![])],
        )],
    );
    sink.accept(SinkRecord::Metrics(req))
        .await
        .expect("accept the integer-valued sum");

    let found = store
        .query(&tenant("acme"), &metric_name(REQUESTS), TimeRange::all())
        .expect("query");
    assert_eq!(found.len(), 1);
    let (metric, point) = &found[0];
    assert_eq!(metric.kind, MetricKind::Sum);
    assert_eq!(point.value, 42.0, "as_int 42 maps to exact f64 42.0");

    cleanup(&base);
}

// =========================================================================
// Skip-not-refuse for unsupported types (DD8 / ADR-0041 Decision 3) —
// the heart of the AC reconciliation (distill/upstream-issues.md)
// =========================================================================

// @driving_port @US-03
//
// A request carrying BOTH a supported gauge AND an unsupported histogram:
// `accept` returns Ok (NOT refused), the gauge persists, and the
// histogram does not (querying its name returns nothing). Skip-not-refuse:
// the histogram is dropped with an observable event while the supported
// point lives. This is the reconciled US-03 AC (was "refused").
#[tokio::test]
async fn an_unsupported_histogram_is_skipped_while_the_gauge_persists() {
    let base = temp_base("skip_histogram_keep_gauge");
    let store = open_metric_store(&base);
    let sink = StorageSink::with_metric_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let req = metrics_request(
        "checkout-api",
        vec![],
        vec![
            gauge_metric(
                CPU,
                "1",
                vec![double_point(1_716_240_000_000_000_000, 0.42, vec![])],
            ),
            histogram_metric(LATENCY, "ms"),
        ],
    );
    sink.accept(SinkRecord::Metrics(req))
        .await
        .expect("a mixed gauge + histogram request is accepted, not refused");

    let gauges = store
        .query(&tenant("acme"), &metric_name(CPU), TimeRange::all())
        .expect("query gauge");
    assert_eq!(gauges.len(), 1, "the supported gauge persists");
    assert_eq!(gauges[0].1.value, 0.42);

    let histograms = store
        .query(&tenant("acme"), &metric_name(LATENCY), TimeRange::all())
        .expect("query histogram name");
    assert!(
        histograms.is_empty(),
        "the unsupported histogram is skipped, not persisted",
    );

    cleanup(&base);
}

// @driving_port @US-03
//
// A request carrying ONLY an unsupported histogram: `accept` still
// returns Ok (it translates to an empty MetricBatch — nothing to persist,
// not an error), and nothing is queryable. Guards that an empty-after-skip
// payload is accepted rather than refused (the collector-faithful liveness
// property, DD8).
#[tokio::test]
async fn a_request_of_only_unsupported_types_is_accepted_and_persists_nothing() {
    let base = temp_base("only_histogram");
    let store = open_metric_store(&base);
    let sink = StorageSink::with_metric_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let req = metrics_request(
        "checkout-api",
        vec![],
        vec![histogram_metric(LATENCY, "ms")],
    );
    sink.accept(SinkRecord::Metrics(req))
        .await
        .expect("an only-unsupported request is accepted (empty batch), not refused");

    let found = store
        .query(&tenant("acme"), &metric_name(LATENCY), TimeRange::all())
        .expect("query");
    assert!(
        found.is_empty(),
        "nothing was persisted from a skipped-only request"
    );

    cleanup(&base);
}

// @driving_port @US-03
//
// Supported-vs-unsupported distinction at the point level. A value-less
// SUPPORTED point (a NumberDataPoint whose value oneof is unset — the
// OTLP-defined "invalid" point) is handled the least-surprising way:
// skipped per-point (not refused, not defaulted to 0), while a sibling
// well-formed point in the SAME gauge still persists. Contrast with the
// unsupported-TYPE case above (whole metric skipped). The arch mapping
// (section 6.3) does not pin a value-less row, so this encodes the
// least-surprising choice flagged to DELIVER in distill/upstream-issues.md;
// if DESIGN later pins a different rule the assertion updates with it.
#[tokio::test]
async fn a_value_less_supported_point_is_skipped_while_its_sibling_persists() {
    let base = temp_base("value_less_point");
    let store = open_metric_store(&base);
    let sink = StorageSink::with_metric_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let req = metrics_request(
        "checkout-api",
        vec![],
        vec![gauge_metric(
            CPU,
            "1",
            vec![
                value_less_point(1_716_240_000_000_000_000),
                double_point(1_716_240_000_500_000_000, 0.55, vec![]),
            ],
        )],
    );
    sink.accept(SinkRecord::Metrics(req))
        .await
        .expect("a value-less point is skipped, not fatal to the request");

    let found = store
        .query(&tenant("acme"), &metric_name(CPU), TimeRange::all())
        .expect("query");
    assert_eq!(
        found.len(),
        1,
        "only the well-formed sibling point persists; the value-less one is skipped",
    );
    assert_eq!(found[0].1.value, 0.55);
    assert_eq!(found[0].1.time_unix_nano, 1_716_240_000_500_000_000);

    cleanup(&base);
}

// =========================================================================
// Durability — persisted points survive a gateway restart (KPI-3)
// =========================================================================

// @real-io @adapter-integration @US-03
//
// Accept through the sink, drop the store, reopen the
// FileBackedMetricStore at the same pillar_root, and the point is still
// queryable, identical. KPI-3 durability promise: 100% of accepted
// gauge/sum points queryable post-restart.
#[tokio::test]
async fn persisted_metrics_survive_a_gateway_restart() {
    let base = temp_base("durability_restart");

    {
        let store = open_metric_store(&base);
        let sink = StorageSink::with_metric_store(
            Arc::clone(&store),
            StorageSinkConfig::with_default_tenant("acme"),
        );
        sink.accept(SinkRecord::Metrics(cpu_gauge_request()))
            .await
            .expect("accept before restart");
        // sink and store dropped here, simulating process exit.
    }

    // Reopen against the same pillar_root, as a restarted process would.
    let reopened = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    let found = reopened
        .query(&tenant("acme"), &metric_name(CPU), TimeRange::all())
        .expect("query after restart");

    assert_eq!(found.len(), 1, "the point survived the restart");
    let (metric, point) = &found[0];
    assert_eq!(point.value, 0.42, "value identical to before the restart");
    assert_eq!(metric.kind, MetricKind::Gauge);
    assert_eq!(
        metric
            .resource_attributes
            .get("service.name")
            .map(String::as_str),
        Some("checkout-api"),
    );

    cleanup(&base);
}

// =========================================================================
// Tenant resolution (DD3 / ADR-0041 Decision 2) — same rule as slices
// 01 / 02; once-per-accept from the first resource (no per-resource).
// =========================================================================

// @driving_port @US-03
//
// (a) An explicit tenant.id resource attribute wins over default_tenant.
// The point files under globex; acme returns nothing.
#[tokio::test]
async fn explicit_tenant_id_attribute_overrides_the_default_tenant() {
    let base = temp_base("tenant_explicit");
    let store = open_metric_store(&base);
    let sink = StorageSink::with_metric_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    let req = metrics_request(
        "billing-worker",
        vec![string_kv("tenant.id", "globex")],
        vec![sum_metric(
            REQUESTS,
            "1",
            vec![double_point(1_716_240_000_000_000_000, 7.0, vec![])],
        )],
    );
    sink.accept(SinkRecord::Metrics(req))
        .await
        .expect("accept with explicit tenant");

    let globex = store
        .query(&tenant("globex"), &metric_name(REQUESTS), TimeRange::all())
        .expect("query globex");
    let acme = store
        .query(&tenant("acme"), &metric_name(REQUESTS), TimeRange::all())
        .expect("query acme");

    assert_eq!(globex.len(), 1, "filed under the explicit tenant.id");
    assert_eq!(globex[0].1.value, 7.0);
    assert!(acme.is_empty(), "nothing leaks into the default tenant");

    cleanup(&base);
}

// @driving_port @US-03
//
// (b) No tenant.id, but the sink is configured with a default_tenant:
// the point files under the default.
#[tokio::test]
async fn missing_tenant_id_falls_back_to_the_configured_default_tenant() {
    let base = temp_base("tenant_default");
    let store = open_metric_store(&base);
    let sink = StorageSink::with_metric_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    // cpu_gauge_request carries no tenant.id attribute.
    sink.accept(SinkRecord::Metrics(cpu_gauge_request()))
        .await
        .expect("accept under default tenant");

    let found = store
        .query(&tenant("acme"), &metric_name(CPU), TimeRange::all())
        .expect("query");
    assert_eq!(found.len(), 1, "filed under the configured default tenant");
    assert_eq!(found[0].1.value, 0.42);

    cleanup(&base);
}

// @driving_port @US-03
//
// (c) No tenant.id AND no default_tenant configured: the record is
// refused (Err) and NOTHING is written. KPI-5 guardrail — refused implies
// writes nothing, never mis-filed. Tenant refusal is distinct from the
// skip-not-refuse policy: a missing tenant IS fatal (the record cannot be
// filed safely), whereas an unsupported point type is skipped. We probe a
// couple of plausible tenants to assert the store is genuinely empty.
#[tokio::test]
async fn a_metric_with_no_resolvable_tenant_is_refused_and_writes_nothing() {
    let base = temp_base("tenant_unresolvable");
    let store = open_metric_store(&base);
    let sink =
        StorageSink::with_metric_store(Arc::clone(&store), StorageSinkConfig::no_default_tenant());

    let result = sink.accept(SinkRecord::Metrics(cpu_gauge_request())).await;

    assert!(
        result.is_err(),
        "an unresolvable tenant must be refused, not silently dropped",
    );

    // Nothing was written under any plausible tenant.
    for candidate in ["acme", "checkout-api", "default", ""] {
        let leaked = store
            .query(&tenant(candidate), &metric_name(CPU), TimeRange::all())
            .expect("query candidate tenant");
        assert!(
            leaked.is_empty(),
            "refused record must not be filed under tenant {candidate:?}",
        );
    }

    cleanup(&base);
}

// =========================================================================
// Probe (DD5 / Earned-Trust) — startup health check
// =========================================================================

// @driving_port @adapter-integration @US-03
//
// Against a writable pillar_root the probe returns Ok: the metric store
// opened and an active write check succeeds.
#[tokio::test]
async fn probe_returns_ok_when_the_pillar_root_is_writable() {
    let base = temp_base("probe_ok");
    let store = open_metric_store(&base);
    let sink = StorageSink::with_metric_store(
        Arc::clone(&store),
        StorageSinkConfig::with_default_tenant("acme"),
    );

    sink.probe()
        .await
        .expect("probe Ok against a writable metric store");

    cleanup(&base);
}

// @infrastructure-failure @real-io @adapter-integration @US-03
//
// Against a read-only pillar_root the probe must return Err: the
// catalogued substrate lie is "the path opens but is not writable". The
// host binary refuses to start in that case (wire then probe then use).
// Skipped on platforms where the chmod-based read-only setup does not
// take effect (e.g. running as root, or filesystems that ignore the
// permission bits) so the test stays meaningful rather than flaky.
#[cfg(unix)]
#[tokio::test]
async fn probe_returns_err_when_the_pillar_root_is_not_writable() {
    use std::os::unix::fs::PermissionsExt;

    let base = temp_base("probe_readonly");
    // Open once while writable so the snapshot / WAL exist, then drop.
    drop(open_metric_store(&base));

    // Make the containing directory read-only so a probe write fails.
    let parent = base.parent().expect("base has a parent dir").to_path_buf();
    let mut perms = fs::metadata(&parent).expect("metadata").permissions();
    let original_mode = perms.mode();
    perms.set_mode(0o500); // r-x------ : not writable
    fs::set_permissions(&parent, perms).expect("set read-only");

    // If we can still create a file, the read-only bit did not take
    // (running as root or a permissive fs); restore and skip the assertion.
    let writable_anyway = fs::write(parent.join(".probe-write-check"), b"x").is_ok();
    if writable_anyway {
        let _ = fs::remove_file(parent.join(".probe-write-check"));
        let mut restore = fs::metadata(&parent).expect("metadata").permissions();
        restore.set_mode(original_mode);
        let _ = fs::set_permissions(&parent, restore);
        cleanup(&base);
        eprintln!("skipping read-only probe assertion: directory remained writable");
        return;
    }

    // Reopening against the read-only path either fails to open or the
    // sink's probe fails the active write check. Either way the operator
    // cannot trust this pillar_root, which is what we assert.
    match FileBackedMetricStore::open(&base, Box::new(NoopRecorder)) {
        Ok(store) => {
            let sink = StorageSink::with_metric_store(
                Arc::new(store),
                StorageSinkConfig::with_default_tenant("acme"),
            );
            assert!(
                sink.probe().await.is_err(),
                "probe must refuse a non-writable pillar_root",
            );
        }
        Err(_) => {
            // Open itself refused the unwritable path: also an acceptable
            // wire-then-probe-then-use refusal.
        }
    }

    // Restore permissions so cleanup can remove the tree.
    let mut restore = fs::metadata(&parent).expect("metadata").permissions();
    restore.set_mode(original_mode);
    let _ = fs::set_permissions(&parent, restore);
    cleanup(&base);
}
