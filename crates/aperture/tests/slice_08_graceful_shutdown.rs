//! Slice 08 — Graceful shutdown (drain in-flight, observable verdict).
//!
//! Maps to `docs/feature/aperture/slices/slice-08-graceful-shutdown.md`.
//! Companion story: US-AP-09.
//!
//! The user-centric outcome: when an orchestrator initiates a
//! shutdown (SIGTERM, k8s preStop), `/readyz` flips to 503
//! `"draining"` within 100 ms, listeners stop accepting new
//! connections, in-flight requests drain to a configurable deadline,
//! the verdict is observable on stderr, and the exit code reflects
//! cleanliness.
//!
//! The integration tests trigger shutdown via `Handle::shutdown`
//! rather than an OS signal so they remain portable and parallel-safe.
//! The user contract — "SIGTERM behaves identically" — is named in a
//! single explicit `#[cfg(unix)]` test that DELIVER may pick up.

mod common;

use std::sync::Arc;
use std::time::Duration;

use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use prost::Message;
use tonic::transport::Channel;

use aperture::config::Config;
use aperture::ports::{OtlpSink, Probe, ProbeError, SinkError, SinkRecord};

use crate::common::{capture_stderr_events, encode_logs_request, expect_stderr_event};

// =========================================================================
// readyz flips to draining on shutdown initiation
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_flips_readyz_to_503_draining_within_100ms() {
    let (sink, _release) = SlowSink::new(Duration::from_millis(0));
    let sink_dyn: Arc<dyn OtlpSink> = sink.clone();
    let config = Config::builder()
        .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
        .http_bind_addr("127.0.0.1:0".parse().unwrap())
        .drain_deadline(Duration::from_secs(5))
        .build()
        .unwrap();
    let handle = aperture::spawn(config, sink_dyn).await.expect("spawn");
    handle.wait_until_ready().await.expect("ready");
    let http_addr = handle.http_addr();
    let client = reqwest::Client::new();

    // Initiate shutdown but observe `/readyz` before the drain
    // completes. We background `shutdown()` and poll `/readyz` for up
    // to 100 ms.
    let shutdown_task = tokio::spawn(async move { handle.shutdown().await });

    let started = std::time::Instant::now();
    let mut saw_draining = false;
    while started.elapsed() < Duration::from_millis(100) {
        let response = client
            .get(format!("http://{http_addr}/readyz"))
            .send()
            .await
            .expect("GET /readyz");
        if response.status().as_u16() == 503 {
            let body = response.text().await.unwrap_or_default();
            if body.trim() == "draining" {
                saw_draining = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let _ = shutdown_task.await;
    assert!(
        saw_draining,
        "/readyz should return 503 draining within 100 ms of shutdown"
    );
}

// =========================================================================
// In-flight requests complete on clean drain
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn in_flight_request_completes_when_drain_finishes_within_deadline() {
    let (sink, release) = SlowSink::new(Duration::from_millis(500));
    let sink_dyn: Arc<dyn OtlpSink> = sink.clone();
    let config = Config::builder()
        .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
        .http_bind_addr("127.0.0.1:0".parse().unwrap())
        .drain_deadline(Duration::from_secs(5))
        .build()
        .unwrap();
    let handle = aperture::spawn(config, sink_dyn).await.expect("spawn");
    handle.wait_until_ready().await.expect("ready");
    let endpoint = format!("http://{}", handle.grpc_addr());

    // Fire a request; sink is "slow" but well within the deadline.
    let req_task = tokio::spawn(async move {
        let channel = Channel::from_shared(endpoint)
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = LogsServiceClient::new(channel);
        client
            .export(decode_logs(encode_logs_request("payments-api", 1)))
            .await
    });

    // Briefly let the request enter the in-flight state, then trigger
    // shutdown.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let shutdown_task = tokio::spawn(async move { handle.shutdown().await });

    // Release sink so the in-flight request can complete.
    release.release();

    let response = req_task
        .await
        .expect("task did not panic")
        .expect("in-flight request should complete on clean drain");
    let _ = response;
    let shutdown_result = shutdown_task.await.expect("task did not panic");
    assert!(shutdown_result.is_ok(), "shutdown should complete cleanly");
}

#[tokio::test(flavor = "multi_thread")]
async fn clean_drain_emits_in_flight_drained_stderr_event() {
    let ((), events) = capture_stderr_events(|| async {
        let (sink, release) = SlowSink::new(Duration::from_millis(100));
        let sink_dyn: Arc<dyn OtlpSink> = sink.clone();
        let config = Config::builder()
            .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
            .http_bind_addr("127.0.0.1:0".parse().unwrap())
            .drain_deadline(Duration::from_secs(5))
            .build()
            .unwrap();
        let handle = aperture::spawn(config, sink_dyn).await.expect("spawn");
        handle.wait_until_ready().await.expect("ready");
        let endpoint = format!("http://{}", handle.grpc_addr());
        let req_task = tokio::spawn(async move {
            let channel = Channel::from_shared(endpoint)
                .unwrap()
                .connect()
                .await
                .unwrap();
            let mut client = LogsServiceClient::new(channel);
            let _ = client
                .export(decode_logs(encode_logs_request("payments-api", 1)))
                .await;
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        let shutdown_task = tokio::spawn(async move { handle.shutdown().await });
        release.release();
        let _ = req_task.await;
        let _ = shutdown_task.await;
    })
    .await;
    let evt = expect_stderr_event(&events, "in_flight_drained");
    assert_eq!(evt.level, "info");
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_initiated_event_carries_signal_field() {
    let ((), events) = capture_stderr_events(|| async {
        let (sink, _release) = SlowSink::new(Duration::from_millis(0));
        let sink_dyn: Arc<dyn OtlpSink> = sink.clone();
        let config = Config::builder()
            .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
            .http_bind_addr("127.0.0.1:0".parse().unwrap())
            .drain_deadline(Duration::from_secs(5))
            .build()
            .unwrap();
        let handle = aperture::spawn(config, sink_dyn).await.expect("spawn");
        handle.wait_until_ready().await.expect("ready");
        let _ = handle.shutdown().await;
    })
    .await;
    let evt = expect_stderr_event(&events, "shutdown_initiated");
    assert!(
        evt.fields.get("signal").is_some(),
        "shutdown_initiated event should carry a `signal` field naming the trigger"
    );
}

// =========================================================================
// Drain deadline exceeded — observable, never silent
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn drain_deadline_exceeded_emits_warn_stderr_event_with_dropped_count() {
    let ((), events) = capture_stderr_events(|| async {
        // Sink takes 5 s to acknowledge; deadline is 200 ms.
        let (sink, _release) = SlowSink::new(Duration::from_secs(5));
        let sink_dyn: Arc<dyn OtlpSink> = sink.clone();
        let config = Config::builder()
            .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
            .http_bind_addr("127.0.0.1:0".parse().unwrap())
            .drain_deadline(Duration::from_millis(200))
            .build()
            .unwrap();
        let handle = aperture::spawn(config, sink_dyn).await.expect("spawn");
        handle.wait_until_ready().await.expect("ready");
        let endpoint = format!("http://{}", handle.grpc_addr());

        // Fire one request that will be in-flight when shutdown
        // initiates.
        let req_task = tokio::spawn(async move {
            let channel = Channel::from_shared(endpoint)
                .unwrap()
                .connect()
                .await
                .unwrap();
            let mut client = LogsServiceClient::new(channel);
            let _ = client
                .export(decode_logs(encode_logs_request("payments-api", 1)))
                .await;
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = handle.shutdown().await;
        // Don't bother awaiting the request — by deadline the sink
        // hasn't returned and Aperture has dropped the in-flight.
        req_task.abort();
    })
    .await;
    let evt = expect_stderr_event(&events, "drain_deadline_exceeded");
    assert_eq!(evt.level, "warn");
    assert!(
        evt.fields.get("dropped_count").is_some(),
        "drain_deadline_exceeded event must include dropped_count"
    );
}

// =========================================================================
// SIGTERM behaviour — DELIVER lands the OS-signal path
// =========================================================================
//
// Andrea's locked acceptance criterion: "SIGINT and SIGTERM behave
// identically". The integration-test seam for "send a real SIGTERM
// to the process" is non-trivial (requires forking a separate
// process). We declare the intent here as a `#[cfg(unix)]`
// placeholder; DELIVER is responsible for landing the
// process-spawning fixture if `Handle::shutdown` is not a
// satisfactory proxy.
//
// In the meantime, `Handle::shutdown` is documented (in `lib.rs`) as
// equivalent-to-SIGTERM, and the tests above validate the drain
// shape. The harness's own `tests/common/mod.rs` follows the same
// pattern: the integration test exercises the application-layer
// surface, and a separate mechanism asserts the OS-signal coupling.

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
#[ignore = "DELIVER lands the SIGTERM-vs-Handle::shutdown equivalence with a process-spawning fixture"]
async fn sigterm_and_handle_shutdown_produce_the_same_drain_sequence() {
    // The fixture: spawn `aperture` as a child process; send SIGTERM;
    // observe the same stderr sequence as the in-process
    // `Handle::shutdown` test above. Documented in
    // `docs/feature/aperture/distill/wave-decisions.md > SIGTERM
    // fixture`.
}

// =========================================================================
// Test fixture: SlowSink — sleep for `delay` before acknowledging
// =========================================================================

struct SlowSink {
    delay: Duration,
    released: Arc<std::sync::atomic::AtomicBool>,
    accepted: Arc<std::sync::Mutex<Vec<SinkRecord>>>,
}

#[derive(Clone)]
struct SlowSinkRelease {
    released: Arc<std::sync::atomic::AtomicBool>,
}

impl SlowSinkRelease {
    fn release(&self) {
        self.released
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

impl SlowSink {
    fn new(delay: Duration) -> (Arc<Self>, SlowSinkRelease) {
        let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let s = Arc::new(SlowSink {
            delay,
            released: released.clone(),
            accepted: Arc::new(std::sync::Mutex::new(Vec::new())),
        });
        (s, SlowSinkRelease { released })
    }
}

impl OtlpSink for SlowSink {
    fn accept<'a>(
        &'a self,
        record: SinkRecord,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), SinkError>> + Send + 'a>>
    {
        Box::pin(async move {
            // Sleep up to the configured delay, polling for early release.
            let started = std::time::Instant::now();
            while started.elapsed() < self.delay {
                if self.released.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            self.accepted.lock().expect("slow-sink mutex").push(record);
            Ok(())
        })
    }
}

impl Probe for SlowSink {
    fn probe<'a>(
        &'a self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ProbeError>> + Send + 'a>>
    {
        Box::pin(async { Ok(()) })
    }
}

fn decode_logs(bytes: Vec<u8>) -> ExportLogsServiceRequest {
    ExportLogsServiceRequest::decode(&bytes[..]).expect("encoder produced valid bytes")
}
