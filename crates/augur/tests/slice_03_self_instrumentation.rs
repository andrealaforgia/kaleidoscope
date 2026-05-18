// Kaleidoscope Augur — observer self-instrumentation acceptance
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

//! Both observers (`ZScoreObserver` and `RareEventObserver`)
//! self-instrument when a recorder is wired via
//! `with_recorder`. The previous narrative entry called this
//! out as a gap; this test proves it closed.

use std::sync::Arc;
use std::time::SystemTime;

use aegis::TenantId;
use augur::{
    AnomalyObserver, CapturingRecorder, MetricsRecorder, RareEventObserver, RecordedEvent,
    ZScoreObserver,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

#[test]
fn zscore_with_recorder_records_observation_for_every_observe_call() {
    let recorder = Arc::new(CapturingRecorder::new());
    let mut obs = ZScoreObserver::new(3.0, 5)
        .with_recorder(recorder.clone() as Arc<dyn MetricsRecorder + Send + Sync>);
    let tn = tenant("acme");

    obs.observe(&tn, 1.0, SystemTime::UNIX_EPOCH);
    obs.observe(&tn, 1.1, SystemTime::UNIX_EPOCH);
    obs.observe(&tn, 0.9, SystemTime::UNIX_EPOCH);

    let events = recorder.snapshot();
    assert_eq!(events.len(), 3, "one observation event per observe call");
    for event in &events {
        match event {
            RecordedEvent::Observation { tenant } => assert_eq!(tenant.0, "acme"),
            _ => panic!("expected only observation events, got {event:?}"),
        }
    }
}

#[test]
fn zscore_with_recorder_records_anomaly_with_signed_score_when_threshold_crossed() {
    let recorder = Arc::new(CapturingRecorder::new());
    let mut obs = ZScoreObserver::new(3.0, 3)
        .with_recorder(recorder.clone() as Arc<dyn MetricsRecorder + Send + Sync>);
    let tn = tenant("acme");

    // Build a baseline around 10.0 with low variance.
    for v in [10.0, 10.1, 9.9, 10.05, 9.95, 10.02, 9.98, 10.01] {
        obs.observe(&tn, v, SystemTime::UNIX_EPOCH);
    }
    // Now hit it with a value far above the baseline.
    let result = obs.observe(&tn, 50.0, SystemTime::UNIX_EPOCH);
    assert!(result.is_some(), "50.0 must cross 3-sigma");

    let events = recorder.snapshot();
    // 9 observation events + 1 anomaly event = 10.
    assert_eq!(events.len(), 10);
    let anomalies: Vec<&RecordedEvent> = events
        .iter()
        .filter(|e| matches!(e, RecordedEvent::Anomaly { .. }))
        .collect();
    assert_eq!(anomalies.len(), 1);
    match anomalies[0] {
        RecordedEvent::Anomaly { tenant, score } => {
            assert_eq!(tenant.0, "acme");
            assert!(
                *score > 3.0,
                "score must be the z-score (positive, > threshold)"
            );
        }
        _ => unreachable!(),
    }
}

#[test]
fn zscore_negative_anomaly_score_round_trips_through_recorder() {
    // A value far BELOW the baseline produces a negative
    // z-score. The recorder must see the signed score, not its
    // absolute value, so a downstream OTLP-JSON Gauge can plot
    // "departure direction".
    let recorder = Arc::new(CapturingRecorder::new());
    let mut obs = ZScoreObserver::new(3.0, 3)
        .with_recorder(recorder.clone() as Arc<dyn MetricsRecorder + Send + Sync>);
    let tn = tenant("acme");
    for v in [10.0, 10.1, 9.9, 10.05, 9.95, 10.02, 9.98, 10.01] {
        obs.observe(&tn, v, SystemTime::UNIX_EPOCH);
    }
    obs.observe(&tn, -50.0, SystemTime::UNIX_EPOCH);

    let events = recorder.snapshot();
    let anomaly = events
        .iter()
        .find_map(|e| match e {
            RecordedEvent::Anomaly { score, .. } => Some(*score),
            _ => None,
        })
        .expect("anomaly fired");
    assert!(anomaly < -3.0, "negative z-score preserved with sign");
}

#[test]
fn zscore_without_recorder_behaves_exactly_as_before() {
    // Back-compat: an observer built with `new` alone has a
    // NoopRecorder and does not emit. The struct works exactly
    // as it did before this change.
    let mut obs = ZScoreObserver::new(3.0, 3);
    let tn = tenant("acme");
    for v in [10.0, 10.1, 9.9, 10.05] {
        let r = obs.observe(&tn, v, SystemTime::UNIX_EPOCH);
        assert!(r.is_none(), "no anomaly during warm-up / within baseline");
    }
    // The detector still works — this proves we didn't break
    // the existing behaviour while adding the recorder field.
    let r = obs.observe(&tn, 100.0, SystemTime::UNIX_EPOCH);
    assert!(r.is_some());
}

#[test]
fn rare_event_with_recorder_records_observation_for_every_observe_call() {
    let recorder = Arc::new(CapturingRecorder::new());
    let mut obs = RareEventObserver::new(0.05, 5)
        .with_recorder(recorder.clone() as Arc<dyn MetricsRecorder + Send + Sync>);
    let tn = tenant("acme");
    for ev in &["a", "a", "a", "b", "b"] {
        obs.observe(&tn, ev.to_string(), SystemTime::UNIX_EPOCH);
    }
    let events = recorder.snapshot();
    assert_eq!(events.len(), 5, "one observation event per observe call");
    for event in &events {
        assert!(matches!(event, RecordedEvent::Observation { .. }));
    }
}

#[test]
fn rare_event_with_recorder_records_anomaly_with_fraction_score_on_first_crossing() {
    let recorder = Arc::new(CapturingRecorder::new());
    let mut obs = RareEventObserver::new(0.05, 10)
        .with_recorder(recorder.clone() as Arc<dyn MetricsRecorder + Send + Sync>);
    let tn = tenant("acme");
    // Build a vocabulary of mostly "common" events, then drop
    // a rare one in.
    for _ in 0..99 {
        obs.observe(&tn, "common".to_string(), SystemTime::UNIX_EPOCH);
    }
    let r = obs.observe(&tn, "rare".to_string(), SystemTime::UNIX_EPOCH);
    assert!(r.is_some(), "1/100 is below 0.05 rarity threshold");

    let events = recorder.snapshot();
    // 100 observation events + 1 anomaly = 101.
    assert_eq!(events.len(), 101);
    let anomalies: Vec<&RecordedEvent> = events
        .iter()
        .filter(|e| matches!(e, RecordedEvent::Anomaly { .. }))
        .collect();
    assert_eq!(anomalies.len(), 1);
    match anomalies[0] {
        RecordedEvent::Anomaly { tenant, score } => {
            assert_eq!(tenant.0, "acme");
            assert!(
                *score > 0.0 && *score <= 0.05,
                "score is the fraction (1/100 = 0.01), bounded by the rarity threshold"
            );
        }
        _ => unreachable!(),
    }
}

#[test]
fn rare_event_repeat_of_already_fired_event_does_not_re_record_anomaly() {
    // v0 first-crossing semantics: once a rare event has fired
    // it doesn't fire again. The recorder must mirror that —
    // observation events keep landing, but no second anomaly
    // event for the same value.
    let recorder = Arc::new(CapturingRecorder::new());
    let mut obs = RareEventObserver::new(0.05, 10)
        .with_recorder(recorder.clone() as Arc<dyn MetricsRecorder + Send + Sync>);
    let tn = tenant("acme");
    for _ in 0..99 {
        obs.observe(&tn, "common".to_string(), SystemTime::UNIX_EPOCH);
    }
    obs.observe(&tn, "rare".to_string(), SystemTime::UNIX_EPOCH);
    obs.observe(&tn, "rare".to_string(), SystemTime::UNIX_EPOCH);
    obs.observe(&tn, "rare".to_string(), SystemTime::UNIX_EPOCH);

    let events = recorder.snapshot();
    let anomaly_count = events
        .iter()
        .filter(|e| matches!(e, RecordedEvent::Anomaly { .. }))
        .count();
    let observation_count = events
        .iter()
        .filter(|e| matches!(e, RecordedEvent::Observation { .. }))
        .count();
    assert_eq!(observation_count, 102, "every observe lands as observation");
    assert_eq!(anomaly_count, 1, "first-crossing only");
}

#[test]
fn observers_remain_clone() {
    // Clone derive on ZScoreObserver and RareEventObserver was
    // load-bearing for some downstream callers. Adding the
    // recorder field could have broken it; wrapping in Arc
    // preserves Clone. This test is the compile-time + runtime
    // guard.
    fn assert_clone<T: Clone>(_: &T) {}
    let z = ZScoreObserver::new(3.0, 5);
    let r = RareEventObserver::new(0.05, 5);
    assert_clone(&z);
    assert_clone(&r);
    let _z2 = z.clone();
    let _r2 = r.clone();
}
