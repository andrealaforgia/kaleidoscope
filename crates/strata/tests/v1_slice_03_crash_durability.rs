// Kaleidoscope Strata v1 — slice 03 crash-durability acceptance suite
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

//! Slice 03 — crash durability, strata (store-fsync-durability-v0, US-03).
//!
//! strata has NO HTTP read path: the snapshot-atomicity outcome is observed
//! at the store-reopen driving port (the parent reopens after the child is
//! killed and queries the recovered state in-process). Two proving
//! mechanisms (ADR-0060 §1): (a) AC-snapshot-atomicity — a real child
//! PROCESS (`CARGO_BIN_EXE_strata-crash-target`) `SIGKILL`ed mid-snapshot,
//! parent reopens, `open()` succeeds + the acked profile is present. (b)
//! AC-wal-fsync — a `LyingFsyncBackend` injected through
//! `open_with_fsync_backend` discards exactly the unsynced bytes a power cut
//! would; the acked profile is ABSENT on flush()-only and PRESENT once
//! sync_all is wired. Plus an empty-store boundary (US-03 domain 3).
//!
//! I-O strategy: C (real local I/O + real child process). RED-not-BROKEN
//! (Mandate 7): every scenario `#[ignore]`d; the `open_with_fsync_backend`
//! seam, the `LyingFsyncBackend` re-export, and the `strata-crash-target`
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
use strata::{
    FileBackedProfileStore, Function, Location, LyingFsyncBackend, Mapping, NoopRecorder, Profile,
    ProfileBatch, ProfileStore, Sample, SampleType, ServiceName, TimeRange, ValueType,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn profile(time: u64, service: &str, profile_type: &str) -> Profile {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    Profile {
        time_unix_nano: time,
        duration_nanos: 10_000_000,
        profile_type: profile_type.to_string(),
        sample_type: vec![SampleType {
            value_type: ValueType {
                type_index: 1,
                unit_index: 2,
            },
            aggregation_temporality: 1,
        }],
        samples: vec![Sample {
            location_ids: vec![1, 2],
            values: vec![100],
            attributes: BTreeMap::new(),
        }],
        locations: vec![Location {
            id: 1,
            mapping_id: 1,
            address: 0x1000,
            function_ids: vec![1],
        }],
        functions: vec![Function {
            id: 1,
            name_index: 3,
            system_name_index: 3,
            filename_index: 4,
            start_line: 10,
        }],
        mappings: vec![Mapping {
            id: 1,
            memory_start: 0x1000,
            memory_limit: 0x2000,
            file_offset: 0,
            filename_index: 4,
            build_id_index: 5,
        }],
        string_table: vec![
            "".to_string(),
            "samples".to_string(),
            "count".to_string(),
            "main".to_string(),
            "main.go".to_string(),
            "build-abc123".to_string(),
        ],
        resource_attributes: resource,
        attributes: BTreeMap::new(),
    }
}

fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("strata-crash-durability-{test_name}-{pid}-{nanos}"));
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
    let mut child = Command::new(env!("CARGO_BIN_EXE_strata-crash-target"))
        .arg(mode)
        .env("KALEIDOSCOPE_CRASH_PILLAR_ROOT", pillar_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn strata-crash-target");
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
#[ignore = "RED until DELIVER: store-fsync-durability-v0 slice 03"]
fn acked_profile_survives_a_mid_snapshot_crash_and_is_present_after_reopen() {
    // @driving_port @real-io @adapter-integration @US-03 @AC-snapshot-atomicity
    let base = temp_base("snapshot_atomicity");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let mut child = spawn_crash_target_until_ready(
        &pillar_root,
        "--seed-then-loop-snapshot",
        Duration::from_secs(10),
    );
    child.kill().expect("SIGKILL mid-snapshot");
    let _ = child.wait();

    let store = FileBackedProfileStore::open(&base, Box::new(NoopRecorder))
        .expect("the store opens cleanly after a mid-snapshot crash — no torn file blocks open");
    let recovered = store
        .query(
            &tenant("acme"),
            &ServiceName::new("payment-svc"),
            TimeRange::all(),
        )
        .expect("query the recovered state");
    assert!(
        !recovered.is_empty(),
        "the acked profile for payment-svc is present after reopen"
    );
    cleanup(&base);
}

#[test]
#[ignore = "RED until DELIVER: store-fsync-durability-v0 slice 03"]
fn canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash() {
    // @driving_port @real-io @adapter-integration @property @US-03 @AC-snapshot-atomicity
    let base = temp_base("snapshot_whole_or_absent");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let mut child = spawn_crash_target_until_ready(
        &pillar_root,
        "--seed-then-loop-snapshot",
        Duration::from_secs(10),
    );
    child.kill().expect("SIGKILL mid-snapshot");
    let _ = child.wait();

    let store = FileBackedProfileStore::open(&base, Box::new(NoopRecorder))
        .expect("reopen finds a whole snapshot, never a torn one");
    store
        .query(
            &tenant("acme"),
            &ServiceName::new("payment-svc"),
            TimeRange::all(),
        )
        .expect("the recovered store serves queries");
    cleanup(&base);
}

// Boundary (US-03 domain 3): an empty store survives a crash before any
// write — open to an empty store, no spurious parse error.

#[test]
#[ignore = "RED until DELIVER: store-fsync-durability-v0 slice 03"]
fn an_empty_store_opens_cleanly_after_a_crash_before_any_write() {
    // @real-io @adapter-integration @US-03 @AC-snapshot-atomicity (boundary)
    let base = temp_base("empty_store");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let mut child =
        spawn_crash_target_until_ready(&pillar_root, "--open-then-idle", Duration::from_secs(10));
    child.kill().expect("SIGKILL before any profile is acked");
    let _ = child.wait();

    let store = FileBackedProfileStore::open(&base, Box::new(NoopRecorder))
        .expect("an empty strata store opens cleanly after a pre-write crash, no parse error");
    let recovered = store
        .query(
            &tenant("acme"),
            &ServiceName::new("payment-svc"),
            TimeRange::all(),
        )
        .expect("query an empty store");
    assert!(
        recovered.is_empty(),
        "no profiles recover from an empty store"
    );
    cleanup(&base);
}

// MECHANISM (b) — AC-wal-fsync (in-suite lying substrate).

#[test]
#[ignore = "RED until DELIVER: store-fsync-durability-v0 slice 03"]
fn an_acked_profile_survives_a_substrate_that_discards_unsynced_bytes() {
    // @driving_port @real-io @adapter-integration @US-03 @AC-wal-fsync
    let base = temp_base("wal_fsync_no_op");

    let store = FileBackedProfileStore::open_with_fsync_backend(
        &base,
        Box::new(NoopRecorder),
        Arc::new(LyingFsyncBackend::no_op()),
    )
    .expect("open with the lying-substrate seam");
    store
        .ingest(
            &tenant("acme"),
            ProfileBatch::with_profiles(vec![profile(100, "payment-svc", "cpu")]),
        )
        .expect("ingest acks the profile");
    drop(store);

    let reopened = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    let recovered = reopened
        .query(
            &tenant("acme"),
            &ServiceName::new("payment-svc"),
            TimeRange::all(),
        )
        .expect("query");
    assert!(
        !recovered.is_empty(),
        "an acked profile must be on stable storage, surviving a lying substrate"
    );
    cleanup(&base);
}

#[test]
#[ignore = "RED until DELIVER: store-fsync-durability-v0 slice 03"]
fn an_acked_profile_survives_a_truncating_substrate() {
    // @driving_port @real-io @adapter-integration @US-03 @AC-wal-fsync
    let base = temp_base("wal_fsync_truncating");

    let store = FileBackedProfileStore::open_with_fsync_backend(
        &base,
        Box::new(NoopRecorder),
        Arc::new(LyingFsyncBackend::truncating()),
    )
    .expect("open with the truncating lying-substrate seam");
    store
        .ingest(
            &tenant("acme"),
            ProfileBatch::with_profiles(vec![profile(200, "payment-svc", "heap")]),
        )
        .expect("ingest acks the profile");
    drop(store);

    let reopened = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    let recovered = reopened
        .query(
            &tenant("acme"),
            &ServiceName::new("payment-svc"),
            TimeRange::all(),
        )
        .expect("query");
    assert!(
        !recovered.is_empty(),
        "a truncating substrate must not drop an acked profile once sync_all is wired"
    );
    cleanup(&base);
}

// MECHANISM (b) variant — AC-substrate-refusal (out-of-process, NEGATIVE).

#[test]
#[ignore = "RED until DELIVER: store-fsync-durability-v0 slice 03"]
fn strata_refuses_to_start_on_a_substrate_that_lies_about_fsync() {
    // @real-io @adapter-integration @US-03 @AC-substrate-refusal @kpi
    let base = temp_base("substrate_refusal");
    let pillar_root = base.parent().unwrap().to_path_buf();

    let output = Command::new(env!("CARGO_BIN_EXE_strata-crash-target"))
        .arg("--probe-lying")
        .env("KALEIDOSCOPE_CRASH_PILLAR_ROOT", &pillar_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run strata-crash-target in probe-lying mode");

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
        "strata exits non-zero without opening the store for writes on a lying substrate"
    );
    cleanup(&base);
}
