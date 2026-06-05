// Kaleidoscope Cinder — WAL-error-surfacing acceptance (behaviourally-RED)
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

//! # Acceptance — cinder surfaces WAL persistence failures (US-01, US-03)
//!
//! Feature: `cinder-wal-error-surfacing-v0`. Wave: DISTILL (acceptance-designer).
//!
//! ## What these tests pin (the falsifiability proof — runs RED TODAY)
//!
//! cinder's `FileBackedTieringStore::place` and `evaluate_at` SWALLOW WAL append
//! failures (`crates/cinder/src/file_backed.rs:270-278` and `:364-368`) and
//! mutate the in-memory tier map UNCONDITIONALLY — the acked-but-not-durable lie
//! (ADR-0065). The fix (DELIVER) makes both operations FALLIBLE and
//! write-ahead-ordered: append to the WAL FIRST, mutate memory ONLY on `Ok`.
//!
//! ## Why these compile and run RED TODAY (Mandate 7 — RED-not-BROKEN)
//!
//! These tests call the EXISTING `place(...) -> ()` / `evaluate_at(...) -> usize`
//! signatures (so the file compiles against today's surface and does NOT break
//! the workspace build / pre-commit hook — DEVOPS C-DEVOPS-3). They inject a
//! `FailingFsyncBackend` (defined below) whose `fsync_file` returns `io::Error`,
//! so `append_wal` returns `Err(PersistenceFailed{..})`. The load-bearing
//! assertion is the **write-ahead-ordering invariant on the LIVE handle**: after a
//! failing `place`, the in-memory map must be UNTOUCHED, so `get_tier` returns the
//! PRIOR value (overwrite) or `None` (fresh). TODAY this FAILS because the swallow
//! path mutates memory anyway. POST-FIX it passes because the `?` returns before
//! `apply_to_entries`.
//!
//! Grounded substrate note: `append_wal` `write_all`s + `flush`es the record to
//! the OS page cache BEFORE `fsync_file`, so an in-process reopen would still read
//! the record back (the page cache survives a same-host reopen — ADR-0060 §1). The
//! reliable discriminator for THIS error-surfacing feature is therefore
//! memory-untouched on the LIVE handle, NOT absence-on-reopen. See
//! `docs/feature/cinder-wal-error-surfacing-v0/distill/wave-decisions.md` DWD-2.
//!
//! The intended post-fix API assertions (`place(...)` returns
//! `Err(PersistenceFailed)` directly) live in the non-compiled companion spec
//! `wal_error_surfacing.intended.rs` next to this file; DELIVER moves it into
//! `tests/` alongside the signature change and un-ignores one scenario at a time.

use std::env;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{
    FileBackedTieringStore, FsyncBackend, ItemId, MigrateError, NoopRecorder, Tier, TierPolicy,
    TieringStore,
};

// --------------------------------------------------------------------
// Failing substrate (the falsifiability seam). `wal-recovery` ships NO
// write/fsync-FAILING backend today (Real/Counting always Ok; Lying
// returns Ok and lies by dropping bytes). `FsyncBackend` is public, so
// this test crate implements its own failing double: `fsync_file`
// returns io::Error => `append_wal` returns PersistenceFailed; `fsync_dir`
// returns Ok so `open` still creates the store. DELIVER may promote this
// into wal-recovery (additive, behaviour-preserving) — not required here.
// --------------------------------------------------------------------

struct FailingFsyncBackend;

impl FsyncBackend for FailingFsyncBackend {
    fn fsync_file(&self, _file: &File) -> io::Result<()> {
        Err(io::Error::other("no space left on device"))
    }

    fn fsync_dir(&self, _dir: &Path) -> io::Result<()> {
        Ok(())
    }
}

// --------------------------------------------------------------------
// Harness (mirrors v1_slice_01_wal_durability.rs: real temp-dir WAL on
// the real local filesystem — Strategy C real-local-IO per DWD-1).
// --------------------------------------------------------------------

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn item(id: &str) -> ItemId {
    ItemId::new(id)
}

fn t(secs: u64) -> std::time::SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

fn temp_base(name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("cinder-wal-err-{name}-{pid}-{nanos}"));
    std::fs::create_dir_all(&path).expect("mkdir");
    path.push("store");
    path
}

fn cleanup(base: &Path) {
    if let Some(dir) = base.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

fn open_failing(base: &Path) -> FileBackedTieringStore {
    FileBackedTieringStore::open_with_fsync_backend(
        base,
        Box::new(NoopRecorder),
        Arc::new(FailingFsyncBackend),
    )
    .expect("open with failing backend (open itself succeeds; only appends fail)")
}

// ====================================================================
// US-01 #3 (the cleanest discriminator) — a failed overwrite preserves
// the prior durable placement.
//
// Scenario: A failed overwrite preserves the prior durable placement.
//   Given "globex" / "batch-007" is durably placed in tier "hot"
//   And the disk subsequently begins failing on WAL append
//   When Priya places "globex" / "batch-007" in tier "cold"
//   Then the operation returns a persistence-failure error  [intended spec]
//   And reading the tier for "globex" / "batch-007" still returns "hot"
//   And after reopening the store the tier is still "hot"
//
// RED TODAY: today `place` swallows the WAL error and mutates memory, so
// the live `get_tier` returns Cold (the un-persisted overwrite). This
// assertion (prior value Hot survives in memory) FAILS on the swallow bug
// and passes only on the write-ahead-ordered fix.
// ====================================================================

#[test]
fn failed_overwrite_preserves_prior_durable_placement_in_memory() {
    let base = temp_base("overwrite_preserves");

    // Given a durable Hot placement on a HEALTHY substrate.
    {
        let healthy = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open ok");
        healthy
            .place(&tenant("globex"), &item("batch-007"), Tier::Hot, t(1_000))
            .expect("healthy place is Ok");
        // dropped: BufWriter flushes; the Hot record is durable.
    }

    // And the disk begins failing on WAL append (reopen with failing backend).
    let failing = open_failing(&base);
    assert_eq!(
        failing.get_tier(&tenant("globex"), &item("batch-007")),
        Some(Tier::Hot),
        "precondition: the prior durable value is recovered as Hot"
    );

    // When Priya places the SAME key in Cold while the disk is failing.
    let result = failing.place(&tenant("globex"), &item("batch-007"), Tier::Cold, t(2_000));

    // Then the operation surfaces a persistence failure.
    assert!(
        matches!(result, Err(MigrateError::PersistenceFailed { .. })),
        "failing-disk overwrite surfaces PersistenceFailed; got {result:?}"
    );

    // And the in-memory map STILL reads Hot (the failed overwrite did not
    // torn-mutate the prior durable value). RED on the swallow bug, which
    // mutates memory to Cold despite the WAL append failing.
    assert_eq!(
        failing.get_tier(&tenant("globex"), &item("batch-007")),
        Some(Tier::Hot),
        "write-ahead ordering: a failed overwrite must leave the prior durable \
         value (Hot) intact in memory; got the un-persisted overwrite instead"
    );

    cleanup(&base);
}

// ====================================================================
// US-01 #2 — a fresh placement that failed to persist is not visible
// in the in-memory map (memory untouched on failure).
//
// Scenario: A placement that failed to persist is not readable.
//   Given Priya places item "trade-002" for tenant "acme" against a
//         failing disk
//   When Priya reads the tier for "acme" / "trade-002"
//   Then the read returns no placement
//   And the in-memory state matches what is on disk (nothing was written)
//
// RED TODAY: the swallow path mutates memory, so the live handle returns
// Some(Warm). The fix leaves memory untouched => None.
// ====================================================================

#[test]
fn failed_fresh_placement_is_not_visible_in_memory() {
    let base = temp_base("fresh_not_visible");
    let failing = open_failing(&base);

    // Pre: nothing placed.
    assert_eq!(
        failing.get_tier(&tenant("acme"), &item("trade-002")),
        None,
        "precondition: no prior placement"
    );

    // When a fresh place hits the failing substrate.
    let result = failing.place(&tenant("acme"), &item("trade-002"), Tier::Warm, t(1_000));

    // Then the operation surfaces a persistence failure.
    assert!(
        matches!(result, Err(MigrateError::PersistenceFailed { .. })),
        "failing-disk fresh place surfaces PersistenceFailed; got {result:?}"
    );

    // And the live in-memory map shows NO placement (memory untouched on
    // the failed append). RED on the swallow bug (memory mutated to Warm).
    assert_eq!(
        failing.get_tier(&tenant("acme"), &item("trade-002")),
        None,
        "write-ahead ordering: a failed fresh place must leave memory \
         untouched; the un-persisted placement must not be readable"
    );

    cleanup(&base);
}

// ====================================================================
// US-01 negative control — a HEALTHY disk places and persists normally.
// Guardrail: the surfacing change must NOT regress the green path.
// Compiles + passes TODAY and post-fix.
// ====================================================================

#[test]
fn healthy_disk_places_and_persists_across_reopen() {
    let base = temp_base("healthy_place");

    {
        let store = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open ok");
        store
            .place(&tenant("acme"), &item("trade-001"), Tier::Hot, t(1_000))
            .expect("healthy place is Ok");
        assert_eq!(
            store.get_tier(&tenant("acme"), &item("trade-001")),
            Some(Tier::Hot),
            "healthy place is readable on the live handle"
        );
    }

    // And it is durable across a real reopen.
    let reopened = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    assert_eq!(
        reopened.get_tier(&tenant("acme"), &item("trade-001")),
        Some(Tier::Hot),
        "healthy place is durable across a reopen"
    );

    cleanup(&base);
}

// ====================================================================
// US-03 #1 negative control — a HEALTHY sweep migrates and the count
// equals the durably-migrated items, all durable across a reopen.
// Compiles + passes TODAY and post-fix.
//
// Scenario: A healthy-disk sweep reports a count equal to the durably-
// migrated items.
// ====================================================================

#[test]
fn healthy_sweep_count_equals_durable_migrations() {
    let base = temp_base("healthy_sweep");
    let policy = TierPolicy::age_based(Duration::from_secs(3_600), Duration::from_secs(86_400));

    {
        let store = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open ok");
        // Three Hot items aged past the hot->warm threshold at now=t(7200).
        store
            .place(&tenant("acme"), &item("a"), Tier::Hot, t(0))
            .expect("Ok");
        store
            .place(&tenant("acme"), &item("b"), Tier::Hot, t(0))
            .expect("Ok");
        store
            .place(&tenant("acme"), &item("c"), Tier::Hot, t(0))
            .expect("Ok");

        let migrated = store
            .evaluate_at(t(7_200), &policy)
            .expect("healthy sweep is Ok");
        assert_eq!(migrated, 3, "all three due items migrate on a healthy disk");
    }

    // And all three are durably Warm across a reopen.
    let reopened = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    for id in ["a", "b", "c"] {
        assert_eq!(
            reopened.get_tier(&tenant("acme"), &item(id)),
            Some(Tier::Warm),
            "migrated item {id} is durably Warm across a reopen"
        );
    }

    cleanup(&base);
}

// ====================================================================
// US-03 #2 — a sweep on a failing disk does not migrate items in memory
// that were never persisted (the count never overstates durability).
//
// Scenario: A sweep on a failing disk does not report migrations that
// were never persisted.
//
// RED TODAY: today `evaluate_at` does `let _ = append_wal(...)` per item
// then mutates memory UNCONDITIONALLY, so after a failing sweep the live
// handle shows items migrated to Warm even though no WAL record persisted.
// The post-fix fail-whole sweep (D3) appends-first and returns on the
// first Err, so a failing item is NOT migrated in memory. This asserts
// the items remain Hot in memory after a failing sweep — RED on the bug.
// ====================================================================

#[test]
fn failing_sweep_does_not_migrate_in_memory_without_persistence() {
    let base = temp_base("failing_sweep");
    let policy = TierPolicy::age_based(Duration::from_secs(3_600), Duration::from_secs(86_400));

    // Seed three due Hot items on a HEALTHY substrate so they are durable.
    {
        let healthy = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open ok");
        healthy
            .place(&tenant("acme"), &item("a"), Tier::Hot, t(0))
            .expect("Ok");
        healthy
            .place(&tenant("acme"), &item("b"), Tier::Hot, t(0))
            .expect("Ok");
        healthy
            .place(&tenant("acme"), &item("c"), Tier::Hot, t(0))
            .expect("Ok");
    }

    // Reopen with a failing substrate and run the sweep.
    let failing = open_failing(&base);
    // Precondition: all three recovered as Hot.
    for id in ["a", "b", "c"] {
        assert_eq!(
            failing.get_tier(&tenant("acme"), &item(id)),
            Some(Tier::Hot),
            "precondition: {id} recovered as Hot"
        );
    }

    let result = failing.evaluate_at(t(7_200), &policy);

    // Then the sweep surfaces a persistence failure (D3 fail-whole, no count).
    assert!(
        matches!(result, Err(MigrateError::PersistenceFailed { .. })),
        "fail-whole sweep surfaces PersistenceFailed; got {result:?}"
    );

    // Then NO item is migrated to Warm in the in-memory map — every WAL
    // append failed, so under write-ahead ordering none of the in-memory
    // tiers may move. RED on the swallow bug (which migrates all three in
    // memory despite the failed appends).
    for id in ["a", "b", "c"] {
        assert_eq!(
            failing.get_tier(&tenant("acme"), &item(id)),
            Some(Tier::Hot),
            "fail-whole sweep: {id} must stay Hot in memory when its WAL \
             append fails; a non-durable migration must not be applied"
        );
    }

    cleanup(&base);
}
