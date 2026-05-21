// Kaleidoscope aperture-storage-sink — OTLP-to-pillar translation
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

//! Field-by-field OTLP-to-lumen translation (ADR-0041 Decision 1,
//! application-architecture.md section 6.1 / 6.4 / 6.5).
//!
//! The whole request is translated to completion BEFORE any ingest
//! (DD7 atomicity). A wrong-length byte-array identifier is a
//! [`TranslationError`] that refuses the entire accept; nothing is
//! persisted.

use std::collections::BTreeMap;

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, KeyValue};

use lumen::{LogRecord, SeverityNumber};
use pulse::{Metric as PulseMetric, MetricKind, MetricName, MetricPoint};
use ray::{
    Span as RaySpan, SpanEvent, SpanId, SpanKind, SpanLink, SpanStatus, StatusCode, TraceId,
};

/// The resource attribute that names the tenant a record belongs to
/// (DD3 / ADR-0041 Decision 2). OTel-namespaced, consistent with aegis.
pub(crate) const TENANT_ID_ATTRIBUTE: &str = "tenant.id";

/// A translation refusal. Names the offending field so the operator can
/// see why the whole accept was refused (DD6 / DD7). Maps to
/// `SinkError::Internal` at the port boundary.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TranslationError {
    /// A `trace_id` / `span_id` byte array was neither empty nor the
    /// exact required length. Carries the field name and the actual
    /// length so the refusal is diagnosable.
    MalformedByteId {
        field: &'static str,
        expected: usize,
        actual: usize,
    },
}

impl std::fmt::Display for TranslationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranslationError::MalformedByteId {
                field,
                expected,
                actual,
            } => write!(
                f,
                "malformed {field}: expected {expected} bytes, got {actual}"
            ),
        }
    }
}

/// Resolve the tenant for an accept from the FIRST resource's
/// attributes (DD3, v0 one-tenant-per-export). Returns the `tenant.id`
/// resource attribute value when present; otherwise `None` so the
/// caller can fall back to the configured `default_tenant` or refuse.
pub(crate) fn resolve_tenant_id(request: &ExportLogsServiceRequest) -> Option<String> {
    let first = request.resource_logs.first()?;
    let resource = first.resource.as_ref()?;
    for kv in &resource.attributes {
        if kv.key == TENANT_ID_ATTRIBUTE {
            return Some(any_value_to_string(kv.value.as_ref()));
        }
    }
    None
}

/// Resolve the tenant for a traces accept from the FIRST resource's
/// attributes (DD3 / ADR-0041 Decision 2, v0 one-tenant-per-export).
/// Mirrors [`resolve_tenant_id`] for the traces signal: the tenant is
/// resolved ONCE per accept from the first `ResourceSpans` — NOT
/// per-resource — because ADR-0041 Decision 2 / DD3 fix "one tenant per
/// export at v0; mixed-tenant batches deferred to v1". Returns the
/// `tenant.id` resource attribute value when present; otherwise `None`
/// so the caller can fall back to the configured `default_tenant` or
/// refuse.
pub(crate) fn resolve_trace_tenant_id(request: &ExportTraceServiceRequest) -> Option<String> {
    let first = request.resource_spans.first()?;
    let resource = first.resource.as_ref()?;
    for kv in &resource.attributes {
        if kv.key == TENANT_ID_ATTRIBUTE {
            return Some(any_value_to_string(kv.value.as_ref()));
        }
    }
    None
}

/// Resolve the tenant for a metrics accept from the FIRST resource's
/// attributes (DD3 / ADR-0041 Decision 2, v0 one-tenant-per-export).
/// Mirrors [`resolve_tenant_id`] / [`resolve_trace_tenant_id`] for the
/// metrics signal: the tenant is resolved ONCE per accept from the
/// first `ResourceMetrics` — NOT per-resource. Returns the `tenant.id`
/// resource attribute value when present; otherwise `None` so the
/// caller can fall back to the configured `default_tenant` or refuse.
pub(crate) fn resolve_metric_tenant_id(request: &ExportMetricsServiceRequest) -> Option<String> {
    let first = request.resource_metrics.first()?;
    let resource = first.resource.as_ref()?;
    for kv in &resource.attributes {
        if kv.key == TENANT_ID_ATTRIBUTE {
            return Some(any_value_to_string(kv.value.as_ref()));
        }
    }
    None
}

/// Translate the whole `ExportMetricsServiceRequest` into `pulse`
/// metrics (section 6.3, ADR-0041 Decisions 1 / 3). Runs to completion
/// before the caller ingests anything (DD7 atomicity).
///
/// Skip-not-refuse (DD8 / ADR-0041 Decision 3): a metric whose data is
/// an unsupported point type at pulse v0 (Histogram /
/// ExponentialHistogram / Summary) is SKIPPED with an observable
/// `metric_point_type_skipped` event — NOT refused. A supported metric
/// (Gauge / Sum) with a value-less point (the OTLP-defined "invalid"
/// point whose value oneof is unset) skips THAT individual point with a
/// `metric_point_value_unset` event while well-formed sibling points
/// still translate. A request of only-unsupported types yields an empty
/// `Vec` (nothing to persist, NOT an error).
pub(crate) fn translate_metrics(request: &ExportMetricsServiceRequest) -> Vec<PulseMetric> {
    let mut metrics = Vec::new();
    for resource_metrics in &request.resource_metrics {
        let resource_attributes = fold_metric_resource_attributes(resource_metrics);
        for scope_metrics in &resource_metrics.scope_metrics {
            for proto in &scope_metrics.metrics {
                if let Some(metric) = translate_metric(proto, &resource_attributes) {
                    metrics.push(metric);
                }
            }
        }
    }
    metrics
}

/// Fold a `ResourceMetrics`' resource attributes into the shared
/// `BTreeMap` shape (section 6.4). An absent resource yields an empty
/// map. The `service.name` lands here so the persisted
/// `Metric::resource_attributes` reads it back.
fn fold_metric_resource_attributes(
    resource_metrics: &opentelemetry_proto::tonic::metrics::v1::ResourceMetrics,
) -> BTreeMap<String, String> {
    match resource_metrics.resource.as_ref() {
        Some(resource) => fold_attributes(&resource.attributes),
        None => BTreeMap::new(),
    }
}

/// Translate one proto `Metric` against its already-folded resource
/// attributes (section 6.3). Returns `None` (skip the whole metric)
/// when the data oneof is an unsupported pulse-v0 type or is absent,
/// emitting the observable skip event. A supported Gauge / Sum maps to
/// the matching [`MetricKind`] with its value-bearing points folded in;
/// value-less points are dropped individually.
fn translate_metric(
    proto: &opentelemetry_proto::tonic::metrics::v1::Metric,
    resource_attributes: &BTreeMap<String, String>,
) -> Option<PulseMetric> {
    use opentelemetry_proto::tonic::metrics::v1::metric::Data;
    let (kind, data_points) = match proto.data.as_ref() {
        Some(Data::Gauge(gauge)) => (MetricKind::Gauge, &gauge.data_points),
        Some(Data::Sum(sum)) => (MetricKind::Sum, &sum.data_points),
        other => {
            emit_metric_point_type_skipped(&proto.name, unsupported_metric_kind(other));
            return None;
        }
    };
    let points = translate_number_points(&proto.name, data_points);
    Some(PulseMetric {
        name: MetricName::new(proto.name.clone()),
        description: proto.description.clone(),
        unit: proto.unit.clone(),
        kind,
        points,
        resource_attributes: resource_attributes.clone(),
    })
}

/// Translate the supported `NumberDataPoint`s of a Gauge / Sum into
/// pulse [`MetricPoint`]s. A point whose value oneof is unset (the
/// OTLP-defined "invalid" point) is SKIPPED individually with an
/// observable `metric_point_value_unset` event; an `as_int` value maps
/// to its exact `f64` (DD11).
fn translate_number_points(
    metric_name: &str,
    data_points: &[opentelemetry_proto::tonic::metrics::v1::NumberDataPoint],
) -> Vec<MetricPoint> {
    let mut points = Vec::new();
    for proto in data_points {
        match number_point_value(proto) {
            Some(value) => points.push(MetricPoint {
                time_unix_nano: proto.time_unix_nano,
                start_time_unix_nano: proto.start_time_unix_nano,
                attributes: fold_attributes(&proto.attributes),
                value,
            }),
            None => {
                emit_metric_point_value_unset(metric_name);
            }
        }
    }
    points
}

/// Read a `NumberDataPoint`'s value oneof (section 6.3 / DD11): prefer
/// `as_double` verbatim; map `as_int` to its exact `f64`; an unset value
/// oneof (the OTLP "invalid" point) yields `None` so the caller skips
/// the point.
fn number_point_value(
    proto: &opentelemetry_proto::tonic::metrics::v1::NumberDataPoint,
) -> Option<f64> {
    use opentelemetry_proto::tonic::metrics::v1::number_data_point::Value;
    match proto.value.as_ref()? {
        Value::AsDouble(d) => Some(*d),
        Value::AsInt(i) => Some(*i as f64),
    }
}

/// Name the unsupported data oneof for the skip event so an operator
/// sees WHICH pulse-v0-unsupported type was dropped. An absent oneof is
/// reported as `none`.
fn unsupported_metric_kind(
    data: Option<&opentelemetry_proto::tonic::metrics::v1::metric::Data>,
) -> &'static str {
    use opentelemetry_proto::tonic::metrics::v1::metric::Data;
    match data {
        Some(Data::Histogram(_)) => "histogram",
        Some(Data::ExponentialHistogram(_)) => "exponential_histogram",
        Some(Data::Summary(_)) => "summary",
        Some(Data::Gauge(_)) | Some(Data::Sum(_)) => "supported",
        None => "none",
    }
}

/// Emit the `metric_point_type_skipped` warn line for an unsupported
/// metric data type (DD8 / ADR-0041 Decision 3) and return the metric
/// name it logged. Returning the name (rather than `()`) makes the
/// emission observable to an inline test, so a mutation that drops the
/// body is caught.
fn emit_metric_point_type_skipped(metric_name: &str, data_kind: &'static str) -> String {
    tracing::warn!(
        event = "metric_point_type_skipped",
        sink = "storage",
        metric = metric_name,
        data_kind = data_kind,
        "unsupported metric data type skipped: pulse v0 persists gauge / sum only",
    );
    metric_name.to_string()
}

/// Emit the `metric_point_value_unset` warn line for a value-less
/// supported point (the OTLP-defined "invalid" point) and return the
/// metric name it logged. Returning the name makes the emission
/// observable to an inline test.
fn emit_metric_point_value_unset(metric_name: &str) -> String {
    tracing::warn!(
        event = "metric_point_value_unset",
        sink = "storage",
        metric = metric_name,
        "value-less metric point skipped: the OTLP value oneof was unset",
    );
    metric_name.to_string()
}

/// Translate the whole `ExportTraceServiceRequest` into `ray` spans
/// (section 6.1, ADR-0041 Decision 1). Runs to completion before the
/// caller ingests anything (DD7 atomicity): a malformed byte-array
/// identifier on ANY span (or any of its links) refuses the whole
/// request and yields no spans, so an otherwise-valid sibling in the
/// same batch is never persisted.
pub(crate) fn translate_traces(
    request: &ExportTraceServiceRequest,
) -> Result<Vec<RaySpan>, TranslationError> {
    let mut spans = Vec::new();
    for resource_spans in &request.resource_spans {
        let resource_attributes = fold_trace_resource_attributes(resource_spans);
        for scope_spans in &resource_spans.scope_spans {
            for proto in &scope_spans.spans {
                spans.push(translate_span(proto, &resource_attributes)?);
            }
        }
    }
    Ok(spans)
}

/// Fold a `ResourceSpans`' resource attributes into the shared
/// `BTreeMap` shape (section 6.4). An absent resource yields an empty
/// map. The `service.name` lands here so `ray::Span::service_name`
/// reads it back.
fn fold_trace_resource_attributes(
    resource_spans: &opentelemetry_proto::tonic::trace::v1::ResourceSpans,
) -> BTreeMap<String, String> {
    match resource_spans.resource.as_ref() {
        Some(resource) => fold_attributes(&resource.attributes),
        None => BTreeMap::new(),
    }
}

/// Translate one proto `Span` against its already-folded resource
/// attributes (section 6.1). Every byte-array identifier is
/// length-validated; a wrong-length trace_id / span_id / parent_span_id
/// — or any link's trace_id / span_id — refuses the whole accept (DD7).
fn translate_span(
    proto: &opentelemetry_proto::tonic::trace::v1::Span,
    resource_attributes: &BTreeMap<String, String>,
) -> Result<RaySpan, TranslationError> {
    Ok(RaySpan {
        trace_id: require_trace_id(&proto.trace_id, "trace_id")?,
        span_id: require_span_id(&proto.span_id, "span_id")?,
        parent_span_id: byte_id_8(&proto.parent_span_id, "parent_span_id")?.map(SpanId),
        name: proto.name.clone(),
        kind: span_kind_from_i32(proto.kind),
        start_time_unix_nano: proto.start_time_unix_nano,
        end_time_unix_nano: proto.end_time_unix_nano,
        status: span_status(proto.status.as_ref()),
        attributes: fold_attributes(&proto.attributes),
        resource_attributes: resource_attributes.clone(),
        events: translate_events(&proto.events),
        links: translate_links(&proto.links)?,
    })
}

/// A span's `trace_id` is REQUIRED to be exactly 16 bytes — unlike a
/// log's optional correlation id, a span without a trace id is
/// untranslatable. Empty or wrong-length refuses the accept.
fn require_trace_id(bytes: &[u8], field: &'static str) -> Result<TraceId, TranslationError> {
    match byte_id_16(bytes, field)? {
        Some(array) => Ok(TraceId(array)),
        None => Err(TranslationError::MalformedByteId {
            field,
            expected: 16,
            actual: 0,
        }),
    }
}

/// A span's `span_id` is REQUIRED to be exactly 8 bytes. Empty or
/// wrong-length refuses the accept.
fn require_span_id(bytes: &[u8], field: &'static str) -> Result<SpanId, TranslationError> {
    match byte_id_8(bytes, field)? {
        Some(array) => Ok(SpanId(array)),
        None => Err(TranslationError::MalformedByteId {
            field,
            expected: 8,
            actual: 0,
        }),
    }
}

/// OTLP `SpanKind` (i32) -> `ray::SpanKind`. An unknown discriminant
/// maps to `Unspecified` (NOT an error) per ADR-0041: an unrecognised
/// kind is forward-compatible data, not a malformed record.
fn span_kind_from_i32(kind: i32) -> SpanKind {
    use opentelemetry_proto::tonic::trace::v1::span::SpanKind as ProtoKind;
    match ProtoKind::try_from(kind) {
        Ok(ProtoKind::Internal) => SpanKind::Internal,
        Ok(ProtoKind::Server) => SpanKind::Server,
        Ok(ProtoKind::Client) => SpanKind::Client,
        Ok(ProtoKind::Producer) => SpanKind::Producer,
        Ok(ProtoKind::Consumer) => SpanKind::Consumer,
        // ProtoKind::Unspecified or any unknown discriminant.
        _ => SpanKind::Unspecified,
    }
}

/// OTLP `Status` -> `ray::SpanStatus`. An absent status is `Unset` with
/// an empty message; an unknown status-code discriminant maps to
/// `Unset` (NOT an error), same forward-compatible posture as kind.
fn span_status(status: Option<&opentelemetry_proto::tonic::trace::v1::Status>) -> SpanStatus {
    match status {
        None => SpanStatus::default(),
        Some(status) => SpanStatus {
            code: status_code_from_i32(status.code),
            message: status.message.clone(),
        },
    }
}

/// OTLP status `StatusCode` (i32) -> `ray::StatusCode`. Unknown -> Unset.
fn status_code_from_i32(code: i32) -> StatusCode {
    use opentelemetry_proto::tonic::trace::v1::status::StatusCode as ProtoCode;
    match ProtoCode::try_from(code) {
        Ok(ProtoCode::Ok) => StatusCode::Ok,
        Ok(ProtoCode::Error) => StatusCode::Error,
        // ProtoCode::Unset or any unknown discriminant.
        _ => StatusCode::Unset,
    }
}

/// Translate the repeated proto `Span.Event` into `ray::SpanEvent`.
/// Events carry no byte-array ids, so this never refuses; attribute
/// lists fold through the shared `fold_attributes`.
fn translate_events(
    events: &[opentelemetry_proto::tonic::trace::v1::span::Event],
) -> Vec<SpanEvent> {
    events
        .iter()
        .map(|event| SpanEvent {
            time_unix_nano: event.time_unix_nano,
            name: event.name.clone(),
            attributes: fold_attributes(&event.attributes),
        })
        .collect()
}

/// Translate the repeated proto `Span.Link` into `ray::SpanLink`. A
/// link's `trace_id` / `span_id` are subject to the SAME exact-length
/// validation as the span's own ids (DD7): a wrong-length link id
/// refuses the whole accept.
fn translate_links(
    links: &[opentelemetry_proto::tonic::trace::v1::span::Link],
) -> Result<Vec<SpanLink>, TranslationError> {
    let mut translated = Vec::with_capacity(links.len());
    for link in links {
        translated.push(SpanLink {
            trace_id: require_trace_id(&link.trace_id, "link.trace_id")?,
            span_id: require_span_id(&link.span_id, "link.span_id")?,
            attributes: fold_attributes(&link.attributes),
        });
    }
    Ok(translated)
}

/// Translate the whole `ExportLogsServiceRequest` into `lumen`
/// log records (section 6.1). Runs to completion before the caller
/// ingests anything (DD7). A malformed identifier on any record refuses
/// the whole request.
pub(crate) fn translate_logs(
    request: &ExportLogsServiceRequest,
) -> Result<Vec<LogRecord>, TranslationError> {
    let mut records = Vec::new();
    for resource_logs in &request.resource_logs {
        let resource_attributes = fold_resource_attributes(resource_logs);
        for scope_logs in &resource_logs.scope_logs {
            for proto in &scope_logs.log_records {
                records.push(translate_record(proto, &resource_attributes)?);
            }
        }
    }
    Ok(records)
}

/// Fold a `ResourceLogs`' resource attributes into the shared
/// `BTreeMap` shape (section 6.4). An absent resource yields an empty
/// map.
fn fold_resource_attributes(
    resource_logs: &opentelemetry_proto::tonic::logs::v1::ResourceLogs,
) -> BTreeMap<String, String> {
    match resource_logs.resource.as_ref() {
        Some(resource) => fold_attributes(&resource.attributes),
        None => BTreeMap::new(),
    }
}

/// Translate one proto log record against its already-folded resource
/// attributes (section 6.1).
fn translate_record(
    proto: &opentelemetry_proto::tonic::logs::v1::LogRecord,
    resource_attributes: &BTreeMap<String, String>,
) -> Result<LogRecord, TranslationError> {
    Ok(LogRecord {
        observed_time_unix_nano: observed_time(proto),
        severity_number: SeverityNumber(proto.severity_number),
        severity_text: proto.severity_text.clone(),
        body: any_value_to_string(proto.body.as_ref()),
        attributes: fold_attributes(&proto.attributes),
        resource_attributes: resource_attributes.clone(),
        trace_id: byte_id_16(&proto.trace_id, "trace_id")?,
        span_id: byte_id_8(&proto.span_id, "span_id")?,
    })
}

/// Use `observed_time_unix_nano` when non-zero, else fall back to the
/// event `time_unix_nano` (section 6.1). lumen has no event-time field.
fn observed_time(proto: &opentelemetry_proto::tonic::logs::v1::LogRecord) -> u64 {
    if proto.observed_time_unix_nano != 0 {
        proto.observed_time_unix_nano
    } else {
        proto.time_unix_nano
    }
}

/// `trace_id: Vec<u8>` -> `Option<[u8; 16]>`. Empty -> `None`; exactly
/// 16 bytes -> `Some`; any other length refuses the accept (DD7).
fn byte_id_16(bytes: &[u8], field: &'static str) -> Result<Option<[u8; 16]>, TranslationError> {
    byte_id::<16>(bytes, field)
}

/// `span_id: Vec<u8>` -> `Option<[u8; 8]>`. Empty -> `None`; exactly 8
/// bytes -> `Some`; any other length refuses the accept (DD7).
fn byte_id_8(bytes: &[u8], field: &'static str) -> Result<Option<[u8; 8]>, TranslationError> {
    byte_id::<8>(bytes, field)
}

/// Generic exact-length byte-id check shared by trace and span ids.
/// Empty -> `None`; exactly `N` bytes -> `Some([u8; N])`; otherwise a
/// `MalformedByteId` refusal naming the field.
fn byte_id<const N: usize>(
    bytes: &[u8],
    field: &'static str,
) -> Result<Option<[u8; N]>, TranslationError> {
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.len() == N {
        let mut array = [0u8; N];
        array.copy_from_slice(bytes);
        return Ok(Some(array));
    }
    Err(TranslationError::MalformedByteId {
        field,
        expected: N,
        actual: bytes.len(),
    })
}

/// Shared attribute fold (section 6.4): a list of proto `KeyValue` into
/// a deterministic `BTreeMap<String, String>`. Later duplicate keys
/// overwrite earlier ones.
fn fold_attributes(attributes: &[KeyValue]) -> BTreeMap<String, String> {
    let mut folded = BTreeMap::new();
    for kv in attributes {
        folded.insert(kv.key.clone(), any_value_to_string(kv.value.as_ref()));
    }
    folded
}

/// `AnyValue` -> `String` (section 6.5). `None` and an empty `AnyValue`
/// both render to the empty string. Non-string scalar kinds render to
/// their natural string form; `Bytes` to lowercase hex; the two
/// composite kinds (`Array`, `Kvlist`) to a compact bracketed rendering
/// of their recursively-rendered children.
fn any_value_to_string(value: Option<&AnyValue>) -> String {
    match value.and_then(|v| v.value.as_ref()) {
        None => String::new(),
        Some(any_value::Value::StringValue(s)) => s.clone(),
        Some(any_value::Value::BoolValue(b)) => b.to_string(),
        Some(any_value::Value::IntValue(i)) => i.to_string(),
        Some(any_value::Value::DoubleValue(d)) => d.to_string(),
        Some(any_value::Value::BytesValue(bytes)) => hex_lower(bytes),
        Some(any_value::Value::ArrayValue(array)) => render_array(&array.values),
        Some(any_value::Value::KvlistValue(list)) => render_kvlist(&list.values),
    }
}

/// Lowercase hex rendering of a byte slice (section 6.5).
fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Compact `[a,b,c]` rendering of an `ArrayValue`'s children, each
/// rendered through the shared `AnyValue` fold (section 6.5).
fn render_array(values: &[AnyValue]) -> String {
    let rendered: Vec<String> = values
        .iter()
        .map(|v| any_value_to_string(Some(v)))
        .collect();
    format!("[{}]", rendered.join(","))
}

/// Compact `{k=v,k2=v2}` rendering of a `KeyValueList`'s entries, each
/// value rendered through the shared `AnyValue` fold (section 6.5).
fn render_kvlist(values: &[KeyValue]) -> String {
    let rendered: Vec<String> = values
        .iter()
        .map(|kv| format!("{}={}", kv.key, any_value_to_string(kv.value.as_ref())))
        .collect();
    format!("{{{}}}", rendered.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry_proto::tonic::common::v1::{ArrayValue, KeyValueList};
    use opentelemetry_proto::tonic::logs::v1::{
        LogRecord as ProtoLogRecord, ResourceLogs, ScopeLogs,
    };
    use opentelemetry_proto::tonic::resource::v1::Resource;

    // -------------------------------------------------------------------
    // any_value_to_string — one assertion per AnyValue kind. The
    // acceptance suite only ever exercises the StringValue and None
    // arms; the scalar / bytes / composite arms are reachable only
    // here, so they carry their own inline pins (mutation survivors the
    // port-level tests cannot reach).
    // -------------------------------------------------------------------

    fn any(value: any_value::Value) -> AnyValue {
        AnyValue { value: Some(value) }
    }

    #[test]
    fn any_value_none_renders_empty_string() {
        assert_eq!(any_value_to_string(None), "");
    }

    #[test]
    fn any_value_empty_anyvalue_renders_empty_string() {
        assert_eq!(any_value_to_string(Some(&AnyValue { value: None })), "");
    }

    #[test]
    fn any_value_string_renders_verbatim() {
        let v = any(any_value::Value::StringValue(
            "order 1001 placed".to_string(),
        ));
        assert_eq!(any_value_to_string(Some(&v)), "order 1001 placed");
    }

    #[test]
    fn any_value_bool_renders_true_or_false() {
        assert_eq!(
            any_value_to_string(Some(&any(any_value::Value::BoolValue(true)))),
            "true"
        );
        assert_eq!(
            any_value_to_string(Some(&any(any_value::Value::BoolValue(false)))),
            "false"
        );
    }

    #[test]
    fn any_value_int_renders_decimal() {
        let v = any(any_value::Value::IntValue(-42));
        assert_eq!(any_value_to_string(Some(&v)), "-42");
    }

    #[test]
    fn any_value_double_renders_round_trip() {
        let v = any(any_value::Value::DoubleValue(1.5));
        assert_eq!(any_value_to_string(Some(&v)), "1.5");
    }

    #[test]
    fn any_value_bytes_renders_lowercase_hex() {
        let v = any(any_value::Value::BytesValue(vec![0x0a, 0xff, 0x10]));
        assert_eq!(any_value_to_string(Some(&v)), "0aff10");
    }

    #[test]
    fn any_value_array_renders_bracketed_children() {
        let v = any(any_value::Value::ArrayValue(ArrayValue {
            values: vec![
                any(any_value::Value::IntValue(1)),
                any(any_value::Value::StringValue("two".to_string())),
            ],
        }));
        assert_eq!(any_value_to_string(Some(&v)), "[1,two]");
    }

    #[test]
    fn any_value_kvlist_renders_braced_entries() {
        let v = any(any_value::Value::KvlistValue(KeyValueList {
            values: vec![KeyValue {
                key: "k".to_string(),
                value: Some(any(any_value::Value::IntValue(7))),
            }],
        }));
        assert_eq!(any_value_to_string(Some(&v)), "{k=7}");
    }

    // -------------------------------------------------------------------
    // byte id length check — the wrong-length refusal and the empty /
    // exact arms. The acceptance suite covers the trace_id 7-byte
    // refusal and the 16/8-byte happy path; these pins close the
    // span_id refusal arm and the per-field naming a mutation could
    // collapse.
    // -------------------------------------------------------------------

    #[test]
    fn byte_id_16_empty_is_none() {
        assert_eq!(byte_id_16(&[], "trace_id"), Ok(None));
    }

    #[test]
    fn byte_id_16_exact_is_some() {
        assert_eq!(byte_id_16(&[0xAB; 16], "trace_id"), Ok(Some([0xAB; 16])));
    }

    #[test]
    fn byte_id_16_wrong_length_refuses_naming_the_field() {
        assert_eq!(
            byte_id_16(&[0x11; 7], "trace_id"),
            Err(TranslationError::MalformedByteId {
                field: "trace_id",
                expected: 16,
                actual: 7,
            })
        );
    }

    #[test]
    fn byte_id_8_wrong_length_refuses_naming_the_field() {
        assert_eq!(
            byte_id_8(&[0x11; 3], "span_id"),
            Err(TranslationError::MalformedByteId {
                field: "span_id",
                expected: 8,
                actual: 3,
            })
        );
    }

    #[test]
    fn translation_error_display_names_field_and_lengths() {
        let err = TranslationError::MalformedByteId {
            field: "trace_id",
            expected: 16,
            actual: 7,
        };
        let rendered = err.to_string();
        assert!(rendered.contains("trace_id"), "got: {rendered}");
        assert!(rendered.contains("16"), "got: {rendered}");
        assert!(rendered.contains("7"), "got: {rendered}");
    }

    // -------------------------------------------------------------------
    // observed_time fallback — observed when non-zero, else event time.
    // The acceptance suite always sends observed == time, so the
    // fallback arm is reachable only here.
    // -------------------------------------------------------------------

    #[test]
    fn observed_time_prefers_observed_when_non_zero() {
        let proto = ProtoLogRecord {
            time_unix_nano: 100,
            observed_time_unix_nano: 200,
            ..Default::default()
        };
        assert_eq!(observed_time(&proto), 200);
    }

    #[test]
    fn observed_time_falls_back_to_event_time_when_observed_is_zero() {
        let proto = ProtoLogRecord {
            time_unix_nano: 100,
            observed_time_unix_nano: 0,
            ..Default::default()
        };
        assert_eq!(observed_time(&proto), 100);
    }

    // -------------------------------------------------------------------
    // resolve_tenant_id — present, absent, no-resource branches. The
    // acceptance suite covers the explicit-id and the missing-id paths
    // through the port; the no-resource and empty-request arms are
    // pinned here.
    // -------------------------------------------------------------------

    fn request_with_resource(attributes: Vec<KeyValue>) -> ExportLogsServiceRequest {
        ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                resource: Some(Resource {
                    attributes,
                    dropped_attributes_count: 0,
                }),
                scope_logs: vec![ScopeLogs::default()],
                schema_url: String::new(),
            }],
        }
    }

    #[test]
    fn resolve_tenant_id_returns_explicit_attribute() {
        let request = request_with_resource(vec![KeyValue {
            key: "tenant.id".to_string(),
            value: Some(any(any_value::Value::StringValue("globex".to_string()))),
        }]);
        assert_eq!(resolve_tenant_id(&request), Some("globex".to_string()));
    }

    #[test]
    fn resolve_tenant_id_returns_none_when_attribute_absent() {
        let request = request_with_resource(vec![KeyValue {
            key: "service.name".to_string(),
            value: Some(any(any_value::Value::StringValue("checkout".to_string()))),
        }]);
        assert_eq!(resolve_tenant_id(&request), None);
    }

    #[test]
    fn resolve_tenant_id_returns_none_when_no_resource_logs() {
        let request = ExportLogsServiceRequest {
            resource_logs: vec![],
        };
        assert_eq!(resolve_tenant_id(&request), None);
    }

    // -------------------------------------------------------------------
    // translate_logs — atomicity: a malformed sibling refuses the whole
    // request and yields no records (the port test asserts the store is
    // empty; this pins the translator returns Err before any record is
    // produced).
    // -------------------------------------------------------------------

    // -------------------------------------------------------------------
    // Traces — kind / status i32 mapping. The acceptance suite exercises
    // Server, Client, Internal kinds and the Ok status through the port,
    // but the Producer / Consumer / Unspecified / unknown kinds and the
    // Error / Unset / unknown status codes are reachable only here. These
    // pins kill the per-arm mapping mutants (an arm collapsed to a single
    // variant, or an unknown discriminant turned into a panic/error).
    // -------------------------------------------------------------------

    #[test]
    fn span_kind_maps_each_known_discriminant() {
        use opentelemetry_proto::tonic::trace::v1::span::SpanKind as ProtoKind;
        assert_eq!(
            span_kind_from_i32(ProtoKind::Internal as i32),
            SpanKind::Internal
        );
        assert_eq!(
            span_kind_from_i32(ProtoKind::Server as i32),
            SpanKind::Server
        );
        assert_eq!(
            span_kind_from_i32(ProtoKind::Client as i32),
            SpanKind::Client
        );
        assert_eq!(
            span_kind_from_i32(ProtoKind::Producer as i32),
            SpanKind::Producer
        );
        assert_eq!(
            span_kind_from_i32(ProtoKind::Consumer as i32),
            SpanKind::Consumer
        );
    }

    #[test]
    fn span_kind_unspecified_and_unknown_map_to_unspecified() {
        use opentelemetry_proto::tonic::trace::v1::span::SpanKind as ProtoKind;
        assert_eq!(
            span_kind_from_i32(ProtoKind::Unspecified as i32),
            SpanKind::Unspecified
        );
        // An out-of-range discriminant is forward-compatible data, not an
        // error: it folds to Unspecified.
        assert_eq!(span_kind_from_i32(9999), SpanKind::Unspecified);
    }

    #[test]
    fn status_code_maps_known_and_unknown() {
        use opentelemetry_proto::tonic::trace::v1::status::StatusCode as ProtoCode;
        assert_eq!(status_code_from_i32(ProtoCode::Ok as i32), StatusCode::Ok);
        assert_eq!(
            status_code_from_i32(ProtoCode::Error as i32),
            StatusCode::Error
        );
        assert_eq!(
            status_code_from_i32(ProtoCode::Unset as i32),
            StatusCode::Unset
        );
        // Unknown discriminant -> Unset (forward-compatible, not an error).
        assert_eq!(status_code_from_i32(9999), StatusCode::Unset);
    }

    #[test]
    fn span_status_none_is_unset_with_empty_message() {
        let status = span_status(None);
        assert_eq!(status.code, StatusCode::Unset);
        assert_eq!(status.message, "");
    }

    #[test]
    fn span_status_carries_code_and_message() {
        use opentelemetry_proto::tonic::trace::v1::Status as ProtoStatus;
        let proto = ProtoStatus {
            code: 2, // Error
            message: "boom".to_string(),
        };
        let status = span_status(Some(&proto));
        assert_eq!(status.code, StatusCode::Error);
        assert_eq!(status.message, "boom");
    }

    // -------------------------------------------------------------------
    // Traces — required id length. A span trace_id / span_id is REQUIRED
    // (empty is untranslatable, unlike a log's optional correlation id).
    // The acceptance suite covers the 7-byte trace_id and 5-byte span_id
    // refusals and the happy path; these pins close the empty-required
    // and parent-empty->None arms a mutation could collapse.
    // -------------------------------------------------------------------

    #[test]
    fn require_trace_id_rejects_empty() {
        assert!(require_trace_id(&[], "trace_id").is_err());
    }

    #[test]
    fn require_trace_id_accepts_exactly_16() {
        assert_eq!(
            require_trace_id(&[0xAB; 16], "trace_id"),
            Ok(TraceId([0xAB; 16]))
        );
    }

    #[test]
    fn require_span_id_rejects_empty_and_accepts_exactly_8() {
        assert!(require_span_id(&[], "span_id").is_err());
        assert_eq!(
            require_span_id(&[0x01; 8], "span_id"),
            Ok(SpanId([0x01; 8]))
        );
    }

    // -------------------------------------------------------------------
    // Traces — events / links translation. The port test asserts one
    // event and one link round-trip; these pins close the empty-list,
    // multi-element, and link-id-length arms (a wrong-length link id
    // refuses the whole accept, the same rule as the span's own ids).
    // -------------------------------------------------------------------

    #[test]
    fn translate_events_maps_name_time_and_attributes() {
        use opentelemetry_proto::tonic::trace::v1::span::Event as ProtoEvent;
        let events = vec![
            ProtoEvent {
                time_unix_nano: 10,
                name: "first".to_string(),
                attributes: vec![KeyValue {
                    key: "k".to_string(),
                    value: Some(any(any_value::Value::StringValue("v".to_string()))),
                }],
                dropped_attributes_count: 0,
            },
            ProtoEvent {
                time_unix_nano: 20,
                name: "second".to_string(),
                attributes: vec![],
                dropped_attributes_count: 0,
            },
        ];
        let translated = translate_events(&events);
        assert_eq!(translated.len(), 2);
        assert_eq!(translated[0].name, "first");
        assert_eq!(translated[0].time_unix_nano, 10);
        assert_eq!(
            translated[0].attributes.get("k").map(String::as_str),
            Some("v")
        );
        assert_eq!(translated[1].name, "second");
        assert!(translated[1].attributes.is_empty());
    }

    #[test]
    fn translate_events_on_empty_yields_empty() {
        assert!(translate_events(&[]).is_empty());
    }

    #[test]
    fn translate_links_maps_ids_and_attributes() {
        use opentelemetry_proto::tonic::trace::v1::span::Link as ProtoLink;
        let link = ProtoLink {
            trace_id: vec![0xCD; 16],
            span_id: vec![0xEF; 8],
            trace_state: String::new(),
            attributes: vec![KeyValue {
                key: "link.kind".to_string(),
                value: Some(any(any_value::Value::StringValue(
                    "follows_from".to_string(),
                ))),
            }],
            dropped_attributes_count: 0,
            flags: 0,
        };
        let translated = translate_links(&[link]).expect("well-formed link translates");
        assert_eq!(translated.len(), 1);
        assert_eq!(translated[0].trace_id, TraceId([0xCD; 16]));
        assert_eq!(translated[0].span_id, SpanId([0xEF; 8]));
        assert_eq!(
            translated[0]
                .attributes
                .get("link.kind")
                .map(String::as_str),
            Some("follows_from")
        );
    }

    #[test]
    fn translate_links_refuses_a_wrong_length_link_trace_id() {
        use opentelemetry_proto::tonic::trace::v1::span::Link as ProtoLink;
        let link = ProtoLink {
            trace_id: vec![0xCD; 15], // not 16 bytes
            span_id: vec![0xEF; 8],
            trace_state: String::new(),
            attributes: vec![],
            dropped_attributes_count: 0,
            flags: 0,
        };
        assert!(translate_links(&[link]).is_err());
    }

    #[test]
    fn translate_links_refuses_a_wrong_length_link_span_id() {
        use opentelemetry_proto::tonic::trace::v1::span::Link as ProtoLink;
        let link = ProtoLink {
            trace_id: vec![0xCD; 16],
            span_id: vec![0xEF; 9], // not 8 bytes
            trace_state: String::new(),
            attributes: vec![],
            dropped_attributes_count: 0,
            flags: 0,
        };
        assert!(translate_links(&[link]).is_err());
    }

    // -------------------------------------------------------------------
    // Traces — translate_span parent + atomicity. A root span's empty
    // parent maps to None; a populated 8-byte parent to Some. A malformed
    // sibling refuses the whole request (the port test asserts the store
    // is empty; this pins the translator returns Err before any span is
    // produced).
    // -------------------------------------------------------------------

    fn proto_span_min(
        trace_id: Vec<u8>,
        span_id: Vec<u8>,
        parent_span_id: Vec<u8>,
    ) -> opentelemetry_proto::tonic::trace::v1::Span {
        opentelemetry_proto::tonic::trace::v1::Span {
            trace_id,
            span_id,
            trace_state: String::new(),
            parent_span_id,
            flags: 0,
            name: "s".to_string(),
            kind: 0,
            start_time_unix_nano: 1,
            end_time_unix_nano: 2,
            attributes: vec![],
            dropped_attributes_count: 0,
            events: vec![],
            dropped_events_count: 0,
            links: vec![],
            dropped_links_count: 0,
            status: None,
        }
    }

    #[test]
    fn translate_span_empty_parent_is_root() {
        let span = translate_span(
            &proto_span_min(vec![0xAB; 16], vec![0x01; 8], vec![]),
            &BTreeMap::new(),
        )
        .expect("root span translates");
        assert_eq!(span.parent_span_id, None);
    }

    #[test]
    fn translate_span_populated_parent_is_some() {
        let span = translate_span(
            &proto_span_min(vec![0xAB; 16], vec![0x02; 8], vec![0x01; 8]),
            &BTreeMap::new(),
        )
        .expect("child span translates");
        assert_eq!(span.parent_span_id, Some(SpanId([0x01; 8])));
    }

    #[test]
    fn translate_span_wrong_length_parent_refuses() {
        assert!(translate_span(
            &proto_span_min(vec![0xAB; 16], vec![0x02; 8], vec![0x01; 3]),
            &BTreeMap::new(),
        )
        .is_err());
    }

    fn traces_request_with(
        resource_attrs: Vec<KeyValue>,
        spans: Vec<opentelemetry_proto::tonic::trace::v1::Span>,
    ) -> ExportTraceServiceRequest {
        use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans};
        ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: Some(Resource {
                    attributes: resource_attrs,
                    dropped_attributes_count: 0,
                }),
                scope_spans: vec![ScopeSpans {
                    scope: None,
                    spans,
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        }
    }

    #[test]
    fn translate_traces_folds_resource_attributes_onto_every_span() {
        let request = traces_request_with(
            vec![KeyValue {
                key: "service.name".to_string(),
                value: Some(any(any_value::Value::StringValue("checkout".to_string()))),
            }],
            vec![proto_span_min(vec![0xAB; 16], vec![0x01; 8], vec![])],
        );
        let spans = translate_traces(&request).expect("translate");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].service_name(), "checkout");
    }

    #[test]
    fn translate_traces_refuses_whole_request_on_a_malformed_sibling() {
        let good = proto_span_min(vec![0xAB; 16], vec![0x01; 8], vec![]);
        let bad = proto_span_min(vec![0x11; 7], vec![0x02; 8], vec![]);
        let request = traces_request_with(vec![], vec![good, bad]);
        assert!(translate_traces(&request).is_err());
    }

    #[test]
    fn resolve_trace_tenant_id_present_absent_and_no_resource() {
        let present = traces_request_with(
            vec![KeyValue {
                key: "tenant.id".to_string(),
                value: Some(any(any_value::Value::StringValue("globex".to_string()))),
            }],
            vec![],
        );
        assert_eq!(
            resolve_trace_tenant_id(&present),
            Some("globex".to_string())
        );

        let absent = traces_request_with(
            vec![KeyValue {
                key: "service.name".to_string(),
                value: Some(any(any_value::Value::StringValue("checkout".to_string()))),
            }],
            vec![],
        );
        assert_eq!(resolve_trace_tenant_id(&absent), None);

        let empty = ExportTraceServiceRequest {
            resource_spans: vec![],
        };
        assert_eq!(resolve_trace_tenant_id(&empty), None);
    }

    #[test]
    fn translate_logs_refuses_whole_request_on_a_malformed_id() {
        let good = ProtoLogRecord {
            body: Some(any(any_value::Value::StringValue("ok".to_string()))),
            ..Default::default()
        };
        let bad = ProtoLogRecord {
            trace_id: vec![0x11; 7],
            ..Default::default()
        };

        let request = ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                resource: None,
                scope_logs: vec![ScopeLogs {
                    scope: None,
                    log_records: vec![good, bad],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };
        assert!(translate_logs(&request).is_err());
    }

    // -------------------------------------------------------------------
    // Metrics — kind mapping, value oneof (as_double / as_int / unset),
    // unsupported-type skip, and the observable skip events. The
    // acceptance suite drives the gauge / sum happy paths and the
    // histogram skip through the port; these pins close the per-arm
    // mapping mutants, the as_int->f64 cast, the value-unset skip, the
    // ExponentialHistogram / Summary / absent-data skip arms, and the
    // skip-event names a mutation could collapse.
    // -------------------------------------------------------------------

    use opentelemetry_proto::tonic::metrics::v1::{
        metric as proto_metric, number_data_point, ExponentialHistogram, Gauge, Histogram, Metric,
        NumberDataPoint, ResourceMetrics, ScopeMetrics, Sum, Summary,
    };

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

    fn int_point(time: u64, value: i64) -> NumberDataPoint {
        NumberDataPoint {
            attributes: vec![],
            start_time_unix_nano: 0,
            time_unix_nano: time,
            exemplars: vec![],
            flags: 0,
            value: Some(number_data_point::Value::AsInt(value)),
        }
    }

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

    fn proto_metric_with(name: &str, unit: &str, data: proto_metric::Data) -> Metric {
        Metric {
            name: name.to_string(),
            description: String::new(),
            unit: unit.to_string(),
            metadata: vec![],
            data: Some(data),
        }
    }

    fn metrics_request_with(
        resource_attrs: Vec<KeyValue>,
        metrics: Vec<Metric>,
    ) -> ExportMetricsServiceRequest {
        ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(Resource {
                    attributes: resource_attrs,
                    dropped_attributes_count: 0,
                }),
                scope_metrics: vec![ScopeMetrics {
                    scope: None,
                    metrics,
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        }
    }

    #[test]
    fn number_point_value_reads_as_double_verbatim() {
        assert_eq!(
            number_point_value(&double_point(1, 0.42, vec![])),
            Some(0.42)
        );
    }

    #[test]
    fn number_point_value_maps_as_int_to_exact_f64() {
        // DD11: an integer value maps to its exact f64; a translator that
        // only reads as_double would drop it (None).
        assert_eq!(number_point_value(&int_point(1, 42)), Some(42.0));
    }

    #[test]
    fn number_point_value_unset_is_none() {
        assert_eq!(number_point_value(&value_less_point(1)), None);
    }

    #[test]
    fn translate_metric_maps_gauge_to_gauge_kind_with_its_points() {
        let proto = proto_metric_with(
            "process.cpu.utilization",
            "1",
            proto_metric::Data::Gauge(Gauge {
                data_points: vec![double_point(100, 0.42, vec![])],
            }),
        );
        let metric = translate_metric(&proto, &BTreeMap::new()).expect("gauge translates");
        assert_eq!(metric.kind, MetricKind::Gauge);
        assert_eq!(metric.name, MetricName::new("process.cpu.utilization"));
        assert_eq!(metric.unit, "1");
        assert_eq!(metric.points.len(), 1);
        assert_eq!(metric.points[0].value, 0.42);
    }

    #[test]
    fn translate_metric_maps_sum_to_sum_kind() {
        let proto = proto_metric_with(
            "http.server.request.count",
            "1",
            proto_metric::Data::Sum(Sum {
                data_points: vec![int_point(100, 7)],
                aggregation_temporality: 2,
                is_monotonic: true,
            }),
        );
        let metric = translate_metric(&proto, &BTreeMap::new()).expect("sum translates");
        assert_eq!(metric.kind, MetricKind::Sum);
        assert_eq!(metric.points[0].value, 7.0);
    }

    #[test]
    fn translate_metric_folds_point_attributes_and_resource_attributes() {
        let mut resource = BTreeMap::new();
        resource.insert("service.name".to_string(), "checkout".to_string());
        let proto = proto_metric_with(
            "rps",
            "1",
            proto_metric::Data::Gauge(Gauge {
                data_points: vec![double_point(
                    100,
                    1.0,
                    vec![KeyValue {
                        key: "http.route".to_string(),
                        value: Some(any(any_value::Value::StringValue("/charge".to_string()))),
                    }],
                )],
            }),
        );
        let metric = translate_metric(&proto, &resource).expect("translates");
        assert_eq!(
            metric
                .resource_attributes
                .get("service.name")
                .map(String::as_str),
            Some("checkout")
        );
        assert_eq!(
            metric.points[0]
                .attributes
                .get("http.route")
                .map(String::as_str),
            Some("/charge")
        );
    }

    #[test]
    fn translate_metric_skips_a_value_less_point_keeping_its_sibling() {
        let proto = proto_metric_with(
            "process.cpu.utilization",
            "1",
            proto_metric::Data::Gauge(Gauge {
                data_points: vec![value_less_point(100), double_point(200, 0.55, vec![])],
            }),
        );
        let metric = translate_metric(&proto, &BTreeMap::new()).expect("gauge with one good point");
        assert_eq!(metric.points.len(), 1, "the value-less point is skipped");
        assert_eq!(metric.points[0].value, 0.55);
    }

    #[test]
    fn translate_metric_skips_each_unsupported_type() {
        // Histogram, ExponentialHistogram, Summary, and an absent data
        // oneof all skip the whole metric (None), NOT an error.
        let histogram = proto_metric_with(
            "lat",
            "ms",
            proto_metric::Data::Histogram(Histogram {
                data_points: vec![],
                aggregation_temporality: 2,
            }),
        );
        assert!(translate_metric(&histogram, &BTreeMap::new()).is_none());

        let exp = proto_metric_with(
            "lat",
            "ms",
            proto_metric::Data::ExponentialHistogram(ExponentialHistogram {
                data_points: vec![],
                aggregation_temporality: 2,
            }),
        );
        assert!(translate_metric(&exp, &BTreeMap::new()).is_none());

        let summary = proto_metric_with(
            "lat",
            "ms",
            proto_metric::Data::Summary(Summary {
                data_points: vec![],
            }),
        );
        assert!(translate_metric(&summary, &BTreeMap::new()).is_none());

        let no_data = Metric {
            name: "lat".to_string(),
            description: String::new(),
            unit: "ms".to_string(),
            metadata: vec![],
            data: None,
        };
        assert!(translate_metric(&no_data, &BTreeMap::new()).is_none());
    }

    #[test]
    fn unsupported_metric_kind_names_each_unsupported_type() {
        assert_eq!(
            unsupported_metric_kind(Some(&proto_metric::Data::Histogram(Histogram {
                data_points: vec![],
                aggregation_temporality: 2,
            }))),
            "histogram"
        );
        assert_eq!(
            unsupported_metric_kind(Some(&proto_metric::Data::ExponentialHistogram(
                ExponentialHistogram {
                    data_points: vec![],
                    aggregation_temporality: 2,
                }
            ))),
            "exponential_histogram"
        );
        assert_eq!(
            unsupported_metric_kind(Some(&proto_metric::Data::Summary(Summary {
                data_points: vec![],
            }))),
            "summary"
        );
        assert_eq!(unsupported_metric_kind(None), "none");
    }

    #[test]
    fn emit_metric_point_type_skipped_returns_the_metric_name() {
        assert_eq!(
            emit_metric_point_type_skipped("http.server.duration", "histogram"),
            "http.server.duration"
        );
    }

    #[test]
    fn emit_metric_point_value_unset_returns_the_metric_name() {
        assert_eq!(
            emit_metric_point_value_unset("process.cpu.utilization"),
            "process.cpu.utilization"
        );
    }

    #[test]
    fn translate_metrics_keeps_supported_drops_unsupported_across_a_request() {
        let request = metrics_request_with(
            vec![],
            vec![
                proto_metric_with(
                    "cpu",
                    "1",
                    proto_metric::Data::Gauge(Gauge {
                        data_points: vec![double_point(100, 0.42, vec![])],
                    }),
                ),
                proto_metric_with(
                    "lat",
                    "ms",
                    proto_metric::Data::Histogram(Histogram {
                        data_points: vec![],
                        aggregation_temporality: 2,
                    }),
                ),
            ],
        );
        let metrics = translate_metrics(&request);
        assert_eq!(metrics.len(), 1, "only the supported gauge survives");
        assert_eq!(metrics[0].name, MetricName::new("cpu"));
    }

    #[test]
    fn translate_metrics_of_only_unsupported_types_yields_empty() {
        let request = metrics_request_with(
            vec![],
            vec![proto_metric_with(
                "lat",
                "ms",
                proto_metric::Data::Histogram(Histogram {
                    data_points: vec![],
                    aggregation_temporality: 2,
                }),
            )],
        );
        assert!(translate_metrics(&request).is_empty());
    }

    #[test]
    fn resolve_metric_tenant_id_present_absent_and_no_resource() {
        let present = metrics_request_with(
            vec![KeyValue {
                key: "tenant.id".to_string(),
                value: Some(any(any_value::Value::StringValue("globex".to_string()))),
            }],
            vec![],
        );
        assert_eq!(
            resolve_metric_tenant_id(&present),
            Some("globex".to_string())
        );

        let absent = metrics_request_with(
            vec![KeyValue {
                key: "service.name".to_string(),
                value: Some(any(any_value::Value::StringValue("checkout".to_string()))),
            }],
            vec![],
        );
        assert_eq!(resolve_metric_tenant_id(&absent), None);

        let empty = ExportMetricsServiceRequest {
            resource_metrics: vec![],
        };
        assert_eq!(resolve_metric_tenant_id(&empty), None);
    }
}
