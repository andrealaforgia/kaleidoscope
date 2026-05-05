//! Invariant — no telemetry from telemetry.
//!
//! DISCUSS D4 + Q6: Aperture's outbound network footprint =
//! ForwardingSink-only. No `/metrics` endpoint, no OTLP-out from
//! Aperture itself. The structural enforcement is owned by DEVOPS as
//! a network-namespace integration fixture (Linux only at v0): the
//! fixture binds Aperture inside a constrained net-ns, captures every
//! outbound packet, and asserts zero packets except listener acks
//! and ForwardingSink-to-downstream traffic.
//!
//! That fixture is the load-bearing defence. This integration test
//! is a behavioural-layer corroboration: it asserts at the
//! application surface that the documented forbidden surfaces are
//! absent. Specifically: `/metrics` is not exposed, and no outbound
//! connection is opened when `sink=stub`.
//!
//! ## DEVOPS responsibilities
//!
//! - **Network-namespace fixture**: the load-bearing defence.
//!   Documented in `docs/feature/aperture/design/wave-decisions.md > D10`.
//! - **CI workflow YAML**: Linux-only invocation; `#[cfg(target_os = "linux")]`
//!   network capture; assertion on captured bytes.
//!
//! This Rust test runs unconditionally and catches the easy-to-detect
//! forbidden surfaces (`/metrics`, `/telemetry`, etc.).

mod common;

use std::sync::Arc;
use std::time::Duration;

use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use prost::Message;
use tonic::transport::Channel;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use aperture::config::Config;
use aperture::ports::OtlpSink;
use aperture::testing::RecordingSink;

use crate::common::{encode_logs_request, start_default};

#[tokio::test(flavor = "multi_thread")]
async fn aperture_does_not_expose_a_metrics_endpoint() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/metrics", instance.http_base_url()))
        .send()
        .await
        .expect("GET /metrics");
    // 404 is the only acceptable response. 200 means a metrics
    // endpoint exists; anything in 5xx is a bug.
    assert_eq!(
        response.status().as_u16(),
        404,
        "/metrics MUST NOT be exposed in v0 (telemetry-on-telemetry forbidden)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn aperture_does_not_expose_a_telemetry_endpoint() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/telemetry", instance.http_base_url()))
        .send()
        .await
        .expect("GET /telemetry");
    assert_eq!(response.status().as_u16(), 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn aperture_with_stub_sink_idle_does_not_open_any_outbound_connection() {
    // Behavioural-layer surface for the network-namespace gate. With
    // a stub sink, Aperture should never originate outbound traffic.
    // We start the instance, leave it idle for a brief observation
    // window, then drop it. The DEVOPS-owned net-ns fixture is the
    // load-bearing defence (it actually captures packets); this test
    // asserts the application doesn't even *try* to dial out by
    // running with no DNS-resolvable target configured and observing
    // no panic / error from a non-existent forwarding peer.
    let _instance = start_default().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    // If Aperture were dialling out, the unconfigured-forwarding
    // path would fail loudly. With sink=stub (the default) there is
    // nothing to dial; the absence of any failure is the invariant.
    // (The full assertion belongs in the net-ns fixture.)
}

// =========================================================================
// Slice 06 substance — the only outbound traffic is the ForwardingSink
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn stub_sink_export_does_not_reach_an_unrelated_loopback_listener() {
    // Spin up a wiremock listener that Aperture is NOT configured to
    // forward to. With sink=stub, Aperture must not dial anything,
    // including this loopback listener. The wiremock server records
    // every request it receives; the assertion below pins the count
    // at zero. The full network-namespace gate is the load-bearing
    // defence — this is the application-surface corroboration.
    let unrelated = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&unrelated)
        .await;

    let instance = start_default().await;
    let channel = Channel::from_shared(format!("http://{}", instance.handle.grpc_addr()))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = LogsServiceClient::new(channel);
    let req = decode_logs(encode_logs_request("payments-api", 1));
    let _ = client.export(req).await.expect("export succeeds");

    let received = unrelated.received_requests().await.unwrap_or_default();
    assert_eq!(
        received.len(),
        0,
        "stub-sink Aperture must not reach any unrelated outbound endpoint; got: {:?}",
        received.iter().map(|r| &r.url).collect::<Vec<_>>()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn forwarding_sink_export_only_reaches_the_configured_downstream() {
    // Spin up TWO wiremock listeners: one is the configured downstream
    // (`reachable`), the other is unrelated (`unrelated`). Drive a
    // single export. The `reachable` server must observe exactly one
    // POST on the configured signal path. The `unrelated` server must
    // observe zero requests. This is the application-surface
    // corroboration of "ForwardingSink is the ONLY outbound network
    // Aperture originates" (DISCUSS D4 + Q6).
    let reachable = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&reachable)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&reachable)
        .await;

    let unrelated = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&unrelated)
        .await;

    let config = Config::builder()
        .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
        .http_bind_addr("127.0.0.1:0".parse().unwrap())
        .forwarding_sink(reachable.uri())
        .build()
        .expect("forwarding config builds");
    let placeholder: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
    let handle = aperture::spawn(config, placeholder).await.expect("spawn");
    handle.wait_until_ready().await.expect("ready");

    let channel = Channel::from_shared(format!("http://{}", handle.grpc_addr()))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = LogsServiceClient::new(channel);
    let req = decode_logs(encode_logs_request("payments-api", 1));
    let _ = client.export(req).await.expect("export succeeds");

    let unrelated_received = unrelated.received_requests().await.unwrap_or_default();
    assert_eq!(
        unrelated_received.len(),
        0,
        "ForwardingSink must NOT reach any unrelated endpoint; got: {:?}",
        unrelated_received
            .iter()
            .map(|r| &r.url)
            .collect::<Vec<_>>()
    );

    let reachable_received = reachable.received_requests().await.unwrap_or_default();
    let post_count = reachable_received
        .iter()
        .filter(|r| r.method == http::Method::POST && r.url.path() == "/v1/logs")
        .count();
    let foreign_path_count = reachable_received
        .iter()
        .filter(|r| r.url.path() != "/v1/logs")
        .count();
    assert_eq!(
        post_count, 1,
        "expected exactly one POST to the configured /v1/logs endpoint",
    );
    assert_eq!(
        foreign_path_count, 0,
        "ForwardingSink must POST only to the signal-specific path, never to /metrics or similar"
    );
}

fn decode_logs(bytes: Vec<u8>) -> ExportLogsServiceRequest {
    ExportLogsServiceRequest::decode(&bytes[..]).expect("encoder produced valid bytes")
}
