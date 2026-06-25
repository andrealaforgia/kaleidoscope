// Kaleidoscope consolidated runtime — Slice S5: sustained steady-state footprint bound
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

//! # Slice S5 — "sustained steady-state footprint bound"
//!
//! Proves that the background retention sweep keeps the on-disk footprint within
//! a SUSTAINED STEADY-STATE BOUND across MANY sweep cycles under continuous
//! old-data ingest. The footprint OSCILLATES around a steady state — it does NOT
//! trend upward.
//!
//! ## The genuinely new property
//!
//! Prior slices (S1–S4) prove removal HAPPENS, not that it KEEPS PACE over a
//! long run. The regression signature S5 must catch is MONOTONIC GROWTH of the
//! post-sweep floor (the footprint trough after each cycle's sweep purges that
//! cycle's batch) across cycles: if tombstones, fragmentation, or snapshot churn
//! cause the floor to creep upward even while each one-shot check passes, S5
//! catches it.
//!
//! The LOAD-BEARING assertion is: the post-sweep floor does NOT climb from early
//! cycles to late cycles, within a generous band. A single big absolute cap at
//! the end is NOT sufficient — the trough must be sampled each cycle and the
//! SERIES must be stable.
//!
//! ## Why no production change is needed
//!
//! The sweep already calls `purge_older_than` on every tick, which calls
//! `snapshot()` (snapshot rewrite of survivors + WAL truncation). After a full
//! purge (all data is old), the stores compact to framing-overhead size. The
//! floor therefore returns to framing-overhead each cycle and should not creep.
//! This test is a characterisation of that existing property.
//!
//! ## Tests
//!
//! - `sustained_post_sweep_floor_does_not_climb_across_cycles` (S5.E1/E2/E3):
//!   10 cycles of old-data ingest + sweep (window=2s, interval=1s); the
//!   post-sweep trough series MUST NOT climb, within a generous 512-byte band.
//!   No fixed sleeps for the windowed proof — poll-to-outcome per cycle.
//! - `no_window_footprint_is_not_bounded_by_retention` (S5.E4 contrast):
//!   same ingest pattern across 10 cycles but window=None (no sweep); the
//!   footprint grows unchecked, proving it is retention that bounds the windowed
//!   test, not some unrelated cap.

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
// Test parameters — stated here and echoed in assertion messages.
//
// window = 2s: old-data batch (stamped 10s ago) ages out immediately.
// sweep interval = 1s: background sweep fires every second.
// CYCLES = 10: enough to detect monotonic growth; expected ~20–30 s total.
// PER_CYCLE_POLL_CAP = 15s: generous per-cycle cap for concurrent test load;
//   the happy path exits in ~1–2 s.
// FLOOR_BAND_BYTES = 512: the troughs are framing-overhead-sized (tens of
//   bytes); 512 bytes is generous for implementation framing churn but still
//   catches real monotonic creep (which would add hundreds of bytes per cycle,
//   not 512 bytes total across 10 cycles).
// =========================================================================

/// Number of ingest-sweep cycles to run.
const CYCLES: usize = 10;

/// Per-cycle poll cap. The happy path exits in ~1–2 s (window=2s, sweep=1s).
/// This cap only bites if the sweep never fires (a real failure, not a flake).
const PER_CYCLE_POLL_CAP: Duration = Duration::from_secs(15);

/// Interval between on-disk footprint checks inside the poll loop.
const POLL_TICK: Duration = Duration::from_millis(200);

/// The maximum drift allowed in the post-sweep floor (trough) across CYCLES.
/// Troughs are framing-overhead-sized (tens of bytes). 512 is generous for
/// framing variability yet tight enough to catch genuine monotonic creep.
const FLOOR_BAND_BYTES: u64 = 512;

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
// OTLP encoders — each takes `time_unix_nano` and a `cycle` index.
// Distinct per-cycle values / trace IDs ensure each cycle writes genuinely
// new WAL entries (not in-place updates to existing series or trace buckets).
// Mirrors the encoder patterns in slice_s4_continuous_sweep.rs.
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
        name: "kaleidoscope.s5.sustained.test".to_string(),
        version: "0.0.0".to_string(),
        attributes: vec![],
        dropped_attributes_count: 0,
    }
}

/// One `sustained_count` Sum data point stamped at `time_unix_nano`.
/// The metric value varies by `cycle` to produce distinct WAL entries.
fn encode_metric_at(time_unix_nano: u64, cycle: usize) -> Vec<u8> {
    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", "s5-sustained-test")],
                dropped_attributes_count: 0,
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: Some(scope()),
                metrics: vec![Metric {
                    name: "sustained_count".to_string(),
                    description: "s5 sustained steady-state test metric".to_string(),
                    unit: "1".to_string(),
                    metadata: vec![],
                    data: Some(MetricData::Sum(Sum {
                        data_points: vec![NumberDataPoint {
                            attributes: vec![],
                            start_time_unix_nano: time_unix_nano,
                            time_unix_nano,
                            exemplars: vec![],
                            flags: 0,
                            // Value varies per cycle so each cycle is a distinct WAL entry.
                            value: Some(number_data_point::Value::AsInt(cycle as i64 + 1)),
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

/// One log record stamped at `time_unix_nano`.
/// The body varies by `cycle` to produce distinct WAL entries.
fn encode_log_at(time_unix_nano: u64, cycle: usize) -> Vec<u8> {
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", "s5-sustained-test")],
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
                        value: Some(any_value::Value::StringValue(format!(
                            "s5-sustained-log-cycle-{cycle}"
                        ))),
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

/// One span stamped at `time_unix_nano`.
/// The trace ID and span ID vary by `cycle` so each cycle creates a new trace
/// bucket in the store (not another span in an existing trace).
fn encode_trace_at(time_unix_nano: u64, cycle: usize) -> Vec<u8> {
    // Distinct trace byte per cycle: 0x11, 0x12, 0x13, … so each cycle writes
    // to a different trace bucket and the bytes accumulate in the WAL.
    let trace_byte = 0x11u8.wrapping_add(cycle as u8);
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", "s5-sustained-test")],
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: Some(scope()),
                spans: vec![Span {
                    trace_id: vec![trace_byte; 16],
                    span_id: vec![trace_byte; 8],
                    trace_state: String::new(),
                    parent_span_id: vec![],
                    flags: 0,
                    name: format!("s5-sustained-span-cycle-{cycle}"),
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
// Each FileBacked*Store::open(base, ...) creates two sibling files under
// `pillar_root`:
//   <signal>.wal      — the write-ahead log
//   <signal>.snapshot — the compacted snapshot
//
// `purge_older_than` triggers snapshot() which rewrites the snapshot with
// only the surviving entries and truncates the WAL. After a full purge (all
// data is old), both files shrink to framing overhead — proving compaction
// keeps pace with ingest across cycles.
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
// Config builder — mirrors config_with_sweep in slice_s4/s6.
// =========================================================================

/// Build an ephemeral test config with demo overlay OFF. Sets both the
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
    p.push(format!("kal-s5-sustained-{label}-{pid}-{nanos}"));
    std::fs::create_dir_all(&p).expect("mkdir pillar root for s5 sustained test");
    p
}

// =========================================================================
// S5.E1 / S5.E2 / S5.E3 — The post-sweep floor MUST NOT climb across CYCLES.
//
// Parameters (see constants above):
//   window = 2s, sweep interval = 1s, CYCLES = 10,
//   per-cycle poll cap = 15s, floor band = 512 bytes.
//   Expected total wall-clock: ~20–30 s (2–3 s per cycle).
//
// Per-cycle flow (NO fixed sleep to wait for the sweep — poll-to-outcome only):
//   a. Ingest one old batch (metrics + logs + traces) stamped 10s in the past.
//   b. Record peak = total_on_disk right after ingest (the WAL write is fsynced
//      synchronously, so bytes are on disk when the HTTP 200 returns).
//   c. Poll (200ms tick, 15s cap) until total_on_disk < peak — i.e. the sweep
//      has fired and compacted the stores. Re-stat for trough after the drop.
//
// After all cycles, the OSCILLATION and STEADY-STATE FLOOR are asserted.
// The falsifiable regression is a climbing trough series (removal not keeping
// pace with ingest); a single absolute cap at the end is not enough — each
// cycle's trough must stay within FLOOR_BAND_BYTES of trough_0.
// =========================================================================
#[tokio::test(flavor = "multi_thread")]
async fn sustained_post_sweep_floor_does_not_climb_across_cycles() {
    let root = fresh_root("windowed");

    // window=2s, sweep interval=1s.
    let rt = spawn_consolidated(config_with_sweep(
        root.clone(),
        Some(Duration::from_secs(2)),
        Duration::from_secs(1),
    ))
    .await
    .expect("spawn runtime with window=2s + sweep-interval=1s for S5 sustained bound test");

    let base = format!("http://{}", rt.ingest_http_addr);

    let mut peaks: Vec<u64> = Vec::with_capacity(CYCLES);
    let mut troughs: Vec<u64> = Vec::with_capacity(CYCLES);

    for cycle in 0..CYCLES {
        // Stamp each batch 10s in the past — already outside the 2s window at
        // ingest time so the very next sweep tick purges it. Distinct per-cycle
        // timestamps (now_nanos() advances between cycles) and cycle-indexed
        // values/IDs ensure genuinely new WAL entries each cycle.
        let old_nanos = now_nanos().saturating_sub(10 * 1_000_000_000);

        let m_status = post_otlp(&base, "metrics", encode_metric_at(old_nanos, cycle)).await;
        assert_eq!(
            m_status, 200,
            "cycle {cycle}: metric ingest must be accepted (200)"
        );
        let l_status = post_otlp(&base, "logs", encode_log_at(old_nanos, cycle)).await;
        assert_eq!(
            l_status, 200,
            "cycle {cycle}: log ingest must be accepted (200)"
        );
        let t_status = post_otlp(&base, "traces", encode_trace_at(old_nanos, cycle)).await;
        assert_eq!(
            t_status, 200,
            "cycle {cycle}: trace ingest must be accepted (200)"
        );

        // Record the on-disk footprint IMMEDIATELY after the three ingest
        // responses (the WAL write is fsynced within the HTTP handler — bytes
        // are durable by the time we receive 200). Measuring before any sleep
        // ensures peak captures the post-ingest rise; poll-to-outcome then
        // absorbs the sweep-tick scheduling variability.
        let peak = total_on_disk(&root);
        peaks.push(peak);

        // POLL (NO fixed sleep) until the sweep fires and the footprint drops
        // below peak. The sweep fires every 1s; the old-data batch (stamped
        // 10s ago) is purged on the next tick. Poll cap = 15s per cycle —
        // generous for concurrent test load; exits in ~1–2 s on a quiet system.
        let cycle_start = std::time::Instant::now();
        while cycle_start.elapsed() < PER_CYCLE_POLL_CAP {
            tokio::time::sleep(POLL_TICK).await;
            if total_on_disk(&root) < peak {
                break;
            }
        }
        // Re-stat for the trough after the drop (the post-sweep floor for this
        // cycle). If the poll cap was hit without a drop, this equals peak,
        // which will fail the oscillation assertion below — correctly signalling
        // that the sweep did not fire within 15s for this cycle.
        let trough = total_on_disk(&root);
        troughs.push(trough);
    }

    // Print the full sampled series so oscillation-not-growth is visible in the
    // test evidence (captured in CI logs on both green and red runs).
    println!("\nS5 sustained steady-state series ({CYCLES} cycles, window=2s, sweep-interval=1s):");
    println!(
        "  {:>5}  {:>14}  {:>14}",
        "cycle", "peak (bytes)", "trough (bytes)"
    );
    for i in 0..CYCLES {
        println!("  {:>5}  {:>14}  {:>14}", i, peaks[i], troughs[i]);
    }

    // -----------------------------------------------------------------------
    // S5.E1 — OSCILLATION: each cycle's peak must exceed its own trough.
    //
    // Proves the sweep fired and compacted the stores on each cycle: removing
    // all old data drops the footprint below the post-ingest level.
    // A failing assertion means the sweep did not purge that cycle's batch
    // within the PER_CYCLE_POLL_CAP window (a real failure, not timing).
    // -----------------------------------------------------------------------
    for i in 0..CYCLES {
        assert!(
            peaks[i] > troughs[i],
            "S5.E1 oscillation: cycle {i} peak ({}) must exceed its trough ({}) — \
             the sweep must fire and compact all three stores on every cycle; \
             peak is the post-ingest footprint, trough is the post-sweep floor; \
             if RED: the sweep did not purge cycle {i}'s old-data batch within \
             the {PER_CYCLE_POLL_CAP:?} per-cycle cap (window=2s, sweep=1s)",
            peaks[i],
            troughs[i]
        );
    }

    // -----------------------------------------------------------------------
    // S5.E2 — STEADY-STATE FLOOR (per-cycle band): no trough may exceed
    // trough_0 + FLOOR_BAND_BYTES.
    //
    // This catches monotonic growth CYCLE BY CYCLE — it is the load-bearing
    // assertion. A single end-to-end cap (S5.E3) is NOT sufficient alone.
    //
    // Falsifiable regression: a climbing trough series means tombstones,
    // fragmentation, or snapshot churn are growing the floor even though the
    // sweep nominally fires each cycle. FLOOR_BAND_BYTES = 512 is generous for
    // framing-overhead variability but tight enough to catch real creep (which
    // would add hundreds of bytes per cycle, not 512 bytes across 10 cycles).
    // -----------------------------------------------------------------------
    let trough_0 = troughs[0];
    for i in 0..CYCLES {
        assert!(
            troughs[i] <= trough_0 + FLOOR_BAND_BYTES,
            "S5.E2 steady-state floor: cycle {i} trough ({}) must not exceed \
             trough_0 ({trough_0}) + FLOOR_BAND_BYTES ({FLOOR_BAND_BYTES}) = {} bytes — \
             a climbing trough series is the regression S5 was built to catch: \
             the sweep is not keeping pace (removal not matching ingest across cycles); \
             full trough series: {troughs:?}",
            troughs[i],
            trough_0 + FLOOR_BAND_BYTES,
        );
    }

    // -----------------------------------------------------------------------
    // S5.E3 — NO UPWARD TREND END-TO-END: the last trough must not exceed
    // trough_0 + FLOOR_BAND_BYTES.
    //
    // Explicit end-to-end summary assertion complementing the per-cycle band.
    // A monotonically rising trough series (removal not keeping pace) causes
    // both S5.E2 and this assertion to fail.
    // -----------------------------------------------------------------------
    let last_trough = *troughs
        .last()
        .expect("troughs vec is non-empty (CYCLES > 0)");
    assert!(
        last_trough <= trough_0 + FLOOR_BAND_BYTES,
        "S5.E3 no upward trend end-to-end: last trough ({last_trough}) must not exceed \
         trough_0 ({trough_0}) + FLOOR_BAND_BYTES ({FLOOR_BAND_BYTES}) = {} bytes — \
         the on-disk footprint MUST NOT trend upward across {CYCLES} cycles; \
         a monotonically rising trough series means the sweep is not keeping pace \
         with ingest over a sustained run (the regression S5 was built to catch); \
         full trough series: {troughs:?}",
        trough_0 + FLOOR_BAND_BYTES,
    );

    rt.shutdown()
        .await
        .expect("shutdown after sustained bound test");
    let _ = std::fs::remove_dir_all(&root);
}

// =========================================================================
// S5.E4 — No-window contrast: WITHOUT retention, the footprint GROWS.
//
// Same continuous ingest pattern across CYCLES, but window = None (no sweep
// task is spawned — no background purge fires). With nothing ever purged, the
// footprint grows unchecked. Assert that the final footprint is at least 2×
// the first — conservative for 10 cycles of equivalent-sized batches (~10×
// expected) — proving it is retention (the background sweep) that bounds the
// windowed test, not some unrelated cap (disk quota, store size limit, etc.).
//
// No sweep to wait for: ingest-and-sample per cycle, no fixed-sleep
// dependence on a sweep.
// =========================================================================
#[tokio::test(flavor = "multi_thread")]
async fn no_window_footprint_is_not_bounded_by_retention() {
    let root = fresh_root("no-window");

    // No retention window (None) => no sweep task is spawned.
    // The sweep interval is required by the builder but is irrelevant here.
    let rt = spawn_consolidated(config_with_sweep(
        root.clone(),
        None,
        Duration::from_secs(1), // irrelevant: no window => no sweep task
    ))
    .await
    .expect("spawn runtime with window=None for S5.E4 no-window contrast test");

    let base = format!("http://{}", rt.ingest_http_addr);

    let mut footprints: Vec<u64> = Vec::with_capacity(CYCLES);

    for cycle in 0..CYCLES {
        // Stamp old enough to PROVE it is retention doing the bounding in the
        // windowed test: if the same data is ingested without a window it must
        // accumulate, whereas with a window it is swept away each cycle.
        let old_nanos = now_nanos().saturating_sub(10 * 1_000_000_000);

        let m_status = post_otlp(&base, "metrics", encode_metric_at(old_nanos, cycle)).await;
        assert_eq!(
            m_status, 200,
            "cycle {cycle}: metric ingest (no window) must be accepted (200)"
        );
        let l_status = post_otlp(&base, "logs", encode_log_at(old_nanos, cycle)).await;
        assert_eq!(
            l_status, 200,
            "cycle {cycle}: log ingest (no window) must be accepted (200)"
        );
        let t_status = post_otlp(&base, "traces", encode_trace_at(old_nanos, cycle)).await;
        assert_eq!(
            t_status, 200,
            "cycle {cycle}: trace ingest (no window) must be accepted (200)"
        );

        // Sample footprint immediately after ingest. No sweep to wait for —
        // the no-window runtime has no background task. WAL writes are fsynced
        // synchronously in the HTTP handler, so bytes are on disk at 200.
        footprints.push(total_on_disk(&root));
    }

    println!("\nS5.E4 no-window contrast footprint series ({CYCLES} cycles, window=None):");
    for (i, &fp) in footprints.iter().enumerate() {
        println!("  cycle {i:>3}: {fp:>14} bytes");
    }

    let first = footprints[0];
    let last = *footprints
        .last()
        .expect("footprints vec is non-empty (CYCLES > 0)");

    // Sanity: first footprint must be non-zero (data landed on disk).
    assert!(
        first > 0,
        "S5.E4 contrast: first footprint must be non-zero after cycle 0 ingest \
         (got {first}) — the test cannot proceed if nothing was written to disk"
    );

    // Growth assertion: the final footprint must be at least 2× the first.
    // With {CYCLES} cycles of equivalent-sized batches and NO purge, the
    // footprint is expected to be ~{CYCLES}× first, so >= 2× is very
    // conservative. A failing assertion would mean the store silently compacts
    // or de-duplicates without a purge call — which would invalidate the
    // windowed proof in S5.E1–E3 (the windowed floor could be bounded by
    // something other than retention).
    assert!(
        last >= first * 2,
        "S5.E4 contrast: final footprint ({last}) must be >= 2× the first ({first}) \
         across {CYCLES} ingest cycles with no retention window (no purge ever fires); \
         if RED: the store is silently compacting without a purge call, which would \
         undermine the windowed proof — retention would not be the binding factor; \
         full footprint series: {footprints:?}"
    );

    rt.shutdown()
        .await
        .expect("shutdown after no-window contrast test");
    let _ = std::fs::remove_dir_all(&root);
}
