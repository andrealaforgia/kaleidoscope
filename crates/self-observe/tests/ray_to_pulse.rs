// Kaleidoscope self-observe — Ray → Pulse acceptance test
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

//! Kaleidoscope observes Ray (its first-party trace storage
//! engine) through its own Pulse metric store. Same template
//! as `lumen_to_pulse`. Different domain (spans vs log
//! records), same shape (single per-tenant counter on each
//! event).

use std::collections::BTreeMap;
use std::sync::Arc;

use aegis::TenantId;
use pulse::{
    InMemoryMetricStore, MetricName, MetricStore, NoopRecorder as PulseNoopRecorder,
    TimeRange as PulseTimeRange,
};
use ray::{
    InMemoryTraceStore, ServiceName, Span, SpanBatch, SpanId, SpanKind, SpanStatus, StatusCode,
    TimeRange as RayTimeRange, TraceId, TraceStore,
};
use self_observe::RayToPulseRecorder;

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn span(trace: [u8; 16], span: [u8; 8], start: u64, end: u64) -> Span {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    Span {
        trace_id: TraceId(trace),
        span_id: SpanId(span),
        parent_span_id: None,
        name: "GET /checkout".to_string(),
        kind: SpanKind::Server,
        start_time_unix_nano: start,
        end_time_unix_nano: end,
        status: SpanStatus {
            code: StatusCode::Ok,
            message: String::new(),
        },
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        events: Vec::new(),
        links: Vec::new(),
    }
}

fn bridge_pulse() -> (Arc<InMemoryMetricStore>, InMemoryTraceStore) {
    let pulse = Arc::new(InMemoryMetricStore::new(Box::new(PulseNoopRecorder)));
    let bridge = RayToPulseRecorder::new(pulse.clone() as Arc<dyn MetricStore + Send + Sync>);
    let ray = InMemoryTraceStore::new(Box::new(bridge));
    (pulse, ray)
}

#[test]
fn ray_ingest_produces_a_pulse_metric_point_under_same_tenant() {
    let (pulse, ray) = bridge_pulse();
    let tn = tenant("acme");
    ray.ingest(
        &tn,
        SpanBatch::with_spans(vec![
            span([1; 16], [1; 8], 100, 200),
            span([1; 16], [2; 8], 100, 200),
            span([2; 16], [1; 8], 100, 200),
        ]),
    )
    .expect("ingest");

    let metric_name = MetricName::new("ray.ingest.count");
    let points = pulse
        .query(&tn, &metric_name, PulseTimeRange::all())
        .expect("pulse query");
    assert_eq!(points.len(), 1, "exactly one ingest event recorded");
    assert_eq!(points[0].1.value, 3.0);
}

#[test]
fn ray_query_produces_a_pulse_metric_point_with_matched_count() {
    let (pulse, ray) = bridge_pulse();
    let tn = tenant("acme");
    ray.ingest(
        &tn,
        SpanBatch::with_spans(vec![
            span([1; 16], [1; 8], 100, 200),
            span([2; 16], [1; 8], 150, 250),
            span([3; 16], [1; 8], 300, 400),
        ]),
    )
    .expect("ingest");

    let svc = ServiceName::new("checkout");
    let out = ray
        .query(&tn, &svc, RayTimeRange::new(150, 260))
        .expect("query");
    assert_eq!(out.len(), 1, "one span in [150, 260)");

    let ingest_metric = MetricName::new("ray.ingest.count");
    let query_metric = MetricName::new("ray.query.count");
    let ingest_points = pulse
        .query(&tn, &ingest_metric, PulseTimeRange::all())
        .expect("ingest q");
    let query_points = pulse
        .query(&tn, &query_metric, PulseTimeRange::all())
        .expect("query q");
    assert_eq!(ingest_points.len(), 1);
    assert_eq!(ingest_points[0].1.value, 3.0);
    assert_eq!(query_points.len(), 1);
    assert_eq!(query_points[0].1.value, 1.0);
}

#[test]
fn two_tenants_ray_events_land_in_isolated_pulse_buckets() {
    let (pulse, ray) = bridge_pulse();
    let acme = tenant("acme");
    let globex = tenant("globex");
    ray.ingest(
        &acme,
        SpanBatch::with_spans(vec![span([1; 16], [1; 8], 100, 200)]),
    )
    .expect("acme");
    ray.ingest(
        &globex,
        SpanBatch::with_spans(vec![
            span([2; 16], [1; 8], 100, 200),
            span([2; 16], [2; 8], 100, 200),
        ]),
    )
    .expect("globex");

    let metric = MetricName::new("ray.ingest.count");
    let acme_points = pulse
        .query(&acme, &metric, PulseTimeRange::all())
        .expect("acme q");
    let globex_points = pulse
        .query(&globex, &metric, PulseTimeRange::all())
        .expect("globex q");
    assert_eq!(acme_points.len(), 1);
    assert_eq!(acme_points[0].1.value, 1.0);
    assert_eq!(globex_points.len(), 1);
    assert_eq!(globex_points[0].1.value, 2.0);
}

#[test]
fn no_ray_event_means_no_pulse_metric_point() {
    let (pulse, _ray) = bridge_pulse();
    let metric = MetricName::new("ray.ingest.count");
    let out = pulse
        .query(&tenant("acme"), &metric, PulseTimeRange::all())
        .expect("pulse query");
    assert!(out.is_empty());
}

#[test]
fn the_bridge_is_send_and_sync() {
    // Ray's MetricsRecorder requires Send + Sync; compile-time
    // assertion that the bridge satisfies the bound.
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<RayToPulseRecorder>();
}
