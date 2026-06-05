// Kaleidoscope CLI — WAL-error-surfacing walking skeleton (subprocess)
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

//! # Walking skeleton — Priya places a tier through the REAL binary (US-01/US-02)
//!
//! Feature: `cinder-wal-error-surfacing-v0`. Wave: DISTILL. `@walking_skeleton
//! @real-io @driving_port`.
//!
//! These are the only scenarios that drive the ACTUAL operator entry point — the
//! `kaleidoscope-cli` binary — via SUBPROCESS (exit code + stdout/stderr), per the
//! Driving-Adapter mandate and the brief's For-Acceptance-Designer driving-ports
//! note. The DESIGN names the CLI ingest/place path as a driving port; the failure
//! AC (D2 fail-the-ingest) is observable here as a non-zero exit + a
//! `persistence failed: io:` stderr message + nothing acked durable.
//!
//! ## WS-A (happy path) — compiled, passes TODAY and post-fix
//! Drives the real binary `place` then `get-tier` on a healthy temp dir: exit 0,
//! the placement line on stdout, the tier readable, and durable across a real
//! reopen (a second `get-tier` process). The negative control at the binary
//! boundary — proves the wiring works end to end.
//!
//! ## WS-B (failure path) — D2 fail-the-ingest, now active
//! Drives the real binary against a REAL read-only WAL substrate (a genuine
//! filesystem `io::Error`, NOT the injected backend — the binary has no flag to
//! inject a FsyncBackend; see distill/wave-decisions.md DWD-3). Asserts non-zero
//! exit + a `persistence failed: io:` stderr substring + the failed placement is
//! NOT durable. GREEN since DELIVER made `place` fallible and `flush`/`place`
//! propagate the persistence failure (`cinder place: persistence failed: io:`).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

fn temp_root(name: &str) -> PathBuf {
    let mut p = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    p.push(format!("kal-cli-walerr-{name}-{pid}-{nanos}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn cleanup(p: &Path) {
    let _ = fs::remove_dir_all(p);
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_kaleidoscope-cli")
}

/// The `cinder_base(data_dir)` join, kept in lock-step with
/// `crates/kaleidoscope-cli/src/lib.rs` (`data_dir.join("cinder")`). The
/// CLI opens Cinder against exactly this base; the WAL is `<base>.wal`.
fn cinder_base(data_dir: &Path) -> PathBuf {
    data_dir.join("cinder")
}

// ====================================================================
// WS-A — happy path through the real binary (negative control).
//
// Scenario: Priya places a tier on a healthy disk and reads it back.
//   Given Priya has a fresh data dir on a healthy disk
//   When Priya places item "trade-001" for tenant "acme" in tier "hot"
//   Then the command succeeds (exit 0) and reports the placement
//   And reading the tier for "acme" / "trade-001" returns "hot"
//   And the placement is durable across a fresh read process (reopen)
// ====================================================================

#[test]
fn place_then_get_tier_through_real_binary_on_healthy_disk() {
    // Given a fresh data dir on a healthy disk.
    let root = temp_root("ws_a_happy");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");

    // When Priya places trade-001 in hot via the REAL binary.
    let place = Command::new(bin())
        .arg("place")
        .arg("acme")
        .arg(&data)
        .arg("trade-001")
        .arg("hot")
        .output()
        .expect("spawn place");

    // Then the place command succeeds and reports the placement.
    assert!(
        place.status.success(),
        "healthy place exits 0 (status {:?}, stderr {:?})",
        place.status,
        String::from_utf8_lossy(&place.stderr)
    );
    let place_out = String::from_utf8(place.stdout).expect("utf8");
    assert_eq!(
        place_out, "placed tenant=acme item=trade-001 tier=hot\n",
        "stdout is the one-line placement report"
    );

    // And reading the tier back via a SEPARATE process (a real reopen of
    // the store from the persisted WAL) returns hot — durability proof.
    let get = Command::new(bin())
        .arg("get-tier")
        .arg("acme")
        .arg(&data)
        .arg("trade-001")
        .output()
        .expect("spawn get-tier");
    assert!(
        get.status.success(),
        "get-tier exits 0 (stderr {:?})",
        String::from_utf8_lossy(&get.stderr)
    );
    let get_out = String::from_utf8(get.stdout).expect("utf8");
    assert!(
        get_out.contains("hot"),
        "get-tier reports the durable tier hot (got {get_out:?})"
    );

    cleanup(&root);
}

// ====================================================================
// WS-B — failure path through the real binary (D2 fail-the-ignest).
//
// Scenario: Priya places onto a failing disk and the command fails loudly.
//   Given Priya has a data dir whose WAL cannot be written (a failing disk)
//   When Priya places item "trade-002" for tenant "acme" in tier "warm"
//   Then the command fails (non-zero exit)
//   And stderr names a persistence failure with its disk reason
//   And the failed placement is NOT durable (a later read returns nothing)
//
// D2 fail-the-ingest is now live: `place` is fallible and the CLI propagates
// the error (`cinder place: persistence failed: io: <reason>`, non-zero exit).
//
// Substrate: a REAL read-only WAL file (genuine filesystem io::Error), not
// the injected FsyncBackend — the binary has no backend-injection flag.
// ====================================================================

#[test]
fn place_onto_failing_disk_fails_loudly_and_is_not_durable() {
    // Given a data dir with a healthy placement so the WAL file exists,
    // then made read-only so a subsequent append fails with a real
    // io::Error from the filesystem.
    let root = temp_root("ws_b_failing");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("mkdir data");

    // Seed one healthy placement (creates <cinder_base>.wal).
    let seed = Command::new(bin())
        .arg("place")
        .arg("acme")
        .arg(&data)
        .arg("seed")
        .arg("hot")
        .output()
        .expect("spawn seed place");
    assert!(
        seed.status.success(),
        "seed place succeeds on a healthy disk"
    );

    // Make the WAL file read-only so the next append cannot write.
    let mut wal_path = cinder_base(&data).into_os_string();
    wal_path.push(".wal");
    let wal_path = PathBuf::from(wal_path);
    let mut perms = fs::metadata(&wal_path).expect("wal metadata").permissions();
    perms.set_readonly(true);
    fs::set_permissions(&wal_path, perms).expect("chmod wal read-only");

    // When Priya places trade-002 in warm against the failing WAL.
    let place = Command::new(bin())
        .arg("place")
        .arg("acme")
        .arg(&data)
        .arg("trade-002")
        .arg("warm")
        .output()
        .expect("spawn failing place");

    // Then the command fails loudly (non-zero exit).
    assert!(
        !place.status.success(),
        "place onto a failing disk exits non-zero (status {:?})",
        place.status
    );

    // And stderr names a persistence failure with its disk reason.
    let stderr = String::from_utf8(place.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("persistence failed") && stderr.contains("io:"),
        "stderr names the persistence failure and its io reason (got {stderr:?})"
    );

    // And nothing was printed as a successful placement.
    let stdout = String::from_utf8(place.stdout).expect("utf8 stdout");
    assert!(
        !stdout.contains("placed tenant=acme item=trade-002"),
        "no placement line is printed on the failure path (got {stdout:?})"
    );

    // And the failed placement is NOT durable: restore permissions and read
    // back — trade-002 must be absent. Use PermissionsExt to restore an
    // explicit owner-writable mode (0o644) rather than `set_readonly(false)`,
    // which would make the file world-writable on Unix.
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o644);
    let _ = fs::set_permissions(&wal_path, perms);
    let get = Command::new(bin())
        .arg("get-tier")
        .arg("acme")
        .arg(&data)
        .arg("trade-002")
        .output()
        .expect("spawn get-tier");
    let get_out = String::from_utf8(get.stdout).expect("utf8");
    assert!(
        !get_out.contains("warm"),
        "the failed placement must not be durable; get-tier must not report warm (got {get_out:?})"
    );

    cleanup(&root);
}
