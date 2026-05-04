//! Slice 02 — HTTP/protobuf transport plus `/healthz` and `/readyz`.
//!
//! Maps to `docs/feature/aperture/slices/slice-02-http-protobuf-and-readiness.md`.
//! Companion stories: US-AP-02 (full), US-AP-03 (HTTP arm),
//! US-AP-04 (HTTP arm).
//!
//! The user-centric outcome: an operator with a Kubernetes-style
//! deployment probes liveness and readiness on the HTTP listener; an
//! OTel SDK that prefers OTLP/HTTP/protobuf POSTs an
//! `ExportLogsServiceRequest` and receives HTTP 200; misconfigured
//! clients (wrong Content-Type, unknown path) receive named refusals.
//!
//! Tests use `reqwest` against the loopback HTTP listener.

mod common;

use std::time::Duration;

use crate::common::{
    capture_stderr_events, encode_logs_request, expect_stderr_event, post_otlp_protobuf,
    start_default, wait_for,
};

// =========================================================================
// Liveness probe — always 200 while the process is up
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn operator_probes_healthz_and_receives_status_200() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/healthz", instance.http_base_url()))
        .send()
        .await
        .expect("GET /healthz");
    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn operator_probes_healthz_and_response_body_is_ok() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let body = client
        .get(format!("{}/healthz", instance.http_base_url()))
        .send()
        .await
        .expect("GET /healthz")
        .text()
        .await
        .expect("read body");
    assert!(body.trim() == "ok", "expected body 'ok', got: {body:?}");
}

// =========================================================================
// Readiness probe — 200 once both listeners are bound
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn operator_probes_readyz_after_startup_and_receives_status_200() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/readyz", instance.http_base_url()))
        .send()
        .await
        .expect("GET /readyz");
    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn operator_probes_readyz_after_startup_and_response_body_is_ready() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let body = client
        .get(format!("{}/readyz", instance.http_base_url()))
        .send()
        .await
        .expect("GET /readyz")
        .text()
        .await
        .expect("read body");
    assert!(
        body.trim() == "ready",
        "expected body 'ready', got: {body:?}"
    );
}

// =========================================================================
// HTTP/protobuf accept path — valid logs body returns HTTP 200
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_valid_logs_body_and_receives_status_200() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let response = post_otlp_protobuf(
        &client,
        &instance.http_base_url(),
        "logs",
        encode_logs_request("payments-api", 1),
    )
    .await;
    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_valid_logs_body_and_record_reaches_sink() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let _ = post_otlp_protobuf(
        &client,
        &instance.http_base_url(),
        "logs",
        encode_logs_request("payments-api", 1),
    )
    .await;
    wait_for(|| !instance.sink.is_empty(), Duration::from_secs(2)).await;
    assert_eq!(instance.sink.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_valid_logs_body_and_sink_accepted_line_names_http_protobuf_transport() {
    let (_, events) = capture_stderr_events(|| async {
        let instance = start_default().await;
        let client = reqwest::Client::new();
        let _ = post_otlp_protobuf(
            &client,
            &instance.http_base_url(),
            "logs",
            encode_logs_request("payments-api", 1),
        )
        .await;
        instance
    })
    .await;
    let received = expect_stderr_event(&events, "request_received");
    let transport = received.fields.get("transport").and_then(|v| v.as_str());
    assert_eq!(transport, Some("http_protobuf"));
}

// =========================================================================
// HTTP/protobuf reject path — wrong content type
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_with_json_content_type_and_receives_status_415() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/v1/logs", instance.http_base_url()))
        .header("Content-Type", "application/json")
        .body(r#"{"not":"valid"}"#)
        .send()
        .await
        .expect("POST /v1/logs");
    assert_eq!(response.status().as_u16(), 415);
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_with_json_content_type_and_unsupported_media_type_event_is_emitted() {
    let (_, events) = capture_stderr_events(|| async {
        let instance = start_default().await;
        let client = reqwest::Client::new();
        let _ = client
            .post(format!("{}/v1/logs", instance.http_base_url()))
            .header("Content-Type", "application/json")
            .body(r#"{"not":"valid"}"#)
            .send()
            .await;
        instance
    })
    .await;
    let evt = expect_stderr_event(&events, "unsupported_media_type");
    assert_eq!(evt.level, "warn");
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_with_json_content_type_and_no_record_reaches_sink() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let _ = client
        .post(format!("{}/v1/logs", instance.http_base_url()))
        .header("Content-Type", "application/json")
        .body(r#"{"not":"valid"}"#)
        .send()
        .await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(instance.sink.is_empty());
}

// =========================================================================
// HTTP/protobuf reject path — unknown OTLP path
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_to_unknown_path_and_receives_status_404() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/v1/profile", instance.http_base_url()))
        .header("Content-Type", "application/x-protobuf")
        .body(Vec::<u8>::new())
        .send()
        .await
        .expect("POST /v1/profile");
    assert_eq!(response.status().as_u16(), 404);
}

// =========================================================================
// HTTP/protobuf reject path — empty body
// =========================================================================
//
// Slice 02 acceptance summary: "An empty body to POST /v1/logs returns
// HTTP 400 with the harness's violation Display string verbatim in the
// body."

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_empty_body_and_receives_status_400() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let response = post_otlp_protobuf(&client, &instance.http_base_url(), "logs", Vec::new()).await;
    assert_eq!(response.status().as_u16(), 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_empty_body_and_response_body_names_empty_input_rule() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let body = post_otlp_protobuf(&client, &instance.http_base_url(), "logs", Vec::new())
        .await
        .text()
        .await
        .expect("read body");
    assert!(
        body.contains("rule=EmptyInput"),
        "expected harness violation Display verbatim; got: {body:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_empty_body_and_response_body_names_http_protobuf_framing() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let body = post_otlp_protobuf(&client, &instance.http_base_url(), "logs", Vec::new())
        .await
        .text()
        .await
        .expect("read body");
    assert!(
        body.contains("framing=HttpProtobuf"),
        "expected framing=HttpProtobuf in body; got: {body:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn customer_posts_empty_body_and_response_content_type_is_text_plain() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let response = post_otlp_protobuf(&client, &instance.http_base_url(), "logs", Vec::new()).await;
    let content_type = response
        .headers()
        .get("Content-Type")
        .map(|h| h.to_str().unwrap_or("").to_string())
        .unwrap_or_default();
    assert!(
        content_type.contains("text/plain"),
        "expected text/plain Content-Type; got: {content_type:?}"
    );
}
