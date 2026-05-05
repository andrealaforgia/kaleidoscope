//! Application core — pure async functions sitting between the driving
//! adapters (gRPC, HTTP) and the driven adapters (sinks).
//!
//! Slice 01 lights up the logs path: `ingest_logs` is the single call
//! site for `validate_logs(_, Framing::GrpcProtobuf)`. Subsequent slices
//! grow the symmetric `ingest_traces` and `ingest_metrics` paths.
//!
//! See `docs/feature/aperture/design/component-design.md > app::*` for
//! the full module breakdown DELIVER will land. The pure helpers here
//! (`summarise_record`, `framing_for_transport`) are unit-testable in
//! isolation; they are pure free functions of their inputs.
//!
//! ## CI invariant — single validator per signal
//!
//! `ingest_logs`, `ingest_traces`, `ingest_metrics` are the ONLY call
//! sites of `validate_logs`/`validate_traces`/`validate_metrics` in the
//! entire `aperture` crate. The `xtask single-validator-per-signal`
//! gate (DEVOPS-owned, AST-walking) enforces this. Adding a second
//! call site fails CI.

use std::sync::Arc;

use otlp_conformance_harness::{
    validate_logs, validate_metrics, validate_traces, Framing, OtlpViolation,
};

use crate::ports::{OtlpSink, SinkError, SinkRecord};

/// Which on-the-wire transport carried the body. Drives the choice of
/// `Framing` the harness asserts against.
#[derive(Debug, Clone, Copy)]
pub enum Transport {
    Grpc,
    HttpProtobuf,
}

/// Map a `Transport` to the harness's `Framing` constant. Pure, total,
/// inlineable.
#[inline]
pub fn framing_for_transport(transport: Transport) -> Framing {
    match transport {
        Transport::Grpc => Framing::GrpcProtobuf,
        Transport::HttpProtobuf => Framing::HttpProtobuf,
    }
}

/// Outcome of a validate-and-route call. The transport adapters
/// translate this to gRPC / HTTP responses.
#[derive(Debug)]
pub enum IngestOutcome {
    Accepted,
    Rejected(OtlpViolation),
    SinkRefused(SinkError),
}

/// Validate a logs body and route the typed record to the sink.
///
/// The single call site for `validate_logs` in Aperture (CI invariant
/// `single_validator_per_signal` enforces). The transport adapter has
/// already emitted `event=request_received` before calling this; the
/// sink emits `event=sink_accepted` (StubSink/RecordingSink) or
/// `event=sink_failed` (ForwardingSink failure path) on its own. This
/// function does not emit events.
pub async fn ingest_logs(
    body: &[u8],
    transport: Transport,
    sink: &Arc<dyn OtlpSink>,
) -> IngestOutcome {
    let framing = framing_for_transport(transport);
    match validate_logs(body, framing) {
        Ok(record) => match sink.accept(SinkRecord::Logs(record)).await {
            Ok(()) => IngestOutcome::Accepted,
            Err(e) => IngestOutcome::SinkRefused(e),
        },
        Err(violation) => IngestOutcome::Rejected(violation),
    }
}

/// Validate a traces body and route the typed record to the sink.
///
/// The single call site for `validate_traces` in Aperture (CI invariant
/// `single_validator_per_signal` enforces). Mirrors `ingest_logs` for
/// the traces signal: the transport adapter has already emitted
/// `event=request_received` before calling this; the sink emits
/// `event=sink_accepted` on its own. A logs body sent to this function
/// surfaces as `Rejected(WireType::SignalMismatch{observed=Logs,
/// asserted=Traces})` from the harness — the symmetric reject path the
/// Slice 03 acceptance tests exercise.
pub async fn ingest_traces(
    body: &[u8],
    transport: Transport,
    sink: &Arc<dyn OtlpSink>,
) -> IngestOutcome {
    let framing = framing_for_transport(transport);
    match validate_traces(body, framing) {
        Ok(record) => match sink.accept(SinkRecord::Traces(record)).await {
            Ok(()) => IngestOutcome::Accepted,
            Err(e) => IngestOutcome::SinkRefused(e),
        },
        Err(violation) => IngestOutcome::Rejected(violation),
    }
}

/// Validate a metrics body and route the typed record to the sink.
///
/// The single call site for `validate_metrics` in Aperture (CI
/// invariant `single_validator_per_signal` enforces). Mirrors
/// `ingest_logs` and `ingest_traces` for the metrics signal: the
/// transport adapter has already emitted `event=request_received`
/// before calling this; the sink emits `event=sink_accepted` on its
/// own. A traces body sent to this function surfaces as
/// `Rejected(WireType::SignalMismatch{observed=Traces,
/// asserted=Metrics})` from the harness — the symmetric reject path
/// the Slice 04 acceptance tests exercise.
pub async fn ingest_metrics(
    body: &[u8],
    transport: Transport,
    sink: &Arc<dyn OtlpSink>,
) -> IngestOutcome {
    let framing = framing_for_transport(transport);
    match validate_metrics(body, framing) {
        Ok(record) => match sink.accept(SinkRecord::Metrics(record)).await {
            Ok(()) => IngestOutcome::Accepted,
            Err(e) => IngestOutcome::SinkRefused(e),
        },
        Err(violation) => IngestOutcome::Rejected(violation),
    }
}

// =========================================================================
// summarise_record — pure helper for `event=sink_accepted` field shape
// =========================================================================

/// Per-record summary: signal name, optional `service.name` from the
/// resource attributes, and the headline count for the
/// `event=sink_accepted` line.
#[derive(Debug)]
pub struct RecordSummary<'a> {
    pub signal: &'static str,
    pub resource_service_name: Option<&'a str>,
    pub count: usize,
}

/// Walk a `SinkRecord` and extract the canonical summary fields.
///
/// Slices 01, 03, and 04 ship the Logs, Traces, and Metrics branches.
/// The typed `SinkRecord` enum is `#[non_exhaustive]` (DISCUSS D2) so
/// future-additive evolution stays non-breaking; the catch-all arm is
/// the lower bound for that guarantee — once a future signal type
/// lands as a new variant, a focused `summarise_record` arm will be
/// added alongside it.
pub fn summarise_record(record: &SinkRecord) -> RecordSummary<'_> {
    match record {
        SinkRecord::Logs(req) => RecordSummary {
            signal: "logs",
            resource_service_name: extract_service_name_logs(req),
            count: count_log_records(req),
        },
        SinkRecord::Traces(req) => RecordSummary {
            signal: "traces",
            resource_service_name: extract_service_name_traces(req),
            count: count_spans(req),
        },
        SinkRecord::Metrics(req) => RecordSummary {
            signal: "metrics",
            resource_service_name: extract_service_name_metrics(req),
            count: count_data_points(req),
        },
    }
}

fn count_log_records(
    req: &opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest,
) -> usize {
    req.resource_logs
        .iter()
        .flat_map(|rl| rl.scope_logs.iter())
        .map(|sl| sl.log_records.len())
        .sum()
}

fn extract_service_name_logs(
    req: &opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest,
) -> Option<&str> {
    req.resource_logs
        .first()
        .and_then(|rl| rl.resource.as_ref())
        .and_then(|r| service_name_from_attributes(&r.attributes))
}

/// DISCUSS picks "spans only" (sum of leaf `Span` counts, not a
/// resource-level rollup) per `slice-03-traces.md > Known unknowns`.
/// Walk the `ResourceSpans -> ScopeSpans -> Span` tree and sum the
/// span vectors at the leaves. Pulse (Phase 4) and the future
/// ForwardingSink reuse this convention.
fn count_spans(
    req: &opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest,
) -> usize {
    req.resource_spans
        .iter()
        .flat_map(|rs| rs.scope_spans.iter())
        .map(|ss| ss.spans.len())
        .sum()
}

fn extract_service_name_traces(
    req: &opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest,
) -> Option<&str> {
    req.resource_spans
        .first()
        .and_then(|rs| rs.resource.as_ref())
        .and_then(|r| service_name_from_attributes(&r.attributes))
}

/// DISCUSS US-AP-06 picks `data_point_count` (sum of `data_points`
/// across every `Metric`'s data oneof) per `slice-04-metrics.md > Known
/// unknowns`. Walk the `ResourceMetrics -> ScopeMetrics -> Metric ->
/// data` tree; for each `Metric`'s populated `Data` oneof variant
/// (Gauge / Sum / Histogram / ExponentialHistogram / Summary) sum the
/// `data_points` vector length. Histograms count one data point per
/// bucket-set, not per bucket — i.e. `histogram.data_points.len()`,
/// regardless of how many bucket boundaries each `HistogramDataPoint`
/// carries. Pulse and the future `ForwardingSink` reuse this
/// convention.
fn count_data_points(
    req: &opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest,
) -> usize {
    use opentelemetry_proto::tonic::metrics::v1::metric::Data;
    req.resource_metrics
        .iter()
        .flat_map(|rm| rm.scope_metrics.iter())
        .flat_map(|sm| sm.metrics.iter())
        .map(|m| match m.data.as_ref() {
            Some(Data::Gauge(g)) => g.data_points.len(),
            Some(Data::Sum(s)) => s.data_points.len(),
            Some(Data::Histogram(h)) => h.data_points.len(),
            Some(Data::ExponentialHistogram(e)) => e.data_points.len(),
            Some(Data::Summary(s)) => s.data_points.len(),
            None => 0,
        })
        .sum()
}

fn extract_service_name_metrics(
    req: &opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest,
) -> Option<&str> {
    req.resource_metrics
        .first()
        .and_then(|rm| rm.resource.as_ref())
        .and_then(|r| service_name_from_attributes(&r.attributes))
}

fn service_name_from_attributes(
    attributes: &[opentelemetry_proto::tonic::common::v1::KeyValue],
) -> Option<&str> {
    use opentelemetry_proto::tonic::common::v1::any_value::Value as AvValue;
    attributes
        .iter()
        .find(|kv| kv.key == "service.name")
        .and_then(|kv| kv.value.as_ref())
        .and_then(|av| av.value.as_ref())
        .and_then(|v| match v {
            AvValue::StringValue(s) => Some(s.as_str()),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
    use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
    use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
    use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, KeyValue};
    use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
    use opentelemetry_proto::tonic::metrics::v1::{
        metric::Data as MetricData, number_data_point, ExponentialHistogram,
        ExponentialHistogramDataPoint, Gauge, Histogram, HistogramDataPoint, Metric,
        NumberDataPoint, ResourceMetrics, ScopeMetrics, Sum, Summary, SummaryDataPoint,
    };
    use opentelemetry_proto::tonic::resource::v1::Resource;
    use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span};

    #[test]
    fn framing_for_grpc_transport_is_grpc_protobuf() {
        assert!(matches!(
            framing_for_transport(Transport::Grpc),
            Framing::GrpcProtobuf
        ));
    }

    #[test]
    fn framing_for_http_protobuf_transport_is_http_protobuf() {
        assert!(matches!(
            framing_for_transport(Transport::HttpProtobuf),
            Framing::HttpProtobuf
        ));
    }

    #[test]
    fn summarise_logs_extracts_service_name_and_record_count() {
        let req = ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                resource: Some(Resource {
                    attributes: vec![KeyValue {
                        key: "service.name".to_string(),
                        value: Some(AnyValue {
                            value: Some(any_value::Value::StringValue("payments-api".to_string())),
                        }),
                    }],
                    dropped_attributes_count: 0,
                }),
                scope_logs: vec![ScopeLogs {
                    scope: None,
                    log_records: vec![
                        LogRecord::default(),
                        LogRecord::default(),
                        LogRecord::default(),
                    ],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };
        let record = SinkRecord::Logs(req);
        let s = summarise_record(&record);
        assert_eq!(s.signal, "logs");
        assert_eq!(s.resource_service_name, Some("payments-api"));
        assert_eq!(s.count, 3);
    }

    #[test]
    fn summarise_logs_returns_none_service_name_when_attribute_missing() {
        let req = ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                resource: Some(Resource {
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                scope_logs: vec![],
                schema_url: String::new(),
            }],
        };
        let record = SinkRecord::Logs(req);
        let s = summarise_record(&record);
        assert_eq!(s.resource_service_name, None);
        assert_eq!(s.count, 0);
    }

    fn span_with_index(i: u8) -> Span {
        Span {
            trace_id: vec![1; 16],
            span_id: vec![i; 8],
            trace_state: String::new(),
            parent_span_id: vec![],
            flags: 0,
            name: format!("span-{i}"),
            kind: 1,
            start_time_unix_nano: 1_700_000_000_000_000_000,
            end_time_unix_nano: 1_700_000_000_000_000_010,
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
    fn summarise_traces_extracts_service_name_and_span_count() {
        let req = ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: Some(Resource {
                    attributes: vec![KeyValue {
                        key: "service.name".to_string(),
                        value: Some(AnyValue {
                            value: Some(any_value::Value::StringValue("checkout-api".to_string())),
                        }),
                    }],
                    dropped_attributes_count: 0,
                }),
                scope_spans: vec![ScopeSpans {
                    scope: None,
                    spans: vec![span_with_index(0), span_with_index(1), span_with_index(2)],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };
        let record = SinkRecord::Traces(req);
        let s = summarise_record(&record);
        assert_eq!(s.signal, "traces");
        assert_eq!(s.resource_service_name, Some("checkout-api"));
        assert_eq!(s.count, 3);
    }

    #[test]
    fn summarise_traces_sums_spans_across_multiple_scope_spans() {
        // Two `ScopeSpans`, each carrying 2 spans. The sum is 4 — pins
        // the "walk every scope_spans entry" semantics so a mutation
        // that swaps `flat_map` for `first().map(...)` is caught.
        let req = ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: None,
                scope_spans: vec![
                    ScopeSpans {
                        scope: None,
                        spans: vec![span_with_index(0), span_with_index(1)],
                        schema_url: String::new(),
                    },
                    ScopeSpans {
                        scope: None,
                        spans: vec![span_with_index(2), span_with_index(3)],
                        schema_url: String::new(),
                    },
                ],
                schema_url: String::new(),
            }],
        };
        let record = SinkRecord::Traces(req);
        let s = summarise_record(&record);
        assert_eq!(s.count, 4);
    }

    #[test]
    fn summarise_traces_returns_none_service_name_when_resource_missing() {
        let req = ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: None,
                scope_spans: vec![],
                schema_url: String::new(),
            }],
        };
        let record = SinkRecord::Traces(req);
        let s = summarise_record(&record);
        assert_eq!(s.resource_service_name, None);
        assert_eq!(s.count, 0);
    }

    // -------------------------------------------------------------------------
    // Metrics summary helpers — pin the per-Metric data-point counting
    // convention DISCUSS US-AP-06 locked. Each focused test caps one
    // mutation surface so cargo-mutants on
    // `app::count_data_points` / `extract_service_name_metrics` /
    // `summarise_record` reaches 100% kill rate on the touched arms.
    // -------------------------------------------------------------------------

    fn sum_metric_with_n_data_points(name: &str, n: usize) -> Metric {
        let data_points = (0..n)
            .map(|i| NumberDataPoint {
                attributes: vec![],
                start_time_unix_nano: 1_700_000_000_000_000_000,
                time_unix_nano: 1_700_000_000_000_000_000 + i as u64,
                exemplars: vec![],
                flags: 0,
                value: Some(number_data_point::Value::AsInt(i as i64)),
            })
            .collect();
        Metric {
            name: name.to_string(),
            description: String::new(),
            unit: "1".to_string(),
            metadata: vec![],
            data: Some(MetricData::Sum(Sum {
                data_points,
                aggregation_temporality: 2,
                is_monotonic: true,
            })),
        }
    }

    fn gauge_metric_with_n_data_points(name: &str, n: usize) -> Metric {
        let data_points = (0..n)
            .map(|i| NumberDataPoint {
                attributes: vec![],
                start_time_unix_nano: 1_700_000_000_000_000_000,
                time_unix_nano: 1_700_000_000_000_000_000 + i as u64,
                exemplars: vec![],
                flags: 0,
                value: Some(number_data_point::Value::AsDouble(i as f64)),
            })
            .collect();
        Metric {
            name: name.to_string(),
            description: String::new(),
            unit: "1".to_string(),
            metadata: vec![],
            data: Some(MetricData::Gauge(Gauge { data_points })),
        }
    }

    fn histogram_metric_with_n_data_points(name: &str, n: usize, bucket_count: usize) -> Metric {
        let data_points = (0..n)
            .map(|_| HistogramDataPoint {
                attributes: vec![],
                start_time_unix_nano: 1_700_000_000_000_000_000,
                time_unix_nano: 1_700_000_000_000_000_010,
                count: 0,
                sum: None,
                bucket_counts: vec![0u64; bucket_count],
                explicit_bounds: vec![0.0; bucket_count.saturating_sub(1)],
                exemplars: vec![],
                flags: 0,
                min: None,
                max: None,
            })
            .collect();
        Metric {
            name: name.to_string(),
            description: String::new(),
            unit: "1".to_string(),
            metadata: vec![],
            data: Some(MetricData::Histogram(Histogram {
                data_points,
                aggregation_temporality: 2,
            })),
        }
    }

    #[test]
    fn summarise_metrics_extracts_service_name_and_data_point_count() {
        // Canonical fixture: one Sum (1 data point) + one Gauge (1 data
        // point) = 2 data points. Pins the basic metrics-arm wiring of
        // `summarise_record`.
        let req = ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(Resource {
                    attributes: vec![KeyValue {
                        key: "service.name".to_string(),
                        value: Some(AnyValue {
                            value: Some(any_value::Value::StringValue("payments-api".to_string())),
                        }),
                    }],
                    dropped_attributes_count: 0,
                }),
                scope_metrics: vec![ScopeMetrics {
                    scope: None,
                    metrics: vec![
                        sum_metric_with_n_data_points("requests", 1),
                        gauge_metric_with_n_data_points("temperature", 1),
                    ],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };
        let record = SinkRecord::Metrics(req);
        let s = summarise_record(&record);
        assert_eq!(s.signal, "metrics");
        assert_eq!(s.resource_service_name, Some("payments-api"));
        assert_eq!(s.count, 2);
    }

    #[test]
    fn summarise_metrics_counts_one_data_point_per_histogram_data_point_not_per_bucket() {
        // DISCUSS US-AP-06 lock: a histogram with 50 buckets but only 1
        // `HistogramDataPoint` contributes 1 to `data_point_count`, not 50.
        // This pins the "one per `Metric`-data-point, not per bucket"
        // convention against the obvious wrong implementation that sums
        // `bucket_counts.len()`.
        let req = ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: None,
                scope_metrics: vec![ScopeMetrics {
                    scope: None,
                    metrics: vec![histogram_metric_with_n_data_points(
                        "latency", 1, /* buckets */ 50,
                    )],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };
        let record = SinkRecord::Metrics(req);
        let s = summarise_record(&record);
        assert_eq!(s.count, 1);
    }

    #[test]
    fn summarise_metrics_sums_data_points_across_multiple_scope_metrics() {
        // Two `ScopeMetrics`, each carrying one Sum metric of two data
        // points. The sum is 4 — pins the "walk every scope_metrics
        // entry" semantics so a mutation that swaps `flat_map` for
        // `first().map(...)` is caught.
        let req = ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: None,
                scope_metrics: vec![
                    ScopeMetrics {
                        scope: None,
                        metrics: vec![sum_metric_with_n_data_points("a", 2)],
                        schema_url: String::new(),
                    },
                    ScopeMetrics {
                        scope: None,
                        metrics: vec![sum_metric_with_n_data_points("b", 2)],
                        schema_url: String::new(),
                    },
                ],
                schema_url: String::new(),
            }],
        };
        let record = SinkRecord::Metrics(req);
        let s = summarise_record(&record);
        assert_eq!(s.count, 4);
    }

    #[test]
    fn summarise_metrics_counts_zero_when_metric_data_oneof_is_none() {
        // Exotic but legal: a `Metric` whose `data` oneof is unset. The
        // counting walk treats `None` as a 0-contribution branch. Pins
        // the `None => 0` arm against a mutation that returns 1.
        let req = ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: None,
                scope_metrics: vec![ScopeMetrics {
                    scope: None,
                    metrics: vec![Metric {
                        name: "no-data".to_string(),
                        description: String::new(),
                        unit: "1".to_string(),
                        metadata: vec![],
                        data: None,
                    }],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };
        let record = SinkRecord::Metrics(req);
        let s = summarise_record(&record);
        assert_eq!(s.count, 0);
    }

    #[test]
    fn summarise_metrics_counts_exponential_histogram_and_summary_data_points() {
        // Belt-and-braces for the remaining `Data` oneof arms: an
        // `ExponentialHistogram` carrying 1 data point and a `Summary`
        // carrying 1 data point sum to 2. Pins both arms against a
        // mutation that returns 0.
        let req = ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: None,
                scope_metrics: vec![ScopeMetrics {
                    scope: None,
                    metrics: vec![
                        Metric {
                            name: "exp-hist".to_string(),
                            description: String::new(),
                            unit: "1".to_string(),
                            metadata: vec![],
                            data: Some(MetricData::ExponentialHistogram(ExponentialHistogram {
                                data_points: vec![ExponentialHistogramDataPoint {
                                    attributes: vec![],
                                    start_time_unix_nano: 1_700_000_000_000_000_000,
                                    time_unix_nano: 1_700_000_000_000_000_010,
                                    count: 0,
                                    sum: None,
                                    scale: 0,
                                    zero_count: 0,
                                    positive: None,
                                    negative: None,
                                    flags: 0,
                                    exemplars: vec![],
                                    min: None,
                                    max: None,
                                    zero_threshold: 0.0,
                                }],
                                aggregation_temporality: 2,
                            })),
                        },
                        Metric {
                            name: "summary".to_string(),
                            description: String::new(),
                            unit: "1".to_string(),
                            metadata: vec![],
                            data: Some(MetricData::Summary(Summary {
                                data_points: vec![SummaryDataPoint {
                                    attributes: vec![],
                                    start_time_unix_nano: 1_700_000_000_000_000_000,
                                    time_unix_nano: 1_700_000_000_000_000_010,
                                    count: 0,
                                    sum: 0.0,
                                    quantile_values: vec![],
                                    flags: 0,
                                }],
                            })),
                        },
                    ],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };
        let record = SinkRecord::Metrics(req);
        let s = summarise_record(&record);
        assert_eq!(s.count, 2);
    }

    #[test]
    fn summarise_metrics_returns_none_service_name_when_resource_missing() {
        let req = ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: None,
                scope_metrics: vec![],
                schema_url: String::new(),
            }],
        };
        let record = SinkRecord::Metrics(req);
        let s = summarise_record(&record);
        assert_eq!(s.resource_service_name, None);
        assert_eq!(s.count, 0);
    }
}
