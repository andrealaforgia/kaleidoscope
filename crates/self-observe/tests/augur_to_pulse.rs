// Kaleidoscope self-observe — Augur → Pulse acceptance test
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

//! Kaleidoscope observes Augur (its anomaly-detection
//! observers) through its own Pulse metric store.
//!
//! Note: Augur's observers (ZScoreObserver, RareEventObserver)
//! do not currently call MetricsRecorder themselves — the
//! trait is a contract awaiting a consumer that wires
//! observation + emission together (Beacon, probably). This
//! test therefore drives the recorder directly, verifying the
//! bridge contract independently of the eventual consumer.

use std::sync::Arc;

use aegis::TenantId;
use augur::MetricsRecorder as AugurRecorder;
use pulse::{
    InMemoryMetricStore, MetricName, MetricStore, NoopRecorder as PulseNoopRecorder,
    TimeRange as PulseTimeRange,
};
use self_observe::AugurToPulseRecorder;

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn build() -> (Arc<InMemoryMetricStore>, AugurToPulseRecorder) {
    let pulse = Arc::new(InMemoryMetricStore::new(Box::new(PulseNoopRecorder)));
    let bridge = AugurToPulseRecorder::new(pulse.clone() as Arc<dyn MetricStore + Send + Sync>);
    (pulse, bridge)
}

#[test]
fn augur_observation_produces_a_pulse_point_under_same_tenant() {
    let (pulse, bridge) = build();
    let tn = tenant("acme");
    bridge.record_observation(&tn);
    bridge.record_observation(&tn);
    bridge.record_observation(&tn);

    let metric = MetricName::new("augur.observation.count");
    let points = pulse
        .query(&tn, &metric, PulseTimeRange::all())
        .expect("pulse query");
    // Each observation lands as its own point with value=1.
    // Operators sum-aggregate across the time range to get
    // total observations.
    assert_eq!(points.len(), 3);
    assert!(points.iter().all(|p| p.1.value == 1.0));
}

#[test]
fn augur_anomaly_emits_both_count_and_score_metrics() {
    let (pulse, bridge) = build();
    let tn = tenant("acme");
    bridge.record_anomaly(&tn, 4.2);

    let count_metric = MetricName::new("augur.anomaly.count");
    let score_metric = MetricName::new("augur.anomaly.score");
    let count_points = pulse
        .query(&tn, &count_metric, PulseTimeRange::all())
        .expect("count query");
    let score_points = pulse
        .query(&tn, &score_metric, PulseTimeRange::all())
        .expect("score query");
    assert_eq!(count_points.len(), 1);
    assert_eq!(count_points[0].1.value, 1.0);
    assert_eq!(score_points.len(), 1);
    assert!(
        (score_points[0].1.value - 4.2).abs() < 1e-9,
        "score lands as the metric value, not as an attribute"
    );
}

#[test]
fn augur_multiple_anomalies_preserve_individual_scores() {
    // Two anomalies with different scores: each one produces
    // its own score point. Operators can plot the time series
    // of anomaly scores directly.
    let (pulse, bridge) = build();
    let tn = tenant("acme");
    bridge.record_anomaly(&tn, 3.5);
    bridge.record_anomaly(&tn, -4.1); // negative z-scores are real
    bridge.record_anomaly(&tn, 5.0);

    let score_metric = MetricName::new("augur.anomaly.score");
    let score_points = pulse
        .query(&tn, &score_metric, PulseTimeRange::all())
        .expect("score query");
    assert_eq!(score_points.len(), 3);
    let scores: Vec<f64> = score_points.iter().map(|p| p.1.value).collect();
    assert!(scores.iter().any(|s| (s - 3.5).abs() < 1e-9));
    assert!(scores.iter().any(|s| (s - -4.1).abs() < 1e-9));
    assert!(scores.iter().any(|s| (s - 5.0).abs() < 1e-9));
}

#[test]
fn two_tenants_augur_events_land_in_isolated_pulse_buckets() {
    let (pulse, bridge) = build();
    let acme = tenant("acme");
    let globex = tenant("globex");
    bridge.record_observation(&acme);
    bridge.record_observation(&acme);
    bridge.record_anomaly(&globex, 3.0);

    let obs = MetricName::new("augur.observation.count");
    let acme_obs = pulse
        .query(&acme, &obs, PulseTimeRange::all())
        .expect("acme");
    let globex_obs = pulse
        .query(&globex, &obs, PulseTimeRange::all())
        .expect("globex");
    assert_eq!(acme_obs.len(), 2);
    assert!(globex_obs.is_empty());

    let anomaly = MetricName::new("augur.anomaly.count");
    let acme_an = pulse
        .query(&acme, &anomaly, PulseTimeRange::all())
        .expect("acme an");
    let globex_an = pulse
        .query(&globex, &anomaly, PulseTimeRange::all())
        .expect("globex an");
    assert!(acme_an.is_empty());
    assert_eq!(globex_an.len(), 1);
}

#[test]
fn no_augur_event_means_no_pulse_metric_point() {
    let (pulse, _bridge) = build();
    let obs = MetricName::new("augur.observation.count");
    let out = pulse
        .query(&tenant("acme"), &obs, PulseTimeRange::all())
        .expect("pulse query");
    assert!(out.is_empty());
}

#[test]
fn the_bridge_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<AugurToPulseRecorder>();
}
