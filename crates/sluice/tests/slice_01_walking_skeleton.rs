// Kaleidoscope Sluice — slice 01 walking-skeleton acceptance test
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

//! Slice 01 — Queue trait + InMemoryQueue walking skeleton
//!
//! Maps to `docs/feature/sluice-v0/slices/slice-01-walking-skeleton.md`.
//! Companion story: US-SL-01. KPI 1: enqueue/dequeue p95 ≤ 50µs.

use aegis::TenantId;
use sluice::{EnqueueError, InMemoryQueue, NoopRecorder, Queue};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn make_queue(cap: usize) -> InMemoryQueue {
    InMemoryQueue::new(cap, Box::new(NoopRecorder))
}

// --------------------------------------------------------------------
// AC-1.1 / AC-1.2 / AC-1.3 — enqueue + dequeue + FIFO
// --------------------------------------------------------------------

#[test]
fn enqueue_returns_message_id_and_dequeue_returns_same_payload() {
    let queue = make_queue(100);
    let t = tenant("acme-prod");
    let id = queue.enqueue(&t, b"payload-1".to_vec()).expect("enq");
    let msg = queue.dequeue(&t).expect("deq");
    assert_eq!(msg.id, id);
    assert_eq!(msg.tenant, t);
    assert_eq!(msg.payload, b"payload-1");
}

#[test]
fn dequeue_on_empty_queue_returns_none() {
    let queue = make_queue(100);
    assert!(queue.dequeue(&tenant("nobody")).is_none());
}

#[test]
fn fifo_ordering_within_a_tenant() {
    let queue = make_queue(100);
    let t = tenant("acme-prod");
    let _ = queue.enqueue(&t, b"first".to_vec()).unwrap();
    let _ = queue.enqueue(&t, b"second".to_vec()).unwrap();
    let _ = queue.enqueue(&t, b"third".to_vec()).unwrap();
    assert_eq!(queue.dequeue(&t).unwrap().payload, b"first");
    assert_eq!(queue.dequeue(&t).unwrap().payload, b"second");
    assert_eq!(queue.dequeue(&t).unwrap().payload, b"third");
}

// --------------------------------------------------------------------
// AC-1.4 — tenant isolation
// --------------------------------------------------------------------

#[test]
fn two_tenants_are_isolated() {
    let queue = make_queue(100);
    let a = tenant("acme-prod");
    let b = tenant("widgetco-staging");
    let _ = queue.enqueue(&a, b"a-1".to_vec()).unwrap();
    let _ = queue.enqueue(&b, b"b-1".to_vec()).unwrap();
    let _ = queue.enqueue(&a, b"a-2".to_vec()).unwrap();

    assert_eq!(queue.dequeue(&a).unwrap().payload, b"a-1");
    assert_eq!(queue.dequeue(&a).unwrap().payload, b"a-2");
    assert!(queue.dequeue(&a).is_none());

    assert_eq!(queue.dequeue(&b).unwrap().payload, b"b-1");
    assert!(queue.dequeue(&b).is_none());
}

// --------------------------------------------------------------------
// AC-1.5 / AC-1.6 — ack / nack semantics
// --------------------------------------------------------------------

#[test]
fn ack_removes_message_permanently() {
    let queue = make_queue(100);
    let t = tenant("acme-prod");
    let id = queue.enqueue(&t, b"only".to_vec()).unwrap();
    let _ = queue.dequeue(&t).unwrap();
    queue.ack(id);
    // Dequeue again: gone.
    assert!(queue.dequeue(&t).is_none());
}

#[test]
fn nack_returns_message_to_head_of_queue() {
    let queue = make_queue(100);
    let t = tenant("acme-prod");
    let id1 = queue.enqueue(&t, b"first".to_vec()).unwrap();
    let _ = queue.enqueue(&t, b"second".to_vec()).unwrap();
    let _ = queue.dequeue(&t).unwrap();
    queue.nack(id1);
    // First again at the head.
    assert_eq!(queue.dequeue(&t).unwrap().payload, b"first");
    assert_eq!(queue.dequeue(&t).unwrap().payload, b"second");
}

#[test]
fn ack_of_unknown_id_is_noop() {
    let queue = make_queue(100);
    queue.ack(sluice::MessageId(999));
    queue.nack(sluice::MessageId(999));
    // No panic; queue still empty.
    assert_eq!(queue.total_depth(), 0);
}

// --------------------------------------------------------------------
// AC-1.7 — backpressure
// --------------------------------------------------------------------

#[test]
fn enqueue_beyond_cap_returns_full() {
    let queue = make_queue(3);
    let t = tenant("acme-prod");
    queue.enqueue(&t, b"1".to_vec()).unwrap();
    queue.enqueue(&t, b"2".to_vec()).unwrap();
    queue.enqueue(&t, b"3".to_vec()).unwrap();
    let err = queue.enqueue(&t, b"4".to_vec()).unwrap_err();
    match err {
        EnqueueError::Full { tenant: who, cap } => {
            assert_eq!(who, t);
            assert_eq!(cap, 3);
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[test]
fn cap_is_per_tenant_not_global() {
    let queue = make_queue(2);
    let a = tenant("acme");
    let b = tenant("widgetco");
    queue.enqueue(&a, b"a1".to_vec()).unwrap();
    queue.enqueue(&a, b"a2".to_vec()).unwrap();
    // Tenant A is now full.
    assert!(matches!(
        queue.enqueue(&a, b"a3".to_vec()),
        Err(EnqueueError::Full { .. })
    ));
    // Tenant B is unaffected.
    queue.enqueue(&b, b"b1".to_vec()).unwrap();
    queue.enqueue(&b, b"b2".to_vec()).unwrap();
}

// --------------------------------------------------------------------
// KPI 1 — enqueue + dequeue p95 ≤ 50 µs
// --------------------------------------------------------------------

#[test]
fn enqueue_and_dequeue_p95_under_fifty_microseconds() {
    if std::env::var("KALEIDOSCOPE_PERF_TESTS").is_err() {
        eprintln!("perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run");
        return;
    }
    let queue = make_queue(100_000);
    let t = tenant("perf");

    // Warm up.
    for i in 0..1_000 {
        queue.enqueue(&t, vec![i as u8]).unwrap();
    }
    for _ in 0..1_000 {
        let _ = queue.dequeue(&t);
    }

    // Measure 10 000 enqueues.
    let mut enq: Vec<u128> = Vec::with_capacity(10_000);
    for i in 0..10_000 {
        let t0 = std::time::Instant::now();
        queue.enqueue(&t, vec![i as u8]).unwrap();
        enq.push(t0.elapsed().as_micros());
    }
    enq.sort_unstable();
    let enq_p95 = enq[9_500];
    assert!(
        enq_p95 <= 50,
        "KPI 1: enqueue p95 must be ≤ 50µs; got {enq_p95}µs"
    );

    // Measure 10 000 dequeues.
    let mut deq: Vec<u128> = Vec::with_capacity(10_000);
    for _ in 0..10_000 {
        let t0 = std::time::Instant::now();
        let _ = queue.dequeue(&t);
        deq.push(t0.elapsed().as_micros());
    }
    deq.sort_unstable();
    let deq_p95 = deq[9_500];
    assert!(
        deq_p95 <= 50,
        "KPI 1: dequeue p95 must be ≤ 50µs; got {deq_p95}µs"
    );
}
