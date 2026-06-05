// COMPANION SPEC — inert until DELIVER. See the cinder
// `wal_error_surfacing.intended.rs` header for the full DELIVER note.
//
// RED: intended post-fix API per ADR-0065 D4. DELIVER removes the
// `#![cfg(any())]` gate IN THE SAME COMMIT that changes the Queue
// signatures to fallible, switches the call sites, and un-ignores the
// scenarios ONE AT A TIME as the outside-in GREEN loop.

#![cfg(any())] // inert until DELIVER removes this and the Queue API becomes fallible

//! # Intended post-fix acceptance — sluice surfaces WAL failures (US-04, R3)
//!
//! Mirror of `wal_error_surfacing_red.rs` re-expressed against the POST-FIX
//! fallible `Queue` API (`dequeue -> Result<Option<Message>, EnqueueError>`,
//! `ack`/`nack -> Result<(), EnqueueError>`). `@uniformity` / R3 carpaccio slice.

use std::fs::File;
use std::io::{self, ErrorKind};
use std::path::Path;
use std::sync::Arc;

use aegis::TenantId;
use sluice::{EnqueueError, FileBackedQueue, FsyncBackend, NoopRecorder, Queue};

struct FailingFsyncBackend;
impl FsyncBackend for FailingFsyncBackend {
    fn fsync_file(&self, _f: &File) -> io::Result<()> {
        Err(io::Error::new(ErrorKind::Other, "no space left on device"))
    }
    fn fsync_dir(&self, _d: &Path) -> io::Result<()> {
        Ok(())
    }
}

// ----- US-04 #2: failing dequeue surfaces, stays consistent -----
//
// Scenario: A failing disk on dequeue is surfaced, not swallowed.
#[test]
fn failing_dequeue_returns_persistence_failed_and_keeps_pending() {
    let base = std::env::temp_dir().join("intended-sluice-dq/queue");
    {
        let healthy = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open");
        healthy
            .enqueue(&TenantId("acme".into()), b"m1".to_vec())
            .expect("enq");
    }
    let failing = FileBackedQueue::open_with_fsync_backend(
        &base,
        100,
        Box::new(NoopRecorder),
        Arc::new(FailingFsyncBackend),
    )
    .expect("reopen");

    // POST-FIX: dequeue -> Result<Option<Message>, EnqueueError>.
    let result = failing.dequeue(&TenantId("acme".into()));
    assert!(
        matches!(result, Err(EnqueueError::PersistenceFailed { .. })),
        "failing dequeue surfaces PersistenceFailed; got {result:?}"
    );
    assert_eq!(
        failing.depth(&TenantId("acme".into())),
        1,
        "the message stays pending (consistent with disk)"
    );
}

// ----- US-04 #3: failing ack surfaces, stays consistent -----
//
// Scenario: A failing disk on ack/nack is surfaced, not swallowed.
#[test]
fn failing_ack_returns_persistence_failed_and_keeps_in_flight() {
    let base = std::env::temp_dir().join("intended-sluice-ack/queue");
    let id = {
        let healthy = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open");
        healthy
            .enqueue(&TenantId("acme".into()), b"m1".to_vec())
            .expect("enq");
        // POST-FIX: dequeue is fallible; healthy path is Ok(Some(msg)).
        healthy
            .dequeue(&TenantId("acme".into()))
            .expect("dequeue Ok")
            .expect("Some message")
            .id
    };
    let failing = FileBackedQueue::open_with_fsync_backend(
        &base,
        100,
        Box::new(NoopRecorder),
        Arc::new(FailingFsyncBackend),
    )
    .expect("reopen");

    // POST-FIX: ack -> Result<(), EnqueueError>.
    let result = failing.ack(id);
    assert!(
        matches!(result, Err(EnqueueError::PersistenceFailed { .. })),
        "failing ack surfaces PersistenceFailed; got {result:?}"
    );
    // The in-flight message is NOT removed; a nack redelivers it.
    let _ = failing.nack(id);
    assert_eq!(
        failing.depth(&TenantId("acme".into())),
        1,
        "the in-flight message is not silently lost; nack redelivers it"
    );
}

// ----- US-04 negative control: healthy dequeue/ack are Ok -----
#[test]
fn healthy_dequeue_ack_are_ok() {
    let base = std::env::temp_dir().join("intended-sluice-ok/queue");
    let q = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open");
    q.enqueue(&TenantId("acme".into()), b"m1".to_vec())
        .expect("enq");
    let msg = q
        .dequeue(&TenantId("acme".into()))
        .expect("dequeue Ok")
        .expect("Some");
    q.ack(msg.id).expect("ack Ok");
    assert_eq!(q.depth(&TenantId("acme".into())), 0, "acked message is gone");
}
