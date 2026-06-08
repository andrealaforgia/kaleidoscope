//! Slice 11 — Body-size cap: an oversized OTLP body is rejected before it is
//! buffered/decoded into memory, and named in one structured event.
//!
//! Feature: `aperture-body-size-cap-v0` (ADR-0073, DD1-DD5). This slice wires
//! the disclosed-but-unwired `max_recv_msg_size` knob to a real size guard at
//! the transport boundary. With a cap set, an over-limit body is REJECTED
//! before the harness decodes/validates it AND before the full oversized body
//! is buffered/decoded into memory (HTTP 413 / gRPC RESOURCE_EXHAUSTED), the
//! sink is UNTOUCHED, and aperture emits exactly ONE warn-level
//! `event=body_too_large transport=<http_protobuf|grpc> signal=<signal>
//! limit=<bytes> size=<bytes>` line. Unset = no cap = today's exact behaviour.
//!
//! Companion stories (DISCUSS `user-stories.md`):
//! US-01 (reject-before-decode + named event; the spine);
//! US-02 (the cap is exact at the inclusive boundary: at-limit accepted,
//!        at-limit-plus-one rejected, config-driven not a constant);
//! US-03 (unset = unchanged; the cap covers logs, traces, AND metrics).
//!
//! ## Honest protection strength (ADR-0073 DD1a, design/upstream-changes.md)
//!
//! These ACs are worded to the strength the transport-boundary placement
//! actually delivers, NEVER overstated:
//! - HTTP with `Content-Length` present and over the cap: rejected BEFORE any
//!   body byte is read (the boundary rejects on the declared length). The
//!   in-suite proxy for "before any byte" is: 413 returned, the recording sink
//!   stays EMPTY (the harness never validated, the record never reached the
//!   sink), and the event fires.
//! - HTTP with absent/lying `Content-Length`: rejected before the FULL body is
//!   buffered (a bounded `<= ~one cap` of bytes may be read before the abort,
//!   NOT the full oversized body) — asserted as "rejected, sink empty, event
//!   fires", not "before any byte".
//! - gRPC: the frame is refused IN THE CODEC before decode; the typed request
//!   is never allocated.
//!
//! `limit` is always the exact configured cap. `size` is the value the
//! rejection surface TRUTHFULLY observed at the point of rejection (DD3): the
//! declared `Content-Length` for the HTTP `Content-Length`-present case, so the
//! boundary edges (US-02) are constructed with an exact `Content-Length` and
//! the `limit=N size=N+1` assertion holds.
//!
//! ## Driving ports (black-box, design/wave-decisions.md "For Acceptance Designer")
//!
//! The running aperture instance, observed ONLY through:
//! 1. HTTP `POST /v1/{logs,traces,metrics}` (`application/x-protobuf`) on the
//!    in-process axum listener (ephemeral port).
//! 2. gRPC `LogsService.Export` / `TraceService.Export` / `MetricsService.Export`
//!    on the in-process tonic server (ephemeral port).
//! 3. the recording sink (empty on reject; record present on accept).
//! 4. structured stderr via `testing::stderr_capture` — exactly ONE
//!    `body_too_large` line per rejection.
//!
//! No internal type is reached: the HTTP length-checked read seam, the gRPC
//! codec-error event surface, and the shared event-constructor are all
//! crate-internal; the tests drive the real binary's request path over real
//! TCP and assert observable outcomes only.
//!
//! ## Ephemeral ports (MANDATORY — fixed-port flake guard)
//!
//! Every instance binds `127.0.0.1:0` (OS-assigned) on BOTH transports and
//! reads the actual address back via `handle.http_addr()` / `handle.grpc_addr()`.
//! NEVER the fixed 4317/4318 defaults: a leaked binder on those fixed ports is
//! a known recurring flake on this project (project memory
//! `aperture_fixed_port_4317_flake`). The shared `common` harness already binds
//! ephemeral; the cap fixtures below add only the `max_recv_msg_size` setter.
//!
//! ## The oversized-body seam (Strategy C — real local, real I/O)
//!
//! Bodies are encoded IN-SUITE with the same `prost::Message::encode_to_vec`
//! the real harness validates, then PADDED to an exact target byte length with
//! a tail of throwaway log records, so an over-limit body is a REAL oversized
//! request driven through the REAL allocation/decode path (the Earned-Trust
//! probe for this driven boundary IS the oversized-body acceptance test). The
//! driven side (the sink) is the existing `RecordingSink`. Tagged conceptually
//! `@real-io @driving_adapter @walking_skeleton`.
//!
//! ## RED-not-BROKEN classification (Mandate 7)
//!
//! aperture exists, so the harness, the `stderr_capture` seam, the
//! tonic/reqwest clients, and the new `Config::builder().max_recv_msg_size(n)`
//! setter (the minimal DISTILL scaffold) all resolve and COMPILE today. The
//! ENFORCEMENT does not exist yet, so every reject/boundary scenario is
//! behaviourally RED: against today's parsed-but-ignored knob an over-limit
//! body is ACCEPTED (HTTP 200 / gRPC OK, the sink is NON-empty, no event), so
//! the "reject + sink empty + one body_too_large event" assertions FAIL on an
//! ASSERTION (not a compile/import error). Each reject test is therefore
//! `#[ignore = "RED until DELIVER: aperture-body-size-cap-v0"]` so
//! `cargo test --workspace` stays green at the DISTILL commit; DELIVER removes
//! the ignores one at a time. The under-limit / at-limit / unset NEGATIVE
//! CONTROLS pass TODAY (today's behaviour already accepts them with no event),
//! so they are NOT ignored — they are the guardrail that the cap does not
//! disturb legitimate or unset traffic.

mod common;

use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::metrics_service_client::MetricsServiceClient;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::trace_service_client::TraceServiceClient;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use prost::Message;
use tonic::transport::Channel;
use tonic::Code;

use aperture::config::Config;
use aperture::Handle;

use crate::common::{
    capture_stderr_events, encode_logs_request, encode_metrics_request, encode_traces_request,
    expect_no_stderr_event, expect_stderr_event, post_otlp_protobuf, start_default,
    start_with_recording_sink, StderrEvent, TestInstance,
};

/// Shared ignore reason for the behaviourally-RED reject/boundary scenarios.
const RED: &str = "RED until DELIVER: aperture-body-size-cap-v0";

/// A 4 MiB cap (the canonical operator setting in the DISCUSS examples).
/// Used ONLY for the under-limit accept controls, where the body is well
/// under both this cap and axum's default body limit.
const CAP_4MIB: u32 = 4 * 1024 * 1024;

/// A deliberately tiny test cap (16 bytes). Any real OTLP body exceeds it, so
/// it is the binding constraint for the HTTP/gRPC reject arms. Crucially it is
/// FAR below axum's built-in 2 MB DefaultBodyLimit (axum-core 0.4.5
/// `DEFAULT_LIMIT = 2_097_152`), so an ordinary ~hundreds-of-bytes body is
/// ACCEPTED today (axum's default does not fire) and rejected ONLY once
/// DELIVER consults `max_recv_msg_size`. This is what makes the reject ACs
/// FALSIFIABLE: a test using a multi-megabyte body would trip axum's default
/// 413 today and pass on the unwired knob (the C-DEVOPS-4 trap). See
/// distill/wave-decisions.md > Falsifiability finding.
const CAP_TINY: u32 = 16;

// =========================================================================
// Fixtures
// =========================================================================

/// Start a real aperture instance with the receive-body-size cap set, bound
/// to ephemeral loopback ports on both transports, fronted by a
/// `RecordingSink`. The `max_recv_msg_size` builder seam is the minimal
/// DISTILL scaffold (DD2); at DISTILL it stores the cap but the transport
/// boundary does not yet consult it, which is why every reject/boundary
/// scenario is `#[ignore]`d RED.
async fn start_with_cap(limit: u32) -> TestInstance {
    let config = Config::builder()
        .grpc_bind_addr("127.0.0.1:0".parse().expect("loopback parses"))
        .http_bind_addr("127.0.0.1:0".parse().expect("loopback parses"))
        .max_recv_msg_size(limit)
        .build()
        .expect("cap-configured test config builds");
    start_with_recording_sink(config).await
}

/// Encode a logs body whose encoded length is EXACTLY `target` bytes, kept a
/// DECODABLE `ExportLogsServiceRequest` by appending a length-delimited unknown
/// field (high field number, wire type 2) that prost skips on decode. For the
/// boundary edges (US-02) the rejection surface must observe an exact
/// `Content-Length`, so the body length must be exact; reqwest sets
/// `Content-Length` to the body length, so `size=N` / `size=N+1` is faithfully
/// observable (DD3). `target` must comfortably exceed the minimal body (102 B).
fn logs_body_exactly(target: usize) -> Vec<u8> {
    let mut body = encode_logs_request("payments-api", 1);
    assert!(
        body.len() + 4 <= target,
        "logs_body_exactly: target {target} too small for minimal body ({} B) + framing; \
         choose a larger boundary N",
        body.len()
    );
    // Unknown field key for field 1000, wire type 2 (length-delimited).
    let key: u64 = (1000 << 3) | 2;
    let mut keybuf = Vec::new();
    prost::encoding::encode_varint(key, &mut keybuf);
    let remaining = target - body.len();
    // Solve keybuf.len() + lenvarint(payload) + payload == remaining.
    let mut payload_len = remaining - keybuf.len();
    let mut lenbuf = Vec::new();
    prost::encoding::encode_varint(payload_len as u64, &mut lenbuf);
    payload_len -= lenbuf.len();
    lenbuf.clear();
    prost::encoding::encode_varint(payload_len as u64, &mut lenbuf);
    body.extend_from_slice(&keybuf);
    body.extend_from_slice(&lenbuf);
    body.extend(std::iter::repeat_n(0u8, payload_len));
    debug_assert_eq!(body.len(), target);
    debug_assert!(
        ExportLogsServiceRequest::decode(&body[..]).is_ok(),
        "padded body must stay decodable"
    );
    body
}

/// A logs body that is LARGE (~419 KB) yet still UNDER axum's 2 MB default body
/// limit, so it is accepted today on an unset gateway (the unset control) and
/// would be rejected under a small configured cap.
fn logs_body_large_under_axum_default() -> Vec<u8> {
    let body = encode_logs_request("bulk-importer", 10_000); // ~419 KB
    debug_assert!(
        body.len() < 2_097_152,
        "must stay under axum's 2 MB default"
    );
    body
}

/// A logs body that is OVER axum 0.7's pre-existing 2 MB `DefaultBodyLimit`
/// (`axum-core` 0.4.5 `DEFAULT_LIMIT = 2_097_152`). The `Bytes` extractor the
/// HTTP handlers used BEFORE aperture-body-size-cap-v0 rejected such a body
/// with 413 even on an unset gateway; the unbounded `body.collect()` the cap
/// feature introduced on the unset path would accept it. ~3 MB so it is
/// comfortably over the 2 MB framework default but stays a small, fast
/// allocation.
fn logs_body_over_axum_2mb_default() -> Vec<u8> {
    // ~419 KB at 10_000 records -> ~75_000 records clears 3 MB comfortably.
    let body = encode_logs_request("dos-probe", 75_000);
    debug_assert!(
        body.len() > 2_097_152,
        "must exceed axum's 2 MB default ({}B); got {}B",
        2_097_152,
        body.len()
    );
    body
}

fn decode_logs(bytes: Vec<u8>) -> ExportLogsServiceRequest {
    ExportLogsServiceRequest::decode(&bytes[..]).expect("encoder produced valid logs bytes")
}

fn decode_traces(bytes: Vec<u8>) -> ExportTraceServiceRequest {
    ExportTraceServiceRequest::decode(&bytes[..]).expect("encoder produced valid traces bytes")
}

fn decode_metrics(bytes: Vec<u8>) -> ExportMetricsServiceRequest {
    ExportMetricsServiceRequest::decode(&bytes[..]).expect("encoder produced valid metrics bytes")
}

fn grpc_endpoint(handle: &Handle) -> String {
    format!("http://{}", handle.grpc_addr())
}

/// Read the `signal` / `transport` / `limit` / `size` fields off a captured
/// `body_too_large` event.
fn field_str<'a>(evt: &'a StderrEvent, key: &str) -> Option<&'a str> {
    evt.fields.get(key).and_then(|v| v.as_str())
}
fn field_u64(evt: &StderrEvent, key: &str) -> Option<u64> {
    evt.fields.get(key).and_then(|v| v.as_u64())
}

// =========================================================================
// WALKING SKELETON 1 — HTTP logs spine (US-01 sc.2)
// Scenario: An oversized logs body over HTTP is rejected before decode and
//           named on stderr.
//   Given aperture is configured with a maximum receive body size of 4 MiB
//   And a logs body well over 4 MiB arrives on POST /v1/logs
//   When aperture processes the request
//   Then the body is rejected before it is validated or forwarded to the sink
//   And the client receives a clear too-large rejection (413)
//   And aperture emits exactly one warn body_too_large event naming
//       signal=logs, the configured limit, and the observed size
// @walking_skeleton @driving_port @real-io @driving_adapter @US-01
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn ws_http_oversized_logs_rejected_413_sink_untouched_one_event() {
    let _ = RED;
    let ((status, sink_empty), events) = capture_stderr_events(|| async {
        // Tiny cap, ordinary body: the body is over the cap but UNDER axum's
        // 2 MB default, so ONLY the wired cap can produce the 413 (falsifiable).
        let inst = start_with_cap(CAP_TINY).await;
        let base = inst.http_base_url();
        let client = reqwest::Client::new();
        let body = encode_logs_request("bulk-importer", 1); // ~102 B, over the 16 B cap
        let resp = post_otlp_protobuf(&client, &base, "logs", body).await;
        let status = resp.status().as_u16();
        let sink_empty = inst.sink.is_empty();
        (status, sink_empty)
    })
    .await;

    // Rejected at the boundary with the precise 413 semantic (DD5).
    assert_eq!(
        status, 413,
        "over-limit HTTP logs body must be rejected 413"
    );
    // Sink untouched: the harness never validated, the record never landed.
    assert!(
        sink_empty,
        "the over-limit body must NOT reach the sink (rejected before validate/forward)"
    );
    // Exactly one body_too_large event, naming the signal + limit.
    let cap_events: Vec<&StderrEvent> = events
        .iter()
        .filter(|e| e.event == "body_too_large")
        .collect();
    assert_eq!(
        cap_events.len(),
        1,
        "exactly one body_too_large event per rejection"
    );
    let evt = cap_events[0];
    assert_eq!(field_str(evt, "signal"), Some("logs"));
    assert_eq!(field_str(evt, "transport"), Some("http_protobuf"));
    assert_eq!(field_u64(evt, "limit"), Some(CAP_TINY as u64));
}

// =========================================================================
// WALKING SKELETON 2 — gRPC traces spine (US-01 sc.3)
// Scenario: An oversized traces body over gRPC is refused in the codec and
//           named on stderr.
//   Given aperture is configured with a maximum receive body size of 4 MiB
//   And a traces frame well over 4 MiB is exported via TraceService.Export
//   When aperture processes the request
//   Then the frame is refused before decode and never reaches the sink
//   And the client receives RESOURCE_EXHAUSTED
//   And aperture emits exactly one warn body_too_large event naming
//       signal=traces, transport=grpc, the limit, and the observed size
// @walking_skeleton @driving_port @real-io @driving_adapter @US-01
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn ws_grpc_oversized_traces_refused_resource_exhausted_one_event() {
    let ((code, sink_empty), events) = capture_stderr_events(|| async {
        // Tiny cap, ordinary frame: over the cap, far under tonic's 4 MB
        // default `max_decoding_message_size`, so ONLY the wired cap refuses it.
        let inst = start_with_cap(CAP_TINY).await;
        let endpoint = grpc_endpoint(&inst.handle);
        let channel = Channel::from_shared(endpoint)
            .expect("endpoint parses")
            .connect()
            .await
            .expect("gRPC connect");
        let mut client = TraceServiceClient::new(channel);
        let result = client
            .export(decode_traces(encode_traces_request("checkout-api", 1)))
            .await;
        let code = result.err().map(|e| e.code());
        let sink_empty = inst.sink.is_empty();
        (code, sink_empty)
    })
    .await;

    assert_eq!(
        code,
        Some(Code::ResourceExhausted),
        "over-limit gRPC traces frame must be refused RESOURCE_EXHAUSTED"
    );
    assert!(sink_empty, "the refused frame must NOT reach the sink");
    let cap_events: Vec<&StderrEvent> = events
        .iter()
        .filter(|e| e.event == "body_too_large")
        .collect();
    assert_eq!(
        cap_events.len(),
        1,
        "exactly one body_too_large event per rejection"
    );
    let evt = cap_events[0];
    assert_eq!(field_str(evt, "signal"), Some("traces"));
    assert_eq!(field_str(evt, "transport"), Some("grpc"));
    assert_eq!(field_u64(evt, "limit"), Some(CAP_TINY as u64));
}

// =========================================================================
// NEGATIVE CONTROL — under-limit logs HTTP accepted, no event (US-01 sc.1)
// Passes TODAY (today's accept-and-ignore already accepts an under-limit
// body); it is the guardrail that the cap does not disturb legitimate
// traffic. NOT ignored.
// @driving_port @real-io @US-01
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn under_limit_logs_http_accepted_no_event() {
    let ((status, sink_len), events) = capture_stderr_events(|| async {
        let inst = start_with_cap(CAP_4MIB).await;
        let base = inst.http_base_url();
        let client = reqwest::Client::new();
        // A normal 12 KB-ish export, well under the 4 MiB cap.
        let resp = post_otlp_protobuf(
            &client,
            &base,
            "logs",
            encode_logs_request("payments-api", 1),
        )
        .await;
        let status = resp.status().as_u16();
        let sink_len = inst.sink.len();
        (status, sink_len)
    })
    .await;

    assert_eq!(
        status, 200,
        "an under-limit body is accepted exactly as today"
    );
    assert_eq!(sink_len, 1, "the under-limit record reaches the sink");
    expect_no_stderr_event(&events, "body_too_large");
}

// =========================================================================
// NEGATIVE CONTROL — under-limit traces gRPC accepted, no event
// Passes TODAY. NOT ignored.
// @driving_port @real-io
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn under_limit_traces_grpc_accepted_no_event() {
    let ((ok, sink_len), events) = capture_stderr_events(|| async {
        let inst = start_with_cap(CAP_4MIB).await;
        let endpoint = grpc_endpoint(&inst.handle);
        let channel = Channel::from_shared(endpoint)
            .expect("endpoint parses")
            .connect()
            .await
            .expect("gRPC connect");
        let mut client = TraceServiceClient::new(channel);
        let ok = client
            .export(decode_traces(encode_traces_request("checkout-api", 1)))
            .await
            .is_ok();
        let sink_len = inst.sink.len();
        (ok, sink_len)
    })
    .await;

    assert!(
        ok,
        "an under-limit traces frame is accepted exactly as today"
    );
    assert_eq!(sink_len, 1, "the under-limit record reaches the sink");
    expect_no_stderr_event(&events, "body_too_large");
}

// =========================================================================
// BOUNDARY — a body exactly at the limit is accepted (US-02 sc.1, inclusive)
// The inclusive-limit BEHAVIOUR: size == limit is accepted. Today an
// at-limit body is accepted too (no cap is consulted), so this control
// passes TODAY and stays green. Its TWIN below (at-limit-plus-one) is the
// RED reject edge. Together they kill the `>`/`>=` boundary mutant (KPI-3).
// @driving_port @real-io @US-02
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn at_limit_logs_http_accepted_no_event() {
    // A small exact cap (4 KiB), comfortably above the minimal logs body and
    // far under axum's 2 MB default, so the body can be padded to exactly N
    // bytes with an exact Content-Length and the inclusive-limit BEHAVIOUR is
    // the only thing under test.
    let n: u32 = 4096; // 4 KiB
    let ((status, sink_len), events) = capture_stderr_events(|| async {
        let inst = start_with_cap(n).await;
        let base = inst.http_base_url();
        let client = reqwest::Client::new();
        let body = logs_body_exactly(n as usize); // size == limit
        let resp = post_otlp_protobuf(&client, &base, "logs", body).await;
        let status = resp.status().as_u16();
        let sink_len = inst.sink.len();
        (status, sink_len)
    })
    .await;

    assert_eq!(
        status, 200,
        "a body whose size EQUALS the limit is accepted (inclusive)"
    );
    assert_eq!(sink_len, 1, "the at-limit record reaches the sink");
    expect_no_stderr_event(&events, "body_too_large");
}

// =========================================================================
// BOUNDARY — a body one byte over the limit is rejected (US-02 sc.2)
// Scenario: A body whose encoded size is exactly N+1 over a cap of N is
//           rejected with limit=N size=N+1.
// RED: today the at-limit-plus-one body is accepted (no cap consulted).
// The HTTP Content-Length-present case, so `size` faithfully observes N+1.
// @driving_port @real-io @US-02
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn at_limit_plus_one_logs_http_rejected_limit_n_size_n_plus_one() {
    let n: u32 = 4096; // 4 KiB, far under axum's 2 MB default
    let ((status, sink_empty), events) = capture_stderr_events(|| async {
        let inst = start_with_cap(n).await;
        let base = inst.http_base_url();
        let client = reqwest::Client::new();
        let body = logs_body_exactly((n as usize) + 1); // exactly one byte over
        let resp = post_otlp_protobuf(&client, &base, "logs", body).await;
        let status = resp.status().as_u16();
        let sink_empty = inst.sink.is_empty();
        (status, sink_empty)
    })
    .await;

    assert_eq!(
        status, 413,
        "a body one byte over the inclusive limit is rejected"
    );
    assert!(sink_empty, "the over-limit body must not reach the sink");
    let evt = expect_stderr_event(&events, "body_too_large");
    assert_eq!(
        field_u64(evt, "limit"),
        Some(n as u64),
        "limit names the exact configured cap"
    );
    assert_eq!(
        field_u64(evt, "size"),
        Some((n as u64) + 1),
        "size observes the exact Content-Length (N+1) at the boundary edge"
    );
}

// =========================================================================
// EDGE — a tiny cap rejects an ordinary body, proving the reject is driven
// by the CONFIGURED limit, not a constant (US-02 sc.3).
// The same ~12 KB body accepted under the 4 MiB cap above is rejected under
// a 16-byte cap here, with limit=16.
// RED: today the tiny cap is ignored and the body is accepted.
// @driving_port @real-io @US-02
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn tiny_cap_rejects_ordinary_logs_body_limit_is_config_driven() {
    let ((status, sink_empty), events) = capture_stderr_events(|| async {
        let inst = start_with_cap(16).await; // 16-byte cap, a test configuration
        let base = inst.http_base_url();
        let client = reqwest::Client::new();
        let body = encode_logs_request("payments-api", 1); // ordinary body, ~hundreds of bytes
        let resp = post_otlp_protobuf(&client, &base, "logs", body).await;
        let status = resp.status().as_u16();
        let sink_empty = inst.sink.is_empty();
        (status, sink_empty)
    })
    .await;

    assert_eq!(
        status, 413,
        "an ordinary body vastly exceeds a 16-byte cap and is rejected"
    );
    assert!(sink_empty, "the rejected body must not reach the sink");
    let evt = expect_stderr_event(&events, "body_too_large");
    assert_eq!(
        field_u64(evt, "limit"),
        Some(16),
        "the reject is driven by the configured limit (16), not a hardcoded threshold"
    );
}

// =========================================================================
// COVERAGE — oversized logs over gRPC rejected RESOURCE_EXHAUSTED (US-03 / D4)
// The logs arm refuses identically over gRPC.
// @driving_port @real-io @US-01 @US-03
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn oversized_logs_grpc_refused_resource_exhausted_signal_logs() {
    let ((code, sink_empty), events) = capture_stderr_events(|| async {
        let inst = start_with_cap(CAP_TINY).await;
        let endpoint = grpc_endpoint(&inst.handle);
        let channel = Channel::from_shared(endpoint)
            .expect("endpoint parses")
            .connect()
            .await
            .expect("gRPC connect");
        let mut client = LogsServiceClient::new(channel);
        let code = client
            .export(decode_logs(encode_logs_request("bulk-importer", 1)))
            .await
            .err()
            .map(|e| e.code());
        let sink_empty = inst.sink.is_empty();
        (code, sink_empty)
    })
    .await;

    assert_eq!(code, Some(Code::ResourceExhausted));
    assert!(sink_empty);
    let evt = expect_stderr_event(&events, "body_too_large");
    assert_eq!(field_str(evt, "signal"), Some("logs"));
    assert_eq!(field_str(evt, "transport"), Some("grpc"));
}

// =========================================================================
// COVERAGE — oversized traces over HTTP rejected 413, signal=traces (US-03)
// The traces arm refuses identically over HTTP.
// @driving_port @real-io @US-01 @US-03
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn oversized_traces_http_rejected_413_signal_traces() {
    let ((status, sink_empty), events) = capture_stderr_events(|| async {
        let inst = start_with_cap(CAP_TINY).await;
        let base = inst.http_base_url();
        let client = reqwest::Client::new();
        let body = encode_traces_request("checkout-api", 1); // ~120 B, over the 16 B cap
        let resp = post_otlp_protobuf(&client, &base, "traces", body).await;
        let status = resp.status().as_u16();
        let sink_empty = inst.sink.is_empty();
        (status, sink_empty)
    })
    .await;

    assert_eq!(status, 413);
    assert!(sink_empty);
    let evt = expect_stderr_event(&events, "body_too_large");
    assert_eq!(field_str(evt, "signal"), Some("traces"));
    assert_eq!(field_str(evt, "transport"), Some("http_protobuf"));
}

// =========================================================================
// COVERAGE — oversized metrics over HTTP rejected 413, signal=metrics
// (US-03 / D4: metrics IS in this slice).
// @driving_port @real-io @US-03
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn oversized_metrics_http_rejected_413_signal_metrics() {
    let ((status, sink_empty), events) = capture_stderr_events(|| async {
        // A 16-byte cap so a single conformant metrics body already exceeds
        // it; keeps the body a valid `ExportMetricsServiceRequest` and the
        // reject unambiguously size-driven (config-driven, not a constant).
        let inst = start_with_cap(16).await;
        let base = inst.http_base_url();
        let client = reqwest::Client::new();
        let body = encode_metrics_request("checkout-api");
        let resp = post_otlp_protobuf(&client, &base, "metrics", body).await;
        let status = resp.status().as_u16();
        let sink_empty = inst.sink.is_empty();
        (status, sink_empty)
    })
    .await;

    assert_eq!(status, 413, "metrics is covered by the cap (DD4)");
    assert!(sink_empty);
    let evt = expect_stderr_event(&events, "body_too_large");
    assert_eq!(field_str(evt, "signal"), Some("metrics"));
}

// =========================================================================
// COVERAGE — oversized metrics over gRPC with a tiny cap refused (US-03/D4)
// A 16-byte cap rejects an ordinary metrics frame, proving the gRPC metrics
// arm is covered and config-driven.
// @driving_port @real-io @US-03
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn oversized_metrics_grpc_tiny_cap_refused_signal_metrics() {
    let ((code, sink_empty), events) = capture_stderr_events(|| async {
        let inst = start_with_cap(16).await; // 16-byte cap; any real frame exceeds it
        let endpoint = grpc_endpoint(&inst.handle);
        let channel = Channel::from_shared(endpoint)
            .expect("endpoint parses")
            .connect()
            .await
            .expect("gRPC connect");
        let mut client = MetricsServiceClient::new(channel);
        let code = client
            .export(decode_metrics(encode_metrics_request("checkout-api")))
            .await
            .err()
            .map(|e| e.code());
        let sink_empty = inst.sink.is_empty();
        (code, sink_empty)
    })
    .await;

    assert_eq!(
        code,
        Some(Code::ResourceExhausted),
        "metrics gRPC arm is covered (DD4)"
    );
    assert!(sink_empty);
    let evt = expect_stderr_event(&events, "body_too_large");
    assert_eq!(field_str(evt, "signal"), Some("metrics"));
    assert_eq!(field_str(evt, "transport"), Some("grpc"));
}

// =========================================================================
// EVENT SHAPE — the rejection event is at warn level (US-01, KPI-2)
// @driving_port @real-io @US-01
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn body_too_large_event_is_warn_level() {
    let (_, events) = capture_stderr_events(|| async {
        let inst = start_with_cap(16).await;
        let base = inst.http_base_url();
        let client = reqwest::Client::new();
        let _ = post_otlp_protobuf(
            &client,
            &base,
            "logs",
            encode_logs_request("payments-api", 1),
        )
        .await;
    })
    .await;

    let evt = expect_stderr_event(&events, "body_too_large");
    assert_eq!(
        evt.level, "warn",
        "body_too_large fires at warn, mirroring concurrency_cap_hit"
    );
}

// =========================================================================
// UNSET — no cap configured leaves the ingest path unchanged (US-03 sc.1)
// A large logs body (one that WOULD be rejected under a set cap) is accepted
// with NO body_too_large event. Passes TODAY (the unset path is today's
// behaviour); it is the hard backward-compat guardrail (KPI-4). NOT ignored.
// @driving_port @real-io @US-03
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn unset_cap_large_logs_http_accepted_no_event() {
    let ((status, sink_len), events) = capture_stderr_events(|| async {
        // `start_default` builds a config with NO max_recv_msg_size (unset).
        let inst: TestInstance = start_default().await;
        let base = inst.http_base_url();
        let client = reqwest::Client::new();
        // A LARGE body (~419 KB) that WOULD be rejected under a small cap, yet
        // stays UNDER axum's 2 MB default so the unset path accepts it today.
        let body = logs_body_large_under_axum_default();
        let resp = post_otlp_protobuf(&client, &base, "logs", body).await;
        let status = resp.status().as_u16();
        let sink_len = inst.sink.len();
        (status, sink_len)
    })
    .await;

    assert_eq!(
        status, 200,
        "with no cap set, a large body is accepted exactly as today"
    );
    assert!(
        sink_len >= 1,
        "the body reaches the sink unchanged (no size check)"
    );
    expect_no_stderr_event(&events, "body_too_large");
}

// =========================================================================
// UNSET — absent cap never becomes a reject-everything configuration
// (US-03 sc.3). An ordinary small body is accepted; the absence of a cap is
// NOT treated as a zero-byte limit. Passes TODAY. NOT ignored.
// @driving_port @real-io @US-03
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn unset_cap_small_body_accepted_not_zero_byte_limit() {
    let ((status, sink_len), events) = capture_stderr_events(|| async {
        let inst = start_default().await;
        let base = inst.http_base_url();
        let client = reqwest::Client::new();
        let resp = post_otlp_protobuf(
            &client,
            &base,
            "logs",
            encode_logs_request("payments-api", 1),
        )
        .await;
        let status = resp.status().as_u16();
        let sink_len = inst.sink.len();
        (status, sink_len)
    })
    .await;

    assert_eq!(
        status, 200,
        "an absent cap accepts an ordinary body (not a zero-byte limit)"
    );
    assert_eq!(sink_len, 1);
    expect_no_stderr_event(&events, "body_too_large");
}

// =========================================================================
// UNSET DEFAULT POSTURE — an unset cap still rejects a body over axum 0.7's
// pre-existing 2 MB DefaultBodyLimit (backward-compatibility guard).
//
// BEFORE aperture-body-size-cap-v0 the HTTP handlers took `body: Bytes`, whose
// axum 0.7 extractor enforces a 2 MB `DefaultBodyLimit` (axum-core 0.4.5
// `DEFAULT_LIMIT = 2_097_152`) — so a >2 MB body on an UNSET gateway was
// rejected with 413 even with no `max_recv_msg_size` configured. The feature
// switched the handlers to a raw `body: Body` collected unbounded on the unset
// path, silently DROPPING that framework default and leaving the default
// posture unbounded — a DoS regression in the very feature whose purpose is a
// DoS guard. This test pins the prior bounded default: with NO cap set, a ~3 MB
// body MUST still be rejected (413) and MUST NOT reach the sink. It does not
// assert a `body_too_large` event — the OLD behaviour emitted no such event,
// and the unset default is the framework-equivalent safety net, not the
// configured cap.
// @driving_port @real-io @US-03 @backward-compat
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn unset_cap_body_over_axum_2mb_default_still_rejected_413_sink_untouched() {
    let (status, sink_len) = {
        // `start_default` builds a config with NO max_recv_msg_size (unset).
        let inst: TestInstance = start_default().await;
        let base = inst.http_base_url();
        let client = reqwest::Client::new();
        // A ~3 MB body, over axum's pre-existing 2 MB framework default.
        let body = logs_body_over_axum_2mb_default();
        let resp = post_otlp_protobuf(&client, &base, "logs", body).await;
        let status = resp.status().as_u16();
        let sink_len = inst.sink.len();
        (status, sink_len)
    };

    assert_eq!(
        status, 413,
        "with no cap set, a body over axum's pre-existing 2 MB DefaultBodyLimit must \
         STILL be rejected 413 (the unset default must be no weaker than before the feature)"
    );
    assert_eq!(
        sink_len, 0,
        "the over-default body must not reach the sink on the unset path"
    );
}

// =========================================================================
// COVERAGE — a single configured cap guards BOTH logs AND traces (US-03 sc.2)
// One cap; an oversized logs body over HTTP and an oversized traces frame
// over gRPC are each rejected with the correct per-signal event. Proves the
// cap is not a half-guard.
// @driving_port @real-io @US-03
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn single_cap_guards_both_logs_and_traces() {
    let ((logs_status, traces_code, sink_empty), events) = capture_stderr_events(|| async {
        let inst = start_with_cap(CAP_TINY).await;
        let base = inst.http_base_url();
        let endpoint = grpc_endpoint(&inst.handle);
        let client = reqwest::Client::new();

        // Oversized logs over HTTP (ordinary body over the tiny cap).
        let logs_status = post_otlp_protobuf(
            &client,
            &base,
            "logs",
            encode_logs_request("bulk-importer", 1),
        )
        .await
        .status()
        .as_u16();

        // Oversized traces over gRPC (ordinary frame over the tiny cap).
        let channel = Channel::from_shared(endpoint)
            .expect("endpoint parses")
            .connect()
            .await
            .expect("gRPC connect");
        let mut tclient = TraceServiceClient::new(channel);
        let traces_code = tclient
            .export(decode_traces(encode_traces_request("checkout-api", 1)))
            .await
            .err()
            .map(|e| e.code());

        let sink_empty = inst.sink.is_empty();
        (logs_status, traces_code, sink_empty)
    })
    .await;

    assert_eq!(logs_status, 413, "the single cap guards the logs arm");
    assert_eq!(
        traces_code,
        Some(Code::ResourceExhausted),
        "the same single cap guards the traces arm"
    );
    assert!(sink_empty, "neither oversized body reached the sink");

    let signals: Vec<&str> = events
        .iter()
        .filter(|e| e.event == "body_too_large")
        .filter_map(|e| field_str(e, "signal"))
        .collect();
    assert!(signals.contains(&"logs"), "a logs-signal event fired");
    assert!(signals.contains(&"traces"), "a traces-signal event fired");
}

// =========================================================================
// RED-reason documentation pin (mirrors the slice_10 precedent)
// =========================================================================

#[test]
fn red_reason_is_documented() {
    assert_eq!(RED, "RED until DELIVER: aperture-body-size-cap-v0");
}
