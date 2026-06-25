// Kaleidoscope consolidated runtime — Slice S1: metrics age-based retention
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

//! # Slice S1 — "old metrics age out of the range query" (B1 / S1).
//!
//! Drives the RUNNING consolidated runtime through its real driving ports (the
//! OTLP ingest HTTP listener + the metrics `query_range` listener) on ephemeral
//! loopback, with the demo overlay OFF so the routers read the RAW store only.
//!
//! - `windowed_query_returns_only_the_fresh_metric_and_the_delete_is_real`
//!   reproduces the Analyst's falsifiable demo (S1.E2/E3/E5) AND proves the
//!   absence is a genuine DELETE, not a read-time mask (S1.E6): after the
//!   windowed runtime purges the old sample, a SECOND runtime reopened on the
//!   same pillar root with NO window still does not return it. A read-time mask
//!   would let it reappear once the filter is gone; it does not, so it was
//!   really removed from the store.
//! - `keep_forever_default_still_returns_the_old_metric` shows that with no
//!   window configured nothing is purged (S1.E4).

mod common;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::{metrics_contains_value, metrics_result_len, poll_until, post_otlp, TENANT_ACME};
use kaleidoscope_runtime::{spawn_consolidated, ConsolidatedConfig, RunningRuntime};

use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::metrics::v1::{
    metric::Data as MetricData, number_data_point, Metric, NumberDataPoint, ResourceMetrics,
    ScopeMetrics, Sum,
};
use opentelemetry_proto::tonic::resource::v1::Resource;
use prost::Message;

const METRIC_NAME: &str = "request_count";
const SERVICE_NAME: &str = "checkout";
/// The retention window under test.
const WINDOW: Duration = Duration::from_secs(60);

/// Real wall-clock now, in epoch nanos and seconds. The runtime enforces
/// retention against its own [`SystemClock`], so the test telemetry is stamped
/// relative to real now: an "old" sample 300s back (well outside a 60s window)
/// and a "fresh" sample 5s back (well inside).
fn now() -> (u64, u64) {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock");
    (d.as_nanos() as u64, d.as_secs())
}

fn string_kv(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_string())),
        }),
    }
}

/// One `request_count` Sum point with integer `value` stamped at
/// `time_unix_nano`, under `service.name=checkout`.
fn encode_metric_at(time_unix_nano: u64, value: i64) -> Vec<u8> {
    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", SERVICE_NAME)],
                dropped_attributes_count: 0,
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: Some(InstrumentationScope {
                    name: "kaleidoscope.retention.test".to_string(),
                    version: "0.0.0".to_string(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                metrics: vec![Metric {
                    name: METRIC_NAME.to_string(),
                    description: "retention test sum".to_string(),
                    unit: "1".to_string(),
                    metadata: vec![],
                    data: Some(MetricData::Sum(Sum {
                        data_points: vec![NumberDataPoint {
                            attributes: vec![],
                            start_time_unix_nano: time_unix_nano,
                            time_unix_nano,
                            exemplars: vec![],
                            flags: 0,
                            value: Some(number_data_point::Value::AsInt(value)),
                        }],
                        aggregation_temporality: 2,
                        is_monotonic: true,
                    })),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec()
}

fn config(root: std::path::PathBuf, retention: Option<Duration>) -> ConsolidatedConfig {
    let mut c = ConsolidatedConfig::for_ephemeral_test(root, TENANT_ACME);
    // Read the RAW store only — no synthetic demo `request_count` to confound
    // the assertion about what the operator actually ingested.
    c.demo_overlay_enabled = false;
    c.retention_max_age = retention;
    c
}

/// GET `/api/v1/query_range` for `request_count` over `[start, end]` epoch secs.
async fn query_range(rt: &RunningRuntime, start_secs: u64, end_secs: u64) -> (u16, String) {
    let url = format!(
        "http://{}/api/v1/query_range?query={METRIC_NAME}&start={start_secs}&end={end_secs}",
        rt.metrics_query_addr
    );
    let resp = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .expect("GET query_range over loopback");
    let status = resp.status().as_u16();
    let body = resp.text().await.expect("read body");
    (status, body)
}

async fn ingest(rt: &RunningRuntime, body: Vec<u8>) {
    let base = format!("http://{}", rt.ingest_http_addr);
    let status = post_otlp(&base, "metrics", body).await;
    assert_eq!(status, 200, "ingest endpoint accepts the metric push");
}

/// A unique pillar root for this test process.
fn fresh_root(label: &str) -> std::path::PathBuf {
    let (nanos, _) = now();
    let mut p = std::env::temp_dir();
    p.push(format!(
        "kal-s1-retention-{label}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&p).expect("mkdir pillar root");
    p
}

#[tokio::test(flavor = "multi_thread")]
async fn windowed_query_returns_only_the_fresh_metric_and_the_delete_is_real() {
    let root = fresh_root("windowed");
    let (now_nanos, now_secs) = now();
    let old_nanos = now_nanos - 300 * 1_000_000_000; // 300s old -> outside 60s window
    let fresh_nanos = now_nanos - 5 * 1_000_000_000; // 5s old   -> inside 60s window
    let (start, end) = (now_secs - 3_600, now_secs + 3_600); // window covers BOTH samples

    // --- Runtime up WITH a 60s window. ---
    let rt = spawn_consolidated(config(root.clone(), Some(WINDOW)))
        .await
        .expect("spawn windowed runtime");

    ingest(&rt, encode_metric_at(old_nanos, 1)).await; // the OLD sample (value 1)
    ingest(&rt, encode_metric_at(fresh_nanos, 2)).await; // the FRESH sample (value 2)

    let (_elapsed, status, body) = poll_until(
        Duration::from_secs(10),
        || query_range(&rt, start, end),
        |s, b| s == 200 && metrics_result_len(b) > 0,
    )
    .await;

    assert_eq!(status, 200, "query_range answers 200; body: {body}");
    assert!(
        metrics_contains_value(&body, "2"),
        "the fresh sample (value 2, 5s old) is returned; body: {body}"
    );
    assert!(
        !metrics_contains_value(&body, "1"),
        "the old sample (value 1, 300s old) is purged out of the window; body: {body}"
    );

    rt.shutdown().await.expect("shutdown windowed runtime");

    // --- Reopen the SAME pillar root with NO window (keep-forever). A read-time
    //     mask would now reveal the old sample; a genuine delete keeps it gone. ---
    let rt2 = spawn_consolidated(config(root.clone(), None))
        .await
        .expect("respawn without a window");

    let (_s, body2) = query_range(&rt2, start, end).await;
    assert!(
        metrics_contains_value(&body2, "2"),
        "the fresh sample is still present after restart; body: {body2}"
    );
    assert!(
        !metrics_contains_value(&body2, "1"),
        "the old sample stays gone even with NO window — it was really deleted, not masked; body: {body2}"
    );

    rt2.shutdown().await.expect("shutdown reopened runtime");
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test(flavor = "multi_thread")]
async fn keep_forever_default_still_returns_the_old_metric() {
    let root = fresh_root("keep-forever");
    let (now_nanos, now_secs) = now();
    let old_nanos = now_nanos - 300 * 1_000_000_000;
    let fresh_nanos = now_nanos - 5 * 1_000_000_000;
    let (start, end) = (now_secs - 3_600, now_secs + 3_600);

    // No window configured => nothing is ever purged.
    let rt = spawn_consolidated(config(root.clone(), None))
        .await
        .expect("spawn keep-forever runtime");

    ingest(&rt, encode_metric_at(old_nanos, 1)).await;
    ingest(&rt, encode_metric_at(fresh_nanos, 2)).await;

    let (_elapsed, status, body) = poll_until(
        Duration::from_secs(10),
        || query_range(&rt, start, end),
        |s, b| s == 200 && metrics_result_len(b) > 0,
    )
    .await;

    assert_eq!(status, 200, "query_range answers 200; body: {body}");
    assert!(
        metrics_contains_value(&body, "2"),
        "the fresh sample is returned; body: {body}"
    );
    assert!(
        metrics_contains_value(&body, "1"),
        "with NO window the OLD sample survives — keep-forever default; body: {body}"
    );

    rt.shutdown().await.expect("shutdown");
    let _ = std::fs::remove_dir_all(&root);
}
