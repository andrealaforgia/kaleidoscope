// Kaleidoscope self-observe — Pulse cardinality bridge acceptance test
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

//! Kaleidoscope observes itself: Pulse cardinality refusals as Pulse
//! points.
//!
//! Maps to ADR-0051 Decision 2 (the longitudinal observability surface
//! lives on the `MetricsRecorder::record_series_refused` seam, bridged
//! into a second pulse store via `PulseCardinalityToPulseRecorder`).
//!
//! The bridge wires Pulse's `record_series_refused` events into a
//! Pulse `MetricStore`. The acceptance test asserts that a refusal
//! call lands as a queryable `pulse.series.refused.count` metric
//! point under the same tenant, with value equal to the refused
//! count, kind Sum, and the tenant carried as a point attribute.
//!
//! Walking-skeleton scope: drives the bridge through the
//! `pulse::MetricsRecorder` trait (the bridge implements it);
//! assertion target is a `pulse::InMemoryMetricStore`. Both adapters
//! are real in-process pulse adapters. Mirrors the in-memory shape
//! of `cinder_to_pulse.rs`.

use std::sync::Arc;

use aegis::TenantId;
use pulse::{
    InMemoryMetricStore, MetricName, MetricStore, MetricsRecorder,
    NoopRecorder as PulseNoopRecorder, TimeRange,
};
use self_observe::PulseCardinalityToPulseRecorder;

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn refused_count_name() -> MetricName {
    MetricName::new("pulse.series.refused.count")
}

/// Construct the standard test wiring: a Pulse in-memory store
/// shared between the bridge (write side) and the assertion (read
/// side). The bridge is held as a `pulse::MetricsRecorder` so the
/// scenario invokes the trait method directly, mirroring the seam
/// the cap-enforcing `apply_ingest` path will invoke in production.
fn wire() -> (
    Arc<InMemoryMetricStore>,
    Box<dyn MetricsRecorder + Send + Sync>,
) {
    let pulse = Arc::new(InMemoryMetricStore::new(Box::new(PulseNoopRecorder)));
    let bridge =
        PulseCardinalityToPulseRecorder::new(pulse.clone() as Arc<dyn MetricStore + Send + Sync>);
    (pulse, Box::new(bridge))
}

// --------------------------------------------------------------------
// Scenario 6 — the bridge emits a `pulse.series.refused.count` point
// per refusal, value=count, kind Sum, tenant attribute on the point.
//
// @driving_port @US-03 @kpi @observability
// --------------------------------------------------------------------

#[test]
fn record_series_refused_emits_pulse_series_refused_count_point_with_tenant_attribute() {
    let (pulse, bridge) = wire();
    let acme = tenant("acme-prod");

    // Simulate the production `apply_ingest` arm firing for tenant
    // "acme-prod" with 3 refused NEW SeriesKeys in one call. The
    // bridge must emit a single point of metric
    // `pulse.series.refused.count` carrying value=3 and the tenant
    // as a point attribute, mirroring the `cinder.<event>.count`
    // emission shape.
    bridge.record_series_refused(&acme, 3);

    let points = pulse
        .query(&acme, &refused_count_name(), TimeRange::all())
        .expect("pulse query");
    assert_eq!(
        points.len(),
        1,
        "one record_series_refused call emits exactly one metric point"
    );
    let (metric, p) = &points[0];
    assert_eq!(
        p.value, 3.0,
        "the emitted point value equals the refused count (3)"
    );
    assert_eq!(
        metric.kind,
        pulse::MetricKind::Sum,
        "pulse.series.refused.count is a Sum metric"
    );
    assert_eq!(
        p.attributes.get("tenant").map(String::as_str),
        Some("acme-prod"),
        "the tenant rides as a point attribute (not a series attribute) so the bridge does not multiply self-observe cardinality"
    );
}
