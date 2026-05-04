//! Slice 04 — Metrics signal end-to-end.
//!
//! Maps to `docs/feature/aperture/slices/slice-04-metrics.md`.
//! Companion story: US-AP-06.
//!
//! The user-centric outcome: an OTel SDK that emits metrics completes
//! the OTLP three-signal contract. A real `ExportMetricsServiceRequest`
//! over either transport round-trips to gRPC OK / HTTP 200; the
//! `RecordingSink` records a `SinkRecord::Metrics`; stderr names the
//! `data_point_count` (one per `Metric`, not per bucket — DISCUSS
//! locked).

mod common;

use std::time::Duration;

use opentelemetry_proto::tonic::collector::metrics::v1::metrics_service_client::MetricsServiceClient;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use prost::Message;
use tonic::transport::Channel;

use crate::common::{
    capture_stderr_events, encode_metrics_request, encode_traces_request, expect_stderr_event,
    post_otlp_protobuf, start_default, wait_for,
};

// =========================================================================
// Metrics accept on gRPC
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn customer_exports_metrics_over_grpc_and_receives_grpc_ok() {
    let instance = start_default().await;
    let channel = Channel::from_shared(instance.grpc_endpoint())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = MetricsServiceClient::new(channel);
    let req = decode_metrics(encode_metrics_request("payments-api"));
    let response = client.export(req).await;
    assert!(response.is_ok(), "got: {response:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_exports_metrics_over_grpc_and_record_carries_metrics_variant() {
    let instance = start_default().await;
    let channel = Channel::from_shared(instance.grpc_endpoint())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = MetricsServiceClient::new(channel);
    let _ = client
        .export(decode_metrics(encode_metrics_request("payments-api")))
        .await;
    wait_for(|| !instance.sink.is_empty(), Duration::from_secs(2)).await;
    let recorded = instance.sink.drain();
    assert!(matches!(
        recorded.first(),
        Some(aperture::ports::SinkRecord::Metrics(_))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_exports_metrics_and_data_point_count_is_two() {
    let (_, events) = capture_stderr_events(|| async {
        let instance = start_default().await;
        let channel = Channel::from_shared(instance.grpc_endpoint())
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = MetricsServiceClient::new(channel);
        let _ = client
            .export(decode_metrics(encode_metrics_request("payments-api")))
            .await;
        instance
    })
    .await;
    let evt = expect_stderr_event(&events, "sink_accepted");
    let count = evt.fields.get("data_point_count").and_then(|v| v.as_u64());
    // The minimal metrics fixture carries one Metric carrying a Sum
    // (one data point) and one Metric carrying a Gauge (one data
    // point). DISCUSS US-AP-06 explicitly chooses
    // "data points" as the unit, with one data point per Metric on
    // the gauge/sum/histogram base case.
    assert_eq!(count, Some(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_exports_metrics_and_signal_field_is_metrics() {
    let (_, events) = capture_stderr_events(|| async {
        let instance = start_default().await;
        let channel = Channel::from_shared(instance.grpc_endpoint())
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = MetricsServiceClient::new(channel);
        let _ = client
            .export(decode_metrics(encode_metrics_request("payments-api")))
            .await;
        instance
    })
    .await;
    let evt = expect_stderr_event(&events, "sink_accepted");
    let signal = evt.fields.get("signal").and_then(|v| v.as_str());
    assert_eq!(signal, Some("metrics"));
}

// =========================================================================
// Metrics accept on HTTP
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_metrics_body_to_http_and_receives_status_200() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let response = post_otlp_protobuf(
        &client,
        &instance.http_base_url(),
        "metrics",
        encode_metrics_request("payments-api"),
    )
    .await;
    assert_eq!(response.status().as_u16(), 200);
}

// =========================================================================
// Metrics signal-mismatch reject
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_traces_body_to_metrics_path_and_receives_status_400() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let response = post_otlp_protobuf(
        &client,
        &instance.http_base_url(),
        "metrics",
        encode_traces_request("payments-api", 1),
    )
    .await;
    assert_eq!(response.status().as_u16(), 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_traces_body_to_metrics_path_and_response_names_signal_mismatch() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let body = post_otlp_protobuf(
        &client,
        &instance.http_base_url(),
        "metrics",
        encode_traces_request("payments-api", 1),
    )
    .await
    .text()
    .await
    .expect("read body");
    assert!(body.contains("rule=WireType::SignalMismatch"));
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_traces_body_to_metrics_path_and_response_names_observed_traces() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let body = post_otlp_protobuf(
        &client,
        &instance.http_base_url(),
        "metrics",
        encode_traces_request("payments-api", 1),
    )
    .await
    .text()
    .await
    .expect("read body");
    assert!(body.contains("observed=Traces"));
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_traces_body_to_metrics_path_and_response_names_asserted_metrics() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let body = post_otlp_protobuf(
        &client,
        &instance.http_base_url(),
        "metrics",
        encode_traces_request("payments-api", 1),
    )
    .await
    .text()
    .await
    .expect("read body");
    assert!(body.contains("asserted=Metrics"));
}

// =========================================================================
// Helpers
// =========================================================================

fn decode_metrics(bytes: Vec<u8>) -> ExportMetricsServiceRequest {
    ExportMetricsServiceRequest::decode(&bytes[..]).expect("encoder produced valid bytes")
}
