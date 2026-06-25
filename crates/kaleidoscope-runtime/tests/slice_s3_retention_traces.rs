// Kaleidoscope consolidated runtime — Slice S3: trace age-based retention
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

//! # Slice S3 — "old spans age out of the service-window and by-id queries"
//!
//! Drives the RUNNING consolidated runtime through its real driving ports on
//! ephemeral loopback. The demo overlay is OFF so the routers read the RAW
//! store only (no synthetic demo traces to confound the assertions).
//!
//! - `windowed_then_restart_no_window` verifies S3.E2/E3/E5/E6:
//!   ingest old + fresh; with a 60s window, the service-window query returns
//!   ONLY the fresh span; by-id of the old trace returns empty; by-id of
//!   the fresh trace returns its span; after a restart with NO window the old
//!   trace is STILL absent on BOTH paths (genuine delete, not a read-time
//!   mask; S3.E5).
//! - `keep_forever_default` verifies S3.E4: with no window, an old span is
//!   still returned alongside a fresh one, on both the list and by-id paths.

mod common;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::{array_len, poll_until, post_otlp, TENANT_ACME};
use kaleidoscope_runtime::{spawn_consolidated, ConsolidatedConfig, RunningRuntime};

use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::any_value;
use opentelemetry_proto::tonic::common::v1::{AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span};
use prost::Message;

/// The retention window under test (S3.E6).
const WINDOW: Duration = Duration::from_secs(60);

/// Trace-id bytes and hex for the span that should be PURGED (stamped 300s old).
const OLD_TRACE_ID_BYTES: [u8; 16] = [0xaa; 16];
const OLD_TRACE_ID_HEX: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// Trace-id bytes and hex for the span that should SURVIVE (stamped 5s old).
const FRESH_TRACE_ID_BYTES: [u8; 16] = [0xbb; 16];
const FRESH_TRACE_ID_HEX: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// The service name both spans are filed under (the window route requires it).
const SERVICE: &str = "checkout";

// =========================================================================
// Time arithmetic — factored into one helper to avoid duplication
// =========================================================================

/// Returns `(now_nanos, now_secs, old_nanos, fresh_nanos, start_secs, end_secs)`.
///
/// - `old_nanos`   = now − 300 s in nanos  (well outside the 60 s window).
/// - `fresh_nanos` = now −   5 s in nanos  (well inside  the 60 s window).
/// - `start_secs` / `end_secs` bracket both samples with ±1 h headroom.
fn time_context() -> (u64, u64, u64, u64, u64, u64) {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock");
    let now_nanos = d.as_nanos() as u64;
    let now_secs = d.as_secs();
    let old_nanos = now_nanos - 300 * 1_000_000_000;
    let fresh_nanos = now_nanos - 5 * 1_000_000_000;
    let start_secs = now_secs - 3_600;
    let end_secs = now_secs + 3_600;
    (
        now_nanos,
        now_secs,
        old_nanos,
        fresh_nanos,
        start_secs,
        end_secs,
    )
}

// =========================================================================
// OTLP trace encoder
// =========================================================================

fn string_kv(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_string())),
        }),
    }
}

/// Encode a single OTLP span with the given `trace_id_bytes` and
/// `start_time_unix_nano`. Filed under service `checkout` so the window
/// route (which requires a `service` parameter) can find it.
fn encode_trace_at(trace_id_bytes: [u8; 16], start_time_unix_nano: u64) -> Vec<u8> {
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", SERVICE)],
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: Some(InstrumentationScope {
                    name: "kaleidoscope.retention.trace.test".to_string(),
                    version: "0.0.0".to_string(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                spans: vec![Span {
                    trace_id: trace_id_bytes.to_vec(),
                    // Unique span-id per trace so the two spans are distinct.
                    span_id: vec![trace_id_bytes[0]; 8],
                    trace_state: String::new(),
                    parent_span_id: vec![],
                    flags: 0,
                    name: "retention-test-span".to_string(),
                    kind: 2, // SPAN_KIND_SERVER
                    start_time_unix_nano,
                    end_time_unix_nano: start_time_unix_nano + 1_000,
                    attributes: vec![],
                    dropped_attributes_count: 0,
                    events: vec![],
                    dropped_events_count: 0,
                    links: vec![],
                    dropped_links_count: 0,
                    status: None,
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec()
}

// =========================================================================
// Driving helpers
// =========================================================================

fn config(root: std::path::PathBuf, retention: Option<Duration>) -> ConsolidatedConfig {
    let mut c = ConsolidatedConfig::for_ephemeral_test(root, TENANT_ACME);
    c.demo_overlay_enabled = false;
    c.retention_max_age = retention;
    c
}

async fn ingest_trace(rt: &RunningRuntime, body: Vec<u8>) {
    let base = format!("http://{}", rt.ingest_http_addr);
    let status = post_otlp(&base, "traces", body).await;
    assert_eq!(status, 200, "ingest endpoint accepts the trace push");
}

/// GET `/api/v1/traces?service={SERVICE}&start={start}&end={end}` (service
/// window route). Returns `(status, body_json)` where body is a bare JSON
/// array of spans.
async fn query_traces(rt: &RunningRuntime, start_secs: u64, end_secs: u64) -> (u16, String) {
    let url = format!(
        "http://{}/api/v1/traces?service={SERVICE}&start={start_secs}&end={end_secs}",
        rt.traces_query_addr
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .expect("GET /api/v1/traces over loopback");
    let status = resp.status().as_u16();
    let text = resp.text().await.expect("read body");
    (status, text)
}

/// GET `/api/v1/traces/by_id?trace_id={hex}`. Returns `(status, body_json)`
/// where body is a bare JSON array of spans. An empty array means not found.
async fn trace_by_id(rt: &RunningRuntime, trace_id_hex: &str) -> (u16, String) {
    let url = format!(
        "http://{}/api/v1/traces/by_id?trace_id={trace_id_hex}",
        rt.traces_query_addr
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .expect("GET /api/v1/traces/by_id over loopback");
    let status = resp.status().as_u16();
    let text = resp.text().await.expect("read body");
    (status, text)
}

/// Return `true` when ANY span in the bare-JSON-array response has a
/// `trace_id` field matching `trace_id_hex`.
fn trace_id_in_results(response_json: &str, trace_id_hex: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(response_json)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .map(|spans| {
            spans.iter().any(|s| {
                s["trace_id"]
                    .as_str()
                    .map(|tid| tid == trace_id_hex)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// A unique pillar root for this test process.
fn fresh_root(label: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let mut p = std::env::temp_dir();
    p.push(format!(
        "kal-s3-retention-{label}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&p).expect("mkdir pillar root");
    p
}

// =========================================================================
// S3.E2 / E3 / E5 / E6: windowed runtime purges old span; restart with no
// window confirms the delete is real, not a read-time mask — on BOTH the
// service-window list and the by-id paths.
// =========================================================================
#[tokio::test(flavor = "multi_thread")]
async fn windowed_then_restart_no_window() {
    let root = fresh_root("windowed");
    let (_now_nanos, _now_secs, old_nanos, fresh_nanos, start_secs, end_secs) = time_context();

    // --- Runtime up WITH a 60 s window (S3.E1 / S3.E6). ---
    let rt = spawn_consolidated(config(root.clone(), Some(WINDOW)))
        .await
        .expect("spawn windowed runtime");

    ingest_trace(&rt, encode_trace_at(OLD_TRACE_ID_BYTES, old_nanos)).await;
    ingest_trace(&rt, encode_trace_at(FRESH_TRACE_ID_BYTES, fresh_nanos)).await;

    // Poll until the fresh span appears on the service-window route
    // (live-visibility guarantee; enforce() on each query also purges OLD).
    let (_elapsed, status, body) = poll_until(
        Duration::from_secs(10),
        || query_traces(&rt, start_secs, end_secs),
        |s, b| s == 200 && array_len(b) > 0,
    )
    .await;

    assert_eq!(status, 200, "traces query answers 200; body: {body}");
    // S3.E3: the in-window (fresh) span IS returned on the list route.
    assert!(
        trace_id_in_results(&body, FRESH_TRACE_ID_HEX),
        "the fresh span (5 s old) appears on the list route; body: {body}"
    );
    // S3.E2: the out-of-window (old) span IS NOT returned on the list route.
    assert!(
        !trace_id_in_results(&body, OLD_TRACE_ID_HEX),
        "the old span (300 s old) is purged out of the 60 s window on the list route; body: {body}"
    );

    // S3.E2 (by-id path): the old trace is also absent on the by-id route.
    let (_s, old_by_id) = trace_by_id(&rt, OLD_TRACE_ID_HEX).await;
    assert_eq!(
        array_len(&old_by_id),
        0,
        "by-id of the old trace returns empty after purge; body: {old_by_id}"
    );

    // S3.E3 (by-id path): the fresh trace IS present on the by-id route.
    let (_s, fresh_by_id) = trace_by_id(&rt, FRESH_TRACE_ID_HEX).await;
    assert!(
        array_len(&fresh_by_id) > 0,
        "by-id of the fresh trace returns its span; body: {fresh_by_id}"
    );

    rt.shutdown().await.expect("shutdown windowed runtime");

    // --- S3.E5: Reopen the SAME pillar root with NO window. A read-time mask
    //     would now reveal the old span; a genuine delete keeps it gone on
    //     BOTH the list and the by-id paths. ---
    let rt2 = spawn_consolidated(config(root.clone(), None))
        .await
        .expect("respawn without a window");

    let (_s, body2) = query_traces(&rt2, start_secs, end_secs).await;
    // S3.E3 (restart): the fresh span is still present on the list route.
    assert!(
        trace_id_in_results(&body2, FRESH_TRACE_ID_HEX),
        "the fresh span is still present after restart on the list route; body: {body2}"
    );
    // S3.E5 (restart, list path): the old span stays gone — it was DELETED.
    assert!(
        !trace_id_in_results(&body2, OLD_TRACE_ID_HEX),
        "the old span stays gone after restart on the list route — delete, not mask; body: {body2}"
    );

    // S3.E5 (restart, by-id path): the old trace is still absent.
    let (_s, old_by_id2) = trace_by_id(&rt2, OLD_TRACE_ID_HEX).await;
    assert_eq!(
        array_len(&old_by_id2),
        0,
        "by-id of the old trace is still empty after restart — delete-not-mask; body: {old_by_id2}"
    );

    rt2.shutdown().await.expect("shutdown reopened runtime");
    let _ = std::fs::remove_dir_all(&root);
}

// =========================================================================
// S3.E4: with NO window, an old span is still returned (keep-forever default),
// on both the list and by-id paths.
// =========================================================================
#[tokio::test(flavor = "multi_thread")]
async fn keep_forever_default() {
    let root = fresh_root("keep-forever");
    let (_now_nanos, _now_secs, old_nanos, fresh_nanos, start_secs, end_secs) = time_context();

    // No window => nothing is ever purged.
    let rt = spawn_consolidated(config(root.clone(), None))
        .await
        .expect("spawn keep-forever runtime");

    ingest_trace(&rt, encode_trace_at(OLD_TRACE_ID_BYTES, old_nanos)).await;
    ingest_trace(&rt, encode_trace_at(FRESH_TRACE_ID_BYTES, fresh_nanos)).await;

    // Poll until BOTH spans appear on the service-window route.
    let (_elapsed, status, body) = poll_until(
        Duration::from_secs(10),
        || query_traces(&rt, start_secs, end_secs),
        |s, b| s == 200 && array_len(b) >= 2,
    )
    .await;

    assert_eq!(status, 200, "traces query answers 200; body: {body}");
    assert!(
        trace_id_in_results(&body, FRESH_TRACE_ID_HEX),
        "the fresh span is returned; body: {body}"
    );
    // S3.E4: the old span is ALSO returned — keep-forever default.
    assert!(
        trace_id_in_results(&body, OLD_TRACE_ID_HEX),
        "with NO window the OLD span survives on the list route — keep-forever default; body: {body}"
    );

    // S3.E4 (by-id paths): both traces are findable by id.
    let (_s, old_by_id) = trace_by_id(&rt, OLD_TRACE_ID_HEX).await;
    assert!(
        array_len(&old_by_id) > 0,
        "old trace is findable by-id with no window; body: {old_by_id}"
    );
    let (_s, fresh_by_id) = trace_by_id(&rt, FRESH_TRACE_ID_HEX).await;
    assert!(
        array_len(&fresh_by_id) > 0,
        "fresh trace is findable by-id with no window; body: {fresh_by_id}"
    );

    rt.shutdown().await.expect("shutdown");
    let _ = std::fs::remove_dir_all(&root);
}
