// Kaleidoscope Beacon v1 — slice 03 rule-state crash-durability suite
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

//! Slice 03 — crash durability, beacon rule-state store
//! (store-fsync-durability-v0, US-06).
//!
//! The rule-state store has NO HTTP read path: the outcome is observed at
//! the store-reopen driving port (parent reopens, `load_all()` in-process).
//! Two proving mechanisms (ADR-0060 §1): (a) AC-snapshot-atomicity — a real
//! child PROCESS (`CARGO_BIN_EXE_beacon-crash-target`) `SIGKILL`ed
//! mid-snapshot, parent reopens, `open()` succeeds + the acked rule-state
//! transition is in the recovered state. (b) AC-wal-fsync — a
//! `CountingFsyncBackend` (honest `RealFsyncBackend` wrapper that delegates
//! the real fsync and counts the seam) injected through
//! `open_with_fsync_backend`: `file_fsync_count` increases per acked
//! transition, the snapshot fsyncs the file + parent dir, and the transition
//! is durable on reopen. Mirrors pulse slice 03; a lying double in the append
//! path would prove REFUSAL, not survival (DELIVER-found correction, see
//! distill/upstream-issues.md).
//!
//! I-O strategy: C (real local I/O + real child process). RED-not-BROKEN
//! (Mandate 7): every scenario `#[ignore]`d; the `open_with_fsync_backend`
//! seam, the `CountingFsyncBackend` re-export, and the `beacon-crash-target`
//! helper binary are RED scaffolds. DELIVER lifts the ignores one at a time.

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use beacon::{CountingFsyncBackend, FileBackedRuleStateStore, RuleState, RuleStateStore};

fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("beacon-crash-durability-{test_name}-{pid}-{nanos}"));
    fs::create_dir_all(&path).expect("mkdir");
    path.push("store");
    path
}

fn cleanup(base: &Path) {
    if let Some(dir) = base.parent() {
        let _ = fs::remove_dir_all(dir);
    }
}

fn firing_at(secs: u64) -> RuleState {
    RuleState::Firing {
        since: UNIX_EPOCH + Duration::from_secs(secs),
    }
}

fn spawn_crash_target_until_ready(
    pillar_root: &Path,
    mode: &str,
    timeout: Duration,
) -> std::process::Child {
    let mut child = Command::new(env!("CARGO_BIN_EXE_beacon-crash-target"))
        .arg(mode)
        .env("KALEIDOSCOPE_CRASH_PILLAR_ROOT", pillar_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn beacon-crash-target");
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
#[ignore = "RED until DELIVER: store-fsync-durability-v0 slice 06"]
fn acked_rule_transition_survives_a_mid_snapshot_crash_and_is_present_after_reopen() {
    // @driving_port @real-io @adapter-integration @US-06 @AC-snapshot-atomicity
    let base = temp_base("snapshot_atomicity");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let mut child = spawn_crash_target_until_ready(
        &pillar_root,
        "--seed-then-loop-snapshot",
        Duration::from_secs(10),
    );
    child.kill().expect("SIGKILL mid-snapshot");
    let _ = child.wait();

    let store = FileBackedRuleStateStore::open(&base)
        .expect("the store opens cleanly after a mid-snapshot crash — no torn file blocks open");
    let recovered = store.load_all().expect("recover the persisted state");
    assert!(
        matches!(recovered.get("r-payment-latency"), Some(RuleState::Firing { .. })),
        "the acked firing transition for r-payment-latency is in the recovered state; got {recovered:?}"
    );
    cleanup(&base);
}

#[test]
#[ignore = "RED until DELIVER: store-fsync-durability-v0 slice 06"]
fn canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash() {
    // @driving_port @real-io @adapter-integration @property @US-06 @AC-snapshot-atomicity
    let base = temp_base("snapshot_whole_or_absent");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let mut child = spawn_crash_target_until_ready(
        &pillar_root,
        "--seed-then-loop-snapshot",
        Duration::from_secs(10),
    );
    child.kill().expect("SIGKILL mid-snapshot");
    let _ = child.wait();

    let store = FileBackedRuleStateStore::open(&base)
        .expect("reopen finds a whole snapshot, never a torn one");
    store.load_all().expect("the recovered store loads cleanly");
    cleanup(&base);
}

// AC-recovery-regression: torn transition tail dropped, acked prefix kept.

#[test]
#[ignore = "RED until DELIVER: store-fsync-durability-v0 slice 06"]
fn a_torn_transition_tail_is_dropped_and_the_acked_prefix_is_recovered() {
    // @real-io @adapter-integration @US-06 @AC-recovery-regression
    let base = temp_base("recovery_regression");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let mut child = spawn_crash_target_until_ready(
        &pillar_root,
        "--seed-then-loop-snapshot",
        Duration::from_secs(10),
    );
    child.kill().expect("SIGKILL mid-write");
    let _ = child.wait();

    let store = FileBackedRuleStateStore::open(&base)
        .expect("reopen recovers the acked prefix past any torn transition tail");
    let recovered = store.load_all().expect("recover");
    assert!(
        matches!(
            recovered.get("r-payment-latency"),
            Some(RuleState::Firing { .. })
        ),
        "the acked transition is in the recovered prefix; got {recovered:?}"
    );
    cleanup(&base);
}

// MECHANISM (b) — AC-wal-fsync (in-suite counting substrate). Proves the
// store reaches the fsync seam per WAL append and around the snapshot
// rename. Mirrors pulse slice 03 (CountingFsyncBackend: honest delegation
// + counting), NOT a lying double injected into the append path.

#[test]
#[ignore = "RED until DELIVER: store-fsync-durability-v0 slice 06"]
fn an_acked_transition_fsyncs_the_wal_per_record_and_is_durable_on_reopen() {
    // @driving_port @real-io @adapter-integration @US-06 @AC-wal-fsync
    let base = temp_base("wal_fsync_per_append");
    let backend = Arc::new(CountingFsyncBackend::new());

    let store = FileBackedRuleStateStore::open_with_fsync_backend(&base, backend.clone())
        .expect("open with the counting-substrate seam");

    let before = backend.file_fsync_count();
    store
        .put("r-payment-latency", firing_at(1_700_000_000))
        .expect("put acks the transition");
    assert!(
        backend.file_fsync_count() > before,
        "an acked transition must fsync the WAL at least once (per-record sync_all)"
    );
    drop(store);

    let reopened = FileBackedRuleStateStore::open(&base).expect("reopen");
    let recovered = reopened.load_all().expect("recover");
    assert!(
        matches!(
            recovered.get("r-payment-latency"),
            Some(RuleState::Firing { .. })
        ),
        "the acked transition is on stable storage and recovered after reopen; got {recovered:?}"
    );
    cleanup(&base);
}

#[test]
#[ignore = "RED until DELIVER: store-fsync-durability-v0 slice 06"]
fn a_snapshot_fsyncs_the_snapshot_file_and_parent_dir_for_rename_durability() {
    // @driving_port @real-io @adapter-integration @US-06 @AC-wal-fsync
    let base = temp_base("wal_fsync_snapshot");
    let backend = Arc::new(CountingFsyncBackend::new());

    let store = FileBackedRuleStateStore::open_with_fsync_backend(&base, backend.clone())
        .expect("open with the counting-substrate seam");
    store
        .put("r-disk-pressure", firing_at(1_700_000_500))
        .expect("put acks the transition");

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

    let reopened = FileBackedRuleStateStore::open(&base).expect("reopen");
    let recovered = reopened.load_all().expect("recover");
    assert!(
        matches!(
            recovered.get("r-disk-pressure"),
            Some(RuleState::Firing { .. })
        ),
        "the snapshotted transition is durable and recovered after reopen; got {recovered:?}"
    );
    cleanup(&base);
}

// MECHANISM (b) variant — AC-substrate-refusal (out-of-process, NEGATIVE).

#[test]
#[ignore = "RED until DELIVER: store-fsync-durability-v0 slice 06"]
fn beacon_rule_state_store_refuses_to_start_on_a_substrate_that_lies_about_fsync() {
    // @real-io @adapter-integration @US-06 @AC-substrate-refusal @kpi
    let base = temp_base("substrate_refusal");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let output = Command::new(env!("CARGO_BIN_EXE_beacon-crash-target"))
        .arg("--probe-lying")
        .env("KALEIDOSCOPE_CRASH_PILLAR_ROOT", &pillar_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run beacon-crash-target in probe-lying mode");

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
        "beacon exits non-zero without opening the store for writes on a lying substrate"
    );
    cleanup(&base);
}
