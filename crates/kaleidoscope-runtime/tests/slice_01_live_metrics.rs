// Kaleidoscope consolidated runtime — Slice 1: the metrics live loop
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

//! # Slice 1 — "I can send a metric and immediately see it" (US-01 + US-02).
//!
//! The feature walking skeleton. Every scenario drives the RUNNING
//! consolidated runtime through its driving ports (the OTLP ingest HTTP
//! listener + the metrics `query_range` listener) on EPHEMERAL `127.0.0.1:0`
//! ports — never the fixed 4317/4318/9090 defaults (the fixed-port flake).
//!
//! The load-bearing guard is `metric_is_queryable_immediately_after_it_is_sent`:
//! POST a metric at T, GET `query_range` over a window covering T, assert the
//! point comes back with value 1, NO restart. On TODAY's separate-process
//! architecture this is RED (the query process froze its store snapshot at its
//! own startup and never re-reads); it goes GREEN only when ingest and query
//! share the live `Arc<Store>` in one process (ADR-0076 DD2). That is the
//! single-process write-then-read proof the sink and router hold the SAME Arc.
//!
//! ## GREEN (DELIVER, slice 1)
//!
//! `spawn_consolidated` wires the shared-`Arc` composition, so every scenario
//! below runs by default (no `#[ignore]`): `cargo test -p kaleidoscope-runtime
//! --test slice_01_live_metrics`.

mod common;

use std::time::Duration;

use common::{
    encode_metric_request_count_one, get_query_range, metrics_contains_value, metrics_result_len,
    metrics_status_success, occupy_loopback_port, poll_until, post_otlp, read_auth_validator,
    spawn_test_runtime, spawn_test_runtime_with, try_spawn, TENANT_ACME, TENANT_GLOBEX,
};
use kaleidoscope_runtime::ConsolidatedConfig;

// =========================================================================
// US-01 — LIVE VISIBILITY (the north star; the walking skeleton)
// @walking_skeleton @driving_port @US-01
// =========================================================================

/// US-01 / AC a-metric-is-queryable-immediately-after-it-is-sent.
///
/// ```gherkin
/// Scenario: A metric is queryable immediately after it is sent
///   Given the consolidated runtime is running with an empty metric store for tenant "acme"
///   When Andrea sends an OTLP metric "request_count" with value 1 at time T for tenant "acme"
///   And Andrea queries "/api/v1/query_range" for "request_count" over a window covering T
///   Then the response status is success
///   And the result contains a point with value 1 at time T
///   And no process was restarted between sending and querying
/// ```
///
/// FALSIFIABILITY: on the separate-process (frozen-snapshot) architecture the
/// query returns empty — this assertion FAILS. GREEN only when the sink and
/// the router share the live `Arc<Store>` in one process.
#[tokio::test(flavor = "multi_thread")]
async fn metric_is_queryable_immediately_after_it_is_sent() {
    let rt = spawn_test_runtime("live-metric", TENANT_ACME).await;

    let status = post_otlp(
        &rt.ingest_http_base(),
        "metrics",
        encode_metric_request_count_one(),
    )
    .await;
    assert_eq!(status, 200, "the ingest endpoint accepts the metric push");

    let (_elapsed, status, body) = poll_until(
        Duration::from_secs(10),
        || get_query_range(rt.metrics_addr()),
        |s, b| s == 200 && metrics_result_len(b) > 0,
    )
    .await;

    assert_eq!(status, 200, "query_range answers 200; body: {body}");
    assert!(
        metrics_status_success(&body),
        "status is success; body: {body}"
    );
    assert!(
        metrics_contains_value(&body, "1"),
        "the point just sent (value 1) comes back live, no restart; body: {body}"
    );
}

/// US-01 / AC a-post-startup-append-is-visible-without-restart (the north-star
/// regression guard — the exact loop that fails in the separate-process world).
///
/// ```gherkin
/// Scenario: A metric sent after the runtime started is visible without a restart
///   Given the consolidated runtime started with an empty metric store before any telemetry arrived
///   When Andrea sends an OTLP metric "request_count" for tenant "acme" some time after startup
///   And Andrea queries "/api/v1/query_range" over a window covering the send time
///   Then the result contains the metric just sent
///   And no restart of the runtime or any component was required to see it
/// ```
#[tokio::test(flavor = "multi_thread")]
async fn metric_sent_after_startup_is_visible_without_restart() {
    let rt = spawn_test_runtime("post-startup-append", TENANT_ACME).await;

    // The store began EMPTY at startup: a query before any telemetry is empty.
    let (_s, before) = get_query_range(rt.metrics_addr()).await;
    assert_eq!(
        metrics_result_len(&before),
        0,
        "empty before any append; body: {before}"
    );

    let status = post_otlp(
        &rt.ingest_http_base(),
        "metrics",
        encode_metric_request_count_one(),
    )
    .await;
    assert_eq!(status, 200, "post-startup append accepted");

    let (_elapsed, _status, after) = poll_until(
        Duration::from_secs(10),
        || get_query_range(rt.metrics_addr()),
        |s, b| s == 200 && metrics_result_len(b) > 0,
    )
    .await;
    assert!(
        metrics_contains_value(&after, "1"),
        "the post-startup append is visible with NO restart; body: {after}"
    );
}

/// US-01 / AC ingest-and-metrics-query-served-by-one-process.
/// @driving_port
///
/// ```gherkin
/// Scenario: The runtime serves ingest and metrics query from one process
///   Given the consolidated runtime has been started with one command
///   Then the OTLP ingest endpoint accepts a metric push
///   And the metrics query endpoint answers a query_range request
///   And both are served by the same single running process
/// ```
#[tokio::test(flavor = "multi_thread")]
async fn ingest_and_metrics_query_served_by_one_process() {
    let rt = spawn_test_runtime("one-process", TENANT_ACME).await;

    let ingest_status = post_otlp(
        &rt.ingest_http_base(),
        "metrics",
        encode_metric_request_count_one(),
    )
    .await;
    assert_eq!(
        ingest_status, 200,
        "the one process accepts the ingest push"
    );

    let (query_status, body) = get_query_range(rt.metrics_addr()).await;
    assert_eq!(
        query_status, 200,
        "the SAME process answers the query; body: {body}"
    );
}

// =========================================================================
// US-01 — EMPTY-BEFORE-INGEST (edge: empty success, never an error)
// @US-01
// =========================================================================

/// US-01 / AC an-empty-store-returns-empty-success-not-error.
///
/// ```gherkin
/// Scenario: Querying an empty store returns an empty success, not an error
///   Given the consolidated runtime is running and no telemetry has been sent
///   When Andrea queries "/api/v1/query_range" for "request_count" over any valid window
///   Then the response status is success
///   And the result is empty
///   And the response is not an error
/// ```
#[tokio::test(flavor = "multi_thread")]
async fn empty_store_returns_empty_success_not_error() {
    let rt = spawn_test_runtime("empty-before", TENANT_ACME).await;

    let (status, body) = get_query_range(rt.metrics_addr()).await;
    assert_eq!(
        status, 200,
        "empty store is a 200, never a 500; body: {body}"
    );
    assert!(
        metrics_status_success(&body),
        "status is success; body: {body}"
    );
    assert_eq!(
        metrics_result_len(&body),
        0,
        "the result is empty; body: {body}"
    );
}

// =========================================================================
// US-02 — TENANT ISOLATION in-process (positive + negative control)
// @US-02
// =========================================================================

/// US-02 / AC owning-tenant-read-returns-its-own-data (positive control).
///
/// ```gherkin
/// Scenario: A query for the owning tenant returns its own data
///   Given the consolidated runtime holds a metric "request_count" ingested for tenant "acme"
///   When a query for "request_count" is made scoped to tenant "acme"
///   Then the result contains the metric ingested for "acme"
/// ```
#[tokio::test(flavor = "multi_thread")]
async fn owning_tenant_read_returns_its_own_data() {
    // Ingest tenant acme; metrics query tenant acme (the positive control).
    let rt = spawn_test_runtime("iso-positive", TENANT_ACME).await;
    let status = post_otlp(
        &rt.ingest_http_base(),
        "metrics",
        encode_metric_request_count_one(),
    )
    .await;
    assert_eq!(status, 200);

    let (_elapsed, _status, body) = poll_until(
        Duration::from_secs(10),
        || get_query_range(rt.metrics_addr()),
        |s, b| s == 200 && metrics_result_len(b) > 0,
    )
    .await;
    assert!(
        metrics_contains_value(&body, "1"),
        "the owning tenant acme sees its own metric; body: {body}"
    );
}

/// US-02 / AC cross-tenant-read-returns-empty (negative control, load-bearing
/// guardrail — a leak here is the worst possible trade).
///
/// ```gherkin
/// Scenario: A query for one tenant never returns another tenant's data
///   Given the consolidated runtime holds a metric "request_count" ingested for tenant "acme"
///   When a query for "request_count" is made scoped to tenant "globex"
///   Then the response status is success
///   And the result is empty
///   And none of "acme"'s data is returned
/// ```
///
/// FALSIFIABILITY: a query that ignored the tenant key would return acme's
/// point regardless of scope — this assertion FAILS. GREEN only when the
/// globex-scoped read scans only globex's (empty) series.
#[tokio::test(flavor = "multi_thread")]
async fn cross_tenant_read_returns_empty() {
    // Ingest under acme (the default ingest tenant), but resolve the metrics
    // query as globex.
    let root = common::fresh_pillar_root("iso-negative");
    let mut config = ConsolidatedConfig::for_ephemeral_test(root, TENANT_ACME);
    config.metrics_query_tenant = Some(TENANT_GLOBEX.to_string());
    let rt = spawn_test_runtime_with("iso-negative", config).await;

    let status = post_otlp(
        &rt.ingest_http_base(),
        "metrics",
        encode_metric_request_count_one(),
    )
    .await;
    assert_eq!(status, 200, "acme's metric is ingested");

    // Give the (would-be) write time to land, then assert globex still sees
    // nothing. A short settle plus a single read: the negative must STAY empty.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (qstatus, body) = get_query_range(rt.metrics_addr()).await;
    assert_eq!(
        qstatus, 200,
        "cross-tenant read is an empty success, not an error; body: {body}"
    );
    assert!(
        metrics_status_success(&body),
        "status is success; body: {body}"
    );
    assert_eq!(
        metrics_result_len(&body),
        0,
        "globex must NOT see acme's metric; body: {body}"
    );
}

// =========================================================================
// US-02 — OPTIONAL READ-AUTH stays fail-closed when configured (error path)
// @US-02
// =========================================================================

/// US-02 / AC optional-read-auth-stays-fail-closed-when-configured.
///
/// ```gherkin
/// Scenario: A read-auth-configured runtime refuses a tokenless query
///   Given the consolidated runtime is configured with per-request read auth and env tenant "acme"
///   When a query is made with no bearer token
///   Then the query is refused
///   And no metric data is returned
/// ```
///
/// FALSIFIABILITY: env tenant is acme, so a fall-through (env-tenant) impl
/// would serve acme's data 200 — the refusal assertion FAILS. GREEN only when
/// the bearer gate refuses before the store and never downgrades to the env
/// tenant (the no-bearer-bypass, ADR-0074).
#[tokio::test(flavor = "multi_thread")]
async fn optional_read_auth_stays_fail_closed_when_configured() {
    let root = common::fresh_pillar_root("read-auth-on");
    let mut config = ConsolidatedConfig::for_ephemeral_test(root, TENANT_ACME);
    config.read_auth = Some(read_auth_validator());
    let rt = spawn_test_runtime_with("read-auth-on", config).await;

    // Seed acme's metric so a fall-through impl would have data to leak.
    let _ = post_otlp(
        &rt.ingest_http_base(),
        "metrics",
        encode_metric_request_count_one(),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Tokenless query against an auth-on runtime must be refused (no env
    // downgrade), and must NOT return acme's metric.
    let (status, body) = get_query_range(rt.metrics_addr()).await;
    assert_ne!(
        status, 200,
        "auth-on tokenless query must be refused, not 200; body: {body}"
    );
    assert_eq!(
        metrics_result_len(&body),
        0,
        "no metric data leaks on the refusal; body: {body}"
    );
}

// =========================================================================
// US-01 / US-05 — FAIL-CLOSED STARTUP (error path: bind conflict -> refuse)
// @US-01 @US-05
// =========================================================================

/// US-01 / US-05 / AC fail-closed-startup. If any of the five listeners cannot
/// bind, the runtime refuses to start — no half-up process.
///
/// ```gherkin
/// Scenario: The runtime refuses to start if a listener cannot bind
///   Given the metrics query port is already occupied by another process
///   When the consolidated runtime is started against that occupied port
///   Then startup is refused
///   And no half-up process is left serving
/// ```
///
/// FALSIFIABILITY: an impl that bound the other four and served anyway (a
/// half-up process) would return Ok — this assertion FAILS. GREEN only when a
/// bind failure on ANY of the five listeners refuses the whole startup.
#[tokio::test(flavor = "multi_thread")]
async fn fail_closed_startup_on_bind_conflict() {
    // Occupy a loopback port and hand it to the runtime as the metrics query
    // bind, so that bind must conflict.
    let (occupied_addr, _guard) = occupy_loopback_port().await;
    let root = common::fresh_pillar_root("fail-closed");
    let mut config = ConsolidatedConfig::for_ephemeral_test(root, TENANT_ACME);
    config.metrics_query_addr = occupied_addr;

    let result = try_spawn(config).await;
    assert!(
        result.is_err(),
        "startup must be REFUSED when a listener cannot bind (no half-up process)"
    );
}

// =========================================================================
// US-01 — FRESHNESS KPI (observability; the acceptance test IS the measure)
// @kpi @US-01
// =========================================================================

/// US-01 / KPI 2 — the ingest-ack -> query-returns interval, p95 < 1 s. For v0
/// the acceptance test is the measurement (outcome-kpis.md). This assertion
/// uses a GENEROUS local budget (the SLO-shaped guardrail is the contractual
/// CI measure; threshold-raising is never the fix for a flake — the p95
/// wall-clock flake class, project memory `p95_wallclock_flakes_overnight`).
///
/// ```gherkin
/// Scenario: A freshly-sent metric is queryable back within the freshness budget
///   Given the consolidated runtime is running for tenant "acme"
///   When Andrea sends a metric and immediately queries it back
///   Then the metric returns within the freshness budget
/// ```
#[tokio::test(flavor = "multi_thread")]
async fn freshness_metric_returns_within_budget() {
    let rt = spawn_test_runtime("freshness", TENANT_ACME).await;
    let status = post_otlp(
        &rt.ingest_http_base(),
        "metrics",
        encode_metric_request_count_one(),
    )
    .await;
    assert_eq!(status, 200);

    let (elapsed, _status, body) = poll_until(
        Duration::from_secs(10),
        || get_query_range(rt.metrics_addr()),
        |s, b| s == 200 && metrics_result_len(b) > 0,
    )
    .await;

    assert!(
        metrics_contains_value(&body, "1"),
        "the metric returns at all; body: {body}"
    );
    // Generous local budget (5 s) well above the p95 < 1 s SLO; CI is the
    // indicative measure of the SLO itself.
    assert!(
        elapsed < Duration::from_secs(5),
        "ingest-ack -> query-returns within the generous local freshness budget; took {elapsed:?}"
    );
}
