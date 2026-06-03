// Kaleidoscope Cinder v1 — slice 03 torn-tail recovery acceptance suite
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

//! Slice 03 — torn-tail recovery, cinder store-reopen path
//! (wal-torn-tail-recovery-v0, US-01; AC-5/AC-6 negatives + AC-7 doc).
//!
//! Feature: a crashed-then-restarted tiering store recovers its intact
//! acked prefix and drops the torn tail, while every OTHER parse failure
//! stays fail-closed. Cinder is the worked pillar for the negative guards
//! (AC-5 mid-file, AC-6 newline-terminated malformed final line) because
//! its module doc falsely claimed this robustness before the feature; it
//! also carries the per-pillar AC-9 core torn-tail-tolerated case.
//!
//! Driving port: `FileBackedTieringStore::open` reopened on a crashed tmp
//! `pillar_root`, then read through the `TieringStore` trait
//! (`get_tier`).
//!
//! ## I-O strategy: C (real local I/O). See
//! `docs/feature/wal-torn-tail-recovery-v0/distill/wave-decisions.md` DWD-1.
//!
//! ## AC-7 (cinder doc correction)
//!
//! AC-7 is a DELIVER-verified DOC criterion, not a runtime assertion. The
//! claim under correction is module prose in
//! `crates/cinder/src/file_backed.rs:36-38` and the `open` doc at lines
//! 104-106, which today falsely state a truncated last WAL line "is
//! detected and ignored". A unit test cannot meaningfully assert the
//! CONTENT of a doc-comment without becoming a brittle string-match on
//! prose. Instead, AC-7 is discharged by the BEHAVIOUR the doc describes:
//! `reopen_recovers_the_intact_prefix_after_a_torn_tail` proves "torn
//! final line is dropped with a warning", and the two negatives prove
//! "every other parse failure is surfaced as PersistenceFailed". The
//! crafter corrects the prose in the same DELIVER commit, and the DISTILL
//! reviewer / DELIVER reviewer read the corrected doc against AC-1..AC-6
//! (ADR-0059 Verification; brief "For Acceptance Designer" AC-7). Recorded
//! as a DELIVER-verified doc criterion in
//! `docs/feature/wal-torn-tail-recovery-v0/distill/wave-decisions.md`
//! DWD-2.
//!
//! ## RED-not-BROKEN posture (Mandate 7)
//!
//! Every scenario is `#[ignore]`d until its DELIVER slice removes the
//! marker (Outside-In). The tests drive ONLY existing public APIs, so they
//! COMPILE with no scaffold. They are RED because today's `open` refuses a
//! torn tail with `PersistenceFailed`; never BROKEN.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{FileBackedTieringStore, ItemId, MigrateError, NoopRecorder, Tier, TieringStore};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn item(id: &str) -> ItemId {
    ItemId::new(id)
}

/// A fixed instant; the exact value is immaterial to recovery.
fn at(secs: u64) -> std::time::SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("cinder-torn-tail-{test_name}-{pid}-{nanos}"));
    fs::create_dir_all(&path).expect("mkdir");
    path.push("store");
    path
}

fn cleanup(base: &Path) {
    if let Some(dir) = base.parent() {
        let _ = fs::remove_dir_all(dir);
    }
}

fn wal_path_of(base: &Path) -> PathBuf {
    let mut p = base.as_os_str().to_owned();
    p.push(".wal");
    PathBuf::from(p)
}

fn append_torn_tail(base: &Path, torn: &str) -> usize {
    let wal = wal_path_of(base);
    let existing = fs::read_to_string(&wal).unwrap_or_default();
    fs::write(&wal, format!("{existing}{torn}")).expect("append torn tail");
    torn.len()
}

/// Seed `placements` Place records, then close so the WAL is flushed.
fn seed_placements(base: &Path, placements: &[(&str, Tier)]) {
    let store = FileBackedTieringStore::open(base, Box::new(NoopRecorder)).expect("seed open");
    for (i, (id, tier)) in placements.iter().enumerate() {
        store.place(&tenant("initech"), &item(id), *tier, at(1_000 + i as u64));
    }
    drop(store);
}

// --------------------------------------------------------------------
// AC-9 scope (cinder): the core torn-tail-tolerated case. The intact
// acked prefix recovers; the torn tail is dropped.
// --------------------------------------------------------------------

#[test]
fn reopen_recovers_the_intact_prefix_after_a_torn_tail() {
    // @real-io @adapter-integration @US-01 @AC-1 @AC-9
    let base = temp_base("cinder_prefix");
    seed_placements(
        &base,
        &[("a", Tier::Hot), ("b", Tier::Hot), ("c", Tier::Warm)],
    );
    // A crash tore a fourth Place mid-write: partial JSON, no newline.
    append_torn_tail(
        &base,
        "{\"op\":\"place\",\"tenant\":\"initech\",\"item\":\"img-9",
    );

    let store = FileBackedTieringStore::open(&base, Box::new(NoopRecorder))
        .expect("reopen recovers the intact prefix");
    assert_eq!(
        store.get_tier(&tenant("initech"), &item("a")),
        Some(Tier::Hot)
    );
    assert_eq!(
        store.get_tier(&tenant("initech"), &item("b")),
        Some(Tier::Hot)
    );
    assert_eq!(
        store.get_tier(&tenant("initech"), &item("c")),
        Some(Tier::Warm)
    );
    // The torn fourth placement never acked, so it is absent.
    assert_eq!(store.get_tier(&tenant("initech"), &item("img-9")), None);
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-5 (NEGATIVE, cinder headline): a mid-file corruption (a malformed
// line that is NOT the last line, followed by valid lines) stays
// fail-closed and names the offending line number.
// --------------------------------------------------------------------

#[test]
fn mid_file_corruption_stays_fail_closed_naming_the_offending_line() {
    // @real-io @adapter-integration @US-01 @AC-5
    let base = temp_base("cinder_midfile");
    seed_placements(
        &base,
        &[
            ("a", Tier::Hot),
            ("b", Tier::Hot),
            ("d", Tier::Hot),
            ("e", Tier::Hot),
        ],
    );
    // Corrupt line 3 of 5 (followed by valid newline-terminated lines):
    // provably not the torn tail.
    let wal = wal_path_of(&base);
    let mut lines: Vec<String> = fs::read_to_string(&wal)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    lines[2] = "{\"op\":\"place\",\"tenant\":\"initech\",\"item\":\"img-9".to_string();
    fs::write(&wal, format!("{}\n", lines.join("\n"))).expect("rewrite wal");

    let err = FileBackedTieringStore::open(&base, Box::new(NoopRecorder))
        .expect_err("mid-file corruption must refuse");
    let MigrateError::PersistenceFailed { reason } = err else {
        panic!("expected PersistenceFailed, got {err:?}");
    };
    assert!(
        reason.contains("line 3"),
        "the refusal names the offending line number; got {reason}"
    );
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-6 (NEGATIVE, cinder): a malformed FINAL line that DOES end in a
// trailing newline stays fail-closed. The trailing newline is the
// discriminator: a complete write, not a torn tear.
// --------------------------------------------------------------------

#[test]
fn newline_terminated_malformed_final_line_stays_fail_closed() {
    // @real-io @adapter-integration @US-01 @AC-6
    let base = temp_base("cinder_newline_malformed");
    seed_placements(&base, &[("a", Tier::Hot), ("b", Tier::Warm)]);
    let wal = wal_path_of(&base);
    let existing = fs::read_to_string(&wal).unwrap();
    // A complete-but-malformed final line, WITH a trailing newline.
    fs::write(&wal, format!("{existing}{{not valid json}}\n")).expect("append malformed line");

    let err = FileBackedTieringStore::open(&base, Box::new(NoopRecorder))
        .expect_err("a complete-but-malformed final line must refuse");
    assert!(matches!(err, MigrateError::PersistenceFailed { .. }));
    cleanup(&base);
}
