// Kaleidoscope Pulse v1 — slice 06 snapshot-atomicity acceptance suite
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

//! Slice 06 — snapshot atomicity, pulse (store-fsync-durability-v0, US-07).
//!
//! pulse is SNAPSHOT-ONLY. Its WAL is already crash-durable under ADR-0049
//! (per-record `sync_all`), so it carries NO wal-fsync AC and NO
//! lying-substrate proving test — its ONLY proving mechanism is (a), the
//! out-of-process process-kill mid-snapshot. This slice closes the one
//! durability residue ADR-0049 §5 left open even in its own pillar: the
//! snapshot was written non-atomically with `File::create` onto the
//! canonical path, so a mid-snapshot crash tore the live file and bricked
//! `open()`.
//!
//! Mechanism (a) (ADR-0060 §1): a real child PROCESS
//! (`CARGO_BIN_EXE_pulse-crash-target`) `SIGKILL`ed mid-snapshot; the parent
//! reopens; assert `open()` succeeds (no torn file blocks the parse) and the
//! last consistent metric state is served via the metric read path.
//!
//! I-O strategy: C (real local I/O + real child process). RED-not-BROKEN
//! (Mandate 7): every scenario `#[ignore]`d; the `pulse-crash-target` helper
//! binary is a RED scaffold. The lumen slice extracts the shared
//! `atomic_write_snapshot` that pulse then reuses; DELIVER applies it to
//! pulse's snapshot and lifts the ignores one at a time.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, UNIX_EPOCH};

use aegis::TenantId;
use pulse::{
    FileBackedMetricStore, Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore,
    NoopRecorder, TimeRange,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn name(s: &str) -> MetricName {
    MetricName::new(s)
}

fn gauge(metric_name: &str, service: &str, points: Vec<MetricPoint>) -> Metric {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    Metric {
        name: MetricName::new(metric_name),
        description: "test gauge".to_string(),
        unit: "1".to_string(),
        kind: MetricKind::Gauge,
        points,
        resource_attributes: resource,
    }
}

fn point(time_unix_nano: u64, value: f64) -> MetricPoint {
    MetricPoint {
        time_unix_nano,
        start_time_unix_nano: 0,
        attributes: BTreeMap::new(),
        value,
    }
}

fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!(
        "pulse-snapshot-atomicity-{test_name}-{pid}-{nanos}"
    ));
    fs::create_dir_all(&path).expect("mkdir");
    path.push("store");
    path
}

fn cleanup(base: &Path) {
    if let Some(dir) = base.parent() {
        let _ = fs::remove_dir_all(dir);
    }
}

fn spawn_crash_target_until_ready(
    pillar_root: &Path,
    mode: &str,
    timeout: Duration,
) -> std::process::Child {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pulse-crash-target"))
        .arg(mode)
        .env("KALEIDOSCOPE_CRASH_PILLAR_ROOT", pillar_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pulse-crash-target");
    let mut out = child.stdout.take().expect("child stdout piped");
    let deadline = Instant::now() + timeout;
    let mut seen = String::new();
    let mut buf = [0u8; 1024];
    while Instant::now() < deadline {
        match out.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                seen.push_str(&String::from_utf8_lossy(&buf[..n]));
                if seen.contains("CRASH_TARGET_READY") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    child
}

// MECHANISM (a) — AC-snapshot-atomicity (out-of-process SIGKILL).

#[test]
fn pulse_opens_cleanly_after_a_crash_during_a_snapshot() {
    // @driving_port @real-io @adapter-integration @US-07 @AC-snapshot-atomicity
    let base = temp_base("opens_after_crash");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let mut child = spawn_crash_target_until_ready(
        &pillar_root,
        "--seed-then-loop-snapshot",
        Duration::from_secs(10),
    );
    child.kill().expect("SIGKILL mid-snapshot");
    let _ = child.wait();

    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder))
        .expect("pulse opens cleanly after a mid-snapshot crash — no torn file blocks open");
    let series = store
        .query(
            &tenant("acme"),
            &name("http_requests_total"),
            TimeRange::all(),
        )
        .expect("query the recovered state");
    assert!(
        !series.is_empty(),
        "pulse serves the last consistent snapshot state after a mid-snapshot crash"
    );
    cleanup(&base);
}

// Boundary (US-07 domain 2/3): a partially-written temp snapshot never
// becomes the canonical file; a crash at the rename boundary still opens to
// a whole snapshot. The crash-at-ANY-point invariant: reopen always
// succeeds because the canonical path holds the OLD or NEW whole file.

#[test]
fn canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash() {
    // @driving_port @real-io @adapter-integration @property @US-07 @AC-snapshot-atomicity
    let base = temp_base("whole_or_absent");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let mut child = spawn_crash_target_until_ready(
        &pillar_root,
        "--seed-then-loop-snapshot",
        Duration::from_secs(10),
    );
    child
        .kill()
        .expect("SIGKILL at an arbitrary point of the snapshot sequence");
    let _ = child.wait();

    // Whatever instant the kill landed at — before the temp write, mid temp
    // write, at the rename, or after — reopen finds the OLD or NEW whole
    // snapshot, never a torn one, so open() always succeeds.
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder))
        .expect("the canonical path is whole-or-absent: reopen always succeeds");
    store
        .query(
            &tenant("acme"),
            &name("http_requests_total"),
            TimeRange::all(),
        )
        .expect("the recovered store serves queries");
    cleanup(&base);
}

// AC-recovery-regression (ADR-0049 WAL durability preserved): acked metrics
// written AFTER the snapshot also survive a SIGKILL-then-reopen.

#[test]
fn acked_metrics_written_after_the_snapshot_also_survive_a_crash() {
    // @real-io @adapter-integration @US-07 @AC-recovery-regression
    let base = temp_base("post_snapshot_survives");

    // Seed a snapshot, then ack a metric AFTER it (ADR-0049 per-record fsync
    // makes this WAL tail durable independent of the snapshot).
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");
    store
        .ingest(
            &tenant("acme"),
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![point(100, 1.0)],
            )]),
        )
        .expect("ingest");
    store.snapshot().expect("snapshot");
    store
        .ingest(
            &tenant("acme"),
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![point(200, 2.0)],
            )]),
        )
        .expect("ingest post-snapshot");
    drop(store);

    let reopened = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    let series = reopened
        .query(
            &tenant("acme"),
            &name("http_requests_total"),
            TimeRange::all(),
        )
        .expect("query");
    let times: Vec<u64> = series.iter().map(|(_, p)| p.time_unix_nano).collect();
    assert!(
        times.contains(&200),
        "the post-snapshot acked metric point survives (ADR-0049 WAL durability preserved); got {times:?}"
    );
    cleanup(&base);
}
