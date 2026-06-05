// Kaleidoscope Sluice v1 — slice 01 WAL durability acceptance test
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

//! Slice 01 — `FileBackedQueue::open` + enqueue + dequeue + ack +
//! nack survive a restart.
//!
//! Maps to `docs/feature/sluice-v1/slices/slice-01-wal-durability.md`.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use sluice::{EnqueueError, FileBackedQueue, NoopRecorder, Queue};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("sluice-v1-{test_name}-{pid}-{nanos}"));
    fs::create_dir_all(&path).expect("mkdir");
    path.push("queue");
    path
}

fn cleanup(base: &std::path::Path) {
    if let Some(dir) = base.parent() {
        let _ = fs::remove_dir_all(dir);
    }
}

// --------------------------------------------------------------------
// AC-1.1 / AC-1.2 — open + enqueue
// --------------------------------------------------------------------

#[test]
fn open_creates_a_fresh_queue_and_enqueue_persists() {
    let base = temp_base("fresh");
    let q = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open");
    let id = q.enqueue(&tenant("acme"), b"hello".to_vec()).expect("enq");
    assert_eq!(id.0, 1);
    assert_eq!(q.depth(&tenant("acme")), 1);
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.6 — restart recovers prior state
// --------------------------------------------------------------------

#[test]
fn restart_recovers_pending_messages_in_fifo_order() {
    let base = temp_base("fifo_recover");
    {
        let q = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open 1");
        q.enqueue(&tenant("acme"), b"first".to_vec()).expect("enq");
        q.enqueue(&tenant("acme"), b"second".to_vec()).expect("enq");
        q.enqueue(&tenant("acme"), b"third".to_vec()).expect("enq");
    }

    let q2 = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open 2");
    let m1 = q2
        .dequeue(&tenant("acme"))
        .expect("dequeue is Ok")
        .expect("msg");
    assert_eq!(m1.payload, b"first".to_vec());
    let m2 = q2
        .dequeue(&tenant("acme"))
        .expect("dequeue is Ok")
        .expect("msg");
    assert_eq!(m2.payload, b"second".to_vec());
    let m3 = q2
        .dequeue(&tenant("acme"))
        .expect("dequeue is Ok")
        .expect("msg");
    assert_eq!(m3.payload, b"third".to_vec());
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.4 — ack survives restart (acked message NOT redelivered)
// --------------------------------------------------------------------

#[test]
fn acked_message_is_not_redelivered_after_restart() {
    let base = temp_base("ack_survives");
    let id_a = {
        let q = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open 1");
        let id_a = q.enqueue(&tenant("acme"), b"a".to_vec()).expect("enq a");
        q.enqueue(&tenant("acme"), b"b".to_vec()).expect("enq b");
        let m = q
            .dequeue(&tenant("acme"))
            .expect("dequeue is Ok")
            .expect("deq a");
        assert_eq!(m.id, id_a);
        q.ack(id_a).expect("ack");
        id_a
    };

    let q2 = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open 2");
    // After restart, the next dequeue must return `b`, not the
    // acked `a`.
    let m = q2
        .dequeue(&tenant("acme"))
        .expect("dequeue is Ok")
        .expect("deq");
    assert_eq!(m.payload, b"b".to_vec());
    assert_ne!(m.id, id_a);
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.5 / AC-1.7 — nack-to-head invariant preserved across restart
// --------------------------------------------------------------------

#[test]
fn nacked_message_returns_to_head_and_survives_restart() {
    let base = temp_base("nack_head");
    {
        let q = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open 1");
        let id_a = q.enqueue(&tenant("acme"), b"a".to_vec()).expect("enq a");
        q.enqueue(&tenant("acme"), b"b".to_vec()).expect("enq b");
        q.enqueue(&tenant("acme"), b"c".to_vec()).expect("enq c");
        let m = q
            .dequeue(&tenant("acme"))
            .expect("dequeue is Ok")
            .expect("deq a");
        assert_eq!(m.id, id_a);
        // Nack a; it returns to the head of acme's queue.
        q.nack(id_a).expect("nack");
    }

    let q2 = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open 2");
    // The next dequeue must return `a` (head), not `b` (was head
    // before nack).
    let m = q2
        .dequeue(&tenant("acme"))
        .expect("dequeue is Ok")
        .expect("deq");
    assert_eq!(m.payload, b"a".to_vec());
    let m = q2
        .dequeue(&tenant("acme"))
        .expect("dequeue is Ok")
        .expect("deq");
    assert_eq!(m.payload, b"b".to_vec());
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.8 — MessageId counter resumes above max(id)
// --------------------------------------------------------------------

#[test]
fn message_id_counter_resumes_above_max_after_restart() {
    let base = temp_base("counter_resumes");
    let last_id = {
        let q = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open 1");
        let mut id = sluice::MessageId(0);
        for i in 0..7 {
            id = q
                .enqueue(&tenant("acme"), format!("msg-{i}").into_bytes())
                .expect("enq");
        }
        id
    };
    assert_eq!(last_id.0, 7);

    let q2 = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open 2");
    let next = q2.enqueue(&tenant("acme"), b"after".to_vec()).expect("enq");
    assert_eq!(next.0, 8, "counter must resume above max(id)");
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.9 — Full does NOT write to WAL (verified by reopening)
// --------------------------------------------------------------------

#[test]
fn enqueue_at_capacity_returns_full_without_persisting() {
    let base = temp_base("full_no_wal");
    {
        let q = FileBackedQueue::open(&base, 2, Box::new(NoopRecorder)).expect("open 1");
        q.enqueue(&tenant("acme"), b"a".to_vec()).expect("enq");
        q.enqueue(&tenant("acme"), b"b".to_vec()).expect("enq");
        let err = q.enqueue(&tenant("acme"), b"c".to_vec()).unwrap_err();
        assert!(matches!(err, EnqueueError::Full { .. }));
    }
    // Reopen — the rejected `c` did NOT persist.
    let q2 = FileBackedQueue::open(&base, 2, Box::new(NoopRecorder)).expect("open 2");
    assert_eq!(q2.depth(&tenant("acme")), 2);
    let m = q2
        .dequeue(&tenant("acme"))
        .expect("dequeue is Ok")
        .expect("deq");
    assert_eq!(m.payload, b"a".to_vec());
    let m = q2
        .dequeue(&tenant("acme"))
        .expect("dequeue is Ok")
        .expect("deq");
    assert_eq!(m.payload, b"b".to_vec());
    assert!(q2
        .dequeue(&tenant("acme"))
        .expect("dequeue is Ok")
        .is_none());
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.10 — corrupted WAL surfaces as PersistenceFailed on open
// --------------------------------------------------------------------

#[test]
fn corrupted_wal_surfaces_typed_persistence_error() {
    let base = temp_base("corrupted");
    {
        let q = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open 1");
        q.enqueue(&tenant("acme"), b"good".to_vec()).expect("enq");
    }
    let wal_path = {
        let mut p = base.as_os_str().to_owned();
        p.push(".wal");
        PathBuf::from(p)
    };
    let existing = fs::read_to_string(&wal_path).expect("read");
    fs::write(&wal_path, format!("{existing}{{not valid json}}\n")).expect("write");

    let err = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect_err("should fail");
    assert!(matches!(err, EnqueueError::PersistenceFailed { .. }));
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-1.11 — tenant isolation across restart
// --------------------------------------------------------------------

#[test]
fn tenant_isolation_preserved_across_restart() {
    let base = temp_base("tenant_iso");
    {
        let q = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open 1");
        q.enqueue(&tenant("acme"), b"a-msg".to_vec()).expect("enq");
        q.enqueue(&tenant("globex"), b"g-msg".to_vec())
            .expect("enq");
    }
    let q2 = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open 2");
    let m = q2
        .dequeue(&tenant("acme"))
        .expect("dequeue is Ok")
        .expect("deq");
    assert_eq!(m.payload, b"a-msg".to_vec());
    assert!(q2
        .dequeue(&tenant("acme"))
        .expect("dequeue is Ok")
        .is_none());
    let m = q2
        .dequeue(&tenant("globex"))
        .expect("dequeue is Ok")
        .expect("deq");
    assert_eq!(m.payload, b"g-msg".to_vec());
    cleanup(&base);
}

// --------------------------------------------------------------------
// Payload byte-stability: arbitrary bytes (including 0x00 and high
// bytes) round-trip via hex encoding
// --------------------------------------------------------------------

#[test]
fn arbitrary_payload_bytes_round_trip_via_hex() {
    let base = temp_base("payload_bytes");
    let raw: Vec<u8> = (0..=255u8).collect();
    {
        let q = FileBackedQueue::open(&base, 10, Box::new(NoopRecorder)).expect("open 1");
        q.enqueue(&tenant("acme"), raw.clone()).expect("enq");
    }
    let q2 = FileBackedQueue::open(&base, 10, Box::new(NoopRecorder)).expect("open 2");
    let m = q2
        .dequeue(&tenant("acme"))
        .expect("dequeue is Ok")
        .expect("deq");
    assert_eq!(m.payload, raw);
    cleanup(&base);
}

// --------------------------------------------------------------------
// KPI 1 — enqueue p95 ≤ 300 µs
// --------------------------------------------------------------------

#[test]
fn enqueue_p95_latency_under_three_hundred_microseconds() {
    if std::env::var("KALEIDOSCOPE_PERF_TESTS").is_err() {
        eprintln!("perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run");
        return;
    }
    let base = temp_base("kpi1");
    let q = FileBackedQueue::open(&base, 100_000, Box::new(NoopRecorder)).expect("open");
    let tn = tenant("perf");

    for _ in 0..100 {
        q.enqueue(&tn, b"warm".to_vec()).expect("warm");
    }

    let mut samples: Vec<u128> = Vec::with_capacity(1000);
    for _ in 0..1000 {
        let t0 = std::time::Instant::now();
        q.enqueue(&tn, b"payload".to_vec()).expect("enq");
        samples.push(t0.elapsed().as_nanos());
    }
    samples.sort_unstable();
    let p95_ns = samples[950];
    let p95_us = p95_ns / 1_000;
    assert!(
        p95_us <= 300,
        "KPI 1: enqueue p95 must be ≤ 300 µs; got {p95_us} µs ({p95_ns} ns) (first 10 ns {:?})",
        &samples[..10]
    );
    cleanup(&base);
}
