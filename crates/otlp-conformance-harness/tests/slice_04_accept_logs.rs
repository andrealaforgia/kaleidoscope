//! Slice 04 — US-04: Accept a minimally valid OTLP logs record.
//!
//! Second walking skeleton — the first accept-path round-trip. Per US-04
//! AC 2 the returned type's full path is exactly
//! `opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest`.
//! Per the iteration-2 fix to scenario 2 (line 372–379 of user-stories.md),
//! the type-path identity is asserted by passing the record to a function
//! whose parameter type is the upstream type — the call type-checks and
//! runs without any explicit conversion.

mod common;

// `use` the upstream type via its FULL path through the upstream crate, NOT
// re-exported by the harness (US-04 AC 2). If the harness ever re-exports
// or wraps this type, this `use` line would still compile but the function
// signature mismatch in `consume_upstream_record` below would break.
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use otlp_conformance_harness::{validate_logs, Framing};

// =========================================================================
// Scenario: A minimal logs export request is accepted and returned typed
// =========================================================================

#[test]
fn minimal_logs_export_request_returns_ok() {
    let bytes = common::encode_minimal_logs();
    let result = validate_logs(&bytes, Framing::HttpProtobuf);
    assert!(
        result.is_ok(),
        "expected Ok for a minimal logs export, got {:?}",
        result.err()
    );
}

#[test]
fn minimal_logs_export_request_round_trips_resource_attributes() {
    let bytes = common::encode_minimal_logs();
    let record = validate_logs(&bytes, Framing::HttpProtobuf)
        .expect("a minimal logs export must be accepted");
    let resource_logs = &record.resource_logs;
    assert_eq!(resource_logs.len(), 1, "expected one ResourceLogs entry");
    let resource = resource_logs[0]
        .resource
        .as_ref()
        .expect("ResourceLogs.resource must be present");
    let service_name = resource
        .attributes
        .iter()
        .find(|kv| kv.key == "service.name")
        .expect("service.name attribute must round-trip");
    let value = service_name
        .value
        .as_ref()
        .and_then(|av| av.value.as_ref())
        .expect("service.name value must be present");
    use opentelemetry_proto::tonic::common::v1::any_value::Value;
    match value {
        Value::StringValue(s) => assert_eq!(s, "kaleidoscope-corpus-fixture"),
        other => panic!("expected StringValue for service.name, got {other:?}"),
    }
}

#[test]
fn minimal_logs_export_request_round_trips_log_record_body() {
    let bytes = common::encode_minimal_logs();
    let record = validate_logs(&bytes, Framing::HttpProtobuf)
        .expect("a minimal logs export must be accepted");
    let log = &record.resource_logs[0].scope_logs[0].log_records[0];
    let body = log.body.as_ref().expect("body must be set");
    use opentelemetry_proto::tonic::common::v1::any_value::Value;
    match body.value.as_ref() {
        Some(Value::StringValue(s)) => {
            assert_eq!(s, "minimal log record for corpus");
        }
        other => panic!("expected StringValue body, got {other:?}"),
    }
}

// =========================================================================
// Scenario: The accepted record is directly usable by a downstream
// consumer expecting the upstream type
//
// Iteration-2 fix to user-stories.md line 372-379: the assertion is now
// runtime-observable, not a compile-time-only type-system claim. The
// downstream consumer's parameter type is the upstream
// `ExportLogsServiceRequest`; the call type-checks and runs without any
// explicit conversion. If the harness ever changed its return type to a
// harness-local wrapper, the `result.unwrap()` line below would no longer
// type-check.
// =========================================================================

#[test]
fn accepted_record_is_directly_usable_by_upstream_typed_consumer() {
    let bytes = common::encode_minimal_logs();
    let result = validate_logs(&bytes, Framing::HttpProtobuf);
    let record = result.expect("a minimal logs export must be accepted");
    // The next line is the *runtime-observable* identity check — the
    // harness's return value is fed into a function whose parameter type
    // is `&ExportLogsServiceRequest` (upstream), without conversion.
    let observed_resource_count = consume_upstream_record(&record);
    assert_eq!(observed_resource_count, 1);
}

/// A stand-in for any downstream consumer that expects the upstream type.
/// Aperture's forwarding exporter, Sluice's queue-write side, and every
/// storage engine consume `&ExportLogsServiceRequest` exactly like this.
fn consume_upstream_record(record: &ExportLogsServiceRequest) -> usize {
    record.resource_logs.len()
}

// =========================================================================
// Scenario: The harness produces no side effects on the accept path
//
// US-04 AC 4: "The harness writes nothing to stdout, stderr, or any
// logger on the accept path (assertion observed across stdout, stderr,
// and the logging facade)." Three separate tests for mutation resistance.
// =========================================================================

#[test]
fn accepting_logs_writes_nothing_to_stdout() {
    let bytes = common::encode_minimal_logs();
    let (result, observations) = common::observe_silence(|| {
        validate_logs(&bytes, Framing::HttpProtobuf)
    });
    assert!(result.is_ok(), "accept path must succeed");
    assert!(
        observations.stdout.is_empty(),
        "stdout was written to on the accept path: {:?}",
        String::from_utf8_lossy(&observations.stdout)
    );
}

#[test]
fn accepting_logs_writes_nothing_to_stderr() {
    let bytes = common::encode_minimal_logs();
    let (result, observations) = common::observe_silence(|| {
        validate_logs(&bytes, Framing::HttpProtobuf)
    });
    assert!(result.is_ok(), "accept path must succeed");
    assert!(
        observations.stderr.is_empty(),
        "stderr was written to on the accept path: {:?}",
        String::from_utf8_lossy(&observations.stderr)
    );
}

#[test]
fn accepting_logs_emits_no_log_records() {
    let bytes = common::encode_minimal_logs();
    let (result, observations) = common::observe_silence(|| {
        validate_logs(&bytes, Framing::HttpProtobuf)
    });
    assert!(result.is_ok(), "accept path must succeed");
    assert!(
        observations.log_records.is_empty(),
        "log records emitted on the accept path: {:?}",
        observations.log_records
    );
}
