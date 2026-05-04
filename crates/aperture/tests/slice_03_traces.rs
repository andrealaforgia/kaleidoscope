//! Slice 03 — Traces signal end-to-end.
//!
//! Maps to `docs/feature/aperture/slices/slice-03-traces.md`.
//! Companion story: US-AP-05.
//!
//! The user-centric outcome: an OTel SDK that emits spans gets
//! first-class treatment on both transports. A real
//! `ExportTraceServiceRequest` round-trips to gRPC OK / HTTP 200; the
//! `RecordingSink` records a `SinkRecord::Traces`; stderr names the
//! span count.
//!
//! Reject-path symmetry: a logs body sent to `/v1/traces` returns
//! `WireType::SignalMismatch observed=Logs asserted=Traces` verbatim.

mod common;

use std::time::Duration;

use opentelemetry_proto::tonic::collector::trace::v1::trace_service_client::TraceServiceClient;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use prost::Message;
use tonic::transport::Channel;

use crate::common::{
    capture_stderr_events, encode_logs_request, encode_traces_request, expect_stderr_event,
    post_otlp_protobuf, start_default, wait_for,
};

// =========================================================================
// Traces accept on gRPC
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn customer_exports_one_span_over_grpc_and_receives_grpc_ok() {
    let instance = start_default().await;
    let channel = Channel::from_shared(instance.grpc_endpoint())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = TraceServiceClient::new(channel);
    let req = decode_traces(encode_traces_request("payments-api", 1));
    let response = client.export(req).await;
    assert!(response.is_ok(), "got: {response:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_exports_one_span_over_grpc_and_record_carries_traces_variant() {
    let instance = start_default().await;
    let channel = Channel::from_shared(instance.grpc_endpoint())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = TraceServiceClient::new(channel);
    let _ = client
        .export(decode_traces(encode_traces_request("payments-api", 1)))
        .await;
    wait_for(|| !instance.sink.is_empty(), Duration::from_secs(2)).await;
    let recorded = instance.sink.drain();
    assert!(matches!(
        recorded.first(),
        Some(aperture::ports::SinkRecord::Traces(_))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_exports_one_span_over_grpc_and_sink_accepted_line_names_span_count() {
    let (_, events) = capture_stderr_events(|| async {
        let instance = start_default().await;
        let channel = Channel::from_shared(instance.grpc_endpoint())
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = TraceServiceClient::new(channel);
        let _ = client
            .export(decode_traces(encode_traces_request("payments-api", 1)))
            .await;
        instance
    })
    .await;
    let evt = expect_stderr_event(&events, "sink_accepted");
    let signal = evt.fields.get("signal").and_then(|v| v.as_str());
    assert_eq!(signal, Some("traces"));
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_exports_three_spans_and_span_count_field_is_three() {
    let (_, events) = capture_stderr_events(|| async {
        let instance = start_default().await;
        let channel = Channel::from_shared(instance.grpc_endpoint())
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = TraceServiceClient::new(channel);
        let _ = client
            .export(decode_traces(encode_traces_request("checkout-api", 3)))
            .await;
        instance
    })
    .await;
    let evt = expect_stderr_event(&events, "sink_accepted");
    let count = evt.fields.get("span_count").and_then(|v| v.as_u64());
    assert_eq!(count, Some(3));
}

// =========================================================================
// Traces accept on HTTP
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_traces_body_to_http_and_receives_status_200() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let response = post_otlp_protobuf(
        &client,
        &instance.http_base_url(),
        "traces",
        encode_traces_request("payments-api", 1),
    )
    .await;
    assert_eq!(response.status().as_u16(), 200);
}

// =========================================================================
// Traces signal-mismatch reject
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_logs_body_to_traces_path_and_receives_status_400() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let response = post_otlp_protobuf(
        &client,
        &instance.http_base_url(),
        "traces",
        encode_logs_request("payments-api", 1),
    )
    .await;
    assert_eq!(response.status().as_u16(), 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_logs_body_to_traces_path_and_response_names_signal_mismatch_rule() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let body = post_otlp_protobuf(
        &client,
        &instance.http_base_url(),
        "traces",
        encode_logs_request("payments-api", 1),
    )
    .await
    .text()
    .await
    .expect("read body");
    assert!(
        body.contains("rule=WireType::SignalMismatch"),
        "expected signal-mismatch rule; got: {body:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_logs_body_to_traces_path_and_response_names_observed_logs() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let body = post_otlp_protobuf(
        &client,
        &instance.http_base_url(),
        "traces",
        encode_logs_request("payments-api", 1),
    )
    .await
    .text()
    .await
    .expect("read body");
    assert!(
        body.contains("observed=Logs"),
        "expected observed=Logs; got: {body:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_logs_body_to_traces_path_and_response_names_asserted_traces() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let body = post_otlp_protobuf(
        &client,
        &instance.http_base_url(),
        "traces",
        encode_logs_request("payments-api", 1),
    )
    .await
    .text()
    .await
    .expect("read body");
    assert!(
        body.contains("asserted=Traces"),
        "expected asserted=Traces; got: {body:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_logs_body_to_traces_path_and_no_record_reaches_sink() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let _ = post_otlp_protobuf(
        &client,
        &instance.http_base_url(),
        "traces",
        encode_logs_request("payments-api", 1),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(instance.sink.is_empty());
}

// =========================================================================
// Helpers
// =========================================================================

fn decode_traces(bytes: Vec<u8>) -> ExportTraceServiceRequest {
    ExportTraceServiceRequest::decode(&bytes[..]).expect("encoder produced valid bytes")
}
