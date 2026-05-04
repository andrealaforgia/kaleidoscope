//! Slice 01 — Walking skeleton.
//!
//! Maps to `docs/feature/aperture/slices/slice-01-walking-skeleton.md`.
//! Companion stories: US-AP-01 (gRPC arm), US-AP-03 (gRPC arm),
//! US-AP-04 (gRPC arm).
//!
//! The user-centric outcome: a real OpenTelemetry SDK sends a real
//! `ExportLogsServiceRequest` over OTLP/gRPC to a freshly-launched
//! Aperture, the request validates against the harness, the typed
//! record reaches `OtlpSink::accept`, the SDK receives gRPC `OK`, and
//! a structured stderr line names the accepted record.
//!
//! The tests below send real OTLP/gRPC frames over the loopback
//! interface using a `tonic`-generated client against the same
//! `LogsService` definition Aperture's listener exposes. The harness
//! is real (not a stub); the sink is the [`RecordingSink`] test
//! double from `aperture::testing`.
//!
//! Mandate Single-Then-Per-Fact: each user-observable claim is its own
//! `#[test]` so a mutation can only kill one assertion at a time.
//! Mandate Hexagonal: every test enters Aperture through its driving
//! port (the gRPC listener over real TCP); no test reaches into
//! internal modules.
//!
//! ## RED-on-day-one
//!
//! `aperture::spawn` and `Handle::wait_until_ready` both panic with
//! `unimplemented!()` at DISTILL. Every test below therefore panics at
//! the first `start_default()` call. DELIVER replaces the panics with
//! a working composition root and the tests progress to the actual
//! assertions.

mod common;

use std::time::Duration;

use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use prost::Message;
use tonic::transport::Channel;
use tonic::Code;

use crate::common::{
    capture_stderr_events, encode_logs_request, expect_stderr_event, start_default, wait_for,
    StderrEvent,
};

// =========================================================================
// Walking skeleton: one valid logs export round-trips end-to-end
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn customer_exports_one_log_record_and_receives_grpc_ok() {
    let instance = start_default().await;

    // The "real OTel SDK" surface for a Rust integration test: a
    // tonic-generated `LogsServiceClient` against the same proto
    // definition Aperture's listener implements. Same wire format,
    // same client an OTel exporter would generate.
    let channel = Channel::from_shared(instance.grpc_endpoint())
        .expect("valid grpc endpoint")
        .connect()
        .await
        .expect("connect to aperture grpc listener");
    let mut client = LogsServiceClient::new(channel);

    let req = decode_request(encode_logs_request("payments-api", 1));
    let response = client.export(req).await;

    assert!(
        response.is_ok(),
        "valid logs export should receive gRPC OK; got: {response:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_exports_one_log_record_and_record_reaches_sink() {
    let instance = start_default().await;
    let channel = Channel::from_shared(instance.grpc_endpoint())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = LogsServiceClient::new(channel);

    let req = decode_request(encode_logs_request("payments-api", 1));
    let _ = client.export(req).await;

    wait_for(|| !instance.sink.is_empty(), Duration::from_secs(2)).await;
    let recorded = instance.sink.drain();
    assert_eq!(
        recorded.len(),
        1,
        "exactly one record should reach the sink"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_exports_one_log_record_and_record_carries_logs_variant() {
    let instance = start_default().await;
    let channel = Channel::from_shared(instance.grpc_endpoint())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = LogsServiceClient::new(channel);

    let req = decode_request(encode_logs_request("payments-api", 1));
    let _ = client.export(req).await;

    wait_for(|| !instance.sink.is_empty(), Duration::from_secs(2)).await;
    let recorded = instance.sink.drain();
    assert!(
        matches!(recorded.first(), Some(aperture::ports::SinkRecord::Logs(_))),
        "the SinkRecord variant passed should be Logs"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_exports_three_log_records_and_record_count_matches() {
    let instance = start_default().await;
    let channel = Channel::from_shared(instance.grpc_endpoint())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = LogsServiceClient::new(channel);

    let req = decode_request(encode_logs_request("checkout-api", 3));
    let _ = client.export(req).await;

    wait_for(|| !instance.sink.is_empty(), Duration::from_secs(2)).await;
    let recorded = instance.sink.drain();
    let span_count = match recorded.first() {
        Some(aperture::ports::SinkRecord::Logs(req)) => req
            .resource_logs
            .iter()
            .flat_map(|rl| rl.scope_logs.iter())
            .map(|sl| sl.log_records.len())
            .sum::<usize>(),
        _ => panic!("expected a Logs SinkRecord"),
    };
    assert_eq!(span_count, 3, "three log records should reach the sink");
}

// =========================================================================
// Stderr observability — listener_bound, request_received, sink_accepted
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn startup_emits_listener_bound_stderr_line_for_grpc_transport() {
    let (_instance, events) = capture_stderr_events(|| async { start_default().await }).await;
    let bound = expect_stderr_event(&events, "listener_bound");
    let transport = bound.fields.get("transport").and_then(|v| v.as_str());
    assert_eq!(transport, Some("grpc"));
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_exports_one_log_record_and_request_received_line_is_emitted() {
    let (events_with_export, events) = capture_stderr_events(|| async {
        let instance = start_default().await;
        let channel = Channel::from_shared(instance.grpc_endpoint())
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = LogsServiceClient::new(channel);
        let _ = client
            .export(decode_request(encode_logs_request("payments-api", 1)))
            .await;
        instance
    })
    .await;
    let _ = events_with_export; // owned for the duration of capture
    let received = expect_stderr_event(&events, "request_received");
    let signal = received.fields.get("signal").and_then(|v| v.as_str());
    assert_eq!(signal, Some("logs"));
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_exports_one_log_record_and_sink_accepted_line_names_record_count() {
    let (_, events) = capture_stderr_events(|| async {
        let instance = start_default().await;
        let channel = Channel::from_shared(instance.grpc_endpoint())
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = LogsServiceClient::new(channel);
        let _ = client
            .export(decode_request(encode_logs_request("payments-api", 1)))
            .await;
        instance
    })
    .await;
    let accepted = expect_stderr_event(&events, "sink_accepted");
    let count = accepted.fields.get("record_count").and_then(|v| v.as_u64());
    assert_eq!(count, Some(1));
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_exports_one_log_record_and_sink_accepted_line_names_service_name() {
    let (_, events) = capture_stderr_events(|| async {
        let instance = start_default().await;
        let channel = Channel::from_shared(instance.grpc_endpoint())
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = LogsServiceClient::new(channel);
        let _ = client
            .export(decode_request(encode_logs_request("payments-api", 1)))
            .await;
        instance
    })
    .await;
    let accepted = expect_stderr_event(&events, "sink_accepted");
    let svc = accepted
        .fields
        .get("resource.service.name")
        .and_then(|v| v.as_str());
    assert_eq!(svc, Some("payments-api"));
}

// =========================================================================
// Reject path: empty body returns INVALID_ARGUMENT with the harness's
// verbatim violation message.
// =========================================================================
//
// Slice 01 names the gRPC reject path explicitly: "An empty body
// produces gRPC INVALID_ARGUMENT with grpc-message carrying the
// harness's OtlpViolation::Display output verbatim".
//
// The harness's `validate_logs(&[], Framing::GrpcProtobuf)` returns
// `Err(OtlpViolation { rule: EmptyInput, ... })`. Aperture's response
// MUST contain the violation's Display output verbatim (DISCUSS D6).

#[tokio::test(flavor = "multi_thread")]
async fn customer_sends_empty_body_and_receives_invalid_argument() {
    let instance = start_default().await;
    let channel = Channel::from_shared(instance.grpc_endpoint())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = LogsServiceClient::new(channel);

    // An "empty" ExportLogsServiceRequest body — zero-length resource_logs.
    // This is what the harness rejects under `Rule::EmptyInput`.
    let zero = ExportLogsServiceRequest {
        resource_logs: vec![],
    };
    let result = client.export(zero).await;
    let err = result.expect_err("empty body should be rejected");
    assert_eq!(err.code(), Code::InvalidArgument);
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_sends_empty_body_and_grpc_message_names_empty_input_rule() {
    let instance = start_default().await;
    let channel = Channel::from_shared(instance.grpc_endpoint())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = LogsServiceClient::new(channel);

    let zero = ExportLogsServiceRequest {
        resource_logs: vec![],
    };
    let err = client
        .export(zero)
        .await
        .expect_err("empty body should be rejected");
    assert!(
        err.message().contains("rule=EmptyInput"),
        "grpc-message should contain harness's verbatim violation, got: {:?}",
        err.message()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_sends_empty_body_and_grpc_message_names_logs_signal() {
    let instance = start_default().await;
    let channel = Channel::from_shared(instance.grpc_endpoint())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = LogsServiceClient::new(channel);

    let zero = ExportLogsServiceRequest {
        resource_logs: vec![],
    };
    let err = client.export(zero).await.expect_err("rejected");
    assert!(
        err.message().contains("signal=Logs"),
        "grpc-message should name the asserted signal: {:?}",
        err.message()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_sends_empty_body_and_grpc_message_names_grpc_protobuf_framing() {
    let instance = start_default().await;
    let channel = Channel::from_shared(instance.grpc_endpoint())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = LogsServiceClient::new(channel);

    let zero = ExportLogsServiceRequest {
        resource_logs: vec![],
    };
    let err = client.export(zero).await.expect_err("rejected");
    assert!(
        err.message().contains("framing=GrpcProtobuf"),
        "grpc-message should name the framing: {:?}",
        err.message()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_sends_empty_body_and_no_record_reaches_sink() {
    let instance = start_default().await;
    let channel = Channel::from_shared(instance.grpc_endpoint())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = LogsServiceClient::new(channel);

    let zero = ExportLogsServiceRequest {
        resource_logs: vec![],
    };
    let _ = client.export(zero).await;

    // Give Aperture time to (incorrectly) hand off if it were going to.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        instance.sink.is_empty(),
        "rejected requests must not reach the sink"
    );
}

// =========================================================================
// Helpers local to this slice
// =========================================================================

/// Decode an already-encoded body back into an
/// `ExportLogsServiceRequest`. The encoder helpers in `common` produce
/// raw bytes (because some tests want truncated/malformed bytes); the
/// gRPC client takes the typed message. This is the round-trip that
/// gives us a single source of truth for "what bytes Aperture sees".
fn decode_request(bytes: Vec<u8>) -> ExportLogsServiceRequest {
    ExportLogsServiceRequest::decode(&bytes[..]).expect("encoder produced valid bytes")
}

#[allow(dead_code)]
fn _events_must_be_used(events: &[StderrEvent]) {
    let _ = events;
}
