// Kaleidoscope CLI — Cinder side of --observe-otlp wiring acceptance test
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

//! # Acceptance tests — Cinder events on the `--observe-otlp` sink
//!
//! When the operator passes `--observe-otlp <path>` to `ingest`, the
//! Cinder recorder is wired (alongside the existing Lumen writer) so
//! that every `cinder.place(...)` call inside the ingest loop appends
//! one `cinder.place.count` OTLP-JSON line to the same file the Lumen
//! writer already appends `lumen.ingest.count` lines to.
//!
//! These tests drive the user-visible outcome of feature
//! `cli-cinder-otlp-wiring-v0`:
//!
//! - **US-01 / OK7**: `cinder.place.count` lines appear in the
//!   operator's existing sink, one per batch flush, alongside the
//!   existing Lumen lines (test #1).
//! - **US-01 / OK6 (principal)**: when both writers emit
//!   concurrently against handles onto the same file, the resulting
//!   byte stream remains valid line-by-line NDJSON terminated by `\n`
//!   (test #2 — concurrent, test #3 — sequential sibling).
//! - **US-01 / OK7 negative**: when the flag is absent, no file is
//!   created (test #4).
//! - **Compile-time witness**: `CinderToOtlpJsonWriter<File>` is
//!   `Send + Sync`, which is what makes test #2 possible (test #5).
//!
//! Note on metric names: the wire-format Lumen metric name produced
//! by `LumenToOtlpJsonWriter::record_ingest` is `lumen.ingest.count`
//! (see `crates/self-observe/src/lumen_otlp_json.rs:194` and the
//! sibling test file `tests/observe_otlp_flag.rs:105`). Several
//! feature-doc snippets refer to it as `lumen.batches.ingested.count`;
//! the wire format wins. The Cinder name is `cinder.place.count`
//! (locked by ADR-0039 §2).

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{MetricsRecorder as CinderRecorder, Tier};
use kaleidoscope_cli::{ingest, DEFAULT_BATCH_SIZE};
use lumen::{LogRecord, MetricsRecorder as LumenRecorder, SeverityNumber};
use self_observe::{CinderToOtlpJsonWriter, LumenToOtlpJsonWriter};
use serde_json::Value;

// --------------------------------------------------------------------
// Helpers (mirror observe_otlp_flag.rs; rule-of-three deferral —
// extraction to tests/common.rs becomes warranted when a third test
// file lands, per DISCUSS D4).
// --------------------------------------------------------------------

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn record(observed: u64, body: &str) -> LogRecord {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    LogRecord {
        observed_time_unix_nano: observed,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO".to_string(),
        body: body.to_string(),
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        trace_id: None,
        span_id: None,
    }
}

fn temp_root(name: &str) -> PathBuf {
    let mut p = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    p.push(format!("kal-cli-otlp-cinder-{name}-{pid}-{nanos}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn cleanup(p: &std::path::Path) {
    let _ = fs::remove_dir_all(p);
}

fn ndjson(records: &[LogRecord]) -> String {
    records
        .iter()
        .map(|r| serde_json::to_string(r).expect("serialise"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Read the file, split on `\n`, return all non-empty lines parsed
/// as `serde_json::Value`. Used by every test below for the
/// per-line-JSON-validity check.
fn parse_ndjson_lines(path: &std::path::Path) -> Vec<Value> {
    let content = fs::read_to_string(path).expect("read otlp file");
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("each non-empty line parses as JSON"))
        .collect()
}

/// True if this OTLP-JSON line carries the given metric name in the
/// locked single-metric position `scopeMetrics[0].metrics[0].name`
/// (ADR-0039 §2).
fn line_has_metric(v: &Value, name: &str) -> bool {
    v["scopeMetrics"][0]["metrics"][0]["name"] == name
}

// --------------------------------------------------------------------
// Test #1 — Happy path: ingest with --observe-otlp produces both
// Cinder AND Lumen lines on the same sink (OK7 + OK7-Lumen-coexist).
// --------------------------------------------------------------------

#[test]
fn ingest_with_observe_otlp_emits_cinder_place_and_lumen_ingest_lines_per_batch() {
    // Given Priya invokes `ingest` with `--observe-otlp /tmp/.../otlp.ndjson`,
    // 6 records for tenant `acme`, batch size 3 (→ 2 batch flushes).
    let root = temp_root("happy_path");
    let data = root.join("data");
    let otlp = root.join("otlp.ndjson");
    let records: Vec<LogRecord> = (0..6u64).map(|i| record(i, "x")).collect();

    // When the call returns Ok.
    let stats = ingest(
        &tenant("acme"),
        &data,
        3,
        Cursor::new(ndjson(&records).into_bytes()),
        Some(&otlp),
    )
    .expect("ingest");
    assert_eq!(
        stats.batches_flushed, 2,
        "6 records / batch_size 3 = 2 flushes"
    );

    // Then the file contains 4 non-empty lines (2 Lumen + 2 Cinder).
    // Order between Lumen and Cinder lines within a batch is
    // unspecified by the spec; assert SET-containment of 2 of each
    // metric name, plus total count.
    let lines = parse_ndjson_lines(&otlp);
    assert_eq!(lines.len(), 4, "two batches → 2 Lumen + 2 Cinder = 4 lines");

    let cinder_lines: Vec<&Value> = lines
        .iter()
        .filter(|v| line_has_metric(v, "cinder.place.count"))
        .collect();
    let lumen_lines: Vec<&Value> = lines
        .iter()
        .filter(|v| line_has_metric(v, "lumen.ingest.count"))
        .collect();

    assert_eq!(
        cinder_lines.len(),
        2,
        "one cinder.place.count line per batch flush"
    );
    assert_eq!(
        lumen_lines.len(),
        2,
        "one lumen.ingest.count line per batch flush"
    );

    // Cinder lines: tenant_id="acme", scope="kaleidoscope.cinder",
    // asInt="1", tier="hot" (ingest loop places under Hot,
    // crates/kaleidoscope-cli/src/lib.rs:228).
    for v in &cinder_lines {
        assert_eq!(
            v["resource"]["attributes"][0]["value"]["stringValue"], "acme",
            "resource tenant_id is the ingest tenant"
        );
        assert_eq!(
            v["scopeMetrics"][0]["scope"]["name"], "kaleidoscope.cinder",
            "scope is kaleidoscope.cinder (ADR-0039 §2)"
        );
        let dp = &v["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0];
        assert_eq!(dp["asInt"], "1", "each place call carries asInt=\"1\"");

        // Tier attribute must be present and equal to "hot".
        let attrs = dp["attributes"]
            .as_array()
            .expect("dataPoints[0].attributes is an array");
        let tier_attr = attrs
            .iter()
            .find(|a| a["key"] == "tier")
            .expect("each place line has a tier point-attribute");
        assert_eq!(
            tier_attr["value"]["stringValue"], "hot",
            "ingest loop places under Tier::Hot"
        );
    }

    // Lumen lines: tenant_id="acme", asInt="3" (batch carried 3
    // records). This is the OK8-shaped assertion at the new test
    // surface; the existing observe_otlp_flag.rs continues to
    // assert it on the Lumen-only invocation as the byte-equivalence
    // probe.
    for v in &lumen_lines {
        assert_eq!(
            v["resource"]["attributes"][0]["value"]["stringValue"],
            "acme"
        );
        let dp = &v["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0];
        assert_eq!(dp["asInt"], "3", "each Lumen batch carried 3 records");
    }

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #2 — OK6 principal: cross-writer NDJSON validity under
// concurrent emission against a real File.
//
// Per ADR-0039 §7 mandate item 3: the test must include a "concurrent
// random pause" scenario. fastrand is NOT a workspace dependency
// (verified against /Cargo.toml workspace.dependencies — only
// opentelemetry-proto / prost / sha2 / serde / serde_json declared),
// so we use deterministic jitter `(i * 7) % 6` ms per iteration per
// thread. This produces enough scheduling variation to surface
// interleaving bugs without adding a new dep (per
// feedback_decide_dont_ask) and keeps the test reproducible across
// runs (which is preferable to fastrand for CI failure diagnosis
// anyway).
//
// The test drives the writers DIRECTLY rather than through `ingest`:
//   - going through two `ingest` calls would need two data dirs and
//     would conflate "what the CLI does" with "what the writers do
//     under concurrency" (DISCUSS D4 explicitly anticipates this
//     posture).
//   - the cross-writer guarantee is a property of the two writers
//     sharing one File via try_clone (DESIGN DD1), not a property of
//     the ingest loop. Isolating to the writers gives a sharper
//     failure-mode signal.
// --------------------------------------------------------------------

const CONCURRENT_EMISSIONS_PER_THREAD: usize = 100;

#[test]
fn cross_writer_ndjson_validity_under_concurrent_emissions() {
    // Given a real shared file opened with create+append (the same
    // flag set the CLI uses at crates/kaleidoscope-cli/src/lib.rs:149-152,
    // and the substrate DESIGN DD1 chose to share via File::try_clone).
    let root = temp_root("concurrent");
    let otlp = root.join("otlp.ndjson");

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&otlp)
        .expect("open shared file");
    let file_clone = file.try_clone().expect("try_clone (DESIGN DD1)");

    // Two writers, each owning its own File handle pointing at the
    // same underlying file description.
    let lumen_writer = Arc::new(LumenToOtlpJsonWriter::new(file));
    let cinder_writer = Arc::new(CinderToOtlpJsonWriter::new(file_clone));

    let acme = tenant("acme");

    // When two threads emit `CONCURRENT_EMISSIONS_PER_THREAD` events
    // each (one drives Lumen, the other drives Cinder), with
    // deterministic jitter `(i * 7) % 6` ms between calls.
    let lumen_thread = {
        let writer = Arc::clone(&lumen_writer);
        let tenant = acme.clone();
        thread::spawn(move || {
            for i in 0..CONCURRENT_EMISSIONS_PER_THREAD {
                writer.record_ingest(&tenant, 3);
                let jitter_ms = ((i * 7) % 6) as u64;
                if jitter_ms > 0 {
                    thread::sleep(Duration::from_millis(jitter_ms));
                }
            }
        })
    };
    let cinder_thread = {
        let writer = Arc::clone(&cinder_writer);
        let tenant = acme.clone();
        thread::spawn(move || {
            for i in 0..CONCURRENT_EMISSIONS_PER_THREAD {
                writer.record_place(&tenant, Tier::Hot);
                // Offset the Cinder phase against the Lumen phase
                // so the two threads' sleeps de-synchronise and
                // surface interleaving windows.
                let jitter_ms = ((i * 7 + 3) % 6) as u64;
                if jitter_ms > 0 {
                    thread::sleep(Duration::from_millis(jitter_ms));
                }
            }
        })
    };

    lumen_thread.join().expect("lumen thread");
    cinder_thread.join().expect("cinder thread");

    // Final flush is guaranteed by the writers' per-emission triple
    // (write_all + write_all(b"\n") + flush) inside their Mutex<W>
    // critical sections (ADR-0039 §2). Dropping our Arc handles to
    // the writers (at end of test) closes the inner File handles via
    // Drop; that does NOT race with the threads because the threads
    // have already joined.

    // Then every non-empty line in the file parses as a JSON value,
    // the file ends with `\n`, and the per-writer line counts match.
    let raw = fs::read_to_string(&otlp).expect("read shared file");

    assert!(
        raw.ends_with('\n'),
        "OK6 trailing-newline invariant: shared file must end with `\\n`"
    );

    let lines: Vec<&str> = raw.lines().collect();
    let non_empty: Vec<&&str> = lines.iter().filter(|l| !l.trim().is_empty()).collect();

    assert_eq!(
        non_empty.len(),
        lines.len(),
        "OK6 no-empty-line invariant: no blank lines between OTLP records"
    );

    assert_eq!(
        non_empty.len(),
        2 * CONCURRENT_EMISSIONS_PER_THREAD,
        "OK6 total-line-count invariant: every emission produced exactly one line"
    );

    let mut lumen_count = 0usize;
    let mut cinder_count = 0usize;
    for line in non_empty {
        let v: Value = serde_json::from_str(line).unwrap_or_else(|e| {
            panic!("OK6 per-line-JSON-validity invariant: line failed to parse: {e}\nline = {line}")
        });
        if line_has_metric(&v, "lumen.ingest.count") {
            lumen_count += 1;
        } else if line_has_metric(&v, "cinder.place.count") {
            cinder_count += 1;
        } else {
            panic!(
                "OK6 metric-name partition invariant: line had unexpected metric name: {}",
                v["scopeMetrics"][0]["metrics"][0]["name"]
            );
        }
    }

    assert_eq!(
        lumen_count, CONCURRENT_EMISSIONS_PER_THREAD,
        "exactly {CONCURRENT_EMISSIONS_PER_THREAD} lumen.ingest.count lines"
    );
    assert_eq!(
        cinder_count, CONCURRENT_EMISSIONS_PER_THREAD,
        "exactly {CONCURRENT_EMISSIONS_PER_THREAD} cinder.place.count lines"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #3 — OK6 sequential sibling: within-process composition of
// the two writers against a shared file produces valid NDJSON even
// without concurrency. This is the cheaper probe of test #2: if the
// sequential case ever broke, the concurrent case could never pass,
// so this isolates "writers compose" from "writers compose under
// scheduling jitter".
// --------------------------------------------------------------------

const SEQUENTIAL_EMISSIONS_PER_WRITER: usize = 5;

#[test]
fn cross_writer_ndjson_validity_under_sequential_alternation() {
    // Given a real shared file opened with create+append, with two
    // writers built over try_clone'd handles (DESIGN DD1).
    let root = temp_root("sequential");
    let otlp = root.join("otlp.ndjson");

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&otlp)
        .expect("open shared file");
    let file_clone = file.try_clone().expect("try_clone");

    let lumen_writer = LumenToOtlpJsonWriter::new(file);
    let cinder_writer = CinderToOtlpJsonWriter::new(file_clone);

    let acme = tenant("acme");

    // When the two writers alternate emissions on the same thread,
    // 5 of each (10 lines total).
    for _ in 0..SEQUENTIAL_EMISSIONS_PER_WRITER {
        lumen_writer.record_ingest(&acme, 3);
        cinder_writer.record_place(&acme, Tier::Hot);
    }

    // Drop the writers to flush + close their File handles.
    drop(lumen_writer);
    drop(cinder_writer);

    // Then every non-empty line parses, the file ends with `\n`, and
    // the per-writer line counts match.
    let raw = fs::read_to_string(&otlp).expect("read shared file");

    assert!(
        raw.ends_with('\n'),
        "trailing-newline invariant: shared file must end with `\\n`"
    );

    let non_empty: Vec<&str> = raw.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        non_empty.len(),
        2 * SEQUENTIAL_EMISSIONS_PER_WRITER,
        "each emission produced exactly one line"
    );

    let mut lumen_count = 0usize;
    let mut cinder_count = 0usize;
    for line in non_empty {
        let v: Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("per-line parse failed: {e}\nline = {line}"));
        if line_has_metric(&v, "lumen.ingest.count") {
            lumen_count += 1;
        } else if line_has_metric(&v, "cinder.place.count") {
            cinder_count += 1;
        } else {
            panic!("unexpected metric on line: {line}");
        }
    }
    assert_eq!(lumen_count, SEQUENTIAL_EMISSIONS_PER_WRITER);
    assert_eq!(cinder_count, SEQUENTIAL_EMISSIONS_PER_WRITER);

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #4 — OK7 negative: --observe-otlp absent → no file appears.
// Mirrors `no_observe_otlp_means_no_otlp_file_created` in
// observe_otlp_flag.rs (OK8 byte-equivalence on the Lumen side),
// re-asserted from the Cinder-wiring test surface to prove the new
// match-arm wiring does not accidentally create a file when the flag
// is absent.
// --------------------------------------------------------------------

#[test]
fn no_observe_otlp_means_no_file_is_created_even_after_cinder_wiring() {
    // Given Priya invokes `ingest` with `otlp_log_path = None`.
    let root = temp_root("no_flag");
    let data = root.join("data");
    let otlp_would_be = root.join("otlp.ndjson");

    // When the call returns Ok.
    let _ = ingest(
        &tenant("acme"),
        &data,
        DEFAULT_BATCH_SIZE,
        Cursor::new(ndjson(&[record(100, "x")]).into_bytes()),
        None,
    )
    .expect("ingest");

    // Then no file exists at the path the test could have specified.
    // (The new wiring must preserve the existing behaviour:
    // Cinder = NoopRecorder, Lumen = LumenToPulseRecorder, no file
    // I/O on the --observe-otlp sink path.)
    assert!(
        !otlp_would_be.exists(),
        "OTLP file must not be created when --observe-otlp is absent"
    );

    cleanup(&root);
}

// --------------------------------------------------------------------
// Test #5 — Compile-time witness: CinderToOtlpJsonWriter<File> is
// Send + Sync.
//
// This is the subtype-check layer of the Earned Trust contract
// (ADR-0039 §1 + Principle 12c). It is what makes test #2's
// `Arc<CinderToOtlpJsonWriter<File>>` legal across thread::spawn.
// Mirrors the equivalent probe in
// `crates/self-observe/tests/cinder_to_otlp_json.rs` (ADR-0039 §3)
// but specialised to the real-File substrate this feature actually
// uses, not the SharedBuf in-memory substrate the library tests
// use. The probe must compile; runtime is a no-op.
// --------------------------------------------------------------------

#[test]
fn cinder_writer_over_real_file_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<CinderToOtlpJsonWriter<std::fs::File>>();
    assert_send_sync::<LumenToOtlpJsonWriter<std::fs::File>>();
}
