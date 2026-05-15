// Kaleidoscope self-observe — Lumen → Pulse acceptance test
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

//! Kaleidoscope observes itself using its own primitives.
//!
//! The bridge wires Lumen's `MetricsRecorder` events into a
//! Pulse `MetricStore`. The acceptance tests assert that
//! Lumen's `ingest` and `query` calls land as queryable metric
//! points in Pulse, with tenant identity and counts preserved.

use std::collections::BTreeMap;
use std::sync::Arc;

use aegis::TenantId;
use lumen::{
    InMemoryLogStore, LogBatch, LogRecord, LogStore, SeverityNumber, TimeRange as LumenTimeRange,
};
use pulse::{
    InMemoryMetricStore, MetricName, MetricStore, NoopRecorder as PulseNoopRecorder,
    TimeRange as PulseTimeRange,
};
use self_observe::LumenToPulseRecorder;

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn log_record(observed: u64, body: &str) -> LogRecord {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), "checkout".to_string());
    LogRecord {
        observed_time_unix_nano: observed,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO".to_string(),
        body: body.to_string(),
        attributes: BTreeMap::new(),
        resource_attributes: resource,
        trace_id: None,
        span_id: None,
    }
}

#[test]
fn lumen_ingest_produces_a_pulse_metric_point_under_same_tenant() {
    let pulse = Arc::new(InMemoryMetricStore::new(Box::new(PulseNoopRecorder)));
    let bridge = LumenToPulseRecorder::new(pulse.clone() as Arc<dyn MetricStore + Send + Sync>);
    let lumen = InMemoryLogStore::new(Box::new(bridge));

    let tn = tenant("acme");
    lumen
        .ingest(
            &tn,
            LogBatch::with_records(vec![
                log_record(100, "first"),
                log_record(200, "second"),
                log_record(300, "third"),
            ]),
        )
        .expect("lumen ingest");

    // Pulse received a `lumen.ingest.count` metric for acme
    // with value = 3.
    let metric_name = MetricName::new("lumen.ingest.count");
    let points = pulse
        .query(&tn, &metric_name, PulseTimeRange::all())
        .expect("pulse query");
    assert_eq!(points.len(), 1, "exactly one ingest event recorded");
    assert_eq!(points[0].1.value, 3.0);
}

#[test]
fn lumen_query_produces_a_pulse_metric_point_with_matched_count() {
    let pulse = Arc::new(InMemoryMetricStore::new(Box::new(PulseNoopRecorder)));
    let bridge = LumenToPulseRecorder::new(pulse.clone() as Arc<dyn MetricStore + Send + Sync>);
    let lumen = InMemoryLogStore::new(Box::new(bridge));
    let tn = tenant("acme");

    lumen
        .ingest(
            &tn,
            LogBatch::with_records(vec![
                log_record(100, "a"),
                log_record(200, "b"),
                log_record(300, "c"),
            ]),
        )
        .expect("lumen ingest");

    let out = lumen
        .query(&tn, LumenTimeRange::new(150, 250))
        .expect("lumen query");
    assert_eq!(out.len(), 1, "lumen returned one matching record");

    // Pulse recorded both events: ingest (3 records) and query
    // (1 match).
    let ingest_metric = MetricName::new("lumen.ingest.count");
    let query_metric = MetricName::new("lumen.query.count");

    let ingest_points = pulse
        .query(&tn, &ingest_metric, PulseTimeRange::all())
        .expect("ingest pulse query");
    assert_eq!(ingest_points.len(), 1);
    assert_eq!(ingest_points[0].1.value, 3.0);

    let query_points = pulse
        .query(&tn, &query_metric, PulseTimeRange::all())
        .expect("query pulse query");
    assert_eq!(query_points.len(), 1);
    assert_eq!(query_points[0].1.value, 1.0);
}

#[test]
fn two_tenants_lumen_events_land_in_isolated_pulse_buckets() {
    let pulse = Arc::new(InMemoryMetricStore::new(Box::new(PulseNoopRecorder)));
    let bridge = LumenToPulseRecorder::new(pulse.clone() as Arc<dyn MetricStore + Send + Sync>);
    let lumen = InMemoryLogStore::new(Box::new(bridge));

    let acme = tenant("acme");
    let globex = tenant("globex");

    lumen
        .ingest(
            &acme,
            LogBatch::with_records(vec![log_record(100, "a1"), log_record(200, "a2")]),
        )
        .expect("acme");
    lumen
        .ingest(&globex, LogBatch::with_records(vec![log_record(150, "g1")]))
        .expect("globex");

    let metric_name = MetricName::new("lumen.ingest.count");
    let acme_points = pulse
        .query(&acme, &metric_name, PulseTimeRange::all())
        .expect("acme pulse query");
    let globex_points = pulse
        .query(&globex, &metric_name, PulseTimeRange::all())
        .expect("globex pulse query");

    // Each tenant's events land under that tenant only.
    assert_eq!(acme_points.len(), 1);
    assert_eq!(acme_points[0].1.value, 2.0);
    assert_eq!(globex_points.len(), 1);
    assert_eq!(globex_points[0].1.value, 1.0);
}

#[test]
fn no_lumen_event_means_no_pulse_metric_point() {
    // Sanity check: the bridge only emits when Lumen tells it
    // to. An unused Lumen store leaves Pulse empty.
    let pulse = Arc::new(InMemoryMetricStore::new(Box::new(PulseNoopRecorder)));
    let bridge = LumenToPulseRecorder::new(pulse.clone() as Arc<dyn MetricStore + Send + Sync>);
    let _lumen = InMemoryLogStore::new(Box::new(bridge));

    let metric_name = MetricName::new("lumen.ingest.count");
    let out = pulse
        .query(&tenant("acme"), &metric_name, PulseTimeRange::all())
        .expect("pulse query");
    assert!(out.is_empty());
}

#[test]
fn empty_batch_ingest_still_emits_a_zero_count_event() {
    // Lumen v0 calls record_ingest(tenant, 0) even on empty
    // batches (sanity: this test confirms that contract holds
    // when the bridge is wired). Operators may want to filter
    // these out downstream; the bridge does not.
    //
    // Note: this is a documentary assertion of v0 Lumen
    // behaviour. If Lumen ever changes the contract to skip
    // emission on empty batches, this test fails and the
    // narrative must be updated.
    let pulse = Arc::new(InMemoryMetricStore::new(Box::new(PulseNoopRecorder)));
    let bridge = LumenToPulseRecorder::new(pulse.clone() as Arc<dyn MetricStore + Send + Sync>);
    let lumen = InMemoryLogStore::new(Box::new(bridge));

    lumen
        .ingest(&tenant("acme"), LogBatch::with_records(vec![]))
        .expect("empty");

    let metric_name = MetricName::new("lumen.ingest.count");
    let out = pulse
        .query(&tenant("acme"), &metric_name, PulseTimeRange::all())
        .expect("pulse");
    // Whichever it is — emitted or skipped — we just lock it
    // down. v0 InMemoryLogStore calls record_ingest with the
    // batch length, including zero.
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].1.value, 0.0);
}

#[test]
fn the_bridge_is_send_and_sync() {
    // Compile-time check: LumenToPulseRecorder is Send+Sync,
    // so Lumen's MetricsRecorder trait bounds are satisfied.
    // This test exists to fail compilation rather than at
    // runtime if either bound is lost.
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<LumenToPulseRecorder>();
}
