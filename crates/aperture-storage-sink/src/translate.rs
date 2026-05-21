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
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, KeyValue};

use lumen::{LogRecord, SeverityNumber};

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
}
