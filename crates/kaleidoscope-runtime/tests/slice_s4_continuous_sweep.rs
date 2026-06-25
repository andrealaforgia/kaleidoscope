// Kaleidoscope consolidated runtime — Slice S4: continuous background retention sweep
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

//! # Slice S4 — "continuous background retention enforcement"
//!
//! Drives the RUNNING consolidated runtime through its real driving ports on
//! ephemeral loopback. The demo overlay is OFF so the routers read the RAW
//! store only.
//!
//! The CRITICAL adversarial constraint: the read-path purge (S1–S3) already
//! exists, so a data QUERY cannot prove S4 — the query itself would trigger
//! the read-path purge, not the background sweep. The acceptance tests MUST
//! prove the purge happened WITHOUT issuing any data query. The sole proof
//! hook is the ON-DISK FOOTPRINT DROP: the test owns the temp pillar root and
//! can `stat` the store files directly.
//!
//! ## Tests
//!
//! - `continuous_sweep_purges_all_signals_with_no_query` (S4.E1 / S4.E2 /
//!   S4.E5): 2 s window + 1 s sweep interval on a quiet system; ingest one
//!   OLD record per signal; issue NO data queries; after the window + a couple
//!   of sweep intervals the on-disk footprint DROPS — proven by file-system
//!   `stat`, not by query. States the enforcement interval (1 s).
//! - `no_window_means_no_sweep` (S4.E3): with NO window, no sweep task is
//!   spawned; footprint stays put.

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::{post_otlp, TENANT_ACME};
use kaleidoscope_runtime::{spawn_consolidated, ConsolidatedConfig};

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use opentelemetry_proto::tonic::metrics::v1::{
    metric::Data as MetricData, number_data_point, Metric, NumberDataPoint, ResourceMetrics,
    ScopeMetrics, Sum,
};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span};
use prost::Message;

// =========================================================================
// Time helpers
// =========================================================================

fn now_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after epoch")
        .as_nanos() as u64
}

// =========================================================================
// OTLP encoders — local, each taking an explicit `time_unix_nano`.
// Mirrors the encoder patterns in slice_s1/s2/s3.
// =========================================================================

fn string_kv(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_string())),
        }),
    }
}

fn scope() -> InstrumentationScope {
    InstrumentationScope {
        name: "kaleidoscope.s4.sweep.test".to_string(),
        version: "0.0.0".to_string(),
        attributes: vec![],
        dropped_attributes_count: 0,
    }
}

/// One `sweep_count` Sum data point stamped at `time_unix_nano`.
fn encode_metric_at(time_unix_nano: u64) -> Vec<u8> {
    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", "sweep-test")],
                dropped_attributes_count: 0,
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: Some(scope()),
                metrics: vec![Metric {
                    name: "sweep_count".to_string(),
                    description: "s4 continuous sweep test metric".to_string(),
                    unit: "1".to_string(),
                    metadata: vec![],
                    data: Some(MetricData::Sum(Sum {
                        data_points: vec![NumberDataPoint {
                            attributes: vec![],
                            start_time_unix_nano: time_unix_nano,
                            time_unix_nano,
                            exemplars: vec![],
                            flags: 0,
                            value: Some(number_data_point::Value::AsInt(42)),
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

/// One log record stamped at `time_unix_nano` (both `time_unix_nano` and
/// `observed_time_unix_nano` set so the lumen store's age check sees the
/// correct timestamp).
fn encode_log_at(time_unix_nano: u64) -> Vec<u8> {
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", "sweep-test")],
                dropped_attributes_count: 0,
            }),
            scope_logs: vec![ScopeLogs {
                scope: Some(scope()),
                log_records: vec![LogRecord {
                    time_unix_nano,
                    observed_time_unix_nano: time_unix_nano,
                    severity_number: 9, // INFO
                    severity_text: "INFO".to_string(),
                    body: Some(AnyValue {
                        value: Some(any_value::Value::StringValue(
                            "s4-continuous-sweep-log".to_string(),
                        )),
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

/// One span stamped at `time_unix_nano` on a fixed trace-id
/// (`0xcc * 16` bytes).
fn encode_trace_at(time_unix_nano: u64) -> Vec<u8> {
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", "sweep-test")],
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: Some(scope()),
                spans: vec![Span {
                    trace_id: vec![0xcc; 16],
                    span_id: vec![0xdd; 8],
                    trace_state: String::new(),
                    parent_span_id: vec![],
                    flags: 0,
                    name: "s4-sweep-span".to_string(),
                    kind: 2, // SPAN_KIND_SERVER
                    start_time_unix_nano: time_unix_nano,
                    end_time_unix_nano: time_unix_nano + 1_000,
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
// On-disk footprint: sum the byte sizes of the six store files that exist.
//
// The three stores are opened as:
//   FileBackedMetricStore::open(pillar_root.join("pulse"), ...)
//   FileBackedLogStore::open(pillar_root.join("lumen"), ...)
//   FileBackedTraceStore::open(pillar_root.join("ray"), ...)
//
// Each `FileBacked*Store::open(base, ...)` creates two files:
//   <base>.wal      — the write-ahead log
//   <base>.snapshot — the compacted snapshot
//
// So the six files are siblings directly under `pillar_root`.
// =========================================================================

fn total_on_disk(root: &Path) -> u64 {
    let candidates = [
        root.join("pulse.wal"),
        root.join("pulse.snapshot"),
        root.join("lumen.wal"),
        root.join("lumen.snapshot"),
        root.join("ray.wal"),
        root.join("ray.snapshot"),
    ];
    candidates
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum()
}

// =========================================================================
// Config builder
// =========================================================================

/// Build an ephemeral test config with demo overlay OFF. Sets both the
/// retention window and the enforcement (sweep) interval.
///
/// `retention_sweep_interval` is S4's new config field — RED until implemented.
fn config_with_sweep(
    root: PathBuf,
    max_age: Option<Duration>,
    sweep_interval: Duration,
) -> ConsolidatedConfig {
    let mut c = ConsolidatedConfig::for_ephemeral_test(root, TENANT_ACME);
    c.demo_overlay_enabled = false;
    c.retention_max_age = max_age;
    // S4 RED: `retention_sweep_interval` does not exist on `ConsolidatedConfig` yet.
    // This line causes a compile error until the GREEN phase adds the field.
    c.retention_sweep_interval = sweep_interval;
    c
}

// =========================================================================
// Unique pillar root per test
// =========================================================================

fn fresh_root(label: &str) -> PathBuf {
    let nanos = now_nanos();
    let pid = std::process::id();
    let mut p = std::env::temp_dir();
    p.push(format!("kal-s4-sweep-{label}-{pid}-{nanos}"));
    std::fs::create_dir_all(&p).expect("mkdir pillar root for s4 sweep test");
    p
}

// =========================================================================
// S4.E1 / S4.E2 / S4.E5 — Continuous sweep purges ALL THREE signals with
// NO data query, proven by on-disk footprint drop.
//
// Enforcement interval: 1 s (configured via `retention_sweep_interval`).
// Window: 2 s (`retention_max_age`).
//
// Adversarial constraint (read first): the S1–S3 read-path purge exists, so
// any data query would trigger that read-path purge, not the background sweep.
// The footprint assertion therefore comes FIRST and is the only proof; any
// subsequent query is corroborating but is NOT the load-bearing assertion.
// =========================================================================
#[tokio::test(flavor = "multi_thread")]
async fn continuous_sweep_purges_all_signals_with_no_query() {
    let root = fresh_root("sweep");

    // Stamp all three records 10 s in the past so they are already outside
    // the 2 s window at ingest time (the sweep purges them on its first tick).
    let old_nanos = now_nanos().saturating_sub(10 * 1_000_000_000);

    // 2 s window, 1 s sweep interval.
    let rt = spawn_consolidated(config_with_sweep(
        root.clone(),
        Some(Duration::from_secs(2)),
        Duration::from_secs(1),
    ))
    .await
    .expect("spawn runtime with continuous sweep enabled");

    let base = format!("http://{}", rt.ingest_http_addr);

    // Ingest one old record per signal (already outside the 2 s window).
    let m_status = post_otlp(&base, "metrics", encode_metric_at(old_nanos)).await;
    assert_eq!(m_status, 200, "metric ingest must be accepted (200)");
    let l_status = post_otlp(&base, "logs", encode_log_at(old_nanos)).await;
    assert_eq!(l_status, 200, "log ingest must be accepted (200)");
    let t_status = post_otlp(&base, "traces", encode_trace_at(old_nanos)).await;
    assert_eq!(t_status, 200, "trace ingest must be accepted (200)");

    // Give the stores a brief moment to flush WAL entries to disk.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // --- NO-READ HOOK: measure on-disk footprint BEFORE the sweep fires. ---
    let before = total_on_disk(&root);
    assert!(
        before > 0,
        "on-disk footprint must be non-zero after ingest; got {before} bytes — \
         the test cannot proceed if nothing was written to disk"
    );

    // ISSUE NO DATA QUERIES — the footprint drop is the SOLE proof of S4.
    // Sleep past the window (2 s) plus a couple of 1 s sweep intervals.
    // 4 s total guarantees at least two sweep ticks have fired.
    tokio::time::sleep(Duration::from_secs(4)).await;

    // Measure footprint AFTER the sweep has fired.
    let after = total_on_disk(&root);
    assert!(
        after < before,
        "on-disk footprint MUST DROP after the continuous sweep purges all signals \
         with NO data query (S4.E1/E2); \
         before={before} bytes, after={after} bytes — \
         if this is RED: the background sweep task has not been implemented yet"
    );

    rt.shutdown()
        .await
        .expect("shutdown after continuous sweep test");
    let _ = std::fs::remove_dir_all(&root);
}

// =========================================================================
// S4.E3 — With NO window configured, the sweep task is never spawned.
// The on-disk footprint must NOT drop: keep-forever default still holds.
// =========================================================================
#[tokio::test(flavor = "multi_thread")]
async fn no_window_means_no_sweep() {
    let root = fresh_root("no-sweep");

    let old_nanos = now_nanos().saturating_sub(10 * 1_000_000_000);

    // No retention window => sweep task is NOT spawned regardless of the
    // sweep interval.
    let rt = spawn_consolidated(config_with_sweep(
        root.clone(),
        None,
        Duration::from_secs(1),
    ))
    .await
    .expect("spawn runtime without retention window");

    let base = format!("http://{}", rt.ingest_http_addr);

    let m_status = post_otlp(&base, "metrics", encode_metric_at(old_nanos)).await;
    assert_eq!(m_status, 200, "metric ingest must be accepted (200)");
    let l_status = post_otlp(&base, "logs", encode_log_at(old_nanos)).await;
    assert_eq!(l_status, 200, "log ingest must be accepted (200)");
    let t_status = post_otlp(&base, "traces", encode_trace_at(old_nanos)).await;
    assert_eq!(t_status, 200, "trace ingest must be accepted (200)");

    tokio::time::sleep(Duration::from_millis(300)).await;

    let before = total_on_disk(&root);
    assert!(
        before > 0,
        "on-disk footprint must be non-zero after ingest; got {before} bytes"
    );

    // Wait long enough for sweeps to fire IF they were accidentally spawned.
    tokio::time::sleep(Duration::from_secs(3)).await;

    let after = total_on_disk(&root);
    assert!(
        after >= before,
        "footprint MUST NOT drop when no retention window is configured \
         (S4.E3 keep-forever default); \
         before={before} bytes, after={after} bytes"
    );

    rt.shutdown().await.expect("shutdown after no-window test");
    let _ = std::fs::remove_dir_all(&root);
}
