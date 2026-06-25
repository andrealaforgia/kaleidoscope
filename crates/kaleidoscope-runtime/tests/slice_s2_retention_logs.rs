// Kaleidoscope consolidated runtime — Slice S2: log age-based retention
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

//! # Slice S2 — "old logs age out of the range query" (B1 / S2).
//!
//! Drives the RUNNING consolidated runtime through its real driving ports on
//! ephemeral loopback. The demo overlay is OFF so the routers read the RAW
//! store only (no synthetic demo logs to confound the assertions).
//!
//! - `windowed_then_restart_no_window` verifies S2.E2/E3/E5/E6:
//!   ingest old + fresh; with a 60s window, query returns ONLY the fresh log;
//!   after a restart with NO window the old log is STILL absent (genuine delete,
//!   not a read-time mask; S2.E5).
//! - `keep_forever_default` verifies S2.E4: with no window, an old log is
//!   still returned alongside a fresh one.

mod common;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::{array_len, poll_until, post_otlp, TENANT_ACME};
use kaleidoscope_runtime::{spawn_consolidated, ConsolidatedConfig, RunningRuntime};

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use opentelemetry_proto::tonic::resource::v1::Resource;
use prost::Message;

/// The retention window under test (S2.E6).
const WINDOW: Duration = Duration::from_secs(60);

/// Body text for the sample that should be PURGED (stamped 300s old).
const OLD_BODY: &str = "old-log";
/// Body text for the sample that should SURVIVE (stamped 5s old).
const FRESH_BODY: &str = "fresh-log";

// =========================================================================
// Time arithmetic — factored into one helper to avoid duplication
// =========================================================================

/// Returns `(now_nanos, now_secs, old_nanos, fresh_nanos, start_secs, end_secs)`.
///
/// - `old_nanos`  = now − 300 s in nanos  (well outside the 60 s window).
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
// OTLP log encoder
// =========================================================================

fn string_kv(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_string())),
        }),
    }
}

/// Encode a single OTLP log record with `observed_time_unix_nano` set to
/// `observed_nano` and `body` set to `body`. All other fields are minimal.
fn encode_log_at(observed_nano: u64, body: &str) -> Vec<u8> {
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", "retention-test")],
                dropped_attributes_count: 0,
            }),
            scope_logs: vec![ScopeLogs {
                scope: Some(InstrumentationScope {
                    name: "kaleidoscope.retention.test".to_string(),
                    version: "0.0.0".to_string(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                log_records: vec![LogRecord {
                    time_unix_nano: observed_nano,
                    observed_time_unix_nano: observed_nano,
                    severity_number: 9, // INFO
                    severity_text: "INFO".to_string(),
                    body: Some(AnyValue {
                        value: Some(any_value::Value::StringValue(body.to_string())),
                    }),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                    flags: 0,
                    trace_id: vec![],
                    span_id: vec![],
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

async fn ingest_log(rt: &RunningRuntime, body_bytes: Vec<u8>) {
    let base = format!("http://{}", rt.ingest_http_addr);
    let status = post_otlp(&base, "logs", body_bytes).await;
    assert_eq!(status, 200, "ingest endpoint accepts the log push");
}

/// GET `/api/v1/logs?start={start_secs}&end={end_secs}` and return
/// `(status, body_json)`.
async fn query_logs(rt: &RunningRuntime, start_secs: u64, end_secs: u64) -> (u16, String) {
    let url = format!(
        "http://{}/api/v1/logs?start={start_secs}&end={end_secs}",
        rt.logs_query_addr
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .expect("GET /api/v1/logs over loopback");
    let status = resp.status().as_u16();
    let text = resp.text().await.expect("read body");
    (status, text)
}

/// Return `true` when ANY record in the bare-JSON-array response has a `body`
/// field that contains `search` as a substring.
fn body_in_results(response_json: &str, search: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(response_json)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .map(|records| {
            records.iter().any(|r| {
                r["body"]
                    .as_str()
                    .map(|b| b.contains(search))
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
        "kal-s2-retention-{label}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&p).expect("mkdir pillar root");
    p
}

// =========================================================================
// S2.E2 / E3 / E5 / E6: windowed runtime purges old log; restart with no
// window confirms the delete is real, not a read-time mask.
// =========================================================================
#[tokio::test(flavor = "multi_thread")]
async fn windowed_then_restart_no_window() {
    let root = fresh_root("windowed");
    let (_now_nanos, _now_secs, old_nanos, fresh_nanos, start_secs, end_secs) = time_context();

    // --- Runtime up WITH a 60 s window (S2.E1 / S2.E6). ---
    let rt = spawn_consolidated(config(root.clone(), Some(WINDOW)))
        .await
        .expect("spawn windowed runtime");

    ingest_log(&rt, encode_log_at(old_nanos, OLD_BODY)).await;
    ingest_log(&rt, encode_log_at(fresh_nanos, FRESH_BODY)).await;

    // Poll until the fresh record appears (live-visibility guarantee).
    let (_elapsed, status, body) = poll_until(
        Duration::from_secs(10),
        || query_logs(&rt, start_secs, end_secs),
        |s, b| s == 200 && array_len(b) > 0,
    )
    .await;

    assert_eq!(status, 200, "logs query answers 200; body: {body}");
    // S2.E3: the in-window (fresh) record IS returned unchanged.
    assert!(
        body_in_results(&body, FRESH_BODY),
        "the fresh log (5 s old) is returned; body: {body}"
    );
    // S2.E2: the out-of-window (old) record IS NOT returned.
    assert!(
        !body_in_results(&body, OLD_BODY),
        "the old log (300 s old) is purged out of the 60 s window; body: {body}"
    );

    rt.shutdown().await.expect("shutdown windowed runtime");

    // --- S2.E5: Reopen the SAME pillar root with NO window. A read-time mask
    //     would now reveal the old log; a genuine delete keeps it gone. ---
    let rt2 = spawn_consolidated(config(root.clone(), None))
        .await
        .expect("respawn without a window");

    let (_s, body2) = query_logs(&rt2, start_secs, end_secs).await;
    // S2.E3: fresh log is still present.
    assert!(
        body_in_results(&body2, FRESH_BODY),
        "the fresh log is still present after restart; body: {body2}"
    );
    // S2.E5: old log is still absent — it was DELETED, not masked.
    assert!(
        !body_in_results(&body2, OLD_BODY),
        "the old log stays gone even with NO window — it was really deleted, not masked; body: {body2}"
    );

    rt2.shutdown().await.expect("shutdown reopened runtime");
    let _ = std::fs::remove_dir_all(&root);
}

// =========================================================================
// S2.E4: with NO window, an old log is still returned (keep-forever default).
// =========================================================================
#[tokio::test(flavor = "multi_thread")]
async fn keep_forever_default() {
    let root = fresh_root("keep-forever");
    let (_now_nanos, _now_secs, old_nanos, fresh_nanos, start_secs, end_secs) = time_context();

    // No window => nothing is ever purged.
    let rt = spawn_consolidated(config(root.clone(), None))
        .await
        .expect("spawn keep-forever runtime");

    ingest_log(&rt, encode_log_at(old_nanos, OLD_BODY)).await;
    ingest_log(&rt, encode_log_at(fresh_nanos, FRESH_BODY)).await;

    let (_elapsed, status, body) = poll_until(
        Duration::from_secs(10),
        || query_logs(&rt, start_secs, end_secs),
        |s, b| s == 200 && array_len(b) >= 2,
    )
    .await;

    assert_eq!(status, 200, "logs query answers 200; body: {body}");
    assert!(
        body_in_results(&body, FRESH_BODY),
        "the fresh log is returned; body: {body}"
    );
    // S2.E4: the old log is ALSO returned — keep-forever default.
    assert!(
        body_in_results(&body, OLD_BODY),
        "with NO window the OLD log survives — keep-forever default; body: {body}"
    );

    rt.shutdown().await.expect("shutdown");
    let _ = std::fs::remove_dir_all(&root);
}
