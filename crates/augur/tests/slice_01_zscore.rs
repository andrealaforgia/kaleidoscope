// Kaleidoscope Augur — slice 01 z-score acceptance test
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

//! Slice 01 — `ZScoreObserver` walking skeleton
//!
//! Maps to `docs/feature/augur-v0/slices/slice-01-zscore.md`.

use std::time::{Duration, UNIX_EPOCH};

use aegis::TenantId;
use augur::{Anomaly, AnomalyObserver, ZScoreObserver};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn t(secs: u64) -> std::time::SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

// --------------------------------------------------------------------
// AC-1.1 / AC-1.2 — trait + constructor
// --------------------------------------------------------------------

#[test]
fn observe_returns_none_when_no_anomaly() {
    let mut obs = ZScoreObserver::new(3.0, 30);
    // Stable baseline at 100.
    for i in 0..30 {
        let r = obs.observe(&tenant("acme"), 100.0, t(i));
        // During warm-up nothing fires.
        assert!(r.is_none());
    }
    // After warm-up, a value at the baseline still fires
    // nothing (z = 0).
    let r = obs.observe(&tenant("acme"), 100.0, t(31));
    assert!(r.is_none());
}

// --------------------------------------------------------------------
// AC-1.3 — warm-up: no anomalies before min_samples
// --------------------------------------------------------------------

#[test]
fn warm_up_suppresses_anomalies_even_with_outlier() {
    let mut obs = ZScoreObserver::new(3.0, 30);
    // First few values establish a baseline near 100.
    for _ in 0..5 {
        let _ = obs.observe(&tenant("acme"), 100.0, t(0));
    }
    // Outlier during warm-up — must NOT fire.
    let r = obs.observe(&tenant("acme"), 1_000.0, t(0));
    assert!(r.is_none());
    assert!(obs.samples_seen() < 30);
}

// --------------------------------------------------------------------
// AC-1.4 — step change fires anomaly after warm-up
// --------------------------------------------------------------------

#[test]
fn step_change_three_sigma_above_baseline_fires_anomaly() {
    let mut obs = ZScoreObserver::new(3.0, 30);
    // Establish a stable baseline at 100 ± 5 with small
    // jitter so stddev > 0.
    for i in 0..100 {
        let jitter = if i % 2 == 0 { 5.0 } else { -5.0 };
        let _ = obs.observe(&tenant("acme"), 100.0 + jitter, t(i));
    }
    let mean = obs.mean();
    let sd = obs.stddev();
    assert!(
        (mean - 100.0).abs() < 1e-9,
        "baseline mean ~100 (got {mean})"
    );
    assert!(sd > 0.0, "baseline stddev > 0 (got {sd})");

    // Inject a value at mean + 5*sd. z = 5, above 3.0.
    let outlier = mean + 5.0 * sd;
    let r = obs.observe(&tenant("acme"), outlier, t(101));
    let anomaly: Anomaly<f64> = r.expect("anomaly expected");
    assert!(
        anomaly.score >= 3.0,
        "z-score >= 3.0 expected; got {}",
        anomaly.score
    );
    assert_eq!(anomaly.value, outlier);
    assert_eq!(anomaly.tenant.0, "acme");
    assert_eq!(anomaly.reason, "z-score >= threshold");
}

#[test]
fn step_change_three_sigma_below_baseline_fires_anomaly() {
    let mut obs = ZScoreObserver::new(3.0, 30);
    for i in 0..100 {
        let jitter = if i % 2 == 0 { 5.0 } else { -5.0 };
        let _ = obs.observe(&tenant("acme"), 100.0 + jitter, t(i));
    }
    let mean = obs.mean();
    let sd = obs.stddev();
    // 5 sigma below.
    let outlier = mean - 5.0 * sd;
    let r = obs.observe(&tenant("acme"), outlier, t(101));
    let anomaly = r.expect("anomaly expected");
    assert!(
        anomaly.score <= -3.0,
        "z-score <= -3.0 expected; got {}",
        anomaly.score
    );
}

// --------------------------------------------------------------------
// AC-1.5 — baseline still updates on anomalies (adaptive)
// --------------------------------------------------------------------

#[test]
fn sustained_anomaly_eventually_moves_the_baseline() {
    let mut obs = ZScoreObserver::new(3.0, 30);
    // Establish baseline at 100.
    for i in 0..100 {
        let jitter = if i % 2 == 0 { 5.0 } else { -5.0 };
        let _ = obs.observe(&tenant("acme"), 100.0 + jitter, t(i));
    }
    let initial_mean = obs.mean();
    assert!((initial_mean - 100.0).abs() < 1e-9);

    // Sustained anomaly at 500 for 200 iterations. After
    // enough samples the mean has drifted significantly
    // toward 500.
    for i in 0..200 {
        let _ = obs.observe(&tenant("acme"), 500.0, t(200 + i));
    }
    let drifted_mean = obs.mean();
    assert!(
        drifted_mean > 300.0,
        "expected adaptive drift past 300 (got {drifted_mean})"
    );
}

// --------------------------------------------------------------------
// AC-1.6 — two observers maintain isolated baselines
// --------------------------------------------------------------------

#[test]
fn two_separate_observers_maintain_isolated_baselines() {
    let mut obs_a = ZScoreObserver::new(3.0, 5);
    let mut obs_b = ZScoreObserver::new(3.0, 5);
    for i in 0..10 {
        let _ = obs_a.observe(&tenant("acme"), 100.0, t(i));
        let _ = obs_b.observe(&tenant("acme"), 10.0, t(i));
    }
    assert!((obs_a.mean() - 100.0).abs() < 1e-9);
    assert!((obs_b.mean() - 10.0).abs() < 1e-9);
}

// --------------------------------------------------------------------
// AC-1.7 — reset clears the baseline
// --------------------------------------------------------------------

#[test]
fn reset_clears_baseline_and_returns_to_warm_up() {
    let mut obs = ZScoreObserver::new(3.0, 30);
    for i in 0..100 {
        let _ = obs.observe(&tenant("acme"), 100.0, t(i));
    }
    assert!(obs.samples_seen() >= 30);
    obs.reset();
    assert_eq!(obs.samples_seen(), 0);
    assert_eq!(obs.mean(), 0.0);
    assert_eq!(obs.stddev(), 0.0);
}

// --------------------------------------------------------------------
// KPI 1 — observe p95 ≤ 10 µs after warm-up
// --------------------------------------------------------------------

#[test]
fn observe_p95_latency_under_ten_microseconds() {
    if std::env::var("KALEIDOSCOPE_PERF_TESTS").is_err() {
        eprintln!("perf test skipped: set KALEIDOSCOPE_PERF_TESTS=1 to run");
        return;
    }
    let mut obs = ZScoreObserver::new(3.0, 100);
    let tn = tenant("perf");

    // Warm up with 1000 stable observations.
    for i in 0..1000 {
        let _ = obs.observe(&tn, 100.0 + (i as f64 % 10.0), t(i));
    }

    let mut samples: Vec<u128> = Vec::with_capacity(10_000);
    for i in 0..10_000 {
        let value = 100.0 + (i as f64 % 50.0);
        let t0 = std::time::Instant::now();
        let _ = obs.observe(&tn, value, t(1_000_000 + i));
        samples.push(t0.elapsed().as_nanos());
    }
    samples.sort_unstable();
    let p95_ns = samples[9_500];
    let p95_us = p95_ns / 1_000;
    assert!(
        p95_us <= 10,
        "KPI 1: observe p95 must be ≤ 10 µs; got {p95_us} µs ({p95_ns} ns) (first 10 ns {:?})",
        &samples[..10]
    );
}
