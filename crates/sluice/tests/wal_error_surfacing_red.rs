// Kaleidoscope Sluice — WAL-error-surfacing acceptance (behaviourally-RED)
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

//! # Acceptance — sluice surfaces WAL persistence failures (US-04, R3)
//!
//! Feature: `cinder-wal-error-surfacing-v0`. Wave: DISTILL. `@uniformity` slice.
//!
//! sluice's `FileBackedQueue::{dequeue, ack, nack}` SWALLOW WAL append failures
//! (`crates/sluice/src/file_backed.rs:346,356,366`, each `let _ = append_wal(..)`)
//! then mutate `pending`/`in_flight`/`total` UNCONDITIONALLY — the same
//! acked-but-not-durable lie as cinder, in the `Queue` trait's state-mutating ops.
//! sluice is UNWIRED (zero live blast radius); this slice is pure durability-posture
//! uniformity so a future wiring inherits a fail-loud queue (ADR-0065 D4).
//!
//! ## Why these compile and run RED TODAY (Mandate 7 — RED-not-BROKEN)
//!
//! These call the EXISTING `dequeue -> Option` / `ack -> ()` / `nack -> ()`
//! signatures (file compiles against today's surface; does not break the build).
//! They inject a `FailingFsyncBackend` so `append_wal` returns `Err`. The
//! load-bearing assertion is the **write-ahead-ordering invariant on the live
//! queue state**: a failing `dequeue` must NOT move the message out of `pending`
//! (depth unchanged); a failing `ack`/`nack` must NOT mutate the in-flight/pending
//! state. TODAY this FAILS because the swallow path mutates the queue state anyway.
//!
//! The intended post-fix API assertions (`dequeue` returns
//! `Result<Option<Message>, EnqueueError>` etc.) live in the non-compiled
//! companion spec `wal_error_surfacing.intended.rs` next to this file; DELIVER
//! moves it into `tests/` with the signature change and un-ignores one at a time.

use std::env;
use std::fs;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use sluice::{EnqueueError, FileBackedQueue, FsyncBackend, NoopRecorder, Queue};

// --------------------------------------------------------------------
// Failing substrate (the falsifiability seam) — same shape as the cinder
// test's double. `FsyncBackend` is public via `sluice::FsyncBackend`.
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
// Harness (mirrors v1_slice_01_wal_durability.rs).
// --------------------------------------------------------------------

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn temp_base(name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("sluice-wal-err-{name}-{pid}-{nanos}"));
    fs::create_dir_all(&path).expect("mkdir");
    path.push("queue");
    path
}

fn cleanup(base: &Path) {
    if let Some(dir) = base.parent() {
        let _ = fs::remove_dir_all(dir);
    }
}

fn open_failing(base: &Path, cap: usize) -> FileBackedQueue {
    FileBackedQueue::open_with_fsync_backend(
        base,
        cap,
        Box::new(NoopRecorder),
        Arc::new(FailingFsyncBackend),
    )
    .expect("open with failing backend (open succeeds; only appends fail)")
}

// ====================================================================
// US-04 negative control — healthy-disk enqueue/dequeue/ack persist and
// survive a reopen. Compiles + passes TODAY and post-fix.
//
// Scenario: Healthy-disk queue operations persist durably.
// ====================================================================

#[test]
fn healthy_queue_dequeue_then_ack_persists_across_reopen() {
    let base = temp_base("healthy");

    {
        let q = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open ok");
        q.enqueue(&tenant("acme"), b"m1".to_vec()).expect("enq");
        let msg = q
            .dequeue(&tenant("acme"))
            .expect("dequeue is Ok")
            .expect("dequeue returns the message");
        q.ack(msg.id).expect("ack is Ok");
        assert_eq!(q.depth(&tenant("acme")), 0, "acked message is gone");
    }

    // Reopen: the message stays acked (not redelivered).
    let reopened = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("reopen");
    assert_eq!(
        reopened.depth(&tenant("acme")),
        0,
        "acked message does not reappear after a reopen"
    );

    cleanup(&base);
}

// ====================================================================
// US-04 negative control — a successful dequeue decrements the
// cross-tenant pending total by exactly one. Pins `state.total -= 1` in
// the write-ahead-ordered dequeue so a mutant that adds (or divides)
// instead of subtracting is caught by an observable `total_depth`.
// ====================================================================

#[test]
fn healthy_dequeue_decrements_total_pending_by_one() {
    let base = temp_base("dequeue_total");
    let q = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open ok");
    // Two pending across two tenants: total_depth == 2.
    q.enqueue(&tenant("acme"), b"m1".to_vec()).expect("enq m1");
    q.enqueue(&tenant("globex"), b"m2".to_vec())
        .expect("enq m2");
    assert_eq!(q.total_depth(), 2, "two messages pending across tenants");

    // One successful dequeue removes exactly one from the pending total.
    let _ = q
        .dequeue(&tenant("acme"))
        .expect("dequeue is Ok")
        .expect("dequeue returns the message");
    assert_eq!(
        q.total_depth(),
        1,
        "a successful dequeue decrements the cross-tenant pending total by one"
    );

    cleanup(&base);
}

// ====================================================================
// US-04 #2 — a failing-disk dequeue is surfaced, not swallowed; the
// in-memory queue state stays consistent (the message stays pending).
//
// Scenario: A failing disk on dequeue is surfaced, not swallowed.
//
// RED TODAY: today `dequeue` pops the message into in_flight and
// decrements total, THEN swallows the WAL error — so the live depth drops
// to 0 even though the Dequeue record never persisted. Post-fix
// (write-ahead) the failing append returns Err BEFORE the state mutation,
// so the message stays pending (depth 1). This asserts depth stays 1.
// ====================================================================

#[test]
fn failing_dequeue_keeps_message_pending() {
    let base = temp_base("failing_dequeue");

    // Seed one durable message on a HEALTHY substrate.
    {
        let healthy = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open ok");
        healthy
            .enqueue(&tenant("acme"), b"m1".to_vec())
            .expect("enq");
    }

    // Reopen with a failing substrate.
    let failing = open_failing(&base, 100);
    assert_eq!(
        failing.depth(&tenant("acme")),
        1,
        "precondition: the enqueued message is recovered as pending"
    );

    // When the consumer dequeues while the disk is failing.
    let result = failing.dequeue(&tenant("acme"));

    // Then the operation surfaces a persistence failure.
    assert!(
        matches!(result, Err(EnqueueError::PersistenceFailed { .. })),
        "failing dequeue surfaces PersistenceFailed; got {result:?}"
    );

    // Then the message stays pending (depth unchanged) — the failed
    // Dequeue WAL append must not move the message in-flight in memory.
    // RED on the swallow bug (which drops depth to 0).
    assert_eq!(
        failing.depth(&tenant("acme")),
        1,
        "write-ahead ordering: a failing dequeue must leave the message \
         pending (consistent with disk), not move it in-flight in memory"
    );

    cleanup(&base);
}

// ====================================================================
// US-04 #3 — a failing-disk ack is surfaced, not swallowed; the in-flight
// message is not silently removed when its Ack record cannot persist.
//
// Scenario: A failing disk on ack/nack is surfaced, not swallowed.
//
// RED TODAY: `ack` removes the message from in_flight then swallows the
// WAL error, so a subsequent `nack` of the same id finds nothing to
// redeliver — the message is silently lost despite the Ack never
// persisting. Post-fix the failing ack returns Err BEFORE removing from
// in_flight, so the message is still in-flight and is NOT silently lost.
//
// Discriminator note (grounded in `append_wal` + write-ahead ordering): the
// in-flight set is private state with no `depth()` projection (depth counts
// pending only), and a same-host reopen is page-cache-ambiguous (append_wal
// write+flushes the Ack bytes BEFORE fsync, so a failing ack still leaves the
// Ack record readable on a reopen — DWD-2). The honest, falsifiable LIVE-handle
// observable that the message is still in-flight is therefore a SECOND ack of
// the same id: post-fix the message is still in_flight, so the second ack again
// reaches the (still-failing) WAL append and surfaces `Err`; on the swallow bug
// the first ack already removed it from in_flight, so the second ack finds
// nothing to ack and is a no-op `Ok(())`. The `Err` on the repeat ack is the
// proof the failing ack did NOT remove the message.
// ====================================================================

#[test]
fn failing_ack_does_not_silently_lose_the_in_flight_message() {
    let base = temp_base("failing_ack");

    // Seed and durably dequeue one message on a HEALTHY substrate so it is
    // in-flight with a persisted Dequeue record.
    let msg_id = {
        let healthy = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open ok");
        healthy
            .enqueue(&tenant("acme"), b"m1".to_vec())
            .expect("enq");
        let msg = healthy
            .dequeue(&tenant("acme"))
            .expect("dequeue is Ok")
            .expect("dequeue returns the message");
        msg.id
    };

    // Reopen with a failing substrate; the message is recovered in-flight.
    let failing = open_failing(&base, 100);
    assert_eq!(
        failing.depth(&tenant("acme")),
        0,
        "precondition: the message is in-flight (not pending) after the durable dequeue"
    );

    // When the consumer acks while the disk is failing (the Ack record
    // cannot persist).
    let result = failing.ack(msg_id);

    // Then the operation surfaces a persistence failure.
    assert!(
        matches!(result, Err(EnqueueError::PersistenceFailed { .. })),
        "failing ack surfaces PersistenceFailed; got {result:?}"
    );

    // And the failing ack must NOT have removed the message from in-flight: a
    // SECOND ack of the same id again reaches the failing WAL append and
    // surfaces `Err` (proving the message is still in-flight). RED on the
    // swallow bug, where the first ack already removed it, so the second ack
    // finds nothing to ack and returns a no-op `Ok(())`.
    let repeat = failing.ack(msg_id);
    assert!(
        matches!(repeat, Err(EnqueueError::PersistenceFailed { .. })),
        "the failing ack must not silently remove the in-flight message; a \
         repeat ack still surfaces PersistenceFailed (got {repeat:?})"
    );

    cleanup(&base);
}
