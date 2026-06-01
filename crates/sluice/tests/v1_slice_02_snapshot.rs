// Kaleidoscope Sluice v1 — slice 02 snapshot acceptance test
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

//! Slice 02 — snapshot compaction
//!
//! Maps to `docs/feature/sluice-v1/slices/slice-02-snapshot.md`.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use sluice::{FileBackedQueue, NoopRecorder, Queue};

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
    path.push(format!("sluice-v1-snap-{test_name}-{pid}-{nanos}"));
    fs::create_dir_all(&path).expect("mkdir");
    path.push("queue");
    path
}

fn cleanup(base: &std::path::Path) {
    if let Some(dir) = base.parent() {
        let _ = fs::remove_dir_all(dir);
    }
}

fn wal_size_bytes(base: &std::path::Path) -> u64 {
    let mut p = base.as_os_str().to_owned();
    p.push(".wal");
    fs::metadata(PathBuf::from(p)).map(|m| m.len()).unwrap_or(0)
}

fn snapshot_exists(base: &std::path::Path) -> bool {
    let mut p = base.as_os_str().to_owned();
    p.push(".snapshot");
    PathBuf::from(p).exists()
}

// --------------------------------------------------------------------
// AC-2.1 — snapshot writes state file + truncates WAL
// --------------------------------------------------------------------

#[test]
fn snapshot_writes_state_and_truncates_wal() {
    let base = temp_base("writes_truncates");
    let q = FileBackedQueue::open(&base, 1000, Box::new(NoopRecorder)).expect("open");
    for i in 0..50u32 {
        q.enqueue(&tenant("acme"), format!("m-{i}").into_bytes())
            .expect("enq");
    }
    let wal_before = wal_size_bytes(&base);
    assert!(wal_before > 0);
    assert!(!snapshot_exists(&base));

    q.snapshot().expect("snapshot");

    assert_eq!(wal_size_bytes(&base), 0, "WAL truncated after snapshot");
    assert!(snapshot_exists(&base));
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-2.2 / AC-2.3 — open reads snapshot then replays WAL
// --------------------------------------------------------------------

#[test]
fn open_reads_snapshot_then_replays_remaining_wal() {
    let base = temp_base("snap_then_replay");
    {
        let q = FileBackedQueue::open(&base, 1000, Box::new(NoopRecorder)).expect("open 1");
        for i in 0..20u32 {
            q.enqueue(&tenant("acme"), format!("s-{i}").into_bytes())
                .expect("enq");
        }
        q.snapshot().expect("snapshot");
        for i in 20..30u32 {
            q.enqueue(&tenant("acme"), format!("w-{i}").into_bytes())
                .expect("enq");
        }
    }
    let q2 = FileBackedQueue::open(&base, 1000, Box::new(NoopRecorder)).expect("open 2");
    assert_eq!(q2.depth(&tenant("acme")), 30);

    // Dequeue everything in FIFO order.
    for i in 0..20u32 {
        let m = q2.dequeue(&tenant("acme")).expect("deq");
        assert_eq!(m.payload, format!("s-{i}").into_bytes());
    }
    for i in 20..30u32 {
        let m = q2.dequeue(&tenant("acme")).expect("deq");
        assert_eq!(m.payload, format!("w-{i}").into_bytes());
    }
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-2.3 — snapshot+WAL recovery produces identical state to
// pure-WAL recovery (parallel stores, same workload)
// --------------------------------------------------------------------

#[test]
fn snapshot_plus_wal_recovery_matches_pure_wal_recovery() {
    let base_a = temp_base("pure");
    let base_b = temp_base("snap");
    {
        let a = FileBackedQueue::open(&base_a, 1000, Box::new(NoopRecorder)).expect("open a");
        let b = FileBackedQueue::open(&base_b, 1000, Box::new(NoopRecorder)).expect("open b");
        for i in 0..15u32 {
            let payload = format!("i-{i}").into_bytes();
            a.enqueue(&tenant("acme"), payload.clone()).expect("a");
            b.enqueue(&tenant("acme"), payload).expect("b");
        }
        b.snapshot().expect("snap b");
        for i in 15..30u32 {
            let payload = format!("i-{i}").into_bytes();
            a.enqueue(&tenant("acme"), payload.clone()).expect("a");
            b.enqueue(&tenant("acme"), payload).expect("b");
        }
        // Dequeue+ack a few from both.
        for _ in 0..3 {
            let ma = a.dequeue(&tenant("acme")).expect("deq a");
            a.ack(ma.id);
            let mb = b.dequeue(&tenant("acme")).expect("deq b");
            b.ack(mb.id);
        }
    }

    let a2 = FileBackedQueue::open(&base_a, 1000, Box::new(NoopRecorder)).expect("reopen a");
    let b2 = FileBackedQueue::open(&base_b, 1000, Box::new(NoopRecorder)).expect("reopen b");

    assert_eq!(a2.total_depth(), b2.total_depth());
    // Compare dequeue sequence — should be byte-identical.
    while a2.depth(&tenant("acme")) > 0 {
        let ma = a2.dequeue(&tenant("acme")).expect("deq a");
        let mb = b2.dequeue(&tenant("acme")).expect("deq b");
        assert_eq!(ma.payload, mb.payload);
    }
    cleanup(&base_a);
    cleanup(&base_b);
}

// --------------------------------------------------------------------
// AC-2.4 — snapshot is idempotent
// --------------------------------------------------------------------

#[test]
fn snapshot_is_idempotent_under_no_intervening_writes() {
    let base = temp_base("idempotent");
    let q = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open");
    q.enqueue(&tenant("acme"), b"a".to_vec()).expect("enq");
    q.snapshot().expect("snap 1");
    q.snapshot().expect("snap 2");
    assert!(snapshot_exists(&base));
    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-2.5 — in-flight messages survive snapshot+restart
// --------------------------------------------------------------------

#[test]
fn in_flight_messages_survive_snapshot_and_restart() {
    let base = temp_base("in_flight");
    let id_held = {
        let q = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open 1");
        let id = q.enqueue(&tenant("acme"), b"held".to_vec()).expect("enq");
        q.enqueue(&tenant("acme"), b"pending".to_vec())
            .expect("enq");
        // Dequeue 'held' but DON'T ack — it's in flight.
        let m = q.dequeue(&tenant("acme")).expect("deq");
        assert_eq!(m.id, id);
        q.snapshot().expect("snap");
        id
    };
    let q2 = FileBackedQueue::open(&base, 100, Box::new(NoopRecorder)).expect("open 2");
    // After restart, nack the in-flight message — it should
    // return to the head of acme's queue.
    q2.nack(id_held);
    let m = q2.dequeue(&tenant("acme")).expect("deq held");
    assert_eq!(m.payload, b"held".to_vec());
    let m = q2.dequeue(&tenant("acme")).expect("deq pending");
    assert_eq!(m.payload, b"pending".to_vec());
    cleanup(&base);
}

// --------------------------------------------------------------------
// KPI 2 — recovery p95 ≤ 500 ms over 10 000 enqueues
// --------------------------------------------------------------------

#[test]
fn recovery_p95_latency_under_five_hundred_milliseconds() {
    if std::env::var("KALEIDOSCOPE_PERF_TESTS").is_err() {
        eprintln!("perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run");
        return;
    }
    let base = temp_base("kpi2");
    {
        let q = FileBackedQueue::open(&base, 20_000, Box::new(NoopRecorder)).expect("open");
        let tn = tenant("perf");
        for i in 0..10_000u32 {
            q.enqueue(&tn, format!("m-{i}").into_bytes()).expect("enq");
        }
        q.snapshot().expect("snap");
        for i in 0..100u32 {
            q.enqueue(&tn, format!("post-{i}").into_bytes())
                .expect("enq");
        }
    }
    let mut samples: Vec<u128> = Vec::with_capacity(20);
    for _ in 0..20 {
        let t0 = std::time::Instant::now();
        let s = FileBackedQueue::open(&base, 20_000, Box::new(NoopRecorder)).expect("reopen");
        samples.push(t0.elapsed().as_micros());
        assert!(s.total_depth() > 9_000);
        drop(s);
    }
    samples.sort_unstable();
    // 95th percentile of 20 samples is the 19th by nearest rank, index
    // 18 when 0-indexed. samples[19] would be the maximum (the single
    // worst reopen), which under CI contention is a fragile thing to
    // gate on; samples[18] is the real p95 and tolerates one outlier.
    let p95_us = samples[18];
    let p95_ms = p95_us / 1_000;
    assert!(
        p95_ms <= 500,
        "KPI 2: recovery p95 must be ≤ 500 ms; got {p95_ms} ms ({p95_us} µs) (samples {samples:?})"
    );
    cleanup(&base);
}
