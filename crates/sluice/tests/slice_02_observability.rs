// Kaleidoscope Sluice — slice 02 observability acceptance test
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

//! Slice 02 — depth observability + MetricsRecorder
//!
//! Maps to `docs/feature/sluice-v0/slices/slice-02-observability.md`.
//! Companion story: US-SL-02. KPI 2: depth lookup is O(1) regardless
//! of queue size.

use aegis::TenantId;
use sluice::{CapturingRecorder, InMemoryQueue, NoopRecorder, Queue, RecordedEvent};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

// --------------------------------------------------------------------
// AC-2.1 / AC-2.2 / AC-2.3 — depth tracking
// --------------------------------------------------------------------

#[test]
fn depth_tracks_enqueue_dequeue_ack_correctly() {
    let queue = InMemoryQueue::new(100, Box::new(NoopRecorder));
    let t = tenant("acme");
    assert_eq!(queue.depth(&t), 0);
    let id1 = queue.enqueue(&t, b"a".to_vec()).unwrap();
    assert_eq!(queue.depth(&t), 1);
    queue.enqueue(&t, b"b".to_vec()).unwrap();
    assert_eq!(queue.depth(&t), 2);
    // Dequeue removes from pending; depth decreases.
    let _ = queue.dequeue(&t).expect("dequeue is Ok");
    assert_eq!(queue.depth(&t), 1);
    // Ack removes permanently; depth unchanged.
    queue.ack(id1).expect("ack");
    assert_eq!(queue.depth(&t), 1);
}

#[test]
fn nack_restores_depth() {
    let queue = InMemoryQueue::new(100, Box::new(NoopRecorder));
    let t = tenant("acme");
    let id = queue.enqueue(&t, b"only".to_vec()).unwrap();
    let _ = queue.dequeue(&t).expect("dequeue is Ok");
    assert_eq!(queue.depth(&t), 0);
    queue.nack(id).expect("nack");
    assert_eq!(queue.depth(&t), 1);
}

#[test]
fn total_depth_sums_across_tenants() {
    let queue = InMemoryQueue::new(100, Box::new(NoopRecorder));
    queue.enqueue(&tenant("a"), b"x".to_vec()).unwrap();
    queue.enqueue(&tenant("a"), b"y".to_vec()).unwrap();
    queue.enqueue(&tenant("b"), b"z".to_vec()).unwrap();
    assert_eq!(queue.total_depth(), 3);
}

#[test]
fn depth_returns_zero_for_unknown_tenant() {
    let queue = InMemoryQueue::new(100, Box::new(NoopRecorder));
    assert_eq!(queue.depth(&tenant("ghost")), 0);
}

// --------------------------------------------------------------------
// KPI 2 — depth lookup is O(1) regardless of queue size
// --------------------------------------------------------------------

#[test]
fn depth_lookup_is_constant_time_at_varied_sizes() {
    let queue = InMemoryQueue::new(20_000, Box::new(NoopRecorder));
    let t = tenant("perf");

    let sizes = [10, 100, 1_000, 10_000];
    let mut samples: Vec<u128> = Vec::with_capacity(sizes.len());

    for size in sizes {
        // Bring the queue's depth up to `size`.
        let current = queue.depth(&t);
        for _ in current..size {
            queue.enqueue(&t, b"x".to_vec()).unwrap();
        }
        assert_eq!(queue.depth(&t), size);

        // Measure depth lookup wall-clock — averaged over 100
        // calls to dampen noise.
        let t0 = std::time::Instant::now();
        for _ in 0..100 {
            let _ = queue.depth(&t);
        }
        samples.push(t0.elapsed().as_nanos() / 100);
    }

    let smallest = *samples.iter().min().unwrap();
    let largest = *samples.iter().max().unwrap();
    // O(1) tolerance: largest sample within 5× smallest. Pure
    // linear-scan would scale 1 000× across the sizes.
    assert!(
        largest <= smallest.max(1) * 5,
        "KPI 2: depth lookup must be O(1); samples {samples:?} (largest {largest}ns vs smallest {smallest}ns)"
    );
}

// --------------------------------------------------------------------
// AC-2.4 — MetricsRecorder trait wired into every operation
// --------------------------------------------------------------------

#[test]
fn capturing_recorder_observes_every_queue_operation() {
    let recorder = CapturingRecorder::new();
    let queue = InMemoryQueue::new(2, Box::new(recorder.clone()));
    let t = tenant("acme");

    // Accepted enqueue.
    let id = queue.enqueue(&t, b"first".to_vec()).unwrap();
    // Accepted second enqueue (fills the queue).
    queue.enqueue(&t, b"second".to_vec()).unwrap();
    // Rejected enqueue (full).
    let _ = queue.enqueue(&t, b"third".to_vec()).err();
    // Dequeue.
    let _ = queue.dequeue(&t).expect("dequeue is Ok");
    // Ack the first.
    queue.ack(id).expect("ack");
    // Nack of an unknown id is silent.
    queue.nack(sluice::MessageId(9_999)).expect("nack");

    let events = recorder.snapshot();
    assert_eq!(
        events.len(),
        5,
        "expected 5 recorder events; got {events:?}"
    );
    assert!(matches!(
        events[0],
        RecordedEvent::Enqueue { accepted: true, .. }
    ));
    assert!(matches!(
        events[1],
        RecordedEvent::Enqueue { accepted: true, .. }
    ));
    assert!(matches!(
        events[2],
        RecordedEvent::Enqueue {
            accepted: false,
            ..
        }
    ));
    assert!(matches!(events[3], RecordedEvent::Dequeue { .. }));
    assert!(matches!(events[4], RecordedEvent::Ack { .. }));
}

#[test]
fn noop_recorder_records_nothing_observable() {
    // The NoopRecorder doesn't capture anything, but the queue
    // operations succeed exactly as they would with any other
    // recorder. The test just exercises the type-system contract.
    let queue = InMemoryQueue::new(100, Box::new(NoopRecorder));
    let t = tenant("acme");
    queue.enqueue(&t, b"x".to_vec()).unwrap();
    let _ = queue.dequeue(&t).expect("dequeue is Ok");
    assert_eq!(queue.total_depth(), 0);
}
