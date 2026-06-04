// Kaleidoscope Cinder v1 — slice 04 crash-durability acceptance suite
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

//! Slice 04 — crash durability, cinder (store-fsync-durability-v0, US-04).
//!
//! cinder has NO HTTP read path: the outcome is observed at the
//! store-reopen driving port (parent reopens, queries `get_tier`
//! in-process). Two proving mechanisms (ADR-0060 §1): (a)
//! AC-snapshot-atomicity — a real child PROCESS
//! (`CARGO_BIN_EXE_cinder-crash-target`) `SIGKILL`ed mid-snapshot, parent
//! reopens, `open()` succeeds + the acked migration is in the recovered
//! ledger. (b) AC-wal-fsync — a `LyingFsyncBackend` injected through
//! `open_with_fsync_backend` discards the unsynced bytes; the acked
//! migration is ABSENT on flush()-only and PRESENT once sync_all is wired.
//!
//! I-O strategy: C (real local I/O + real child process). RED-not-BROKEN
//! (Mandate 7): every scenario `#[ignore]`d; the `open_with_fsync_backend`
//! seam, the `LyingFsyncBackend` re-export, and the `cinder-crash-target`
//! helper binary are RED scaffolds. DELIVER lifts the ignores one at a time.

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{FileBackedTieringStore, ItemId, LyingFsyncBackend, NoopRecorder, Tier, TieringStore};

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
    path.push(format!("cinder-crash-durability-{test_name}-{pid}-{nanos}"));
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
    let mut child = Command::new(env!("CARGO_BIN_EXE_cinder-crash-target"))
        .arg(mode)
        .env("KALEIDOSCOPE_CRASH_PILLAR_ROOT", pillar_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cinder-crash-target");
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
#[ignore = "RED until DELIVER: store-fsync-durability-v0 slice 04"]
fn acked_migration_survives_a_mid_snapshot_crash_and_is_present_after_reopen() {
    // @driving_port @real-io @adapter-integration @US-04 @AC-snapshot-atomicity
    let base = temp_base("snapshot_atomicity");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let mut child = spawn_crash_target_until_ready(
        &pillar_root,
        "--seed-then-loop-snapshot",
        Duration::from_secs(10),
    );
    child.kill().expect("SIGKILL mid-snapshot");
    let _ = child.wait();

    let store = FileBackedTieringStore::open(&base, Box::new(NoopRecorder))
        .expect("the store opens cleanly after a mid-snapshot crash — no torn file blocks open");
    let tier = store.get_tier(&tenant("acme"), &ItemId("blk-7781".to_string()));
    assert_eq!(
        tier,
        Some(Tier::Warm),
        "the acked hot-to-warm migration of blk-7781 is in the recovered ledger; got {tier:?}"
    );
    cleanup(&base);
}

#[test]
#[ignore = "RED until DELIVER: store-fsync-durability-v0 slice 04"]
fn canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash() {
    // @driving_port @real-io @adapter-integration @property @US-04 @AC-snapshot-atomicity
    let base = temp_base("snapshot_whole_or_absent");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let mut child = spawn_crash_target_until_ready(
        &pillar_root,
        "--seed-then-loop-snapshot",
        Duration::from_secs(10),
    );
    child.kill().expect("SIGKILL mid-snapshot");
    let _ = child.wait();

    let store = FileBackedTieringStore::open(&base, Box::new(NoopRecorder))
        .expect("reopen finds a whole snapshot, never a torn one");
    let _ = store.get_tier(&tenant("acme"), &ItemId("blk-7781".to_string()));
    cleanup(&base);
}

// AC-recovery-regression: torn migration tail dropped, acked prefix kept
// (cinder is on the shared recovery routine — ADR-0059 covers it).

#[test]
#[ignore = "RED until DELIVER: store-fsync-durability-v0 slice 04"]
fn a_torn_migration_tail_is_dropped_and_the_acked_prefix_is_recovered() {
    // @real-io @adapter-integration @US-04 @AC-recovery-regression
    let base = temp_base("recovery_regression");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let mut child = spawn_crash_target_until_ready(
        &pillar_root,
        "--seed-then-loop-snapshot",
        Duration::from_secs(10),
    );
    child.kill().expect("SIGKILL mid-write");
    let _ = child.wait();

    let store = FileBackedTieringStore::open(&base, Box::new(NoopRecorder))
        .expect("reopen recovers the acked prefix past any torn migration tail");
    let tier = store.get_tier(&tenant("acme"), &ItemId("blk-7781".to_string()));
    assert_eq!(
        tier,
        Some(Tier::Warm),
        "the acked migration is in the recovered prefix; got {tier:?}"
    );
    cleanup(&base);
}

// MECHANISM (b) — AC-wal-fsync (in-suite lying substrate).

#[test]
#[ignore = "RED until DELIVER: store-fsync-durability-v0 slice 04"]
fn an_acked_migration_survives_a_substrate_that_discards_unsynced_bytes() {
    // @driving_port @real-io @adapter-integration @US-04 @AC-wal-fsync
    let base = temp_base("wal_fsync_no_op");

    let store = FileBackedTieringStore::open_with_fsync_backend(
        &base,
        Box::new(NoopRecorder),
        Arc::new(LyingFsyncBackend::no_op()),
    )
    .expect("open with the lying-substrate seam");
    let item = ItemId("blk-7781".to_string());
    store.place(&tenant("acme"), &item, Tier::Hot, SystemTime::UNIX_EPOCH);
    store
        .migrate(&tenant("acme"), &item, Tier::Warm, SystemTime::UNIX_EPOCH)
        .expect("migrate acks the move");
    drop(store);

    let reopened = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    let tier = reopened.get_tier(&tenant("acme"), &item);
    assert_eq!(
        tier,
        Some(Tier::Warm),
        "an acked migration must be on stable storage, surviving a lying substrate; got {tier:?}"
    );
    cleanup(&base);
}

#[test]
#[ignore = "RED until DELIVER: store-fsync-durability-v0 slice 04"]
fn an_acked_migration_survives_a_truncating_substrate() {
    // @driving_port @real-io @adapter-integration @US-04 @AC-wal-fsync
    let base = temp_base("wal_fsync_truncating");

    let store = FileBackedTieringStore::open_with_fsync_backend(
        &base,
        Box::new(NoopRecorder),
        Arc::new(LyingFsyncBackend::truncating()),
    )
    .expect("open with the truncating lying-substrate seam");
    let item = ItemId("blk-9002".to_string());
    store.place(&tenant("acme"), &item, Tier::Hot, SystemTime::UNIX_EPOCH);
    store
        .migrate(&tenant("acme"), &item, Tier::Cold, SystemTime::UNIX_EPOCH)
        .expect("migrate acks the move");
    drop(store);

    let reopened = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    let tier = reopened.get_tier(&tenant("acme"), &item);
    assert_eq!(
        tier,
        Some(Tier::Cold),
        "a truncating substrate must not drop an acked migration once sync_all is wired; got {tier:?}"
    );
    cleanup(&base);
}

// MECHANISM (b) variant — AC-substrate-refusal (out-of-process, NEGATIVE).

#[test]
#[ignore = "RED until DELIVER: store-fsync-durability-v0 slice 04"]
fn cinder_refuses_to_start_on_a_substrate_that_lies_about_fsync() {
    // @real-io @adapter-integration @US-04 @AC-substrate-refusal @kpi
    let base = temp_base("substrate_refusal");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let output = Command::new(env!("CARGO_BIN_EXE_cinder-crash-target"))
        .arg("--probe-lying")
        .env("KALEIDOSCOPE_CRASH_PILLAR_ROOT", &pillar_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run cinder-crash-target in probe-lying mode");

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
        "cinder exits non-zero without opening the store for writes on a lying substrate"
    );
    cleanup(&base);
}
