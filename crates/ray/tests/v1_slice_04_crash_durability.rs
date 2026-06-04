// Kaleidoscope Ray v1 — slice 04 crash-durability acceptance suite
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

//! Slice 04 — crash durability, ray (store-fsync-durability-v0, US-02).
//!
//! Two proving mechanisms (ADR-0060 §1; brief "For Acceptance Designer"):
//! (a) AC-snapshot-atomicity — a real child PROCESS
//! (`CARGO_BIN_EXE_ray-crash-target`) `SIGKILL`ed mid-snapshot; the parent
//! reopens and asserts `open()` succeeds + the acked span is served via the
//! trace read path. (b) AC-wal-fsync — a `CountingFsyncBackend` (honest
//! `RealFsyncBackend` wrapper that delegates the real fsync and counts the
//! seam) injected through `FileBackedTraceStore::open_with_fsync_backend`:
//! `file_fsync_count` increases per acked append, the snapshot fsyncs the
//! file + parent dir, and the data is durable on reopen (the mechanism that
//! distinguishes flush from sync_all). Mirrors pulse slice 03; a lying
//! double injected into the append path would prove REFUSAL, not survival
//! (DELIVER-found correction, see distill/upstream-issues.md).
//!
//! I-O strategy: C (real local I/O + real child process). RED-not-BROKEN
//! (Mandate 7): every scenario `#[ignore]`d; the `open_with_fsync_backend`
//! seam, the `CountingFsyncBackend` re-export, and the `ray-crash-target`
//! helper binary are RED scaffolds. DELIVER lifts the ignores one at a time.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use aegis::TenantId;
use ray::{
    CountingFsyncBackend, FileBackedTraceStore, NoopRecorder, Span, SpanBatch, SpanId, SpanKind,
    SpanStatus, TraceId, TraceStore,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn span(trace: u8, span_byte: u8, service: &str, name: &str, start: u64) -> Span {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    Span {
        trace_id: TraceId([trace; 16]),
        span_id: SpanId([span_byte; 8]),
        parent_span_id: None,
        name: name.to_string(),
        kind: SpanKind::Server,
        start_time_unix_nano: start,
        end_time_unix_nano: start + 10,
        status: SpanStatus::default(),
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        events: Vec::new(),
        links: Vec::new(),
    }
}

fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("ray-crash-durability-{test_name}-{pid}-{nanos}"));
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
    let mut child = Command::new(env!("CARGO_BIN_EXE_ray-crash-target"))
        .arg(mode)
        .env("KALEIDOSCOPE_CRASH_PILLAR_ROOT", pillar_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ray-crash-target");
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
fn acked_span_survives_a_mid_snapshot_crash_and_is_queryable_after_restart() {
    // @driving_port @real-io @adapter-integration @US-02 @AC-snapshot-atomicity
    let base = temp_base("snapshot_atomicity");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let mut child = spawn_crash_target_until_ready(
        &pillar_root,
        "--seed-then-loop-snapshot",
        Duration::from_secs(10),
    );
    child.kill().expect("SIGKILL the crash target mid-snapshot");
    let _ = child.wait();

    let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder))
        .expect("the store opens cleanly after a mid-snapshot crash — no torn file blocks open");
    let recovered = store
        .get_trace(&tenant("acme"), &TraceId([0x4b; 16]))
        .expect("get_trace the recovered state");
    let names: Vec<&str> = recovered.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"POST /checkout"),
        "the acked span is present after restart; got {names:?}"
    );
    cleanup(&base);
}

#[test]
fn canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash() {
    // @driving_port @real-io @adapter-integration @property @US-02 @AC-snapshot-atomicity
    let base = temp_base("snapshot_whole_or_absent");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let mut child = spawn_crash_target_until_ready(
        &pillar_root,
        "--seed-then-loop-snapshot",
        Duration::from_secs(10),
    );
    child.kill().expect("SIGKILL mid-snapshot");
    let _ = child.wait();

    // The kill can land at any instant; reopen ALWAYS succeeds because the
    // canonical path holds the OLD or NEW whole snapshot, never a torn one.
    let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder))
        .expect("reopen finds a whole snapshot, never a torn one");
    store
        .get_trace(&tenant("acme"), &TraceId([0x4b; 16]))
        .expect("the recovered store serves queries");
    cleanup(&base);
}

// AC-recovery-regression (kept SIGKILL+read, re-labelled): torn never-acked
// tail dropped, acked prefix kept, event=wal.recovery.torn_tail_dropped
// pillar=ray. Recovery guard, NOT the wal-fsync proof.

#[test]
fn only_acked_spans_are_recovered_after_a_torn_tail_crash() {
    // @real-io @adapter-integration @US-02 @AC-recovery-regression
    let base = temp_base("recovery_regression");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let mut child = spawn_crash_target_until_ready(
        &pillar_root,
        "--seed-then-loop-snapshot",
        Duration::from_secs(10),
    );
    child.kill().expect("SIGKILL mid-write");
    let _ = child.wait();

    let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder))
        .expect("reopen recovers the acked prefix past any torn tail");
    let recovered = store
        .get_trace(&tenant("acme"), &TraceId([0x4b; 16]))
        .expect("get_trace");
    let names: Vec<&str> = recovered.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"POST /checkout"),
        "the acked span is in the recovered prefix; got {names:?}"
    );
    cleanup(&base);
}

// MECHANISM (b) — AC-wal-fsync (in-suite counting substrate). Proves the
// store reaches the fsync seam per WAL append and around the snapshot
// rename — the mechanism that distinguishes flush() from sync_all().
// Mirrors pulse slice 03 (CountingFsyncBackend, honest delegation +
// counting), NOT a lying double injected into the append path.

#[test]
fn an_acked_append_fsyncs_the_wal_per_record_and_is_durable_on_reopen() {
    // @driving_port @real-io @adapter-integration @US-02 @AC-wal-fsync
    let base = temp_base("wal_fsync_per_append");
    let backend = Arc::new(CountingFsyncBackend::new());

    let store = FileBackedTraceStore::open_with_fsync_backend(
        &base,
        Box::new(NoopRecorder),
        backend.clone(),
    )
    .expect("open with the counting-substrate seam");

    let before = backend.file_fsync_count();
    store
        .ingest(
            &tenant("acme"),
            SpanBatch::with_spans(vec![span(0x4b, 0x00, "checkout", "POST /checkout", 100)]),
        )
        .expect("ingest acks the span");
    let after = backend.file_fsync_count();
    assert!(
        after > before,
        "an acked append must fsync the WAL at least once (per-record sync_all); \
         before={before}, after={after}"
    );
    drop(store);

    let reopened = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    let recovered = reopened
        .get_trace(&tenant("acme"), &TraceId([0x4b; 16]))
        .expect("get_trace");
    let names: Vec<&str> = recovered.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"POST /checkout"),
        "the acked span is on stable storage and queryable after reopen; got {names:?}"
    );
    cleanup(&base);
}

#[test]
fn a_snapshot_fsyncs_the_snapshot_file_and_parent_dir_for_rename_durability() {
    // @driving_port @real-io @adapter-integration @US-02 @AC-wal-fsync
    let base = temp_base("wal_fsync_snapshot");
    let backend = Arc::new(CountingFsyncBackend::new());

    let store = FileBackedTraceStore::open_with_fsync_backend(
        &base,
        Box::new(NoopRecorder),
        backend.clone(),
    )
    .expect("open with the counting-substrate seam");
    store
        .ingest(
            &tenant("acme"),
            SpanBatch::with_spans(vec![span(0x4b, 0x01, "checkout", "GET /cart", 200)]),
        )
        .expect("ingest acks the span");

    let file_before = backend.file_fsync_count();
    let dir_before = backend.dir_fsync_count();
    store.snapshot().expect("snapshot");
    assert!(
        backend.file_fsync_count() > file_before,
        "snapshot must fsync the snapshot file"
    );
    assert!(
        backend.dir_fsync_count() > dir_before,
        "snapshot must fsync the parent directory for rename durability"
    );
    drop(store);

    let reopened = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    let recovered = reopened
        .get_trace(&tenant("acme"), &TraceId([0x4b; 16]))
        .expect("get_trace");
    let names: Vec<&str> = recovered.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"GET /cart"),
        "the snapshotted span is durable and queryable after reopen; got {names:?}"
    );
    cleanup(&base);
}

// MECHANISM (b) variant — AC-substrate-refusal (out-of-process, NEGATIVE).

#[test]
fn ray_refuses_to_start_on_a_substrate_that_lies_about_fsync() {
    // @real-io @adapter-integration @US-02 @AC-substrate-refusal @kpi
    let base = temp_base("substrate_refusal");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let output = Command::new(env!("CARGO_BIN_EXE_ray-crash-target"))
        .arg("--probe-lying")
        .env("KALEIDOSCOPE_CRASH_PILLAR_ROOT", &pillar_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run ray-crash-target in probe-lying mode");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("health.startup.refused"),
        "the composition root emits event=health.startup.refused; stderr: {stderr}"
    );
    assert!(
        stderr.contains("substrate="),
        "the refusal names the substrate with substrate=<descriptor>; stderr: {stderr}"
    );
    assert!(
        !output.status.success(),
        "ray exits non-zero without binding the listener on a lying substrate"
    );
    cleanup(&base);
}
