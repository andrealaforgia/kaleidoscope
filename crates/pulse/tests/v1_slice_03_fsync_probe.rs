// Kaleidoscope Pulse — slice 03 fsync-honesty probe acceptance test
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

//! Slice 03 — Earned-Trust fsync-honesty probe
//!
//! Maps to feature `earned-trust-fsync-probe-v0`, slice 01
//! (`docs/feature/earned-trust-fsync-probe-v0/slices/slice-01-fsync-probe-walking-skeleton.md`)
//! and ADR-0049
//! (`docs/product/architecture/adr-0049-earned-trust-honour-fsync.md`).
//!
//! Two layers of acceptance:
//!
//! 1. The probe LEVEL (walking skeleton) — `fsync_probe` returns
//!    `Ok(())` against an honest substrate and the matching
//!    `FsyncProbeError` variant against each class of lying substrate.
//! 2. The write-path LEVEL — ingest and snapshot through
//!    `FileBackedMetricStore` invoke `fsync_file` (per WAL record) and
//!    `fsync_dir` (parent-directory durability on snapshot rename) at
//!    least once each, observed via a `CountingFsyncBackend` injected
//!    through `FileBackedMetricStore::open_with_fsync_backend`.

use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use pulse::{
    fsync_probe, FileBackedMetricStore, FsyncBackend, FsyncProbeError, LyingFsyncBackend, Metric,
    MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore, NoopRecorder, RealFsyncBackend,
};

// --------------------------------------------------------------------
// Test helpers — mirror v1_slice_01 / v1_slice_02 shape so the
// crate's existing tempdir + cleanup pattern is reused.
// --------------------------------------------------------------------

fn temp_root(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("pulse-fsync-probe-{test_name}-{pid}-{nanos}"));
    fs::create_dir_all(&path).expect("mkdir probe root");
    path
}

fn cleanup(root: &Path) {
    let _ = fs::remove_dir_all(root);
}

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
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

const SENTINEL_FILENAME: &str = ".fsync-probe";

// --------------------------------------------------------------------
// Counting wrapper around `RealFsyncBackend` — used by the
// write-path scenarios and the cleanup-on-success scenario to assert
// that fsync calls were actually invoked. Honest at the substrate
// level (delegates to `RealFsyncBackend`), counts at the seam level.
// --------------------------------------------------------------------

struct CountingFsyncBackend {
    inner: RealFsyncBackend,
    file_fsyncs: AtomicUsize,
    dir_fsyncs: AtomicUsize,
}

impl CountingFsyncBackend {
    fn new() -> Self {
        Self {
            inner: RealFsyncBackend,
            file_fsyncs: AtomicUsize::new(0),
            dir_fsyncs: AtomicUsize::new(0),
        }
    }

    fn file_fsync_count(&self) -> usize {
        self.file_fsyncs.load(Ordering::SeqCst)
    }

    fn dir_fsync_count(&self) -> usize {
        self.dir_fsyncs.load(Ordering::SeqCst)
    }
}

impl FsyncBackend for CountingFsyncBackend {
    fn fsync_file(&self, file: &File) -> io::Result<()> {
        self.file_fsyncs.fetch_add(1, Ordering::SeqCst);
        self.inner.fsync_file(file)
    }

    fn fsync_dir(&self, dir: &Path) -> io::Result<()> {
        self.dir_fsyncs.fetch_add(1, Ordering::SeqCst);
        self.inner.fsync_dir(dir)
    }
}

// ====================================================================
// PROBE LEVEL (walking skeleton + lie modes)
// ====================================================================

// --------------------------------------------------------------------
// AC-3.1 (walking skeleton, US-01 Scenario 1, US-02 Domain 1) —
// honest substrate, the probe returns `Ok(())`.
// --------------------------------------------------------------------

#[test]
fn honest_substrate_probe_returns_ok() {
    let root = temp_root("honest");
    let backend = RealFsyncBackend;

    let outcome = fsync_probe(&root, &backend);

    assert!(
        outcome.is_ok(),
        "an honest tempdir on the test runner's filesystem must pass the probe; got {outcome:?}",
    );
    cleanup(&root);
}

// --------------------------------------------------------------------
// AC-3.2 (US-01 Scenario 2, US-02 Domain 2) — no-op fsync backend,
// the probe refuses with `FsyncIgnored`.
// --------------------------------------------------------------------

#[test]
fn no_op_fsync_backend_refuses_with_fsync_ignored() {
    let root = temp_root("no_op");
    let backend = LyingFsyncBackend::no_op();

    let outcome = fsync_probe(&root, &backend);

    assert!(
        matches!(outcome, Err(FsyncProbeError::FsyncIgnored)),
        "a no-op fsync substrate must refuse with FsyncIgnored; got {outcome:?}",
    );
    cleanup(&root);
}

// --------------------------------------------------------------------
// AC-3.3 (US-01 Scenario 3, US-02 Domain 3) — truncating fsync
// backend, the probe refuses with `BytesLost`.
// --------------------------------------------------------------------

#[test]
fn truncating_fsync_backend_refuses_with_bytes_lost() {
    let root = temp_root("truncating");
    let backend = LyingFsyncBackend::truncating();

    let outcome = fsync_probe(&root, &backend);

    assert!(
        matches!(outcome, Err(FsyncProbeError::BytesLost)),
        "a truncating fsync substrate must refuse with BytesLost; got {outcome:?}",
    );
    cleanup(&root);
}

// --------------------------------------------------------------------
// AC-3.4 (US-01 boundary, ADR-0049 §1 third class) — byte-flipping
// fsync backend, the probe refuses with `BytesMismatch`.
// --------------------------------------------------------------------

#[test]
fn byte_flipping_fsync_backend_refuses_with_bytes_mismatch() {
    let root = temp_root("byte_flipping");
    let backend = LyingFsyncBackend::byte_flipping();

    let outcome = fsync_probe(&root, &backend);

    assert!(
        matches!(outcome, Err(FsyncProbeError::BytesMismatch)),
        "a byte-flipping fsync substrate must refuse with BytesMismatch; got {outcome:?}",
    );
    cleanup(&root);
}

// --------------------------------------------------------------------
// AC-3.5 (edge, ADR-0049 "Negative" consequence on sentinel hygiene)
// — the probe cleans up its sentinel file on the success path so no
// accumulating state across restarts.
// --------------------------------------------------------------------

#[test]
fn probe_cleans_up_its_sentinel_on_success() {
    let root = temp_root("cleanup_on_success");
    let backend = RealFsyncBackend;

    fsync_probe(&root, &backend).expect("honest substrate must pass");

    let sentinel = root.join(SENTINEL_FILENAME);
    assert!(
        !sentinel.exists(),
        "the probe must remove its sentinel on success; found {} still present",
        sentinel.display(),
    );
    cleanup(&root);
}

// ====================================================================
// WRITE-PATH LEVEL
//
// The acceptance suite injects a CountingFsyncBackend through the
// `FileBackedMetricStore::open_with_fsync_backend` seam (ADR-0049 §6)
// and observes the fsync calls produced by `ingest` and `snapshot`.
// ====================================================================

// --------------------------------------------------------------------
// AC-3.6 (ADR-0049 §2 + §4, the Luna finding) — ingesting through
// `FileBackedMetricStore` increments the file-fsync counter at least
// once per WAL append (per-record fsync at slice 01).
// --------------------------------------------------------------------

#[test]
fn ingest_invokes_file_fsync_once_per_wal_append() {
    let root = temp_root("ingest_fsync");
    let backend = Arc::new(CountingFsyncBackend::new());
    let store = FileBackedMetricStore::open_with_fsync_backend(
        root.join("store"),
        Box::new(NoopRecorder),
        backend.clone(),
    )
    .expect("open");

    let before = backend.file_fsync_count();
    store
        .ingest(
            &tenant("acme"),
            MetricBatch::with_metrics(vec![gauge("m", "svc", vec![point(100, 1.0)])]),
        )
        .expect("ingest");
    let after = backend.file_fsync_count();

    assert!(
        after > before,
        "ingest must call fsync_file at least once (per WAL append); before={before}, after={after}",
    );
    cleanup(&root);
}

// --------------------------------------------------------------------
// AC-3.7 (ADR-0049 §5, snapshot file durability) — calling
// `snapshot()` triggers `fsync_file` on the snapshot file before the
// WAL is truncated.
// --------------------------------------------------------------------

#[test]
fn snapshot_invokes_file_fsync_on_the_snapshot_file() {
    let root = temp_root("snapshot_file_fsync");
    let backend = Arc::new(CountingFsyncBackend::new());
    let store = FileBackedMetricStore::open_with_fsync_backend(
        root.join("store"),
        Box::new(NoopRecorder),
        backend.clone(),
    )
    .expect("open");
    store
        .ingest(
            &tenant("acme"),
            MetricBatch::with_metrics(vec![gauge("m", "svc", vec![point(100, 1.0)])]),
        )
        .expect("ingest");

    let before = backend.file_fsync_count();
    store.snapshot().expect("snapshot");
    let after = backend.file_fsync_count();

    assert!(
        after > before,
        "snapshot must fsync the snapshot file; before={before}, after={after}",
    );
    cleanup(&root);
}

// --------------------------------------------------------------------
// AC-3.8 (ADR-0049 §5, POSIX rename durability) — calling
// `snapshot()` triggers a parent-directory fsync so the snapshot's
// directory entry is durable before the WAL truncate that depends
// on it (and a second one after the WAL recreate).
// --------------------------------------------------------------------

#[test]
fn snapshot_invokes_parent_directory_fsync_for_rename_durability() {
    let root = temp_root("snapshot_dir_fsync");
    let backend = Arc::new(CountingFsyncBackend::new());
    let store = FileBackedMetricStore::open_with_fsync_backend(
        root.join("store"),
        Box::new(NoopRecorder),
        backend.clone(),
    )
    .expect("open");
    store
        .ingest(
            &tenant("acme"),
            MetricBatch::with_metrics(vec![gauge("m", "svc", vec![point(100, 1.0)])]),
        )
        .expect("ingest");

    let before = backend.dir_fsync_count();
    store.snapshot().expect("snapshot");
    let after = backend.dir_fsync_count();

    assert!(
        after >= before + 2,
        "snapshot must fsync the parent directory twice (between snapshot and WAL truncate, then after the WAL recreate); before={before}, after={after}",
    );
    cleanup(&root);
}

// ====================================================================
// GATEWAY COMPOSITION SEAM (US-02 refuse pattern)
//
// The gateway's composition seam lives in
// `crates/kaleidoscope-gateway/src/composition.rs` and is unit-tested
// there (inline `#[cfg(test)]`). The cross-crate acceptance scenario
// below confirms the public pulse surface composes with the gateway's
// `probe_or_refuse` shape: an honest backend produces Ok over a real
// tempdir, a lying backend produces an Err whose substrate descriptor
// is reachable through `FsyncProbeError::substrate_descriptor`.
// ====================================================================

#[test]
fn gateway_composition_seam_refuses_on_lying_backend_and_proceeds_on_honest_one() {
    let root = temp_root("gateway_seam");

    let honest = RealFsyncBackend;
    fsync_probe(&root, &honest).expect("honest probe proceeds");

    let lying = LyingFsyncBackend::no_op();
    let refusal = fsync_probe(&root, &lying).expect_err("lying backend refuses");
    assert_eq!(
        refusal.substrate_descriptor(),
        "fsync-noop",
        "the substrate descriptor names the lie class",
    );

    cleanup(&root);
}
