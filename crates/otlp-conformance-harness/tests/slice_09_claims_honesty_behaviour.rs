//! Slice 09 — claims-honesty-pass-v0 harness behaviour assertions.
//!
//! Feature: `claims-honesty-pass-v0`. These two behaviour tests pin the
//! REAL current harness behaviour that the CORRECTED docs (US-04, US-06)
//! will describe. They pass TODAY and guard against future regression —
//! so they are NOT `#[ignore]`d (unlike the doc-lint guards in slice_08,
//! which are RED until DELIVER edits the prose).
//!
//! - **US-04** — the harness performs STRUCTURAL DECODE-LEVEL validation,
//!   not semantic OTLP-wire-spec conformance. A trace export request with
//!   a 4-byte `trace_id` (invalid per the OTLP/W3C 16-byte rule) decodes
//!   cleanly and is ACCEPTED. This pins the documented boundary "semantic
//!   checks are out of scope".
//! - **US-06** — `Framing` is INERT: prefix-stripped bytes validate
//!   identically under `HttpProtobuf` and `GrpcProtobuf`; a still-
//!   length-prefixed body under `GrpcProtobuf` fails to decode (the caller
//!   must strip the gRPC length prefix first). This pins the documented
//!   "GrpcProtobuf is a non-behavioural label" boundary.

mod common;

use prost::Message;

use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span};

use otlp_conformance_harness::{
    validate_logs, validate_traces, Framing, OtlpViolation, Rule, WireTypeRule,
};

// =====================================================================
// US-04 — structural decode-level, NOT semantic: a structurally-valid
// but semantically-bogus body (4-byte trace_id) is ACCEPTED.
// =====================================================================

/// Build an `ExportTraceServiceRequest` that is structurally well-formed
/// (decodes as the asserted prost type, resource-field-first, non-empty)
/// but semantically INVALID: the `trace_id` is 4 bytes, violating the
/// OTLP/W3C 16-byte rule. A semantic validator would reject this; the
/// structural decode-level harness does not.
fn encode_traces_with_short_trace_id() -> Vec<u8> {
    let req = ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![KeyValue {
                    key: "service.name".to_string(),
                    value: Some(AnyValue {
                        value: Some(any_value::Value::StringValue(
                            "kaleidoscope-honesty-fixture".to_string(),
                        )),
                    }),
                }],
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: Some(InstrumentationScope {
                    name: "kaleidoscope.test".to_string(),
                    version: "0.0.0".to_string(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                spans: vec![Span {
                    trace_id: vec![1, 2, 3, 4], // 4 bytes — semantically invalid (must be 16)
                    span_id: vec![1, 2, 3, 4],  // 4 bytes — semantically invalid (must be 8)
                    trace_state: String::new(),
                    parent_span_id: vec![],
                    flags: 0,
                    name: "semantically-bogus-span".to_string(),
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
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    };
    req.encode_to_vec()
}

/// @US-04
///
/// Given a trace export request that decodes cleanly but carries a 4-byte
/// `trace_id` (invalid per the OTLP/W3C 16-byte rule),
/// When it is validated with `validate_traces` under HTTP framing,
/// Then the harness ACCEPTS it (structural decode succeeds) — pinning the
/// now-documented boundary that semantic checks are out of scope. The
/// corrected US-04 prose predicts exactly this.
#[test]
fn structurally_valid_semantically_bogus_trace_id_is_accepted() {
    let bytes = encode_traces_with_short_trace_id();
    let result = validate_traces(&bytes, Framing::HttpProtobuf);
    assert!(
        result.is_ok(),
        "structural decode-level validation accepts a semantically-invalid \
         (4-byte trace_id) but structurally-valid body; got {:?}",
        result.err()
    );
    // The bogus id round-trips untouched — proving no semantic length check
    // ran (a semantic validator would have rejected or normalised it).
    let record = result.expect("accepted");
    let span = &record.resource_spans[0].scope_spans[0].spans[0];
    assert_eq!(
        span.trace_id.len(),
        4,
        "the harness performed no trace_id length check"
    );
}

// =====================================================================
// US-06 — Framing is INERT (DOCUMENT). Prefix-stripped bytes validate
// identically under both framings; a length-prefixed body under
// GrpcProtobuf fails to decode (caller must strip the prefix).
// =====================================================================

/// @US-06
///
/// Given prefix-stripped `ExportLogsServiceRequest` bytes,
/// When they are validated under `HttpProtobuf` and under `GrpcProtobuf`,
/// Then both accept identically — proving `Framing` is a non-behavioural
/// label, matching the corrected US-06 documentation.
#[test]
fn prefix_stripped_bytes_validate_identically_under_both_framings() {
    let bytes = common::encode_minimal_logs();

    let http = validate_logs(&bytes, Framing::HttpProtobuf);
    let grpc = validate_logs(&bytes, Framing::GrpcProtobuf);

    assert!(
        http.is_ok(),
        "prefix-stripped bytes accepted under HttpProtobuf: {:?}",
        http.err()
    );
    assert!(
        grpc.is_ok(),
        "prefix-stripped bytes accepted under GrpcProtobuf (framing is \
         inert): {:?}",
        grpc.err()
    );

    // Both accept-path records carry the same decoded payload — framing
    // changed nothing about the outcome.
    let http_record = http.expect("http accepted");
    let grpc_record = grpc.expect("grpc accepted");
    assert_eq!(
        http_record.encode_to_vec(),
        grpc_record.encode_to_vec(),
        "the two framings produce byte-identical decoded payloads — Framing \
         is inert"
    );
}

/// @US-06
///
/// Given a still-length-prefixed gRPC-framed OTLP body (a 5-byte gRPC
/// frame header: 1 compression flag + 4-byte big-endian length, then the
/// message),
/// When it is validated under `GrpcProtobuf`,
/// Then it FAILS to decode — because the harness does NOT strip the gRPC
/// length prefix; the caller must. This is the boundary the corrected
/// US-06 doc tells the caller about.
#[test]
fn length_prefixed_body_under_grpc_framing_fails_to_decode() {
    let payload = common::encode_minimal_logs();

    // Prepend a gRPC length-prefix frame header: [0u8 compression flag]
    // ++ [u32 big-endian message length].
    let mut framed = Vec::with_capacity(5 + payload.len());
    framed.push(0u8);
    framed.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    framed.extend_from_slice(&payload);

    let result = validate_logs(&framed, Framing::GrpcProtobuf);
    let violation: OtlpViolation = result
        .expect_err("a length-prefixed body must fail to decode; the caller strips the prefix");
    assert_eq!(
        violation.rule,
        Rule::WireType(WireTypeRule::ProtobufDecode),
        "the un-stripped gRPC frame header corrupts the protobuf decode"
    );
}
