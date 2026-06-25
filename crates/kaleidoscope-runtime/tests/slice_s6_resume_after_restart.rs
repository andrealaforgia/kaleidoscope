// Kaleidoscope consolidated runtime — Slice S6: retention enforcement resumes after restart
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

//! # Slice S6 — "retention enforcement resumes after restart"
//!
//! Proves that the continuous background sweep is re-spawned when
//! `spawn_consolidated` is called again on the SAME pillar root with the SAME
//! config — i.e. when the runtime restarts in-process. After the restart, a
//! record INGESTED AFTER the restart that subsequently AGES PAST THE WINDOW
//! AFTER the restart MUST be purged by the resumed sweep, with NO data query.
//!
//! The demo overlay is OFF so the routers read the RAW store only.
//!
//! ## Why no production change is needed
//!
//! `spawn_consolidated` always spawns the background sweep task when
//! `retention_max_age` is `Some`. A "restart" in-process = shut down the
//! `RunningRuntime` (dropping `_retention_sweep_shutdown` which stops the old
//! sweep loop via the oneshot channel), then call `spawn_consolidated` AGAIN
//! with the SAME config on the SAME pillar root — which opens the same on-disk
//! stores and re-spawns a fresh sweep loop. No production change is required;
//! this slice is a characterization / acceptance test.
//!
//! ## Tests
//!
//! - `enforcement_resumes_after_restart_purges_post_restart_record`
//!   (S6.E1/E2/E5): 2 s window + 1 s sweep interval. Build ONE config, clone
//!   it to BOTH spawns. Spawn rt1 on empty root, ingest nothing, shut down
//!   rt1. Spawn rt2 on the SAME root (same config, no reconfiguration). Ingest
//!   one record per signal stamped at ~now (in-window at ingest; ages past the
//!   2 s window after the restart). Issue NO data queries. Poll on-disk
//!   footprint until it drops. The only thing that could have purged a
//!   post-restart, in-window record without a data query is the resumed sweep.
//! - `keep_forever_default_across_restart` (S6.E3): window = None for BOTH
//!   spawns. After restart, ingest three records; measure `before`; wait
//!   `sweep_interval * 3` (a real wait — no success event to poll for); assert
//!   footprint did NOT drop (no sweep is spawned without a window).

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
// OTLP encoders — each takes an explicit `time_unix_nano`.
// Mirror the encoder pattern from slice_s4_continuous_sweep.rs.
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
        name: "kaleidoscope.s6.resume.test".to_string(),
        version: "0.0.0".to_string(),
        attributes: vec![],
        dropped_attributes_count: 0,
    }
}

/// One `resume_count` Sum data point stamped at `time_unix_nano`.
fn encode_metric_at(time_unix_nano: u64) -> Vec<u8> {
    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", "resume-test")],
                dropped_attributes_count: 0,
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: Some(scope()),
                metrics: vec![Metric {
                    name: "resume_count".to_string(),
                    description: "s6 resume-after-restart test metric".to_string(),
                    unit: "1".to_string(),
                    metadata: vec![],
                    data: Some(MetricData::Sum(Sum {
                        data_points: vec![NumberDataPoint {
                            attributes: vec![],
                            start_time_unix_nano: time_unix_nano,
                            time_unix_nano,
                            exemplars: vec![],
                            flags: 0,
                            value: Some(number_data_point::Value::AsInt(1)),
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
                attributes: vec![string_kv("service.name", "resume-test")],
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
                            "s6-resume-after-restart-log".to_string(),
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

/// One span stamped at `time_unix_nano` on a fixed trace-id (`0xee * 16`).
fn encode_trace_at(time_unix_nano: u64) -> Vec<u8> {
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", "resume-test")],
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: Some(scope()),
                spans: vec![Span {
                    trace_id: vec![0xee; 16],
                    span_id: vec![0xee; 8],
                    trace_state: String::new(),
                    parent_span_id: vec![],
                    flags: 0,
                    name: "s6-resume-span".to_string(),
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
// Each `FileBacked*Store::open(base, ...)` creates two sibling files under
// `pillar_root`:
//   <signal>.wal      — the write-ahead log
//   <signal>.snapshot — the compacted snapshot
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

/// Build an ephemeral test config with demo overlay OFF, setting both the
/// retention window and the enforcement (sweep) interval.
fn config_with_sweep(
    root: PathBuf,
    max_age: Option<Duration>,
    sweep_interval: Duration,
) -> ConsolidatedConfig {
    let mut c = ConsolidatedConfig::for_ephemeral_test(root, TENANT_ACME);
    c.demo_overlay_enabled = false;
    c.retention_max_age = max_age;
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
    p.push(format!("kal-s6-resume-{label}-{pid}-{nanos}"));
    std::fs::create_dir_all(&p).expect("mkdir pillar root for s6 resume test");
    p
}

// =========================================================================
// S6.E1 / S6.E2 / S6.E5 — Enforcement resumes after restart and purges a
// post-restart-ingested, post-restart-aged record, with NO data query.
//
// One config value (cfg) is built once and CLONED to BOTH spawns — provably
// the same window with no reconfiguration between the first and second spawn.
//
// Window: 2 s.  Sweep interval: 1 s.  Poll cap: 30 s.
//
// Adversarial constraint: records are stamped at ~now (in-window at ingest)
// so the read-path purge (S1–S3) cannot have deleted them — that would require
// a data query, which we never issue. The sole proof is the on-disk footprint
// drop, driven ONLY by the background sweep resuming after restart.
// =========================================================================
#[tokio::test(flavor = "multi_thread")]
async fn enforcement_resumes_after_restart_purges_post_restart_record() {
    let root = fresh_root("resume");

    // Build ONE config; clone to BOTH spawns (no reconfiguration).
    // window=2s, sweep-interval=1s.
    let cfg = config_with_sweep(
        root.clone(),
        Some(Duration::from_secs(2)),
        Duration::from_secs(1),
    );

    // --- "First process": start on EMPTY root, ingest NOTHING, shut down. ---
    // S6.E2: no record exists before the restart; the post-restart proof cannot
    // rely on any pre-restart on-disk state.
    let rt1 = spawn_consolidated(cfg.clone())
        .await
        .expect("first spawn on empty root");
    rt1.shutdown()
        .await
        .expect("first shutdown (nothing ingested)");

    // --- "Second process": restart on the SAME root with the SAME config. ---
    // `spawn_consolidated` always spawns the sweep when `retention_max_age` is
    // `Some`, so the sweep re-starts here without any production-code change.
    let rt2 = spawn_consolidated(cfg.clone())
        .await
        .expect("second spawn (restart) on same root with same window");

    let base = format!("http://{}", rt2.ingest_http_addr);

    // Stamp records at ~now: IN-WINDOW at ingest time, so they must AGE past
    // the 2 s window AFTER the restart before the sweep can purge them. The
    // only thing that can purge them is the resumed sweep.
    let now = now_nanos();
    let m_status = post_otlp(&base, "metrics", encode_metric_at(now)).await;
    assert_eq!(
        m_status, 200,
        "metric ingest must be accepted (200) after restart"
    );
    let l_status = post_otlp(&base, "logs", encode_log_at(now)).await;
    assert_eq!(
        l_status, 200,
        "log ingest must be accepted (200) after restart"
    );
    let t_status = post_otlp(&base, "traces", encode_trace_at(now)).await;
    assert_eq!(
        t_status, 200,
        "trace ingest must be accepted (200) after restart"
    );

    // Brief flush pause so WAL entries land on disk before measuring.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // --- NO-READ HOOK: measure on-disk footprint BEFORE the sweep ages records. ---
    let before = total_on_disk(&root);
    assert!(
        before > 0,
        "on-disk footprint must be non-zero after post-restart ingest; got {before} bytes — \
         the test cannot proceed if nothing was written to disk"
    );

    // ISSUE NO DATA QUERIES — the footprint drop is the SOLE proof of S6.
    // Poll the on-disk footprint (a pure `stat`, never a data read) until it
    // drops. The records age past the 2 s window ~2 s after ingest; the sweep
    // fires every 1 s; the poll cap is 30 s — generous enough to survive
    // concurrent test load without racing on a fixed sleep.
    // window=2s, sweep-interval=1s, cap=30s.
    let poll_deadline = Duration::from_secs(30);
    let poll_tick = Duration::from_millis(200);
    let started = std::time::Instant::now();
    let mut after = before;
    while started.elapsed() < poll_deadline {
        after = total_on_disk(&root);
        if after < before {
            break;
        }
        tokio::time::sleep(poll_tick).await;
    }
    assert!(
        after < before,
        "on-disk footprint MUST DROP after the resumed sweep purges all three signals \
         (metric + log + trace) with NO data query (S6.E1/E2/E5); \
         before={before} bytes, after={after} bytes — \
         if this is RED: the background sweep task did not resume after restart"
    );

    rt2.shutdown()
        .await
        .expect("shutdown after resume-enforcement test");
    let _ = std::fs::remove_dir_all(&root);
}

// =========================================================================
// S6.E3 — With NO window across BOTH spawns, no sweep is ever spawned.
// The on-disk footprint must NOT drop after restart.
//
// This is a negative proof: wait sweep_interval * 3 (a real wait — there is
// no observable success event to poll for), then assert footprint is unchanged.
// =========================================================================
#[tokio::test(flavor = "multi_thread")]
async fn keep_forever_default_across_restart() {
    let root = fresh_root("no-sweep");

    // No window in either spawn: keep-forever default across both.
    let sweep_interval = Duration::from_secs(1);
    let cfg = config_with_sweep(root.clone(), None, sweep_interval);

    // First spawn (no window => no sweep task).
    let rt1 = spawn_consolidated(cfg.clone())
        .await
        .expect("first spawn (no window)");
    rt1.shutdown().await.expect("first shutdown (no window)");

    // Second spawn on same root (still no window => no sweep task re-spawned).
    let rt2 = spawn_consolidated(cfg.clone())
        .await
        .expect("second spawn / restart (no window)");

    let base = format!("http://{}", rt2.ingest_http_addr);

    let now = now_nanos();
    let m_status = post_otlp(&base, "metrics", encode_metric_at(now)).await;
    assert_eq!(m_status, 200, "metric ingest after restart (no window)");
    let l_status = post_otlp(&base, "logs", encode_log_at(now)).await;
    assert_eq!(l_status, 200, "log ingest after restart (no window)");
    let t_status = post_otlp(&base, "traces", encode_trace_at(now)).await;
    assert_eq!(t_status, 200, "trace ingest after restart (no window)");

    tokio::time::sleep(Duration::from_millis(300)).await;

    let before = total_on_disk(&root);
    assert!(
        before > 0,
        "on-disk footprint must be non-zero after post-restart ingest; got {before} bytes"
    );

    // A negative proof genuinely needs a real wait: there is no success event
    // to poll for. Derive from the sweep interval (3 ticks) — long enough that
    // a sweep WOULD have fired and dropped the footprint if one had been
    // spawned. sweep-interval=1s, wait=3s.
    tokio::time::sleep(sweep_interval * 3).await;

    let after = total_on_disk(&root);
    assert!(
        after >= before,
        "footprint MUST NOT drop when no retention window is configured across restart \
         (S6.E3 keep-forever default); \
         before={before} bytes, after={after} bytes"
    );

    rt2.shutdown()
        .await
        .expect("shutdown after no-window restart test");
    let _ = std::fs::remove_dir_all(&root);
}
