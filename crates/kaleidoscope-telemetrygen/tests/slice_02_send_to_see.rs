// Kaleidoscope telemetry generator — Slice 2: the send-to-see loop (C3)
// Copyright (C) 2026 The Kaleidoscope authors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with this program. If not, see <https://www.gnu.org/licenses/>.

//! # Slice 2 — "I can send sample telemetry and see it" (US-04).
//!
//! The C3 generator's acceptance suite. The NORTH STAR is the send-to-see
//! loop: a `kaleidoscope-telemetrygen` run pushes sample OTLP across all three
//! signals to a RUNNING consolidated runtime, and that telemetry becomes
//! QUERYABLE — end to end through the real OTLP/gRPC wire and the live shared
//! store.
//!
//! ## Test architecture (DISTILL decision)
//!
//! - The suite REUSES the C1 composition root (`kaleidoscope_runtime::
//!   spawn_consolidated`) to stand up a live consolidated runtime IN THE TEST
//!   PROCESS on EPHEMERAL `127.0.0.1:0` ports — NEVER the fixed
//!   4317/4318/9090/9091/9092 defaults (the fixed-port flake, project memory
//!   `aperture_fixed_port_4317_flake`). The actual bound ports are read back
//!   from the `RunningRuntime`.
//! - The generator is driven through its TRUE driving port: the COMPILED BIN
//!   run as a SUBPROCESS (`CARGO_BIN_EXE_kaleidoscope-telemetrygen`), pointed
//!   at the runtime's bound ingest gRPC port via `OTEL_EXPORTER_OTLP_ENDPOINT`
//!   and `KALEIDOSCOPE_TENANT`. Process isolation gives every run its own
//!   pristine spark/OTel global state, so "re-run is safe" is two REAL
//!   processes — exactly as a user re-runs the command — and `spark`'s
//!   single-init-per-process invariant is never violated. This is real I/O:
//!   real subprocess, real OTLP wire, real live store (`@real-io`).
//! - The pre-flight reachability probe contract is ALSO driven directly as a
//!   library seam ([`kaleidoscope_telemetrygen::probe_reachable`]) so the
//!   down-stack failure mode is locked both at the contract and through the
//!   real bin.
//! - The "see" half GETs the runtime's three query routers over loopback and
//!   asserts the BUSINESS OUTCOME (the sample metric / log / span returns),
//!   never transport details.
//!
//! ## Status (DELIVER, Slice 2 / C3)
//!
//! GREEN. `generate` probes the ingest endpoint (fail-closed on a down stack),
//! then `spark::init`s at the resolved endpoint for the tenant and pushes the
//! demo dataset (one `request_count` metric point, one
//! `checkout failed: card declined` log, one checkout-shaped
//! `POST /api/v1/checkout` span pinned to trace id
//! `4bf92f3577b34da6a3ce929d0e0e4736`), force-flushing on
//! guard drop. Every scenario below drives the real compiled bin over the real
//! OTLP/gRPC wire against a live consolidated runtime and asserts the telemetry
//! returns from the query routers.
//!
//! ## Scope (US-04, not US-05)
//!
//! This in-process suite covers US-04: send all three signals, the down-stack
//! clear failure, tenant scoping, and safe re-run. The once-only SEED (US-05)
//! is a COMPOSE concern (a marker-gated one-shot service on the shared volume)
//! verified by the CI HTTP smoke, NOT by this in-process suite (ADR-0077 F3).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Output;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use kaleidoscope_runtime::{spawn_consolidated, ConsolidatedConfig, RunningRuntime};
use kaleidoscope_telemetrygen::probe_reachable;

// =========================================================================
// The C1 sample vocabulary, reused verbatim (shared-artifacts-registry).
// =========================================================================

/// The single local-experiment tenant.
const TENANT_ACME: &str = "acme";
/// A second tenant — the cross-tenant negative control.
const TENANT_GLOBEX: &str = "globex";
/// The metric the generator pushes and the suite queries back.
const METRIC_NAME: &str = "request_count";
/// The log body the generator pushes and the suite queries back.
const LOG_BODY: &str = "checkout failed: card declined";
/// The `service.name` the sample telemetry is filed under (the traces window
/// query keys on this).
const DEMO_SERVICE: &str = "kaleidoscope-demo";
/// The trace id the generator pushes and the by-id query looks up.
const TRACE_ID_HEX: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
/// The demo span's operation name — a CHECKOUT-shaped name, coherent with the
/// `checkout failed: card declined` error message and cause log. NOT a generic
/// `query_range` read: a newcomer opening the trace sees a checkout span fail
/// with a checkout error, one coherent story.
const SPAN_NAME: &str = "POST /api/v1/checkout";

/// The query window (epoch seconds) brackets whatever "now" the generator
/// stamps. The query routers parse start/end as epoch seconds AND enforce a
/// maximum span of `query_http_common::MAX_WINDOW_SECONDS` (86 400 s = 1 day),
/// rejecting a wider window with HTTP 400 before any data is read. So the
/// window is a +/- 1 h bracket around the test's own "now" (a 2 h span, well
/// within the cap) rather than an unbounded 0..u64 range.
const WINDOW_HALF_SPAN_SECS: u64 = 3_600;

/// `(start, end)` epoch-second bounds bracketing the current wall clock by
/// [`WINDOW_HALF_SPAN_SECS`] either side. The generator stamps its telemetry
/// with the real wall clock, so this window contains it while staying within
/// the query routers' maximum-window cap.
fn query_window() -> (u64, u64) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    (
        now.saturating_sub(WINDOW_HALF_SPAN_SECS),
        now + WINDOW_HALF_SPAN_SECS,
    )
}

/// How long the "see" half polls for telemetry to appear (the live loop
/// tolerates async accept + batch flush).
const SEE_TIMEOUT: Duration = Duration::from_secs(10);
/// How long the negative (absence) checks wait before concluding the data
/// never crosses the tenant boundary.
const ABSENCE_SETTLE: Duration = Duration::from_secs(2);

// =========================================================================
// Runtime lifecycle (REUSE the C1 composition root on EPHEMERAL ports)
// =========================================================================

/// A live consolidated runtime plus the pillar root it owns (kept alive so the
/// temp dir is not reclaimed mid-test).
struct TestRuntime {
    runtime: RunningRuntime,
    _pillar_root: PathBuf,
}

fn fresh_pillar_root(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let pid = std::process::id();
    let mut path = std::env::temp_dir();
    path.push(format!("kal-telemetrygen-{label}-{pid}-{nanos}"));
    std::fs::create_dir_all(&path).expect("mkdir pillar root");
    path
}

/// Spawn a consolidated runtime on EPHEMERAL `127.0.0.1:0` ports whose ingest
/// default and all three query roles are `tenant`, with a fresh empty pillar
/// root.
async fn spawn_runtime(label: &str, tenant: &str) -> TestRuntime {
    let pillar_root = fresh_pillar_root(label);
    let config = ConsolidatedConfig::for_ephemeral_test(pillar_root.clone(), tenant);
    let runtime = spawn_consolidated(config)
        .await
        .expect("consolidated runtime spawns on ephemeral ports");
    TestRuntime {
        runtime,
        _pillar_root: pillar_root,
    }
}

// =========================================================================
// Driving the generator through its true driving port (the compiled BIN)
// =========================================================================

/// Run the compiled `kaleidoscope-telemetrygen` BIN once as a subprocess,
/// pointed at `grpc_addr` for `tenant`. Returns the process `Output` (status +
/// captured stderr/stdout). Real subprocess, real OTLP/gRPC wire.
async fn run_generator(grpc_addr: SocketAddr, tenant: &str) -> Output {
    tokio::process::Command::new(env!("CARGO_BIN_EXE_kaleidoscope-telemetrygen"))
        .env("OTEL_EXPORTER_OTLP_ENDPOINT", format!("http://{grpc_addr}"))
        .env("KALEIDOSCOPE_TENANT", tenant)
        .env("OTEL_SERVICE_NAME", DEMO_SERVICE)
        .output()
        .await
        .expect("run the kaleidoscope-telemetrygen binary")
}

/// Bind then immediately drop an ephemeral loopback listener, yielding an
/// address that (almost certainly) nothing is listening on — a "down stack"
/// ingest endpoint for the unreachability scenarios.
async fn closed_loopback_addr() -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral loopback port");
    let addr = listener.local_addr().expect("read back addr");
    drop(listener);
    addr
}

// =========================================================================
// The "see" half — GET the query routers, assert business outcomes
// =========================================================================

async fn get(url: &str) -> (u16, String) {
    let resp = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .expect("GET query endpoint over loopback");
    let status = resp.status().as_u16();
    let body = resp.text().await.expect("read query response body");
    (status, body)
}

async fn metrics_query(addr: SocketAddr) -> (u16, String) {
    let (start, end) = query_window();
    get(&format!(
        "http://{addr}/api/v1/query_range?query={METRIC_NAME}&start={start}&end={end}"
    ))
    .await
}

async fn logs_query(addr: SocketAddr) -> (u16, String) {
    let (start, end) = query_window();
    get(&format!(
        "http://{addr}/api/v1/logs?start={start}&end={end}"
    ))
    .await
}

async fn traces_window_query(addr: SocketAddr) -> (u16, String) {
    let (start, end) = query_window();
    get(&format!(
        "http://{addr}/api/v1/traces?service={DEMO_SERVICE}&start={start}&end={end}"
    ))
    .await
}

async fn trace_by_id_query(addr: SocketAddr) -> (u16, String) {
    get(&format!(
        "http://{addr}/api/v1/traces/by_id?trace_id={TRACE_ID_HEX}"
    ))
    .await
}

/// GET the logs router filtered BY TRACE ID WITH NO WINDOW. The logs query
/// accepts a `trace_id` alone (no start/end) and post-filters to records whose
/// `trace_id` equals the requested id, rendering the id as lowercase hex. This
/// is the correlation probe: it only returns the demo failure log if that log
/// was emitted INSIDE the demo span and therefore carries the pinned trace id.
async fn logs_by_trace_id_query(addr: SocketAddr) -> (u16, String) {
    get(&format!(
        "http://{addr}/api/v1/logs?trace_id={TRACE_ID_HEX}"
    ))
    .await
}

/// Number of result series in a `query_range` success body
/// (`data.result.length`).
fn metrics_result_len(body: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v["data"]["result"].as_array().map(|a| a.len()))
        .unwrap_or(0)
}

/// Whether a `query_range` body reports `status: success`.
fn metrics_status_success(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .map(|v| v["status"] == "success")
        .unwrap_or(false)
}

/// Number of records/spans in a logs or traces success body (a bare JSON
/// array).
fn array_len(body: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.as_array().map(|a| a.len()))
        .unwrap_or(0)
}

/// The status `(code, message)` of the demo span in a by-id traces body, if a
/// span carrying the demo trace id is present. The by-id response is a bare
/// JSON array of `ray::Span` objects; each span's `status` serialises as
/// `{"code": "Unset|Ok|Error", "message": "<text>"}`. Returns `None` until the
/// demo span is queryable, so the poll can wait for it to land.
fn demo_span_status(body: &str) -> Option<(String, String)> {
    let spans: serde_json::Value = serde_json::from_str(body).ok()?;
    let span = spans
        .as_array()?
        .iter()
        .find(|s| s["trace_id"] == TRACE_ID_HEX)?;
    let code = span["status"]["code"].as_str()?.to_string();
    let message = span["status"]["message"].as_str()?.to_string();
    Some((code, message))
}

/// The operation name of the demo span in a by-id traces body, if a span
/// carrying the demo trace id is present. The by-id response is a bare JSON
/// array of `ray::Span` objects; each span serialises its operation as `name`.
fn demo_span_name(body: &str) -> Option<String> {
    let spans: serde_json::Value = serde_json::from_str(body).ok()?;
    let span = spans
        .as_array()?
        .iter()
        .find(|s| s["trace_id"] == TRACE_ID_HEX)?;
    Some(span["name"].as_str()?.to_string())
}

/// How many spans in a by-id traces body carry the demo trace id. Pins the
/// single-copy contract: a single seed must yield exactly one demo span, never
/// a duplicate emission.
fn demo_span_count(body: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.as_array().map(|spans| {
                spans
                    .iter()
                    .filter(|s| s["trace_id"] == TRACE_ID_HEX)
                    .count()
            })
        })
        .unwrap_or(0)
}

/// How many log records in a by-trace_id logs body are the correlated demo
/// cause log: they carry the demo trace id (NOT orphaned with a null/absent
/// trace_id) AND their body is the cause message. Pins the single-copy +
/// correlation contract for the cause log.
fn demo_cause_log_count(body: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.as_array().map(|records| {
                records
                    .iter()
                    .filter(|r| {
                        r["trace_id"] == TRACE_ID_HEX
                            && r["body"].as_str().is_some_and(|b| b.contains(LOG_BODY))
                    })
                    .count()
            })
        })
        .unwrap_or(0)
}

/// Poll `f` until `done` holds or `timeout` elapses. Returns the final
/// `(status, body)`.
async fn poll_until<F, Fut>(
    timeout: Duration,
    mut f: F,
    done: impl Fn(u16, &str) -> bool,
) -> (u16, String)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = (u16, String)>,
{
    let start = Instant::now();
    loop {
        let (status, body) = f().await;
        if done(status, &body) || start.elapsed() >= timeout {
            return (status, body);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

// =========================================================================
// US-04 — SEND TO SEE (the north star; the walking skeleton)
// @walking_skeleton @driving_port @real-io @adapter-integration @US-04
// =========================================================================

/// US-04 / AC one-command-pushes-all-three-signals.
///
/// ```gherkin
/// @walking_skeleton @driving_port @real-io @US-04
/// Scenario: Generated telemetry becomes queryable across all three signals
///   Given a consolidated runtime is running for tenant "acme"
///   When the telemetry generator runs once against the ingest endpoint
///   Then a metrics query returns "request_count"
///   And a logs query returns "checkout failed: card declined"
///   And a traces query returns the sample span by service and by id
/// ```
///
/// FALSIFIABILITY: the scaffold bin exits non-zero without pushing, so every
/// query stays empty and these assertions FAIL. GREEN only when the generator
/// pushes real OTLP that the live shared store returns — the send-to-see loop.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn generated_telemetry_is_queryable_across_all_three_signals() {
    let rt = spawn_runtime("send-to-see", TENANT_ACME).await;

    let out = run_generator(rt.runtime.ingest_grpc_addr, TENANT_ACME).await;
    assert!(
        out.status.success(),
        "the generator exits cleanly against a running stack; stderr: {}",
        stderr_of(&out)
    );

    let (status, body) = poll_until(
        SEE_TIMEOUT,
        || metrics_query(rt.runtime.metrics_query_addr),
        |s, b| s == 200 && metrics_result_len(b) > 0,
    )
    .await;
    assert_eq!(status, 200, "metrics query answers 200; body: {body}");
    assert!(
        metrics_status_success(&body) && metrics_result_len(&body) > 0,
        "the pushed \"{METRIC_NAME}\" comes back live; body: {body}"
    );

    let (_s, body) = poll_until(
        SEE_TIMEOUT,
        || logs_query(rt.runtime.logs_query_addr),
        |s, b| s == 200 && b.contains(LOG_BODY),
    )
    .await;
    assert!(
        body.contains(LOG_BODY),
        "the pushed log \"{LOG_BODY}\" comes back; body: {body}"
    );

    let (_s, body) = poll_until(
        SEE_TIMEOUT,
        || traces_window_query(rt.runtime.traces_query_addr),
        |s, b| s == 200 && array_len(b) > 0,
    )
    .await;
    assert!(
        array_len(&body) > 0,
        "the pushed span comes back from the traces window query; body: {body}"
    );

    let (_s, body) = poll_until(
        SEE_TIMEOUT,
        || trace_by_id_query(rt.runtime.traces_query_addr),
        |s, b| s == 200 && b.contains(TRACE_ID_HEX),
    )
    .await;
    assert!(
        body.contains(TRACE_ID_HEX),
        "the pushed span is found by trace id \"{TRACE_ID_HEX}\"; body: {body}"
    );
}

// =========================================================================
// US-04 — LOG/TRACE CORRELATION (the demo failure log lives in the demo span)
// @driving_port @real-io @adapter-integration @US-04
// =========================================================================

/// US-04 — the demo failure log is correlated to the demo trace.
///
/// ```gherkin
/// @driving_port @real-io @US-04
/// Scenario: The demo failure log carries the demo trace id
///   Given a consolidated runtime is running for tenant "acme"
///   When the telemetry generator runs once against the ingest endpoint
///   Then a logs query by trace id (no window) returns the demo failure log
///   And that log carries the demo trace id "4bf92f3577b34da6a3ce929d0e0e4736"
/// ```
///
/// FALSIFIABILITY: when the failure log is emitted OUTSIDE the demo span it
/// lands with `trace_id` null, so the by-trace_id logs query (which post-filters
/// on `trace_id == Some(id)`) returns `[]` and the body contains neither the log
/// body nor the trace id — these assertions FAIL. GREEN only when the generator
/// emits the failure log INSIDE the active demo span, so the appender bridge
/// stamps the pinned trace id and the correlation query finds it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_demo_failure_log_is_correlated_to_the_demo_trace() {
    let rt = spawn_runtime("log-trace-correlation", TENANT_ACME).await;

    let out = run_generator(rt.runtime.ingest_grpc_addr, TENANT_ACME).await;
    assert!(
        out.status.success(),
        "the generator exits cleanly against a running stack; stderr: {}",
        stderr_of(&out)
    );

    let (status, body) = poll_until(
        SEE_TIMEOUT,
        || logs_by_trace_id_query(rt.runtime.logs_query_addr),
        |s, b| s == 200 && b.contains(LOG_BODY) && b.contains(TRACE_ID_HEX),
    )
    .await;
    assert_eq!(
        status, 200,
        "the by-trace_id logs query answers 200; body: {body}"
    );
    assert!(
        body.contains(LOG_BODY),
        "the by-trace_id logs query returns the demo failure log \"{LOG_BODY}\" \
         (it was emitted inside the demo span); body: {body}"
    );
    assert!(
        body.contains(TRACE_ID_HEX),
        "the returned demo failure log carries the demo trace id \"{TRACE_ID_HEX}\"; body: {body}"
    );
}

// =========================================================================
// US-04 — THE FAILED-CHECKOUT SPAN SHOWS *WHERE* IT FAILED (Error status)
// @driving_port @real-io @adapter-integration @US-04
// =========================================================================

/// US-04 — the demo failed-checkout span carries an Error status (the WHERE).
///
/// ```gherkin
/// @driving_port @real-io @US-04
/// Scenario: The demo failed-checkout span shows where it failed
///   Given a consolidated runtime is running for tenant "acme"
///   When the telemetry generator runs once against the ingest endpoint
///   Then a by-id traces query returns the demo span
///   And that span's status code is Error
///   And that span carries a readable status message
/// ```
///
/// FALSIFIABILITY: a span emitted WITHOUT an explicit status exports with the
/// default `Unset` code and an empty message, so the status assertions FAIL
/// (the in-span cause log alone is the WHY, never the WHERE). GREEN only when
/// the generator sets the span status to Error with a readable message before
/// the span ends, and that Error status survives OTLP export into the live ray
/// store so the by-id query returns it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_failed_checkout_span_carries_an_error_status() {
    let rt = spawn_runtime("error-status", TENANT_ACME).await;

    let out = run_generator(rt.runtime.ingest_grpc_addr, TENANT_ACME).await;
    assert!(
        out.status.success(),
        "the generator exits cleanly against a running stack; stderr: {}",
        stderr_of(&out)
    );

    let (status, body) = poll_until(
        SEE_TIMEOUT,
        || trace_by_id_query(rt.runtime.traces_query_addr),
        |s, b| s == 200 && demo_span_status(b).is_some_and(|(code, _)| code == "Error"),
    )
    .await;
    assert_eq!(
        status, 200,
        "the by-id traces query answers 200; body: {body}"
    );

    let (code, message) = demo_span_status(&body)
        .unwrap_or_else(|| panic!("the demo span is present in the by-id query; body: {body}"));
    assert_eq!(
        code, "Error",
        "the failed-checkout span carries an Error status code (the WHERE); body: {body}"
    );
    assert!(
        !message.is_empty(),
        "the failed-checkout span carries a readable, non-empty status message; body: {body}"
    );
    assert!(
        message.contains(LOG_BODY),
        "the status message is readable and names the cause; got: {message:?}"
    );
}

// =========================================================================
// US-04 — THE DEMO TELLS ONE COHERENT FAILED-CHECKOUT STORY (single copy)
// @driving_port @real-io @adapter-integration @US-04
// =========================================================================

/// US-04 — a single seed emits ONE coherent failed-checkout story: a single
/// checkout-shaped span marked Error, and a single trace-correlated cause log.
///
/// ```gherkin
/// @driving_port @real-io @US-04
/// Scenario: The demo emits a coherent single-copy failed-checkout trace
///   Given a consolidated runtime is running for tenant "acme"
///   When the telemetry generator runs once against the ingest endpoint
///   Then a by-id traces query returns exactly one span for the demo trace id
///   And that span's operation name is checkout-shaped ("POST /api/v1/checkout"),
///       not a generic "GET /api/v1/query_range" read
///   And that span carries an Error status whose message names the cause
///   And a by-trace_id logs query returns exactly one cause log
///   And that cause log carries the demo trace id (it is not orphaned)
/// ```
///
/// FALSIFIABILITY: while the demo span is named `GET /api/v1/query_range` the
/// checkout-shaped name assertion FAILS — a generic read span cannot carry a
/// checkout failure coherently. If the cause log were emitted OUTSIDE the demo
/// span it would land orphaned (null trace_id) and the by-trace_id count would
/// be 0. A duplicate emission would make either count exceed 1. GREEN only when
/// one checkout-shaped Error span and one trace-correlated cause log are the
/// single clean emission per seed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_demo_emits_a_coherent_single_copy_failed_checkout() {
    let rt = spawn_runtime("coherent-checkout", TENANT_ACME).await;

    let out = run_generator(rt.runtime.ingest_grpc_addr, TENANT_ACME).await;
    assert!(
        out.status.success(),
        "the generator exits cleanly against a running stack; stderr: {}",
        stderr_of(&out)
    );

    // The span: poll until the demo span lands marked Error.
    let (status, body) = poll_until(
        SEE_TIMEOUT,
        || trace_by_id_query(rt.runtime.traces_query_addr),
        |s, b| s == 200 && demo_span_status(b).is_some_and(|(code, _)| code == "Error"),
    )
    .await;
    assert_eq!(
        status, 200,
        "the by-id traces query answers 200; body: {body}"
    );

    // Single copy: exactly one span carries the demo trace id.
    assert_eq!(
        demo_span_count(&body),
        1,
        "a single seed yields exactly one demo span (single copy); body: {body}"
    );

    // Checkout-shaped name — coherent with the checkout-failure message.
    let name = demo_span_name(&body)
        .unwrap_or_else(|| panic!("the demo span is present in the by-id query; body: {body}"));
    assert_eq!(
        name, SPAN_NAME,
        "the demo span is checkout-shaped, coherent with the checkout-failure message; body: {body}"
    );
    assert!(
        name.to_lowercase().contains("checkout"),
        "the demo span name names a checkout; got: {name:?}"
    );
    assert!(
        !name.contains("query_range"),
        "the demo span is no longer an incoherent generic query_range read; got: {name:?}"
    );

    // Error status whose readable message names the cause (the WHERE).
    let (code, message) = demo_span_status(&body)
        .unwrap_or_else(|| panic!("the demo span carries a status; body: {body}"));
    assert_eq!(
        code, "Error",
        "the checkout-shaped span is marked Error (the WHERE); body: {body}"
    );
    assert!(
        message.contains(LOG_BODY),
        "the span's status message is readable and names the cause; got: {message:?}"
    );

    // The cause log: exactly one, correlated to the demo trace (not orphaned).
    let (_s, body) = poll_until(
        SEE_TIMEOUT,
        || logs_by_trace_id_query(rt.runtime.logs_query_addr),
        |s, b| s == 200 && demo_cause_log_count(b) == 1,
    )
    .await;
    assert_eq!(
        demo_cause_log_count(&body),
        1,
        "a single seed yields exactly one cause log carrying the demo trace id \
         (single copy, non-orphaned WHY); body: {body}"
    );
}

// =========================================================================
// US-04 — THE PRE-FLIGHT REACHABILITY PROBE (Earned Trust, ADR-0077 F3)
// @infrastructure-failure @US-04
// =========================================================================

/// US-04 / AC generator-against-a-down-stack-fails-clearly — the probe contract.
///
/// ```gherkin
/// @infrastructure-failure @US-04
/// Scenario: The reachability probe reports a clear failure when the ingest endpoint is down
///   Given an ingest endpoint that nothing is listening on
///   When the generator's pre-flight reachability probe runs
///   Then it reports a clear failure naming the unreachable endpoint
///   And it does not silently succeed
/// ```
///
/// FALSIFIABILITY: the scaffold returns `GenError::Scaffold` ("…not yet
/// implemented"), which is NOT an unreachability report — the assertion that
/// the error clearly names unreachability FAILS. GREEN only when the probe
/// performs a real connect and reports the down endpoint.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reachability_probe_reports_a_clear_failure_when_the_endpoint_is_down() {
    let addr = closed_loopback_addr().await;
    let endpoint = format!("http://{addr}");

    let err = probe_reachable(&endpoint)
        .await
        .expect_err("probing a closed ingest endpoint must fail, never silently succeed");
    let msg = err.to_string();

    assert!(
        msg.contains("unreachable") || msg.contains("could not reach"),
        "the probe error clearly names unreachability; got: {msg}"
    );
    assert!(
        msg.contains(&endpoint) || msg.contains(&addr.to_string()),
        "the probe error names the endpoint it could not reach; got: {msg}"
    );
    assert!(
        !msg.contains("not yet implemented"),
        "RED discriminator: the probe must report unreachability, not the scaffold marker; got: {msg}"
    );
}

/// US-04 / AC generator-against-a-down-stack-fails-clearly — through the BIN.
///
/// ```gherkin
/// @infrastructure-failure @real-io @US-04
/// Scenario: Running the generator against a down stack fails clearly, not silently
///   Given no runtime is listening on the chosen ingest endpoint
///   When the telemetry generator command runs
///   Then it exits non-zero
///   And it tells the user the endpoint is unreachable and to bring the stack up first
/// ```
///
/// FALSIFIABILITY: the scaffold bin exits non-zero but prints the scaffold
/// marker, not an unreachability message — the message assertions FAIL. GREEN
/// only when the bin's pre-flight probe reports the down stack actionably.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn generator_against_a_down_stack_fails_clearly() {
    let addr = closed_loopback_addr().await;

    let out = run_generator(addr, TENANT_ACME).await;
    let stderr = stderr_of(&out);

    assert!(
        !out.status.success(),
        "the generator exits non-zero against a down stack; stderr: {stderr}"
    );
    assert!(
        stderr.contains("unreachable") || stderr.contains("could not reach"),
        "it tells the user the endpoint is unreachable; stderr: {stderr}"
    );
    assert!(
        stderr.contains("make up") || stderr.contains("bring the stack up"),
        "it suggests bringing the stack up first; stderr: {stderr}"
    );
    assert!(
        !stderr.contains("not yet implemented"),
        "RED discriminator: a real unreachability message, not the scaffold marker; stderr: {stderr}"
    );
}

// =========================================================================
// US-04 — TENANT SCOPING (present for its tenant, absent for another)
// @driving_port @real-io @adapter-integration @US-04
// =========================================================================

/// US-04 — generated telemetry is scoped to its tenant.
///
/// ```gherkin
/// @driving_port @real-io @US-04
/// Scenario: Generated telemetry is present for its tenant and invisible to another tenant
///   Given a consolidated runtime whose queries are scoped to tenant "acme"
///   And a second consolidated runtime whose queries are scoped to tenant "globex"
///   When the generator pushes telemetry for tenant "acme" to each runtime in turn
///   Then the "acme"-scoped query returns the telemetry
///   And the "globex"-scoped query returns nothing (the "acme" data is isolated)
/// ```
///
/// FALSIFIABILITY: the PRESENT half anchors RED — the scaffold pushes nothing,
/// so the "acme" query stays empty and the assertion FAILS. The ABSENT half
/// proves isolation: "acme"-tagged telemetry pushed into a "globex"-scoped
/// runtime is never returned by its "globex" query.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn generated_telemetry_is_scoped_to_its_tenant() {
    // PRESENT: "acme" telemetry visible to an "acme"-scoped query.
    let rt_acme = spawn_runtime("tenant-present", TENANT_ACME).await;
    let out_acme = run_generator(rt_acme.runtime.ingest_grpc_addr, TENANT_ACME).await;
    assert!(
        out_acme.status.success(),
        "the generator pushes for \"acme\"; stderr: {}",
        stderr_of(&out_acme)
    );
    let (_s, body) = poll_until(
        SEE_TIMEOUT,
        || metrics_query(rt_acme.runtime.metrics_query_addr),
        |s, b| s == 200 && metrics_result_len(b) > 0,
    )
    .await;
    assert!(
        metrics_result_len(&body) > 0,
        "the \"acme\" telemetry is returned by the \"acme\" query; body: {body}"
    );

    // ABSENT: the SAME "acme"-tagged telemetry pushed into a "globex"-scoped
    // runtime is never returned by its "globex" query (store-level tenant
    // isolation). The PRESENT half above proves the generator + wire work, so
    // an empty "globex" result here is isolation, not a missing push.
    let rt_globex = spawn_runtime("tenant-absent", TENANT_GLOBEX).await;
    let out_into_globex = run_generator(rt_globex.runtime.ingest_grpc_addr, TENANT_ACME).await;
    assert!(
        out_into_globex.status.success(),
        "the generator pushes \"acme\" telemetry into the globex-scoped runtime; stderr: {}",
        stderr_of(&out_into_globex)
    );
    let (_s, body) = poll_until(
        ABSENCE_SETTLE,
        || metrics_query(rt_globex.runtime.metrics_query_addr),
        |s, b| s == 200 && metrics_result_len(b) > 0,
    )
    .await;
    assert_eq!(
        metrics_result_len(&body),
        0,
        "the \"acme\" telemetry is NOT visible to the \"globex\" query; body: {body}"
    );
}

// =========================================================================
// US-04 — SAFE RE-RUN (running it twice is safe)
// @real-io @adapter-integration @US-04
// =========================================================================

/// US-04 / AC re-running-the-generator-is-safe.
///
/// ```gherkin
/// @real-io @US-04
/// Scenario: Re-running the generator is safe and the telemetry stays queryable
///   Given a consolidated runtime is running for tenant "acme"
///   When the telemetry generator runs twice against it
///   Then both runs succeed
///   And a metrics query still returns "request_count"
/// ```
///
/// FALSIFIABILITY: the scaffold's first run exits non-zero, so "both runs
/// succeed" FAILS. GREEN only when each invocation is an independent,
/// repeatable push that the store keeps queryable.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn re_running_the_generator_is_safe() {
    let rt = spawn_runtime("safe-rerun", TENANT_ACME).await;

    let first = run_generator(rt.runtime.ingest_grpc_addr, TENANT_ACME).await;
    assert!(
        first.status.success(),
        "the first run succeeds; stderr: {}",
        stderr_of(&first)
    );

    let second = run_generator(rt.runtime.ingest_grpc_addr, TENANT_ACME).await;
    assert!(
        second.status.success(),
        "the second run succeeds without error (re-run is safe); stderr: {}",
        stderr_of(&second)
    );

    let (_s, body) = poll_until(
        SEE_TIMEOUT,
        || metrics_query(rt.runtime.metrics_query_addr),
        |s, b| s == 200 && metrics_result_len(b) > 0,
    )
    .await;
    assert!(
        metrics_result_len(&body) > 0,
        "the telemetry is still queryable after re-running; body: {body}"
    );
}
