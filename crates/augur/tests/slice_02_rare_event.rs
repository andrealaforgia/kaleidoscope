// Kaleidoscope Augur — slice 02 rare-event acceptance test
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

//! Slice 02 — `RareEventObserver`
//!
//! Maps to `docs/feature/augur-v0/slices/slice-02-rare-event.md`.

use std::time::{Duration, UNIX_EPOCH};

use aegis::TenantId;
use augur::{AnomalyObserver, RareEventObserver};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn t(secs: u64) -> std::time::SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

// --------------------------------------------------------------------
// AC-2.1 — frequency below rarity_threshold fires anomaly
// --------------------------------------------------------------------

#[test]
fn frequent_event_does_not_fire_anomaly() {
    let mut obs = RareEventObserver::new(0.01, 100);
    let tn = tenant("acme");
    // Feed only "INFO request handled" — single dominant
    // event, frequency = 1.0, well above 1%.
    for i in 0..500 {
        let r = obs.observe(&tn, "INFO request handled".to_string(), t(i));
        assert!(r.is_none(), "frequent event should never fire");
    }
}

// --------------------------------------------------------------------
// AC-2.2 — warm-up
// --------------------------------------------------------------------

#[test]
fn warm_up_suppresses_anomalies() {
    let mut obs = RareEventObserver::new(0.01, 100);
    let tn = tenant("acme");
    // First observation is rare (1/1 = 1.0 actually,
    // but with min_samples=100 we are still warming up).
    let r = obs.observe(&tn, "WARN unusual".to_string(), t(0));
    assert!(r.is_none(), "warm-up should suppress");
}

// --------------------------------------------------------------------
// AC-2.3 — new event after warm-up is anomaly if its
// frequency is below rarity_threshold
// --------------------------------------------------------------------

#[test]
fn new_event_after_warm_up_fires_when_below_rarity_threshold() {
    let mut obs = RareEventObserver::new(0.005, 100);
    let tn = tenant("acme");
    // Feed 200 frequent events.
    for i in 0..200 {
        let _ = obs.observe(&tn, "INFO ok".to_string(), t(i));
    }
    // Brand-new event. Its frequency = 1/201 ≈ 0.005.
    // 1/201 ≈ 0.00498, just below 0.005.
    let r = obs.observe(&tn, "ERROR rare".to_string(), t(201));
    let anomaly = r.expect("anomaly expected");
    assert_eq!(anomaly.value, "ERROR rare");
    assert!(anomaly.score <= 0.005);
    assert_eq!(anomaly.reason, "frequency below rarity_threshold");
}

// --------------------------------------------------------------------
// AC-2.4 — first-crossing only (no re-emission)
// --------------------------------------------------------------------

#[test]
fn previously_fired_rare_event_does_not_re_fire() {
    let mut obs = RareEventObserver::new(0.5, 5);
    let tn = tenant("acme");
    // 5 mainstream events.
    for i in 0..5 {
        let _ = obs.observe(&tn, "main".to_string(), t(i));
    }
    // Two observations of the same rare event.
    let r1 = obs.observe(&tn, "rare".to_string(), t(6));
    let r2 = obs.observe(&tn, "rare".to_string(), t(7));
    assert!(r1.is_some(), "first observation fires");
    assert!(r2.is_none(), "second observation suppressed");
}

// --------------------------------------------------------------------
// AC-2.5 — reset clears state
// --------------------------------------------------------------------

#[test]
fn reset_clears_frequency_table_and_fired_set() {
    let mut obs = RareEventObserver::new(0.5, 2);
    let tn = tenant("acme");
    let _ = obs.observe(&tn, "a".to_string(), t(0));
    let _ = obs.observe(&tn, "b".to_string(), t(1));
    let _ = obs.observe(&tn, "c".to_string(), t(2));
    assert!(obs.vocabulary_size() >= 2);
    obs.reset();
    assert_eq!(obs.samples_seen(), 0);
    assert_eq!(obs.vocabulary_size(), 0);
}

// --------------------------------------------------------------------
// KPI 2 — observe p95 ≤ 20 µs on 1000-event vocabulary
// --------------------------------------------------------------------

#[test]
fn observe_p95_latency_under_twenty_microseconds() {
    if std::env::var("KALEIDOSCOPE_PERF_TESTS").is_err() {
        eprintln!("perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run");
        return;
    }
    let mut obs = RareEventObserver::new(0.0001, 100);
    let tn = tenant("perf");

    // Seed 1000 distinct events with high frequency so
    // none are rare enough to trigger emission during the
    // measurement loop. We want to measure observe cost,
    // not the small extra cost of emitting an anomaly.
    let vocabulary: Vec<String> = (0..1000).map(|i| format!("event-{i:04}")).collect();
    for round in 0..200 {
        for ev in &vocabulary {
            let _ = obs.observe(&tn, ev.clone(), t(round));
        }
    }

    let mut samples: Vec<u128> = Vec::with_capacity(5000);
    for i in 0..5000usize {
        let ev = vocabulary[i % vocabulary.len()].clone();
        let t0 = std::time::Instant::now();
        let _ = obs.observe(&tn, ev, t(1_000_000 + (i as u64)));
        samples.push(t0.elapsed().as_nanos());
    }
    samples.sort_unstable();
    let p95_ns = samples[4750];
    let p95_us = p95_ns / 1_000;
    assert!(
        p95_us <= 20,
        "KPI 2: observe p95 must be ≤ 20 µs; got {p95_us} µs ({p95_ns} ns) (first 10 ns {:?})",
        &samples[..10]
    );
}
