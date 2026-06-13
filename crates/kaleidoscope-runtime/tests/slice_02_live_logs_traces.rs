// Kaleidoscope consolidated runtime — Slice 2: logs + traces + capstone
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

//! # Slice 2 — "the whole signal set is live in one command" (US-03/04/05).
//!
//! Applies the slice-1 shared-`Arc` live-visibility pattern to logs (lumen,
//! `/api/v1/logs`) and traces (ray, both `/api/v1/traces` window and
//! `/api/v1/traces/by_id`), then the capstone US-05: ONE process binds the
//! ingest ports AND all three query ports on EPHEMERAL `127.0.0.1:0`, and a
//! metric + a log + a trace are each queryable back live with NO restart.
//!
//! ## RED-not-BROKEN (Mandate 7)
//!
//! `spawn_consolidated` is a `__SCAFFOLD__` panic until DELIVER; every scenario
//! is `#[ignore]`d. Observe RED with:
//! `cargo test -p kaleidoscope-runtime --test slice_02_live_logs_traces -- --ignored`.

mod common;

use std::time::Duration;

use common::{
    array_len, encode_log_checkout_failed, encode_metric_request_count_one, encode_trace_span,
    fresh_pillar_root, get_logs, get_query_range, get_trace_by_id, get_traces_window,
    metrics_contains_value, metrics_result_len, metrics_status_success, poll_until, post_otlp,
    spawn_test_runtime, spawn_test_runtime_with, TENANT_ACME, TENANT_GLOBEX, TRACE_ID_HEX,
};
use kaleidoscope_runtime::ConsolidatedConfig;
use tokio::net::TcpStream;

// =========================================================================
// US-03 — LOGS live loop
// @US-03 @driving_port
// =========================================================================

/// US-03 / AC a-log-is-queryable-immediately-after-it-is-sent.
///
/// ```gherkin
/// Scenario: A log is queryable immediately after it is sent
///   Given the consolidated runtime is running with an empty log store for tenant "acme"
///   When Andrea sends an OTLP log "checkout failed: card declined" at time T for tenant "acme"
///   And Andrea queries "/api/v1/logs" over a window covering T
///   Then the response status is success
///   And the result contains the log just sent
///   And no restart was required
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER wires the shared-Arc composition; run with --ignored"]
async fn log_is_queryable_immediately_after_it_is_sent() {
    let rt = spawn_test_runtime("live-log", TENANT_ACME).await;

    let status = post_otlp(&rt.ingest_http_base(), "logs", encode_log_checkout_failed()).await;
    assert_eq!(status, 200, "the ingest endpoint accepts the log push");

    let (_elapsed, qstatus, body) = poll_until(
        Duration::from_secs(10),
        || get_logs(rt.logs_addr()),
        |s, b| s == 200 && array_len(b) > 0,
    )
    .await;
    assert_eq!(qstatus, 200, "logs query answers 200; body: {body}");
    assert!(
        array_len(&body) > 0,
        "the log just sent comes back live, no restart; body: {body}"
    );
}

/// US-03 / AC empty-before-send (edge: empty success, not an error).
///
/// ```gherkin
/// Scenario: Querying logs before any are sent returns an empty success
///   Given the consolidated runtime is running and no logs have been sent
///   When Andrea queries "/api/v1/logs" over any valid window
///   Then the response status is success
///   And the result is empty
///   And the response is not an error
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER wires the shared-Arc composition; run with --ignored"]
async fn logs_empty_before_send_returns_empty_success() {
    let rt = spawn_test_runtime("logs-empty", TENANT_ACME).await;
    let (status, body) = get_logs(rt.logs_addr()).await;
    assert_eq!(
        status, 200,
        "empty logs store is a 200, not a 500; body: {body}"
    );
    assert_eq!(array_len(&body), 0, "the result is empty; body: {body}");
}

/// US-03 / AC cross-tenant log read returns empty (negative control).
///
/// ```gherkin
/// Scenario: A log for one tenant is not returned to another tenant
///   Given the consolidated runtime holds a log ingested for tenant "acme"
///   When a "/api/v1/logs" query is made scoped to tenant "globex"
///   Then the result is empty
///   And none of "acme"'s logs are returned
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER wires the shared-Arc composition; run with --ignored"]
async fn cross_tenant_log_read_returns_empty() {
    let root = fresh_pillar_root("logs-iso-neg");
    let mut config = ConsolidatedConfig::for_ephemeral_test(root, TENANT_ACME);
    config.logs_query_tenant = Some(TENANT_GLOBEX.to_string());
    let rt = spawn_test_runtime_with("logs-iso-neg", config).await;

    let status = post_otlp(&rt.ingest_http_base(), "logs", encode_log_checkout_failed()).await;
    assert_eq!(status, 200, "acme's log is ingested");

    tokio::time::sleep(Duration::from_millis(200)).await;
    let (qstatus, body) = get_logs(rt.logs_addr()).await;
    assert_eq!(
        qstatus, 200,
        "cross-tenant log read is an empty success; body: {body}"
    );
    assert_eq!(
        array_len(&body),
        0,
        "globex must NOT see acme's log; body: {body}"
    );
}

// =========================================================================
// US-04 — TRACES live loop (window route + lookup-by-id route)
// @US-04 @driving_port
// =========================================================================

/// US-04 / AC a-trace-is-queryable-by-window.
///
/// ```gherkin
/// Scenario: A trace is queryable by time window immediately after it is sent
///   Given the consolidated runtime is running with an empty trace store for tenant "acme"
///   When Andrea sends an OTLP span on trace "4bf9...4736" at time T for tenant "acme"
///   And Andrea queries "/api/v1/traces" over a window covering T
///   Then the response status is success
///   And the result contains the span just sent
///   And no restart was required
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER wires the shared-Arc composition; run with --ignored"]
async fn trace_is_queryable_by_window_immediately() {
    let rt = spawn_test_runtime("live-trace-window", TENANT_ACME).await;

    let status = post_otlp(&rt.ingest_http_base(), "traces", encode_trace_span()).await;
    assert_eq!(status, 200, "the ingest endpoint accepts the span push");

    let (_elapsed, qstatus, body) = poll_until(
        Duration::from_secs(10),
        || get_traces_window(rt.traces_addr()),
        |s, b| s == 200 && array_len(b) > 0,
    )
    .await;
    assert_eq!(
        qstatus, 200,
        "traces window query answers 200; body: {body}"
    );
    assert!(
        array_len(&body) > 0,
        "the span comes back live by window, no restart; body: {body}"
    );
}

/// US-04 / AC the-same-trace-is-retrievable-by-its-trace-id.
///
/// ```gherkin
/// Scenario: The same trace is retrievable by its trace id
///   Given the consolidated runtime holds the span on trace "4bf9...4736" for tenant "acme"
///   When Andrea looks up "/api/v1/traces/by_id" for "4bf9...4736"
///   Then the result contains the span
///   And no restart was required
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER wires the shared-Arc composition; run with --ignored"]
async fn trace_is_retrievable_by_id() {
    let rt = spawn_test_runtime("live-trace-byid", TENANT_ACME).await;

    let status = post_otlp(&rt.ingest_http_base(), "traces", encode_trace_span()).await;
    assert_eq!(status, 200);

    let (_elapsed, qstatus, body) = poll_until(
        Duration::from_secs(10),
        || get_trace_by_id(rt.traces_addr(), TRACE_ID_HEX),
        |s, b| s == 200 && array_len(b) > 0,
    )
    .await;
    assert_eq!(qstatus, 200, "by-id lookup answers 200; body: {body}");
    assert!(
        array_len(&body) > 0,
        "the span is found by trace id live, no restart; body: {body}"
    );
}

/// US-04 / AC by-id-before-any-trace returns empty success (edge).
///
/// ```gherkin
/// Scenario: A trace lookup before any trace is sent returns an empty success
///   Given the consolidated runtime is running and no traces have been sent
///   When Andrea looks up "/api/v1/traces/by_id" for any trace id
///   Then the response status is success
///   And the result is empty
///   And the response is not an error
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER wires the shared-Arc composition; run with --ignored"]
async fn trace_by_id_before_any_trace_returns_empty_success() {
    let rt = spawn_test_runtime("traces-empty-byid", TENANT_ACME).await;
    let (status, body) = get_trace_by_id(rt.traces_addr(), TRACE_ID_HEX).await;
    assert_eq!(
        status, 200,
        "by-id on an empty store is a 200, not a 500; body: {body}"
    );
    assert_eq!(array_len(&body), 0, "the result is empty; body: {body}");
}

/// US-04 / AC cross-tenant trace read returns empty (negative control,
/// including the lookup-by-id path which must ALSO be isolated, ADR-0053).
///
/// ```gherkin
/// Scenario: A trace for one tenant is not returned to another tenant
///   Given the consolidated runtime holds a trace ingested for tenant "acme"
///   When a "/api/v1/traces" query is made scoped to tenant "globex"
///   Then the result is empty
///   And the by-id lookup for "acme"'s trace also returns empty for "globex"
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER wires the shared-Arc composition; run with --ignored"]
async fn cross_tenant_trace_read_returns_empty() {
    let root = fresh_pillar_root("traces-iso-neg");
    let mut config = ConsolidatedConfig::for_ephemeral_test(root, TENANT_ACME);
    config.traces_query_tenant = Some(TENANT_GLOBEX.to_string());
    let rt = spawn_test_runtime_with("traces-iso-neg", config).await;

    let status = post_otlp(&rt.ingest_http_base(), "traces", encode_trace_span()).await;
    assert_eq!(status, 200, "acme's span is ingested");
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (wstatus, wbody) = get_traces_window(rt.traces_addr()).await;
    assert_eq!(
        wstatus, 200,
        "cross-tenant window read is an empty success; body: {wbody}"
    );
    assert_eq!(
        array_len(&wbody),
        0,
        "globex must NOT see acme's span by window; body: {wbody}"
    );

    let (bstatus, bbody) = get_trace_by_id(rt.traces_addr(), TRACE_ID_HEX).await;
    assert_eq!(
        bstatus, 200,
        "cross-tenant by-id read is an empty success; body: {bbody}"
    );
    assert_eq!(
        array_len(&bbody),
        0,
        "globex must NOT find acme's trace by id (lookup-by-id isolation, ADR-0053); body: {bbody}"
    );
}

// =========================================================================
// US-05 — CAPSTONE: one command, all five ports, three signals live
// @US-05
// =========================================================================

/// US-05 / AC all-five-endpoints-bind-without-port-conflict.
///
/// ```gherkin
/// Scenario: One command brings up ingest and all three query endpoints on one process
///   Given Andrea starts the consolidated runtime with a single command
///   Then the OTLP ingest endpoint accepts pushes
///   And the metrics, logs, and traces query endpoints all answer requests
///   And all of them are served by the same single running process without port conflict
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER wires the shared-Arc composition; run with --ignored"]
async fn one_command_binds_all_five_ports() {
    let rt = spawn_test_runtime("all-five-ports", TENANT_ACME).await;

    // All five addresses are distinct, bound (non-zero) ports.
    let addrs = [
        rt.ingest_grpc_addr(),
        rt.runtime.ingest_http_addr,
        rt.metrics_addr(),
        rt.logs_addr(),
        rt.traces_addr(),
    ];
    for a in &addrs {
        assert_ne!(
            a.port(),
            0,
            "each of the five listeners reports a real bound port: {a}"
        );
    }

    // The ingest gRPC listener accepts a TCP connection (it is genuinely bound).
    assert!(
        TcpStream::connect(rt.ingest_grpc_addr()).await.is_ok(),
        "the ingest gRPC listener accepts connections on {}",
        rt.ingest_grpc_addr()
    );

    // The ingest HTTP listener accepts a push.
    let ingest = post_otlp(
        &rt.ingest_http_base(),
        "metrics",
        encode_metric_request_count_one(),
    )
    .await;
    assert_eq!(ingest, 200, "the ingest HTTP listener accepts a push");

    // Each of the three query listeners answers.
    let (m, _) = get_query_range(rt.metrics_addr()).await;
    let (l, _) = get_logs(rt.logs_addr()).await;
    let (t, _) = get_traces_window(rt.traces_addr()).await;
    assert_eq!(
        (m, l, t),
        (200, 200, 200),
        "all three query endpoints answer on the one process"
    );
}

/// US-05 / AC every-signal-sent-is-queryable-back-live (the three-signal
/// walking skeleton).
/// @walking_skeleton
///
/// ```gherkin
/// Scenario: Every signal sent is queryable back live, no restart
///   Given the consolidated runtime is running for tenant "acme"
///   When Andrea sends one metric, one log, and one trace for "acme"
///   And Andrea queries the metrics, logs, and traces endpoints in turn
///   Then each query returns the telemetry just sent
///   And no restart of the runtime or any component was required
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER wires the shared-Arc composition; run with --ignored"]
async fn every_signal_queryable_back_live_no_restart() {
    let rt = spawn_test_runtime("three-signal", TENANT_ACME).await;
    let base = rt.ingest_http_base();

    assert_eq!(
        post_otlp(&base, "metrics", encode_metric_request_count_one()).await,
        200
    );
    assert_eq!(
        post_otlp(&base, "logs", encode_log_checkout_failed()).await,
        200
    );
    assert_eq!(post_otlp(&base, "traces", encode_trace_span()).await, 200);

    let (_e, _s, mbody) = poll_until(
        Duration::from_secs(10),
        || get_query_range(rt.metrics_addr()),
        |s, b| s == 200 && metrics_result_len(b) > 0,
    )
    .await;
    assert!(
        metrics_contains_value(&mbody, "1"),
        "metric back live; body: {mbody}"
    );

    let (_e, _s, lbody) = poll_until(
        Duration::from_secs(10),
        || get_logs(rt.logs_addr()),
        |s, b| s == 200 && array_len(b) > 0,
    )
    .await;
    assert!(array_len(&lbody) > 0, "log back live; body: {lbody}");

    let (_e, _s, tbody) = poll_until(
        Duration::from_secs(10),
        || get_traces_window(rt.traces_addr()),
        |s, b| s == 200 && array_len(b) > 0,
    )
    .await;
    assert!(array_len(&tbody) > 0, "trace back live; body: {tbody}");
}

/// US-05 / AC a-fresh-stack-is-consistent-not-half-empty (edge: all three
/// signals return empty successes before any telemetry).
///
/// ```gherkin
/// Scenario: A fresh stack returns empty successes across all signals before any telemetry
///   Given the consolidated runtime has just started on an empty pillar root
///   When Andrea queries the metrics, logs, and traces endpoints
///   Then each returns a success with an empty result
///   And none returns an error
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER wires the shared-Arc composition; run with --ignored"]
async fn fresh_stack_returns_empty_success_across_all_signals() {
    let rt = spawn_test_runtime("fresh-stack", TENANT_ACME).await;

    let (m, mbody) = get_query_range(rt.metrics_addr()).await;
    assert_eq!(m, 200, "metrics empty success; body: {mbody}");
    assert!(
        metrics_status_success(&mbody),
        "metrics status success; body: {mbody}"
    );
    assert_eq!(
        metrics_result_len(&mbody),
        0,
        "metrics empty; body: {mbody}"
    );

    let (l, lbody) = get_logs(rt.logs_addr()).await;
    assert_eq!(l, 200, "logs empty success; body: {lbody}");
    assert_eq!(array_len(&lbody), 0, "logs empty; body: {lbody}");

    let (t, tbody) = get_traces_window(rt.traces_addr()).await;
    assert_eq!(t, 200, "traces empty success; body: {tbody}");
    assert_eq!(array_len(&tbody), 0, "traces empty; body: {tbody}");
}
