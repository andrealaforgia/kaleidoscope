// Kaleidoscope Sluice v1 — slice 03 crash-durability acceptance suite
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

//! Slice 03 — crash durability, sluice (store-fsync-durability-v0, US-05).
//!
//! sluice has NO HTTP read path: the outcome is observed at the
//! store-reopen driving port (parent reopens, dequeues in-process). Two
//! proving mechanisms (ADR-0060 §1): (a) AC-snapshot-atomicity — a real
//! child PROCESS (`CARGO_BIN_EXE_sluice-crash-target`) `SIGKILL`ed
//! mid-snapshot, parent reopens, `open()` succeeds + the acked enqueue is
//! present and dequeuable. (b) AC-wal-fsync — a `CountingFsyncBackend`
//! (honest `RealFsyncBackend` wrapper that delegates the real fsync and
//! counts the seam) injected through `open_with_fsync_backend`:
//! `file_fsync_count` increases per acked enqueue, the snapshot fsyncs the
//! file + parent dir, and the enqueue is durable on reopen. Mirrors pulse
//! slice 03; a lying double in the append path would prove REFUSAL, not
//! survival (DELIVER-found correction, see distill/upstream-issues.md).
//!
//! I-O strategy: C (real local I/O + real child process). RED-not-BROKEN
//! (Mandate 7): every scenario `#[ignore]`d; the `open_with_fsync_backend`
//! seam, the `CountingFsyncBackend` re-export, and the `sluice-crash-target`
//! helper binary are RED scaffolds. DELIVER lifts the ignores one at a time.

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use aegis::TenantId;
use sluice::{CountingFsyncBackend, FileBackedQueue, NoopRecorder, Queue};

const CAP: usize = 1000;

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("sluice-crash-durability-{test_name}-{pid}-{nanos}"));
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
    let mut child = Command::new(env!("CARGO_BIN_EXE_sluice-crash-target"))
        .arg(mode)
        .env("KALEIDOSCOPE_CRASH_PILLAR_ROOT", pillar_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sluice-crash-target");
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
fn acked_enqueue_survives_a_mid_snapshot_crash_and_is_dequeuable_after_reopen() {
    // @driving_port @real-io @adapter-integration @US-05 @AC-snapshot-atomicity
    let base = temp_base("snapshot_atomicity");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let mut child = spawn_crash_target_until_ready(
        &pillar_root,
        "--seed-then-loop-snapshot",
        Duration::from_secs(10),
    );
    child.kill().expect("SIGKILL mid-snapshot");
    let _ = child.wait();

    let store = FileBackedQueue::open(&base, CAP, Box::new(NoopRecorder))
        .expect("the store opens cleanly after a mid-snapshot crash — no torn file blocks open");
    let dequeued = store.dequeue(&tenant("acme")).expect("dequeue is Ok");
    let payload = dequeued.map(|m| String::from_utf8_lossy(&m.payload).into_owned());
    assert_eq!(
        payload.as_deref(),
        Some("job-5521"),
        "the acked enqueue is present and dequeuable after reopen; got {payload:?}"
    );
    cleanup(&base);
}

#[test]
fn canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash() {
    // @driving_port @real-io @adapter-integration @property @US-05 @AC-snapshot-atomicity
    let base = temp_base("snapshot_whole_or_absent");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let mut child = spawn_crash_target_until_ready(
        &pillar_root,
        "--seed-then-loop-snapshot",
        Duration::from_secs(10),
    );
    child.kill().expect("SIGKILL mid-snapshot");
    let _ = child.wait();

    let store = FileBackedQueue::open(&base, CAP, Box::new(NoopRecorder))
        .expect("reopen finds a whole snapshot, never a torn one");
    let _ = store.depth(&tenant("acme"));
    cleanup(&base);
}

// Boundary (US-05 domain 3): an in-flight item is recovered, not dropped.

#[test]
fn an_in_flight_item_is_recovered_after_a_crash_not_silently_dropped() {
    // @real-io @adapter-integration @US-05 @AC-snapshot-atomicity (boundary)
    let base = temp_base("in_flight");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let mut child = spawn_crash_target_until_ready(
        &pillar_root,
        "--seed-then-dequeue-inflight",
        Duration::from_secs(10),
    );
    child.kill().expect("SIGKILL while job-5521 is in-flight");
    let _ = child.wait();

    let store = FileBackedQueue::open(&base, CAP, Box::new(NoopRecorder))
        .expect("reopen recovers in-flight state");
    assert!(
        store.depth(&tenant("acme")) > 0,
        "the in-flight item is recovered to its pre-crash state, not silently dropped"
    );
    cleanup(&base);
}

// MECHANISM (b) — AC-wal-fsync (in-suite counting substrate). Proves the
// store reaches the fsync seam per WAL append and around the snapshot
// rename. Mirrors pulse slice 03 (CountingFsyncBackend: honest delegation
// + counting), NOT a lying double injected into the append path.

#[test]
fn an_acked_enqueue_fsyncs_the_wal_per_record_and_is_durable_on_reopen() {
    // @driving_port @real-io @adapter-integration @US-05 @AC-wal-fsync
    let base = temp_base("wal_fsync_per_append");
    let backend = Arc::new(CountingFsyncBackend::new());

    let store = FileBackedQueue::open_with_fsync_backend(
        &base,
        CAP,
        Box::new(NoopRecorder),
        backend.clone(),
    )
    .expect("open with the counting-substrate seam");

    let before = backend.file_fsync_count();
    store
        .enqueue(&tenant("acme"), b"job-5521".to_vec())
        .expect("enqueue acks the item");
    assert!(
        backend.file_fsync_count() > before,
        "an acked enqueue must fsync the WAL at least once (per-record sync_all)"
    );
    drop(store);

    let reopened = FileBackedQueue::open(&base, CAP, Box::new(NoopRecorder)).expect("reopen");
    let dequeued = reopened.dequeue(&tenant("acme")).expect("dequeue is Ok");
    let payload = dequeued.map(|m| String::from_utf8_lossy(&m.payload).into_owned());
    assert_eq!(
        payload.as_deref(),
        Some("job-5521"),
        "the acked enqueue is on stable storage and dequeuable after reopen; got {payload:?}"
    );
    cleanup(&base);
}

#[test]
fn a_snapshot_fsyncs_the_snapshot_file_and_parent_dir_for_rename_durability() {
    // @driving_port @real-io @adapter-integration @US-05 @AC-wal-fsync
    let base = temp_base("wal_fsync_snapshot");
    let backend = Arc::new(CountingFsyncBackend::new());

    let store = FileBackedQueue::open_with_fsync_backend(
        &base,
        CAP,
        Box::new(NoopRecorder),
        backend.clone(),
    )
    .expect("open with the counting-substrate seam");
    store
        .enqueue(&tenant("acme"), b"job-9002".to_vec())
        .expect("enqueue acks the item");

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

    let reopened = FileBackedQueue::open(&base, CAP, Box::new(NoopRecorder)).expect("reopen");
    let dequeued = reopened.dequeue(&tenant("acme")).expect("dequeue is Ok");
    let payload = dequeued.map(|m| String::from_utf8_lossy(&m.payload).into_owned());
    assert_eq!(
        payload.as_deref(),
        Some("job-9002"),
        "the snapshotted enqueue is durable and dequeuable after reopen; got {payload:?}"
    );
    cleanup(&base);
}

// MECHANISM (b) variant — AC-substrate-refusal (out-of-process, NEGATIVE).

#[test]
fn sluice_refuses_to_start_on_a_substrate_that_lies_about_fsync() {
    // @real-io @adapter-integration @US-05 @AC-substrate-refusal @kpi
    let base = temp_base("substrate_refusal");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let output = Command::new(env!("CARGO_BIN_EXE_sluice-crash-target"))
        .arg("--probe-lying")
        .env("KALEIDOSCOPE_CRASH_PILLAR_ROOT", &pillar_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run sluice-crash-target in probe-lying mode");

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
        "sluice exits non-zero without opening the store for writes on a lying substrate"
    );
    cleanup(&base);
}
