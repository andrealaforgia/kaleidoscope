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

use otlp_conformance_harness::{validate_logs, Framing, OtlpViolation};

use crate::ports::{OtlpSink, SinkError, SinkRecord};

/// Which on-the-wire transport carried the body. Drives the choice of
/// `Framing` the harness asserts against.
#[derive(Debug, Clone, Copy)]
pub enum Transport {
    Grpc,
    /// Reserved for Slice 02 (HTTP/protobuf listener). Kept here so the
    /// `framing_for_transport` helper has its full match coverage now;
    /// removing the variant when Slice 02 lands would be a needless
    /// public-API churn.
    #[allow(dead_code)]
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
/// Logs: `count` = sum of `log_records.len()` across all scope_logs;
/// `service.name` from `resource_logs[0].resource.attributes`.
///
/// Traces and metrics paths are land-with-Slice-03/Slice-04; Slice 01
/// only exercises the Logs branch.
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
            count: count_metrics(req),
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

fn count_spans(
    req: &opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest,
) -> usize {
    req.resource_spans
        .iter()
        .flat_map(|rs| rs.scope_spans.iter())
        .map(|ss| ss.spans.len())
        .sum()
}

fn count_metrics(
    req: &opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest,
) -> usize {
    req.resource_metrics
        .iter()
        .flat_map(|rm| rm.scope_metrics.iter())
        .map(|sm| sm.metrics.len())
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

fn extract_service_name_traces(
    req: &opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest,
) -> Option<&str> {
    req.resource_spans
        .first()
        .and_then(|rs| rs.resource.as_ref())
        .and_then(|r| service_name_from_attributes(&r.attributes))
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
    use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, KeyValue};
    use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
    use opentelemetry_proto::tonic::resource::v1::Resource;

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
}
