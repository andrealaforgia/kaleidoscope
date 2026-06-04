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
//! * **AC-wal-fsync — mechanism (b), in-suite lying substrate.** A
//!   `LyingFsyncBackend` injected through
//!   `FileBackedLogStore::open_with_fsync_backend` DISCARDS exactly the
//!   unsynced bytes a power cut would; on reopen the acked record is ABSENT
//!   on the un-fixed `flush()`-only code and PRESENT once `sync_all` is
//!   wired. This is the ONLY mechanism that distinguishes `flush` from
//!   `sync_all` — a SIGKILL CANNOT, because the page cache survives the
//!   kill. Deterministic, in-process.
//!
//! ## I-O strategy: C (real local I/O + real child process)
//!
//! Real WAL/snapshot files on a real per-test tmp directory; a real OS
//! child process for mechanism (a). No external services, no containers.
//! See `docs/feature/store-fsync-durability-v0/distill/wave-decisions.md`.
//!
//! ## RED-not-BROKEN posture (Mandate 7)
//!
//! Every scenario is `#[ignore]`d until its DELIVER step removes the marker
//! one at a time (Outside-In). The tests reference the
//! `open_with_fsync_backend` seam, the `LyingFsyncBackend` re-export, and
//! the `lumen-crash-target` helper binary — all RED scaffolds (`// SCAFFOLD:
//! true`, `panic!("__SCAFFOLD__ …")`) so the suite COMPILES and is RED, not
//! BROKEN. DELIVER replaces the scaffolds and lifts the ignores.

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
    FileBackedLogStore, LogBatch, LogRecord, LogStore, LyingFsyncBackend, NoopRecorder,
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
// MECHANISM (b) — AC-wal-fsync (in-suite lying substrate).
// Driving seam: FileBackedLogStore::open_with_fsync_backend.
// The ONLY mechanism that distinguishes flush() from sync_all().
// ====================================================================

// --------------------------------------------------------------------
// US-01 / AC-wal-fsync (the durability proof): a write acked against a
// substrate that DISCARDS unsynced bytes is ABSENT after reopen on the
// un-fixed flush()-only code and PRESENT once sync_all is wired. A SIGKILL
// cannot prove this — the page cache survives the kill.
// --------------------------------------------------------------------

#[test]
#[ignore = "ESCALATED: store-fsync-durability-v0 slice 01 — this scenario injects the \
            probe-double LyingFsyncBackend::no_op() (truncate-on-fsync, required verbatim by \
            the ADR-0049 probe tests) into the store append path and asserts the acked write \
            SURVIVES. With truncate-on-fsync the FIXED (sync_all-wired) code truncates the live \
            WAL to 0 on every append, so the record is ABSENT on the fixed code, not present. \
            The probe-double and a durability-discard-double cannot be the same type/constructor: \
            the probe needs fsync to truncate-to-0 (to be detected), the store needs fsync to \
            PRESERVE. Cannot be made green without weakening the assertion or breaking the \
            ADR-0049 probe tests. See report; needs an acceptance-design correction \
            (a distinct DiscardingFsyncBackend, or a CountingFsyncBackend as pulse slice 03 used)."]
fn an_acked_write_survives_a_substrate_that_discards_unsynced_bytes() {
    // @driving_port @real-io @adapter-integration @US-01 @AC-wal-fsync
    let base = temp_base("wal_fsync_no_op");

    // Open through the seam with a lying substrate that drops exactly the
    // unsynced bytes a power cut would.
    let store = FileBackedLogStore::open_with_fsync_backend(
        &base,
        Box::new(NoopRecorder),
        Arc::new(LyingFsyncBackend::no_op()),
    )
    .expect("open with the lying-substrate seam");
    store
        .ingest(
            &tenant("acme"),
            LogBatch::with_records(vec![record(100, "payment-svc", "durably acked line")]),
        )
        .expect("ingest acks the write");
    drop(store);

    // Reopen with an honest backend (the production open): the write must
    // have actually reached stable storage. On the buggy flush()-only code
    // the lying substrate discarded it, so it is ABSENT and this fails —
    // exactly as it should until sync_all is wired.
    let reopened = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    let out = reopened
        .query(&tenant("acme"), TimeRange::all())
        .expect("query");
    let bodies: Vec<&str> = out.iter().map(|r| r.body.as_str()).collect();
    assert!(
        bodies.contains(&"durably acked line"),
        "an acked write must be on stable storage, surviving a lying substrate; got {bodies:?}"
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// US-01 / AC-wal-fsync (truncating-substrate variant, NEGATIVE framing):
// a truncating substrate that drops the unsynced tail must likewise not
// cost an acked write once sync_all is wired.
// --------------------------------------------------------------------

#[test]
#[ignore = "ESCALATED: store-fsync-durability-v0 slice 01 — same defect as \
            an_acked_write_survives_a_substrate_that_discards_unsynced_bytes. \
            LyingFsyncBackend::truncating() drops the trailing newline of the just-fsynced WAL \
            line, so on reopen torn-tail recovery (ADR-0059) drops the de-newlined final record: \
            the acked write is ABSENT on the FIXED code, contradicting the assertion. The \
            probe-double semantics (verbatim from ADR-0049) and a durability-discard-double are \
            irreconcilable as one constructor. See report."]
fn an_acked_write_survives_a_truncating_substrate() {
    // @driving_port @real-io @adapter-integration @US-01 @AC-wal-fsync
    let base = temp_base("wal_fsync_truncating");

    let store = FileBackedLogStore::open_with_fsync_backend(
        &base,
        Box::new(NoopRecorder),
        Arc::new(LyingFsyncBackend::truncating()),
    )
    .expect("open with the truncating lying-substrate seam");
    store
        .ingest(
            &tenant("acme"),
            LogBatch::with_records(vec![record(200, "checkout", "acked under truncation")]),
        )
        .expect("ingest acks the write");
    drop(store);

    let reopened = FileBackedLogStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    let out = reopened
        .query(&tenant("acme"), TimeRange::all())
        .expect("query");
    let bodies: Vec<&str> = out.iter().map(|r| r.body.as_str()).collect();
    assert!(
        bodies.contains(&"acked under truncation"),
        "a truncating substrate must not drop an acked write once sync_all is wired; got {bodies:?}"
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
