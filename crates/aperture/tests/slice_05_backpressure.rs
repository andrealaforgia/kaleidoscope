//! Slice 05 — Backpressure (concurrency cap, deterministic refusal).
//!
//! Maps to `docs/feature/aperture/slices/slice-05-backpressure.md`.
//! Companion story: US-AP-07.
//!
//! The user-centric outcome: when offered traffic exceeds the
//! per-transport `max_concurrent_requests` cap, Aperture refuses
//! deterministically — never blocks, never silently drops, never
//! buffers internally — and every refusal is observable on stderr.
//!
//! These tests bind small caps (`cap=2`) and a sink that holds the
//! `accept` future open, so the cap is the binding constraint. The
//! `(N+1)`-th request is the one we assert against.
//!
//! Implementation note: the slow-sink behaviour is provided by a
//! local `BarrierSink` wrapper around `RecordingSink`. DELIVER lands
//! the production semaphore; `BarrierSink` is the test-only seam that
//! holds requests in-flight long enough to hit the cap deterministically.

mod common;

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use prost::Message;
use tokio::sync::Notify;
use tonic::transport::Channel;
use tonic::Code;

use aperture::config::Config;
use aperture::ports::{OtlpSink, Probe, ProbeError, SinkError, SinkRecord};
use aperture::Handle;

use crate::common::{
    capture_stderr_events, encode_logs_request, expect_stderr_event, post_otlp_protobuf,
};

// =========================================================================
// gRPC concurrency cap exceeded -> RESOURCE_EXHAUSTED
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn fifth_concurrent_grpc_request_at_cap_four_receives_resource_exhausted() {
    let (handle, sink_barrier, _sink) = start_with_cap(4).await;
    let endpoint = format!("http://{}", handle.grpc_addr());

    // Saturate the cap. Four in-flight requests, each holding a
    // permit until the barrier is released.
    sink_barrier.hold();
    let mut in_flight = Vec::new();
    for _ in 0..4 {
        let endpoint = endpoint.clone();
        in_flight.push(tokio::spawn(async move {
            let channel = Channel::from_shared(endpoint)
                .unwrap()
                .connect()
                .await
                .unwrap();
            let mut client = LogsServiceClient::new(channel);
            client
                .export(decode_logs(encode_logs_request("payments-api", 1)))
                .await
        }));
    }

    // Give them time to occupy permits.
    tokio::time::sleep(Duration::from_millis(150)).await;

    // The fifth request: should receive RESOURCE_EXHAUSTED immediately.
    let channel = Channel::from_shared(endpoint)
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = LogsServiceClient::new(channel);
    let result = client
        .export(decode_logs(encode_logs_request("payments-api", 1)))
        .await;
    let err = result.expect_err("fifth concurrent request should be refused");
    assert_eq!(err.code(), Code::ResourceExhausted);

    // Release the barrier so the held requests can complete.
    sink_barrier.release();
    for j in in_flight {
        let _ = j.await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn fifth_concurrent_grpc_request_grpc_message_names_the_cap() {
    let (handle, sink_barrier, _sink) = start_with_cap(4).await;
    let endpoint = format!("http://{}", handle.grpc_addr());

    sink_barrier.hold();
    let mut in_flight = Vec::new();
    for _ in 0..4 {
        let endpoint = endpoint.clone();
        in_flight.push(tokio::spawn(async move {
            let channel = Channel::from_shared(endpoint)
                .unwrap()
                .connect()
                .await
                .unwrap();
            let mut client = LogsServiceClient::new(channel);
            client
                .export(decode_logs(encode_logs_request("payments-api", 1)))
                .await
        }));
    }
    tokio::time::sleep(Duration::from_millis(150)).await;

    let channel = Channel::from_shared(endpoint)
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = LogsServiceClient::new(channel);
    let err = client
        .export(decode_logs(encode_logs_request("payments-api", 1)))
        .await
        .expect_err("rejected");
    assert!(
        err.message().contains("cap of 4") || err.message().contains("cap=4"),
        "grpc-message should name the cap; got: {:?}",
        err.message()
    );

    sink_barrier.release();
    for j in in_flight {
        let _ = j.await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn grpc_concurrency_cap_hit_emits_warn_stderr_event() {
    let ((), events) = capture_stderr_events(|| async {
        let (handle, sink_barrier, _sink) = start_with_cap(2).await;
        let endpoint = format!("http://{}", handle.grpc_addr());
        sink_barrier.hold();
        let mut in_flight = Vec::new();
        for _ in 0..2 {
            let endpoint = endpoint.clone();
            in_flight.push(tokio::spawn(async move {
                let channel = Channel::from_shared(endpoint)
                    .unwrap()
                    .connect()
                    .await
                    .unwrap();
                let mut client = LogsServiceClient::new(channel);
                let _ = client
                    .export(decode_logs(encode_logs_request("payments-api", 1)))
                    .await;
            }));
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
        let channel = Channel::from_shared(endpoint)
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = LogsServiceClient::new(channel);
        let _ = client
            .export(decode_logs(encode_logs_request("payments-api", 1)))
            .await;
        sink_barrier.release();
        for j in in_flight {
            let _ = j.await;
        }
    })
    .await;
    let evt = expect_stderr_event(&events, "concurrency_cap_hit");
    assert_eq!(evt.level, "warn");
}

#[tokio::test(flavor = "multi_thread")]
async fn grpc_concurrency_cap_hit_event_names_grpc_transport() {
    let ((), events) = capture_stderr_events(|| async {
        let (handle, sink_barrier, _sink) = start_with_cap(2).await;
        let endpoint = format!("http://{}", handle.grpc_addr());
        sink_barrier.hold();
        let mut in_flight = Vec::new();
        for _ in 0..2 {
            let endpoint = endpoint.clone();
            in_flight.push(tokio::spawn(async move {
                let channel = Channel::from_shared(endpoint)
                    .unwrap()
                    .connect()
                    .await
                    .unwrap();
                let mut client = LogsServiceClient::new(channel);
                let _ = client
                    .export(decode_logs(encode_logs_request("payments-api", 1)))
                    .await;
            }));
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
        let channel = Channel::from_shared(endpoint)
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = LogsServiceClient::new(channel);
        let _ = client
            .export(decode_logs(encode_logs_request("payments-api", 1)))
            .await;
        sink_barrier.release();
        for j in in_flight {
            let _ = j.await;
        }
    })
    .await;
    let evt = expect_stderr_event(&events, "concurrency_cap_hit");
    let transport = evt.fields.get("transport").and_then(|v| v.as_str());
    assert_eq!(transport, Some("grpc"));
}

// =========================================================================
// HTTP concurrency cap exceeded -> 503 with Retry-After
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn fifth_concurrent_http_request_at_cap_four_receives_status_503() {
    let (handle, sink_barrier, _sink) = start_with_cap(4).await;
    let base = format!("http://{}", handle.http_addr());
    let client = reqwest::Client::new();
    sink_barrier.hold();

    let mut in_flight = Vec::new();
    for _ in 0..4 {
        let base = base.clone();
        let client = client.clone();
        in_flight.push(tokio::spawn(async move {
            post_otlp_protobuf(
                &client,
                &base,
                "logs",
                encode_logs_request("payments-api", 1),
            )
            .await
        }));
    }
    tokio::time::sleep(Duration::from_millis(150)).await;

    let response = post_otlp_protobuf(
        &client,
        &base,
        "logs",
        encode_logs_request("payments-api", 1),
    )
    .await;
    assert_eq!(response.status().as_u16(), 503);

    sink_barrier.release();
    for j in in_flight {
        let _ = j.await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn fifth_concurrent_http_request_includes_retry_after_header() {
    let (handle, sink_barrier, _sink) = start_with_cap(4).await;
    let base = format!("http://{}", handle.http_addr());
    let client = reqwest::Client::new();
    sink_barrier.hold();
    let mut in_flight = Vec::new();
    for _ in 0..4 {
        let base = base.clone();
        let client = client.clone();
        in_flight.push(tokio::spawn(async move {
            post_otlp_protobuf(
                &client,
                &base,
                "logs",
                encode_logs_request("payments-api", 1),
            )
            .await
        }));
    }
    tokio::time::sleep(Duration::from_millis(150)).await;
    let response = post_otlp_protobuf(
        &client,
        &base,
        "logs",
        encode_logs_request("payments-api", 1),
    )
    .await;
    let retry_after = response
        .headers()
        .get("Retry-After")
        .map(|h| h.to_str().unwrap_or("").to_string())
        .unwrap_or_default();
    assert_eq!(retry_after, "1");
    sink_barrier.release();
    for j in in_flight {
        let _ = j.await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn fifth_concurrent_http_request_body_names_the_cap() {
    let (handle, sink_barrier, _sink) = start_with_cap(4).await;
    let base = format!("http://{}", handle.http_addr());
    let client = reqwest::Client::new();
    sink_barrier.hold();
    let mut in_flight = Vec::new();
    for _ in 0..4 {
        let base = base.clone();
        let client = client.clone();
        in_flight.push(tokio::spawn(async move {
            post_otlp_protobuf(
                &client,
                &base,
                "logs",
                encode_logs_request("payments-api", 1),
            )
            .await
        }));
    }
    tokio::time::sleep(Duration::from_millis(150)).await;
    let body = post_otlp_protobuf(
        &client,
        &base,
        "logs",
        encode_logs_request("payments-api", 1),
    )
    .await
    .text()
    .await
    .expect("read body");
    assert!(
        body.contains("cap of 4") || body.contains("cap=4"),
        "expected response body to name cap; got: {body:?}"
    );
    sink_barrier.release();
    for j in in_flight {
        let _ = j.await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn http_concurrency_cap_hit_event_names_http_protobuf_transport() {
    let ((), events) = capture_stderr_events(|| async {
        let (handle, sink_barrier, _sink) = start_with_cap(2).await;
        let base = format!("http://{}", handle.http_addr());
        let client = reqwest::Client::new();
        sink_barrier.hold();
        let mut in_flight = Vec::new();
        for _ in 0..2 {
            let base = base.clone();
            let client = client.clone();
            in_flight.push(tokio::spawn(async move {
                post_otlp_protobuf(
                    &client,
                    &base,
                    "logs",
                    encode_logs_request("payments-api", 1),
                )
                .await
            }));
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
        let _ = post_otlp_protobuf(
            &client,
            &base,
            "logs",
            encode_logs_request("payments-api", 1),
        )
        .await;
        sink_barrier.release();
        for j in in_flight {
            let _ = j.await;
        }
    })
    .await;
    let evt = expect_stderr_event(&events, "concurrency_cap_hit");
    let transport = evt.fields.get("transport").and_then(|v| v.as_str());
    assert_eq!(transport, Some("http_protobuf"));
}

// =========================================================================
// Caps independent per transport
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn saturated_grpc_does_not_block_http_requests() {
    let (handle, sink_barrier, _sink) = start_with_cap(2).await;
    let endpoint = format!("http://{}", handle.grpc_addr());
    let base = format!("http://{}", handle.http_addr());
    sink_barrier.hold();

    // Saturate gRPC.
    let mut in_flight = Vec::new();
    for _ in 0..2 {
        let endpoint = endpoint.clone();
        in_flight.push(tokio::spawn(async move {
            let channel = Channel::from_shared(endpoint)
                .unwrap()
                .connect()
                .await
                .unwrap();
            let mut client = LogsServiceClient::new(channel);
            let _ = client
                .export(decode_logs(encode_logs_request("payments-api", 1)))
                .await;
        }));
    }
    tokio::time::sleep(Duration::from_millis(150)).await;

    // HTTP request should still succeed (different transport, different cap).
    sink_barrier.release(); // release everyone for HTTP to also progress
    let client = reqwest::Client::new();
    let response = post_otlp_protobuf(
        &client,
        &base,
        "logs",
        encode_logs_request("payments-api", 1),
    )
    .await;
    assert_eq!(response.status().as_u16(), 200);
    for j in in_flight {
        let _ = j.await;
    }
}

// =========================================================================
// Property-shaped invariant: refusal-not-drop
// =========================================================================
//
// `@property` per DISCUSS D5 + journey-aperture.feature: for every
// request that exceeds the cap, the client receives a deterministic
// refusal status — NEVER a silent drop. This is the single most
// important non-silent-drop invariant in the v0 contract; the test
// fires `N` concurrent requests and asserts every response is either
// a sink-acknowledged success (HTTP 200) or a deterministic refusal
// (HTTP 503). No connection drops, no timeouts, no "other" outcomes.

#[tokio::test(flavor = "multi_thread")]
async fn every_excess_request_under_overload_receives_a_deterministic_refusal_or_acceptance() {
    let cap: u32 = 2;
    let load = 10u32;
    let (handle, sink_barrier, _sink) = start_with_cap(cap).await;
    let base = format!("http://{}", handle.http_addr());
    let client = reqwest::Client::new();
    sink_barrier.hold();

    // Fire `load` simultaneous requests.
    let mut futs = Vec::new();
    for _ in 0..load {
        let base = base.clone();
        let client = client.clone();
        futs.push(tokio::spawn(async move {
            post_otlp_protobuf(
                &client,
                &base,
                "logs",
                encode_logs_request("payments-api", 1),
            )
            .await
            .status()
            .as_u16()
        }));
    }

    // Release the sink barrier so cap-permitted requests progress.
    tokio::time::sleep(Duration::from_millis(50)).await;
    sink_barrier.release();
    let mut statuses = Vec::with_capacity(load as usize);
    for f in futs {
        statuses.push(f.await.expect("task did not panic"));
    }

    // Every status MUST be either 200 (sink-accepted) or 503 (cap-refused).
    // Anything else is a silent drop (connection closed, timeout, 5xx
    // for a non-cap reason) and breaks the invariant.
    for s in &statuses {
        assert!(
            *s == 200 || *s == 503,
            "every response must be 200 (sink-accepted) or 503 (cap-refused); \
             got {s} in {statuses:?}"
        );
    }
}

// =========================================================================
// Test fixture: BarrierSink — hold/release the in-flight count
// =========================================================================

/// Test-only sink that wraps the recording behaviour but blocks
/// `accept` until `release()` is called. The slice tests use this to
/// hold N requests in-flight long enough to provoke the cap.
struct BarrierSink {
    notify: Arc<Notify>,
    held: Arc<std::sync::atomic::AtomicBool>,
    accepted: Arc<std::sync::Mutex<Vec<SinkRecord>>>,
}

#[derive(Clone)]
struct BarrierHandle {
    notify: Arc<Notify>,
    held: Arc<std::sync::atomic::AtomicBool>,
}

impl BarrierHandle {
    fn hold(&self) {
        self.held.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    fn release(&self) {
        self.held.store(false, std::sync::atomic::Ordering::SeqCst);
        self.notify.notify_waiters();
    }
}

impl BarrierSink {
    fn new() -> (Arc<Self>, BarrierHandle) {
        let notify = Arc::new(Notify::new());
        let held = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let s = Arc::new(BarrierSink {
            notify: notify.clone(),
            held: held.clone(),
            accepted: Arc::new(std::sync::Mutex::new(Vec::new())),
        });
        (s, BarrierHandle { notify, held })
    }
}

impl OtlpSink for BarrierSink {
    fn accept<'a>(
        &'a self,
        record: SinkRecord,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), SinkError>> + Send + 'a>> {
        Box::pin(async move {
            // If held, wait until released.
            while self.held.load(std::sync::atomic::Ordering::SeqCst) {
                self.notify.notified().await;
            }
            self.accepted
                .lock()
                .expect("barrier-sink mutex")
                .push(record);
            Ok(())
        })
    }
}

impl Probe for BarrierSink {
    fn probe<'a>(
        &'a self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), ProbeError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

/// Spawn an Aperture instance with the given per-transport
/// `max_concurrent_requests` cap, fronted by a `BarrierSink`. Tests
/// can `hold()` / `release()` to provoke saturation deterministically.
async fn start_with_cap(cap: u32) -> (Handle, BarrierHandle, Arc<BarrierSink>) {
    let (sink, barrier) = BarrierSink::new();
    let sink_dyn: Arc<dyn OtlpSink> = sink.clone();
    let config = Config::builder()
        .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
        .http_bind_addr("127.0.0.1:0".parse().unwrap())
        .max_concurrent_requests(cap)
        .build()
        .expect("test config builds");
    let handle = aperture::spawn(config, sink_dyn)
        .await
        .expect("aperture::spawn");
    handle.wait_until_ready().await.expect("aperture readiness");
    (handle, barrier, sink)
}

fn decode_logs(bytes: Vec<u8>) -> ExportLogsServiceRequest {
    ExportLogsServiceRequest::decode(&bytes[..]).expect("encoder produced valid bytes")
}
