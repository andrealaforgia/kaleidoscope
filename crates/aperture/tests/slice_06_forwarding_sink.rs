//! Slice 06 — ForwardingSink (downstream OTLP write).
//!
//! Maps to `docs/feature/aperture/slices/slice-06-forwarding-sink.md`.
//! Companion story: US-AP-08.
//!
//! The user-centric outcome: an operator runs Aperture with
//! `sink=forwarding` pointing at their OTel-compatible backend
//! (Loki/Tempo/Mimir/OTel Collector). Accepted records reach the
//! downstream verbatim; the SDK sees gRPC OK / HTTP 200 only when the
//! downstream itself returned 2xx; downstream failures surface as
//! gRPC `UNAVAILABLE` / HTTP 503 with named stderr lines.
//!
//! These tests stand up a `wiremock` server as the downstream double
//! and assert Aperture's two-stage probe (OPTIONS then POST), the
//! happy-path POST forwarding, and the three failure modes (5xx,
//! connection refused, downstream timeout).

mod common;

use std::sync::Arc;
use std::time::Duration;

use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use prost::Message;
use tonic::transport::Channel;
use tonic::Code;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use aperture::config::Config;
use aperture::ports::OtlpSink;
use aperture::testing::RecordingSink;

use crate::common::{capture_stderr_events, encode_logs_request, expect_stderr_event};

// =========================================================================
// ForwardingSink probe — two-stage OPTIONS then POST
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn forwarding_sink_probe_succeeds_against_options_responder() {
    let downstream = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&downstream)
        .await;

    let config = Config::builder()
        .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
        .http_bind_addr("127.0.0.1:0".parse().unwrap())
        .forwarding_sink(downstream.uri())
        .build()
        .expect("forwarding config builds");
    // The composition root invokes ForwardingSink::probe at startup.
    // Wire-then-probe-then-use: a successful probe lets startup
    // proceed; a failed probe surfaces as an error from `spawn`.
    let sink: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
    let result = aperture::spawn(config, sink).await;
    assert!(result.is_ok(), "probe should succeed; got: {result:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn forwarding_sink_probe_falls_back_to_post_when_options_returns_405() {
    let downstream = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(405))
        .mount(&downstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&downstream)
        .await;

    let config = Config::builder()
        .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
        .http_bind_addr("127.0.0.1:0".parse().unwrap())
        .forwarding_sink(downstream.uri())
        .build()
        .expect("forwarding config builds");
    let sink: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
    let result = aperture::spawn(config, sink).await;
    assert!(
        result.is_ok(),
        "degraded probe (POST fallback) should succeed; got: {result:?}"
    );
}

// =========================================================================
// ForwardingSink probe lies — catalogued substrate lie
// =========================================================================
//
// The catalogued v0 substrate lie: a downstream that returns 200 to
// OPTIONS but 503 to POST. Aperture's probe MUST catch this at
// startup (the degraded-probe path sends a zero-records POST and
// requires 2xx). This is the same scenario `tests/probe_gold_runner.rs`
// names in `component-design.md`; we encode it here as part of Slice
// 06 because it is the user-observable acceptance for "wire then
// probe then use".

#[tokio::test(flavor = "multi_thread")]
async fn forwarding_sink_probe_refuses_startup_when_downstream_lies_with_503_on_post() {
    let downstream = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&downstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&downstream)
        .await;

    let config = Config::builder()
        .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
        .http_bind_addr("127.0.0.1:0".parse().unwrap())
        .forwarding_sink(downstream.uri())
        .build()
        .expect("forwarding config builds");
    let sink: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
    let result = aperture::spawn(config, sink).await;
    assert!(
        result.is_err(),
        "probe should refuse startup when downstream lies; got: {result:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn forwarding_sink_probe_failure_emits_health_startup_refused_event() {
    let (_, events) = capture_stderr_events(|| async {
        let downstream = MockServer::start().await;
        Mock::given(method("OPTIONS"))
            .and(path("/v1/logs"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&downstream)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/logs"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&downstream)
            .await;
        let config = Config::builder()
            .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
            .http_bind_addr("127.0.0.1:0".parse().unwrap())
            .forwarding_sink(downstream.uri())
            .build()
            .unwrap();
        let sink: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
        let _ = aperture::spawn(config, sink).await;
    })
    .await;
    let evt = expect_stderr_event(&events, "health.startup.refused");
    assert_eq!(evt.level, "error");
}

// =========================================================================
// Happy path — typed record reaches downstream
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn customer_exports_one_log_record_and_downstream_receives_protobuf_post() {
    let downstream = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&downstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&downstream)
        .await;

    let config = Config::builder()
        .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
        .http_bind_addr("127.0.0.1:0".parse().unwrap())
        .forwarding_sink(downstream.uri())
        .build()
        .unwrap();
    let sink: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
    let handle = aperture::spawn(config, sink).await.expect("spawn");
    handle.wait_until_ready().await.expect("ready");

    let channel = Channel::from_shared(format!("http://{}", handle.grpc_addr()))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = LogsServiceClient::new(channel);
    let response = client
        .export(decode_logs(encode_logs_request("payments-api", 1)))
        .await;
    assert!(response.is_ok(), "got: {response:?}");

    // The downstream MUST have received exactly one POST.
    let received = downstream.received_requests().await.unwrap_or_default();
    let post_count = received
        .iter()
        .filter(|r| r.method == http::Method::POST && r.url.path() == "/v1/logs")
        .count();
    // The probe phase POSTs zero records when OPTIONS=200; this
    // downstream returns 204 to OPTIONS (no fallback POST). Therefore
    // the POST count after a single export = 1 (the export itself).
    assert_eq!(post_count, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn forwarding_sink_accepted_event_includes_downstream_endpoint() {
    let (_, events) = capture_stderr_events(|| async {
        let downstream = MockServer::start().await;
        Mock::given(method("OPTIONS"))
            .and(path("/v1/logs"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&downstream)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/logs"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&downstream)
            .await;
        let config = Config::builder()
            .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
            .http_bind_addr("127.0.0.1:0".parse().unwrap())
            .forwarding_sink(downstream.uri())
            .build()
            .unwrap();
        let sink: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
        let handle = aperture::spawn(config, sink).await.expect("spawn");
        handle.wait_until_ready().await.expect("ready");
        let channel = Channel::from_shared(format!("http://{}", handle.grpc_addr()))
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = LogsServiceClient::new(channel);
        let _ = client
            .export(decode_logs(encode_logs_request("payments-api", 1)))
            .await;
    })
    .await;
    let evt = expect_stderr_event(&events, "sink_accepted");
    let sink_kind = evt.fields.get("sink").and_then(|v| v.as_str());
    assert_eq!(sink_kind, Some("forwarding"));
}

#[tokio::test(flavor = "multi_thread")]
async fn forwarding_sink_accepted_event_includes_downstream_latency_ms_field() {
    let (_, events) = capture_stderr_events(|| async {
        let downstream = MockServer::start().await;
        Mock::given(method("OPTIONS"))
            .and(path("/v1/logs"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&downstream)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/logs"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&downstream)
            .await;
        let config = Config::builder()
            .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
            .http_bind_addr("127.0.0.1:0".parse().unwrap())
            .forwarding_sink(downstream.uri())
            .build()
            .unwrap();
        let sink: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
        let handle = aperture::spawn(config, sink).await.expect("spawn");
        handle.wait_until_ready().await.expect("ready");
        let channel = Channel::from_shared(format!("http://{}", handle.grpc_addr()))
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = LogsServiceClient::new(channel);
        let _ = client
            .export(decode_logs(encode_logs_request("payments-api", 1)))
            .await;
    })
    .await;
    let evt = expect_stderr_event(&events, "sink_accepted");
    assert!(
        evt.fields.get("downstream_latency_ms").is_some(),
        "expected downstream_latency_ms field on forwarding sink_accepted event"
    );
}

// =========================================================================
// Downstream 5xx -> upstream UNAVAILABLE
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn customer_exports_when_downstream_returns_503_and_receives_grpc_unavailable() {
    let downstream = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&downstream)
        .await;
    // Probe is OPTIONS-204; only POSTs return 503. (If we made
    // POST=503 unconditionally the probe would refuse startup, which
    // is the previous test's path. Here we use up_to(1) to make the
    // probe path independent of the export path — the probe never
    // POSTs because OPTIONS succeeded.)
    Mock::given(method("POST"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&downstream)
        .await;

    let config = Config::builder()
        .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
        .http_bind_addr("127.0.0.1:0".parse().unwrap())
        .forwarding_sink(downstream.uri())
        .build()
        .unwrap();
    let sink: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
    let handle = aperture::spawn(config, sink).await.expect("spawn");
    handle.wait_until_ready().await.expect("ready");

    let channel = Channel::from_shared(format!("http://{}", handle.grpc_addr()))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = LogsServiceClient::new(channel);
    let result = client
        .export(decode_logs(encode_logs_request("payments-api", 1)))
        .await;
    let err = result.expect_err("downstream 503 should surface upstream");
    assert_eq!(err.code(), Code::Unavailable);
}

#[tokio::test(flavor = "multi_thread")]
async fn forwarding_sink_failure_emits_sink_failed_stderr_event() {
    let (_, events) = capture_stderr_events(|| async {
        let downstream = MockServer::start().await;
        Mock::given(method("OPTIONS"))
            .and(path("/v1/logs"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&downstream)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/logs"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&downstream)
            .await;
        let config = Config::builder()
            .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
            .http_bind_addr("127.0.0.1:0".parse().unwrap())
            .forwarding_sink(downstream.uri())
            .build()
            .unwrap();
        let sink: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
        let handle = aperture::spawn(config, sink).await.expect("spawn");
        handle.wait_until_ready().await.expect("ready");
        let channel = Channel::from_shared(format!("http://{}", handle.grpc_addr()))
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = LogsServiceClient::new(channel);
        let _ = client
            .export(decode_logs(encode_logs_request("payments-api", 1)))
            .await;
    })
    .await;
    let evt = expect_stderr_event(&events, "sink_failed");
    assert_eq!(evt.level, "error");
}

// =========================================================================
// Downstream connection refused -> upstream UNAVAILABLE
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn customer_exports_when_downstream_endpoint_is_unreachable_and_receives_unavailable() {
    // Pick a port that nothing is listening on. We deliberately do
    // NOT start a wiremock server so the probe and the request both
    // hit "connection refused".
    let nowhere = "http://127.0.0.1:1";
    let config = Config::builder()
        .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
        .http_bind_addr("127.0.0.1:0".parse().unwrap())
        .forwarding_sink(nowhere)
        .build()
        .unwrap();
    let sink: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
    // The probe will refuse startup because the endpoint is
    // unreachable. (DISCUSS US-AP-08 Domain Examples #3 — the
    // misconfigured-endpoint path is by design caught at startup.)
    let result = aperture::spawn(config, sink).await;
    assert!(
        result.is_err(),
        "probe should refuse startup when endpoint is unreachable"
    );
}

// =========================================================================
// Downstream timeout -> upstream UNAVAILABLE
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn customer_exports_when_downstream_hangs_past_timeout_and_receives_unavailable() {
    let downstream = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&downstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(2_000)))
        .mount(&downstream)
        .await;

    let config = Config::builder()
        .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
        .http_bind_addr("127.0.0.1:0".parse().unwrap())
        .forwarding_sink(downstream.uri())
        // Force a sub-second timeout; the downstream sleeps 2 s.
        .forwarding_timeout(Duration::from_millis(250))
        .build()
        .unwrap();
    let sink: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
    let handle = aperture::spawn(config, sink).await.expect("spawn");
    handle.wait_until_ready().await.expect("ready");

    let channel = Channel::from_shared(format!("http://{}", handle.grpc_addr()))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = LogsServiceClient::new(channel);
    let result = client
        .export(decode_logs(encode_logs_request("payments-api", 1)))
        .await;
    let err = result.expect_err("downstream timeout should surface upstream");
    assert_eq!(err.code(), Code::Unavailable);
}

fn decode_logs(bytes: Vec<u8>) -> ExportLogsServiceRequest {
    ExportLogsServiceRequest::decode(&bytes[..]).expect("encoder produced valid bytes")
}
