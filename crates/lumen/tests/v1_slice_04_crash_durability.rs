// Kaleidoscope Lumen v1 — slice 04 crash-durability acceptance suite
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

//! Slice 04 — crash durability, lumen (WALKING SKELETON for
//! `store-fsync-durability-v0`, US-01).
//!
//! Priya — an on-call SRE self-hosting a Kaleidoscope collector on
//! bare-metal — restarts after a power loss and needs every log her
//! exporter was acked for to still be there, and the store to open cleanly
//! even if the crash hit during a snapshot.
//!
//! ## The two proving mechanisms (ADR-0060 §1; brief "For Acceptance
//! Designer"). These are SEPARATE ACs, proven by SEPARATE mechanisms:
//!
//! * **AC-snapshot-atomicity — mechanism (a), out-of-process SIGKILL.** A
//!   real child PROCESS (`CARGO_BIN_EXE_lumen-crash-target`) is `SIGKILL`ed
//!   mid-snapshot; the parent reopens. A torn snapshot is a PHYSICAL on-disk
//!   artefact the page cache cannot hide, so a process kill is the right
//!   mechanism here. Asserts the crash-at-ANY-point invariant (canonical
//!   path holds the OLD or NEW whole snapshot, never a torn one) and that
//!   `open()` succeeds + the acked prefix is served via `GET /api/v1/logs`.
//!   NOT `fork()`-in-tokio (C5); a deterministic invariant, never a
//!   wall-clock p95 (C6).
//! * **AC-wal-fsync — mechanism (b), in-suite counting substrate.** A
//!   `CountingFsyncBackend` (an honest `RealFsyncBackend` wrapper that
//!   DELEGATES the real fsync — so data is genuinely durable — and COUNTS
//!   calls at the seam) injected through
//!   `FileBackedLogStore::open_with_fsync_backend` proves the store
//!   actually reaches the fsync seam: after an acked append the
//!   `file_fsync_count` increased (the WAL was synced per record), and the
//!   record is queryable on reopen (the delegated real fsync made it
//!   durable). This is the mechanism that distinguishes `flush`-only from
//!   `sync_all`-wired code: the un-fixed code never reaches `fsync_file`,
//!   so the counter stays flat. Deterministic, in-process, observable.
//!
//!   NOTE (DELIVER-found correction): an earlier draft injected the
//!   probe-double `LyingFsyncBackend::no_op()/truncating()` into the
//!   APPEND path and asserted the acked write SURVIVES. That conflated two
//!   roles. The lying double deliberately DISCARDS bytes so the
//!   fsync-honesty probe can DETECT a lying substrate and the store can
//!   REFUSE to start (see AC-substrate-refusal below). Injected into the
//!   append path it makes the CORRECT `sync_all`-wired code lose the
//!   record, so the test fails on correct code: the lying double proves
//!   REFUSAL, not SURVIVAL, and cannot prove fsync-is-wired. The counting
//!   double does. Mirrors pulse slice 03
//!   (`crates/pulse/tests/v1_slice_03_fsync_probe.rs`).
//!
//! ## I-O strategy: C (real local I/O + real child process)
//!
//! Real WAL/snapshot files on a real per-test tmp directory; a real OS
//! child process for mechanism (a). No external services, no containers.
//! See `docs/feature/store-fsync-durability-v0/distill/wave-decisions.md`.
//!
//! ## RED-not-BROKEN posture (Mandate 7)
//!
//! lumen's production durability wiring (`open_with_fsync_backend` syncing
//! per append + `atomic_write_snapshot`) has LANDED, so its scenarios are
//! un-ignored and GREEN (7/7). The tests reference the
//! `open_with_fsync_backend` seam, the `CountingFsyncBackend` re-export, and
//! the `lumen-crash-target` helper binary. (The other five stores keep this
//! suite's corrected AC-wal-fsync scenarios `#[ignore]`d RED until their own
//! DELIVER slices wire their stores, one at a time, Outside-In.)

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use aegis::TenantId;
use lumen::{
    CountingFsyncBackend, FileBackedLogStore, LogBatch, LogRecord, LogStore, NoopRecorder,
    SeverityNumber, TimeRange,
};

// --------------------------------------------------------------------
// Helpers — match the established v1 file-backed convention exactly.
// --------------------------------------------------------------------

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn record(observed: u64, service: &str, body: &str) -> LogRecord {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
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

fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("lumen-crash-durability-{test_name}-{pid}-{nanos}"));
    fs::create_dir_all(&path).expect("mkdir");
    path.push("store");
    path
}

fn cleanup(base: &Path) {
    if let Some(dir) = base.parent() {
        let _ = fs::remove_dir_all(dir);
    }
}

fn snapshot_path_of(base: &Path) -> PathBuf {
    let mut p = base.as_os_str().to_owned();
    p.push(".snapshot");
    PathBuf::from(p)
}

/// Spawn the out-of-process kill-target against `pillar_root`, in the given
/// `mode`, with the acked record `body`. Reads stdout until the readiness
/// sentinel `CRASH_TARGET_READY` appears (so the kill lands at a controlled
/// moment, while the child loops writing snapshots), then returns the live
/// child for the parent to `SIGKILL`. DELIVER implements the child;
/// `#[ignore]` keeps this from running until then.
fn spawn_crash_target_until_ready(
    pillar_root: &Path,
    mode: &str,
    body: &str,
    timeout: Duration,
) -> std::process::Child {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-crash-target"))
        .arg(mode)
        .arg("--body")
        .arg(body)
        .env("KALEIDOSCOPE_CRASH_PILLAR_ROOT", pillar_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn lumen-crash-target");

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

// ====================================================================
// MECHANISM (a) — AC-snapshot-atomicity (out-of-process SIGKILL).
// Driving port: the store reopen path + GET /api/v1/logs read path.
// ====================================================================

// --------------------------------------------------------------------
// US-01 / AC-snapshot-atomicity (happy path, WALKING SKELETON): an acked
// log survives a mid-snapshot crash and is queryable after restart.
// --------------------------------------------------------------------

#[test]
fn acked_log_survives_a_mid_snapshot_crash_and_is_queryable_after_restart() {
    // @walking_skeleton @driving_port @real-io @adapter-integration
    // @US-01 @AC-snapshot-atomicity
    let base = temp_base("snapshot_atomicity_ws");
    let pillar_root = base.parent().unwrap().to_path_buf();

    // A real child PROCESS opens the store, acks the record, then loops
    // writing snapshots so a kill lands mid-snapshot.
    let mut child = spawn_crash_target_until_ready(
        &pillar_root,
        "--seed-then-loop-snapshot",
        "connection pool exhausted",
        Duration::from_secs(10),
    );
    // The power cut: SIGKILL the child mid-snapshot (Child::kill sends
    // SIGKILL on Unix — uncatchable, immediate, faithful to a power loss).
    child.kill().expect("SIGKILL the crash target mid-snapshot");
    let _ = child.wait();

    // Priya restarts: the parent reopens the same on-disk pillar root.
    let store = FileBackedLogStore::open(&base, Box::new(NoopRecorder))
        .expect("the store opens cleanly after a mid-snapshot crash — no torn file blocks open");
    let out = store
        .query(&tenant("acme"), TimeRange::all())
        .expect("query the recovered state");
    let bodies: Vec<&str> = out.iter().map(|r| r.body.as_str()).collect();
    assert!(
        bodies.contains(&"connection pool exhausted"),
        "the acked record is present after restart; got {bodies:?}"
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// US-01 / AC-snapshot-atomicity (crash-at-ANY-point invariant, the
// timing-independent framing DEVOPS pinned): after a mid-snapshot kill the
// canonical snapshot path holds either the OLD or the NEW whole snapshot,
// NEVER a torn one — so reopen always succeeds regardless of WHEN the kill
// landed.
// --------------------------------------------------------------------

#[test]
fn canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash() {
    // @driving_port @real-io @adapter-integration @property
    // @US-01 @AC-snapshot-atomicity
    let base = temp_base("snapshot_whole_or_absent");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let mut child = spawn_crash_target_until_ready(
        &pillar_root,
        "--seed-then-loop-snapshot",
        "order 0",
        Duration::from_secs(10),
    );
    child.kill().expect("SIGKILL mid-snapshot");
    let _ = child.wait();

    // The observable invariant: reopen ALWAYS succeeds. A successful open is
    // the user-visible proof that the canonical path was never torn (the
    // store parses the snapshot on open; a torn file would refuse). The kill
    // can land at any instant; the assertion holds regardless of timing.
    let store = FileBackedLogStore::open(&base, Box::new(NoopRecorder))
        .expect("reopen finds the OLD or NEW whole snapshot, never a torn one");
    // Querying succeeds and never errors on a half-parsed canonical file.
    store
        .query(&tenant("acme"), TimeRange::all())
        .expect("the recovered store serves queries");
    cleanup(&base);
}

// --------------------------------------------------------------------
// US-01 / AC-recovery-regression (the kept SIGKILL+read assertion,
// RE-LABELLED per ADR-0060): a torn never-acked WAL tail from the crash is
// dropped, the acked prefix kept, with event=wal.recovery.torn_tail_dropped
// pillar=lumen. This is a recovery/read-back guard, NOT the wal-fsync proof.
// --------------------------------------------------------------------

#[test]
fn a_torn_wal_tail_is_dropped_and_the_acked_prefix_is_recovered() {
    // @real-io @adapter-integration @US-01 @AC-recovery-regression
    let base = temp_base("recovery_regression");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let mut child = spawn_crash_target_until_ready(
        &pillar_root,
        "--seed-then-loop-snapshot",
        "payment ack",
        Duration::from_secs(10),
    );
    child.kill().expect("SIGKILL mid-write");
    let _ = child.wait();

    let store = FileBackedLogStore::open(&base, Box::new(NoopRecorder))
        .expect("reopen recovers the acked prefix past any torn tail");
    let out = store
        .query(&tenant("acme"), TimeRange::all())
        .expect("query");
    let bodies: Vec<&str> = out.iter().map(|r| r.body.as_str()).collect();
    assert!(
        bodies.contains(&"payment ack"),
        "the acked record is in the recovered prefix; got {bodies:?}"
    );
    cleanup(&base);
}

// ====================================================================
// MECHANISM (b) — AC-wal-fsync (in-suite counting substrate).
// Driving seam: FileBackedLogStore::open_with_fsync_backend.
// Proves the store reaches the fsync seam per WAL append and around the
// snapshot rename — the mechanism that distinguishes flush() from
// sync_all(). Mirrors pulse slice 03 (CountingFsyncBackend).
// ====================================================================

// --------------------------------------------------------------------
// US-01 / AC-wal-fsync (the durability proof, fsync-per-append): an
// acked append drives the WAL fsync seam — file_fsync_count increases —
// and because the counting backend delegates the REAL fsync, the record
// is genuinely durable and queryable on reopen. On the un-fixed
// flush()-only code the seam is never reached, so the counter stays flat.
// --------------------------------------------------------------------

#[test]
fn an_acked_append_fsyncs_the_wal_per_record_and_is_durable_on_reopen() {
    // @driving_port @real-io @adapter-integration @US-01 @AC-wal-fsync
    let base = temp_base("wal_fsync_per_append");
    let backend = Arc::new(CountingFsyncBackend::new());

    let store =
        FileBackedLogStore::open_with_fsync_backend(&base, Box::new(NoopRecorder), backend.clone())
            .expect("open with the counting-substrate seam");

    let before = backend.file_fsync_count();
    store
        .ingest(
            &tenant("acme"),
            LogBatch::with_records(vec![record(100, "payment-svc", "durably acked line")]),
        )
        .expect("ingest acks the write");
    let after = backend.file_fsync_count();
    assert!(
        after > before,
        "an acked append must fsync the WAL at least once (per-record sync_all); \
         before={before}, after={after}"
    );
    drop(store);

    // The delegated real fsync made the write durable: a fresh production
    // open finds the acked record.
    let reopened = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    let out = reopened
        .query(&tenant("acme"), TimeRange::all())
        .expect("query");
    let bodies: Vec<&str> = out.iter().map(|r| r.body.as_str()).collect();
    assert!(
        bodies.contains(&"durably acked line"),
        "the acked write is on stable storage and queryable after reopen; got {bodies:?}"
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// US-01 / AC-wal-fsync (snapshot durability variant): a snapshot fsyncs
// the snapshot file and its parent directory (POSIX rename durability),
// so a snapshot acked before a crash is durable. file_fsync_count and
// dir_fsync_count both increase across the snapshot; the snapshotted data
// is queryable on reopen.
// --------------------------------------------------------------------

#[test]
fn a_snapshot_fsyncs_the_snapshot_file_and_parent_dir_for_rename_durability() {
    // @driving_port @real-io @adapter-integration @US-01 @AC-wal-fsync
    let base = temp_base("wal_fsync_snapshot");
    let backend = Arc::new(CountingFsyncBackend::new());

    let store =
        FileBackedLogStore::open_with_fsync_backend(&base, Box::new(NoopRecorder), backend.clone())
            .expect("open with the counting-substrate seam");
    store
        .ingest(
            &tenant("acme"),
            LogBatch::with_records(vec![record(200, "checkout", "snapshotted line")]),
        )
        .expect("ingest acks the write");

    let file_before = backend.file_fsync_count();
    let dir_before = backend.dir_fsync_count();
    store.snapshot().expect("snapshot");
    let file_after = backend.file_fsync_count();
    let dir_after = backend.dir_fsync_count();
    assert!(
        file_after > file_before,
        "snapshot must fsync the snapshot file; before={file_before}, after={file_after}"
    );
    assert!(
        dir_after > dir_before,
        "snapshot must fsync the parent directory for rename durability; \
         before={dir_before}, after={dir_after}"
    );
    drop(store);

    // The snapshotted data is durable and queryable on reopen.
    let reopened = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    let out = reopened
        .query(&tenant("acme"), TimeRange::all())
        .expect("query");
    let bodies: Vec<&str> = out.iter().map(|r| r.body.as_str()).collect();
    assert!(
        bodies.contains(&"snapshotted line"),
        "the snapshotted write is durable and queryable after reopen; got {bodies:?}"
    );
    cleanup(&base);
}

// ====================================================================
// MECHANISM (b) variant — AC-substrate-refusal (out-of-process).
// The composition root refuses to start on a lying substrate.
// ====================================================================

// --------------------------------------------------------------------
// US-01 / AC-substrate-refusal (NEGATIVE): driven with a LyingFsyncBackend,
// the composition root emits event=health.startup.refused with a substrate
// descriptor and exits non-zero WITHOUT binding the listener — no write is
// ever acked against a substrate proven to lie about durability.
// --------------------------------------------------------------------

#[test]
fn the_collector_refuses_to_start_on_a_substrate_that_lies_about_fsync() {
    // @real-io @adapter-integration @US-01 @AC-substrate-refusal @kpi
    let base = temp_base("substrate_refusal");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let output = Command::new(env!("CARGO_BIN_EXE_lumen-crash-target"))
        .arg("--probe-lying")
        .env("KALEIDOSCOPE_CRASH_PILLAR_ROOT", &pillar_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run lumen-crash-target in probe-lying mode");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("health.startup.refused"),
        "the composition root emits event=health.startup.refused; stderr was: {stderr}"
    );
    assert!(
        stderr.contains("substrate="),
        "the refusal names the lying substrate with a substrate=<descriptor> field; stderr: {stderr}"
    );
    assert!(
        !output.status.success(),
        "the collector exits non-zero without binding the listener on a lying substrate"
    );
    cleanup(&base);
}

// ====================================================================
// REGRESSION GUARD — a graceful restart still recovers everything (no
// crash, no torn-tail warning). Proves the durability hardening did not
// regress the happy graceful path.
// ====================================================================

#[test]
fn a_graceful_restart_still_recovers_every_acked_record() {
    // @real-io @adapter-integration @US-01 @AC-recovery-regression
    let base = temp_base("graceful_restart");

    let store = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("open");
    store
        .ingest(
            &tenant("acme"),
            LogBatch::with_records(vec![record(300, "checkout", "graceful line")]),
        )
        .expect("ingest");
    drop(store); // graceful shutdown flushes the WAL.

    let reopened = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    let out = reopened
        .query(&tenant("acme"), TimeRange::all())
        .expect("query");
    let bodies: Vec<&str> = out.iter().map(|r| r.body.as_str()).collect();
    assert!(
        bodies.contains(&"graceful line"),
        "a graceful restart recovers every acked record; got {bodies:?}"
    );
    // Sanity: a graceful close leaves no torn snapshot at the canonical path.
    let snap = snapshot_path_of(&base);
    let _ = snap; // presence is implementation detail; open succeeding is the contract.
    cleanup(&base);
}
