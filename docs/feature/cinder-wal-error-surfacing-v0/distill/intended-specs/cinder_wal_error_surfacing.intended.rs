// COMPANION SPEC — NOT COMPILED (filename does not end in a Cargo test
// target shape under tests/ that Cargo builds: it ends in `.intended.rs`,
// and Cargo only builds `tests/*.rs` top-level files — this IS a top-level
// .rs file, so to keep it out of the build DELIVER must either move it or
// the crate's Cargo.toml must not auto-discover it. See the note below.)
//
// RED: intended post-fix API per ADR-0065 D1 / D3. DELIVER renames this to
// `wal_error_surfacing_intended.rs` (or merges it into
// wal_error_surfacing_red.rs) IN THE SAME COMMIT that changes the
// TieringStore signatures, switches the call sites, and un-ignores the
// scenarios ONE AT A TIME as the outside-in GREEN loop.
//
// IMPORTANT FOR DELIVER: because Cargo auto-discovers every `tests/*.rs`
// file as an integration test target, this `.intended.rs` file WOULD be
// picked up and FAIL TO COMPILE today (it calls the not-yet-fallible API).
// It is therefore authored with a leading `#![cfg(any())]` so it compiles
// to nothing until DELIVER removes that attribute. This is the smallest
// inert form; it is NOT a fake scaffold (it asserts the real intended
// contract), it is simply gated off until the signature lands.

#![cfg(any())] // inert until DELIVER removes this and the API becomes fallible

//! # Intended post-fix acceptance — cinder surfaces WAL failures (US-01, US-03)
//!
//! Mirror of `wal_error_surfacing_red.rs` re-expressed against the POST-FIX
//! fallible API. The RED file proves the defect TODAY via the memory-untouched
//! invariant on the present `() / usize` signatures; THIS file pins the exact
//! surfaced-error contract DELIVER must satisfy.

use std::fs::File;
use std::io::{self, ErrorKind};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{
    FileBackedTieringStore, FsyncBackend, ItemId, MigrateError, NoopRecorder, Tier, TierPolicy,
    TieringStore,
};

struct FailingFsyncBackend;
impl FsyncBackend for FailingFsyncBackend {
    fn fsync_file(&self, _f: &File) -> io::Result<()> {
        Err(io::Error::new(ErrorKind::Other, "no space left on device"))
    }
    fn fsync_dir(&self, _d: &Path) -> io::Result<()> {
        Ok(())
    }
}

fn t(s: u64) -> std::time::SystemTime {
    UNIX_EPOCH + Duration::from_secs(s)
}

// ----- US-01 #1: a failing disk makes a placement fail loudly -----
//
// Scenario: A failing disk makes a placement fail loudly.
//   Given Priya has a cinder store whose WAL append fails
//   When Priya places "trade-002" for "acme" in "warm"
//   Then the operation returns a persistence-failure error naming the disk reason
//   And no success is reported
#[test]
fn failing_place_returns_persistence_failed() {
    let store = FileBackedTieringStore::open_with_fsync_backend(
        std::env::temp_dir().join("intended-place-fail/store"),
        Box::new(NoopRecorder),
        Arc::new(FailingFsyncBackend),
    )
    .expect("open");

    // POST-FIX: place returns Result<(), MigrateError>.
    let result = store.place(
        &TenantId("acme".into()),
        &ItemId::new("trade-002"),
        Tier::Warm,
        t(1_000),
    );
    assert!(
        matches!(result, Err(MigrateError::PersistenceFailed { .. })),
        "failing-disk place surfaces PersistenceFailed; got {result:?}"
    );
    // And memory untouched.
    assert_eq!(
        store.get_tier(&TenantId("acme".into()), &ItemId::new("trade-002")),
        None,
        "memory untouched on the failed append"
    );
}

// ----- US-01 #3: failed overwrite preserves prior durable value, surfaced -----
#[test]
fn failing_overwrite_surfaces_error_and_preserves_prior_value() {
    let base = std::env::temp_dir().join("intended-overwrite/store");
    {
        let healthy = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open");
        healthy
            .place(&TenantId("globex".into()), &ItemId::new("batch-007"), Tier::Hot, t(1_000))
            .expect("healthy place is Ok"); // POST-FIX: Result
    }
    let failing = FileBackedTieringStore::open_with_fsync_backend(
        &base,
        Box::new(NoopRecorder),
        Arc::new(FailingFsyncBackend),
    )
    .expect("reopen");
    let result = failing.place(
        &TenantId("globex".into()),
        &ItemId::new("batch-007"),
        Tier::Cold,
        t(2_000),
    );
    assert!(matches!(result, Err(MigrateError::PersistenceFailed { .. })));
    assert_eq!(
        failing.get_tier(&TenantId("globex".into()), &ItemId::new("batch-007")),
        Some(Tier::Hot),
        "failed overwrite preserves the prior durable Hot"
    );
}

// ----- US-03: fail-whole sweep surfaces error; count never overstates -----
//
// Scenario: A sweep on a failing disk surfaces a persistence-failure error
// and the count never includes a migration that is not on disk (D3 fail-whole).
#[test]
fn failing_sweep_returns_persistence_failed_and_no_count() {
    let base = std::env::temp_dir().join("intended-sweep/store");
    {
        let healthy = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open");
        for id in ["a", "b", "c"] {
            healthy
                .place(&TenantId("acme".into()), &ItemId::new(id), Tier::Hot, t(0))
                .expect("Ok");
        }
    }
    let failing = FileBackedTieringStore::open_with_fsync_backend(
        &base,
        Box::new(NoopRecorder),
        Arc::new(FailingFsyncBackend),
    )
    .expect("reopen");
    let policy = TierPolicy::age_based(Duration::from_secs(3_600), Duration::from_secs(86_400));

    // POST-FIX: evaluate_at returns Result<usize, MigrateError>; fail-whole
    // returns Err on the first WAL failure, carrying NO count.
    let result = failing.evaluate_at(t(7_200), &policy);
    assert!(
        matches!(result, Err(MigrateError::PersistenceFailed { .. })),
        "fail-whole sweep surfaces PersistenceFailed; got {result:?}"
    );
    // And no item migrated in memory (write-ahead ordering per item).
    for id in ["a", "b", "c"] {
        assert_eq!(
            failing.get_tier(&TenantId("acme".into()), &ItemId::new(id)),
            Some(Tier::Hot),
            "{id} stays Hot — no non-durable migration applied"
        );
    }
}

// ----- US-03 negative control: Ok(n) == durable count -----
#[test]
fn healthy_sweep_ok_count_equals_durable() {
    let base = std::env::temp_dir().join("intended-sweep-ok/store");
    let store = FileBackedTieringStore::open(&base, Box::new(NoopRecorder)).expect("open");
    for id in ["a", "b", "c"] {
        store
            .place(&TenantId("acme".into()), &ItemId::new(id), Tier::Hot, t(0))
            .expect("Ok");
    }
    let policy = TierPolicy::age_based(Duration::from_secs(3_600), Duration::from_secs(86_400));
    // POST-FIX: Ok(count); count == durably-migrated.
    let n = store
        .evaluate_at(t(7_200), &policy)
        .expect("healthy sweep is Ok");
    assert_eq!(n, 3, "Ok(n) equals the durably-migrated count");
}
