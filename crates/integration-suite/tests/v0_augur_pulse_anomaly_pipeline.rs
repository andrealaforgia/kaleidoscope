// Kaleidoscope integration suite — Augur observes Pulse stream
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

//! Cross-pillar functional composition: Augur observes Pulse.
//!
//! Different lesson from
//! `v1_three_adapters_compose_under_restart`. That test pins
//! durable adapters coexisting under shared tenant identity.
//! This test pins **functional composition**: Pulse v0 (metrics
//! pillar) and Augur v0 (anomaly pillar) cooperate to produce
//! derived behaviour neither crate produces alone.
//!
//! The shape: an application ingests metric points into
//! `InMemoryMetricStore`, and as it does so it feeds each value
//! into a per-`(tenant, metric_name)` `ZScoreObserver`. The
//! observer accumulates a baseline; an anomalous point fires an
//! `Anomaly<f64>` event. Both crates share `aegis::TenantId`.
//!
//! v1 of either crate could add a built-in subscriber bridge so
//! the wiring is no longer application-level. v0 leaves it
//! explicit, which is the right choice because it documents the
//! contract in compiled code.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use augur::{Anomaly, AnomalyObserver, ZScoreObserver};
use pulse::{
    InMemoryMetricStore, Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore,
    NoopRecorder as PulseRecorder, TimeRange,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn point(time: u64, value: f64) -> MetricPoint {
    MetricPoint {
        time_unix_nano: time,
        start_time_unix_nano: 0,
        attributes: BTreeMap::new(),
        value,
    }
}

fn metric(name: &str, service: &str, points: Vec<MetricPoint>) -> Metric {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    Metric {
        name: MetricName::new(name),
        description: String::new(),
        unit: "1".to_string(),
        kind: MetricKind::Gauge,
        points,
        resource_attributes: resource,
    }
}

#[test]
fn augur_observes_pulse_stream_and_flags_spike_anomaly() {
    let store = InMemoryMetricStore::new(Box::new(PulseRecorder));
    let mut observer = ZScoreObserver::new(3.0, 30);
    let tn = tenant("acme");
    let metric_name = MetricName::new("cpu.utilization");

    // Phase 1: feed a stable baseline of 100 points oscillating
    // around 50% with a small jitter. Each Pulse ingest also
    // feeds the observer, mirroring the application-level
    // wiring an operator would write.
    let mut anomalies: Vec<Anomaly<f64>> = Vec::new();
    let now_base = SystemTime::now();
    for i in 0..100u64 {
        let value = 50.0 + if i % 2 == 0 { 1.0 } else { -1.0 };
        // Ingest into Pulse.
        store
            .ingest(
                &tn,
                MetricBatch::with_metrics(vec![metric(
                    "cpu.utilization",
                    "checkout",
                    vec![point(i + 1, value)],
                )]),
            )
            .expect("pulse ingest");
        // Feed Augur the same value.
        if let Some(a) = observer.observe(&tn, value, now_base + std::time::Duration::from_secs(i))
        {
            anomalies.push(a);
        }
    }
    assert!(
        anomalies.is_empty(),
        "stable baseline must not fire anomalies; got {anomalies:?}"
    );

    // Phase 2: inject a 5-sigma spike. Augur flags it; Pulse
    // simply stores it.
    let mean = observer.mean();
    let sd = observer.stddev();
    let spike_value = mean + 5.0 * sd;
    store
        .ingest(
            &tn,
            MetricBatch::with_metrics(vec![metric(
                "cpu.utilization",
                "checkout",
                vec![point(200, spike_value)],
            )]),
        )
        .expect("pulse ingest spike");
    let anomaly = observer
        .observe(
            &tn,
            spike_value,
            now_base + std::time::Duration::from_secs(200),
        )
        .expect("anomaly expected");
    assert!(
        anomaly.score >= 3.0,
        "z-score >= 3.0; got {}",
        anomaly.score
    );
    assert_eq!(anomaly.tenant, tn);

    // Phase 3: assertion that both pillars agree on what
    // happened. The spike value is the most recent point in
    // Pulse's store, AND it is the value Augur flagged. The
    // same f64 byte-equality is the cross-pillar correlation
    // contract.
    let out = store
        .query(&tn, &metric_name, TimeRange::all())
        .expect("pulse query");
    let last = out.last().expect("at least one point");
    assert_eq!(last.1.value.to_bits(), spike_value.to_bits());
    assert_eq!(anomaly.value.to_bits(), spike_value.to_bits());
}

#[test]
fn two_tenants_observers_are_isolated_under_pulse_ingest() {
    // Each tenant gets its own observer with its own baseline.
    // The "operator wires one observer per (tenant, signal)"
    // contract Augur v0 documented.
    let store = InMemoryMetricStore::new(Box::new(PulseRecorder));
    let mut obs_acme = ZScoreObserver::new(3.0, 10);
    let mut obs_globex = ZScoreObserver::new(3.0, 10);
    let acme = tenant("acme");
    let globex = tenant("globex");

    // acme runs near 100; globex runs near 10. Different
    // regimes entirely.
    for i in 0..50u64 {
        let acme_val = 100.0 + if i % 2 == 0 { 1.0 } else { -1.0 };
        let globex_val = 10.0 + if i % 2 == 0 { 0.5 } else { -0.5 };

        store
            .ingest(
                &acme,
                MetricBatch::with_metrics(vec![metric("m", "svc", vec![point(i + 1, acme_val)])]),
            )
            .expect("acme ingest");
        store
            .ingest(
                &globex,
                MetricBatch::with_metrics(vec![metric("m", "svc", vec![point(i + 1, globex_val)])]),
            )
            .expect("globex ingest");

        let now = SystemTime::now();
        obs_acme.observe(&acme, acme_val, now);
        obs_globex.observe(&globex, globex_val, now);
    }

    // Acme's baseline ~100; globex's baseline ~10.
    assert!((obs_acme.mean() - 100.0).abs() < 1.0);
    assert!((obs_globex.mean() - 10.0).abs() < 1.0);

    // A value that looks anomalous for globex (e.g. 50) does
    // not look anomalous for acme. Inject 50 into acme's
    // observer: it's well within acme's regime.
    let r_acme = obs_acme.observe(&acme, 50.0, SystemTime::now());
    let r_globex = obs_globex.observe(&globex, 50.0, SystemTime::now());

    // For acme the value 50 is 50 sigmas below the mean if
    // stddev is ~1, so it WILL fire. For globex it is 80 sigmas
    // above the mean. Both fire; what matters is they each
    // measured against their own baseline. The reported
    // z-scores differ in sign and magnitude.
    let a_acme = r_acme.expect("acme anomaly");
    let a_globex = r_globex.expect("globex anomaly");
    assert!(a_acme.score < -3.0, "acme z is large-negative");
    assert!(a_globex.score > 3.0, "globex z is large-positive");
    assert_ne!(a_acme.score, a_globex.score, "baselines are independent");
}

fn _utc(secs: u64) -> SystemTime {
    UNIX_EPOCH + std::time::Duration::from_secs(secs)
}
