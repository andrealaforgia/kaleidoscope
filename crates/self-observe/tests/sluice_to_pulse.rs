// Kaleidoscope self-observe — Sluice → Pulse acceptance test
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

//! Kaleidoscope observes Sluice (its durable queue port)
//! through its own Pulse metric store. Same template as
//! `lumen_to_pulse` and `cinder_to_pulse`. The interesting
//! piece is the `accepted` attribute on enqueue events, which
//! turns capacity-based back-pressure into a per-tenant
//! visible signal.

use std::sync::Arc;

use aegis::TenantId;
use pulse::{
    InMemoryMetricStore, MetricName, MetricStore, NoopRecorder as PulseNoopRecorder,
    TimeRange as PulseTimeRange,
};
use self_observe::SluiceToPulseRecorder;
use sluice::{InMemoryQueue, Queue};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn bridge_pulse(cap: usize) -> (Arc<InMemoryMetricStore>, InMemoryQueue) {
    let pulse = Arc::new(InMemoryMetricStore::new(Box::new(PulseNoopRecorder)));
    let bridge = SluiceToPulseRecorder::new(pulse.clone() as Arc<dyn MetricStore + Send + Sync>);
    let queue = InMemoryQueue::new(cap, Box::new(bridge));
    (pulse, queue)
}

#[test]
fn sluice_enqueue_produces_a_pulse_point_with_accepted_true_when_capacity_available() {
    let (pulse, queue) = bridge_pulse(10);
    let tn = tenant("acme");
    queue.enqueue(&tn, b"payload".to_vec()).expect("enqueue");

    let metric_name = MetricName::new("sluice.enqueue.count");
    let points = pulse
        .query(&tn, &metric_name, PulseTimeRange::all())
        .expect("pulse query");
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].1.value, 1.0);
    assert_eq!(
        points[0].1.attributes.get("accepted").map(String::as_str),
        Some("true")
    );
}

#[test]
fn sluice_enqueue_at_capacity_emits_accepted_false_event_and_returns_error() {
    let (pulse, queue) = bridge_pulse(1);
    let tn = tenant("acme");
    queue.enqueue(&tn, b"a".to_vec()).expect("first ok");
    // Queue is now at cap=1.
    let err = queue.enqueue(&tn, b"b".to_vec()).expect_err("at capacity");
    assert!(matches!(err, sluice::EnqueueError::Full { .. }));

    let metric_name = MetricName::new("sluice.enqueue.count");
    let points = pulse
        .query(&tn, &metric_name, PulseTimeRange::all())
        .expect("pulse query");
    // Both events recorded (one accepted, one rejected). The
    // operator dashboard can therefore distinguish "we are
    // shedding load" from "we have no traffic".
    assert_eq!(points.len(), 2);
    let accepts: Vec<&str> = points
        .iter()
        .map(|p| {
            p.1.attributes
                .get("accepted")
                .map(String::as_str)
                .unwrap_or("")
        })
        .collect();
    assert!(accepts.contains(&"true"));
    assert!(accepts.contains(&"false"));
}

#[test]
fn sluice_dequeue_produces_a_pulse_point() {
    let (pulse, queue) = bridge_pulse(10);
    let tn = tenant("acme");
    queue.enqueue(&tn, b"x".to_vec()).expect("enqueue");
    let _ = queue.dequeue(&tn).expect("dequeue");

    let metric_name = MetricName::new("sluice.dequeue.count");
    let points = pulse
        .query(&tn, &metric_name, PulseTimeRange::all())
        .expect("pulse query");
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].1.value, 1.0);
}

#[test]
fn sluice_ack_produces_a_pulse_point() {
    let (pulse, queue) = bridge_pulse(10);
    let tn = tenant("acme");
    queue.enqueue(&tn, b"x".to_vec()).expect("enqueue");
    let msg = queue.dequeue(&tn).expect("dequeue");
    queue.ack(msg.id);

    let metric_name = MetricName::new("sluice.ack.count");
    let points = pulse
        .query(&tn, &metric_name, PulseTimeRange::all())
        .expect("pulse query");
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].1.value, 1.0);
}

#[test]
fn sluice_nack_produces_a_pulse_point_separately_from_ack() {
    let (pulse, queue) = bridge_pulse(10);
    let tn = tenant("acme");
    queue.enqueue(&tn, b"x".to_vec()).expect("enqueue");
    let msg = queue.dequeue(&tn).expect("dequeue");
    queue.nack(msg.id);

    let ack_metric = MetricName::new("sluice.ack.count");
    let nack_metric = MetricName::new("sluice.nack.count");
    let ack_points = pulse
        .query(&tn, &ack_metric, PulseTimeRange::all())
        .expect("ack query");
    let nack_points = pulse
        .query(&tn, &nack_metric, PulseTimeRange::all())
        .expect("nack query");
    assert!(ack_points.is_empty());
    assert_eq!(nack_points.len(), 1);
    assert_eq!(nack_points[0].1.value, 1.0);
}

#[test]
fn two_tenants_sluice_events_land_in_isolated_pulse_buckets() {
    let (pulse, queue) = bridge_pulse(10);
    let acme = tenant("acme");
    let globex = tenant("globex");
    queue.enqueue(&acme, b"a1".to_vec()).expect("a1");
    queue.enqueue(&acme, b"a2".to_vec()).expect("a2");
    queue.enqueue(&globex, b"g1".to_vec()).expect("g1");

    let metric = MetricName::new("sluice.enqueue.count");
    let acme_points = pulse
        .query(&acme, &metric, PulseTimeRange::all())
        .expect("acme");
    let globex_points = pulse
        .query(&globex, &metric, PulseTimeRange::all())
        .expect("globex");
    assert_eq!(acme_points.len(), 2);
    assert_eq!(globex_points.len(), 1);
}

#[test]
fn the_bridge_is_send_and_sync() {
    // Sluice's MetricsRecorder requires Send + Sync; this is a
    // compile-time assertion that the bridge satisfies that
    // bound. Failure here is a build error, not a runtime one.
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SluiceToPulseRecorder>();
}
