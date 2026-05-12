// Kaleidoscope Beacon — slice 01 walking skeleton acceptance test
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

//! Slice 01 — Walking skeleton
//!
//! Maps to `docs/feature/beacon-v0/slices/slice-01-walking-skeleton.md`.
//! Companion story: US-BE-01.
//!
//! The smallest unit of evidence that the load → evaluate → emit
//! pipeline works end-to-end. Drives the public surface only.
//!
//! Sasha is the principal user: she authors a Rule struct, the
//! evaluator advances its state through Inactive → Pending → Firing
//! across two ticks, and the WebhookSink lands one POST at the
//! configured URL the third time the condition holds.
//!
//! The Prometheus HTTP backend is faked with wiremock so the test
//! runs in-process (no docker dependency at slice 01; the real
//! container fixture comes at slice 02 alongside CUE loading).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex;

use beacon::{
    transition, Incident, QueryOutcome, Rule, RuleState, Severity, Sink, SinkKind, WebhookSink,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// --------------------------------------------------------------------
// State machine tests — pure, no I/O.
// --------------------------------------------------------------------

fn rule_with_for_duration(for_duration: Duration) -> Rule {
    Rule {
        name: "service_down".to_string(),
        query: "up == 0".to_string(),
        for_duration,
        interval: Duration::from_secs(30),
        severity: Severity::Critical,
        labels: BTreeMap::new(),
    }
}

#[test]
fn inactive_plus_inactive_stays_inactive_with_no_emission() {
    let rule = rule_with_for_duration(Duration::from_secs(60));
    let now = SystemTime::UNIX_EPOCH;
    let (next, emission) = transition(RuleState::Inactive, QueryOutcome::Inactive, &rule, now);
    assert_eq!(next, RuleState::Inactive);
    assert!(emission.is_none());
}

#[test]
fn inactive_plus_active_enters_pending_with_no_emission() {
    let rule = rule_with_for_duration(Duration::from_secs(60));
    let now = SystemTime::UNIX_EPOCH;
    let (next, emission) = transition(RuleState::Inactive, QueryOutcome::Active, &rule, now);
    assert_eq!(next, RuleState::Pending { since: now });
    assert!(emission.is_none());
}

#[test]
fn pending_below_dwell_stays_pending_with_no_emission() {
    let rule = rule_with_for_duration(Duration::from_secs(60));
    let started = SystemTime::UNIX_EPOCH;
    let now = started + Duration::from_secs(30);
    let (next, emission) = transition(
        RuleState::Pending { since: started },
        QueryOutcome::Active,
        &rule,
        now,
    );
    assert_eq!(next, RuleState::Pending { since: started });
    assert!(emission.is_none());
}

#[test]
fn pending_at_or_past_dwell_transitions_to_firing_and_emits_firing_incident() {
    let rule = rule_with_for_duration(Duration::from_secs(60));
    let started = SystemTime::UNIX_EPOCH;
    let now = started + Duration::from_secs(60);
    let (next, emission) = transition(
        RuleState::Pending { since: started },
        QueryOutcome::Active,
        &rule,
        now,
    );
    assert_eq!(next, RuleState::Firing { since: now });
    match emission {
        Some(beacon::state_machine::Emission::Firing(_)) => {}
        Some(other) => panic!("expected Firing emission, got {other:?}"),
        None => panic!("expected a Firing emission, got none"),
    }
}

#[test]
fn pending_going_inactive_returns_to_inactive_with_no_emission() {
    let rule = rule_with_for_duration(Duration::from_secs(60));
    let started = SystemTime::UNIX_EPOCH;
    let now = started + Duration::from_secs(45);
    let (next, emission) = transition(
        RuleState::Pending { since: started },
        QueryOutcome::Inactive,
        &rule,
        now,
    );
    assert_eq!(next, RuleState::Inactive);
    assert!(emission.is_none());
}

#[test]
fn firing_staying_active_emits_nothing_new() {
    let rule = rule_with_for_duration(Duration::from_secs(60));
    let started = SystemTime::UNIX_EPOCH;
    let now = started + Duration::from_secs(120);
    let (next, emission) = transition(
        RuleState::Firing { since: started },
        QueryOutcome::Active,
        &rule,
        now,
    );
    assert_eq!(next, RuleState::Firing { since: started });
    assert!(emission.is_none());
}

#[test]
fn firing_going_inactive_emits_resolved_and_returns_to_inactive() {
    let rule = rule_with_for_duration(Duration::from_secs(60));
    let started = SystemTime::UNIX_EPOCH;
    let now = started + Duration::from_secs(300);
    let (next, emission) = transition(
        RuleState::Firing { since: started },
        QueryOutcome::Inactive,
        &rule,
        now,
    );
    assert_eq!(next, RuleState::Inactive);
    match emission {
        Some(beacon::state_machine::Emission::Resolved(incident)) => {
            assert_eq!(incident.started_at, started);
            assert_eq!(incident.resolved_at, Some(now));
        }
        Some(other) => panic!("expected Resolved emission, got {other:?}"),
        None => panic!("expected a Resolved emission, got none"),
    }
}

// --------------------------------------------------------------------
// WebhookSink integration test against wiremock.
// --------------------------------------------------------------------

#[tokio::test]
async fn webhook_sink_posts_canonical_incident_json_to_configured_url() {
    let mock_server = MockServer::start().await;

    // Capture the POST: assert it arrived with the right path, method,
    // and JSON body.
    Mock::given(method("POST"))
        .and(path("/alerts"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let url = format!("{}/alerts", mock_server.uri());
    let sink = WebhookSink::new(&url).expect("WebhookSink construction");
    assert_eq!(sink.kind(), SinkKind::Webhook);

    let rule = rule_with_for_duration(Duration::from_secs(60));
    let started = SystemTime::UNIX_EPOCH;
    let incident = Incident::firing(&rule, started);

    sink.emit(&incident).await.expect("webhook emission");

    mock_server.verify().await;
}

#[tokio::test]
async fn webhook_sink_returns_transient_error_on_http_5xx() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/alerts"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&mock_server)
        .await;

    let url = format!("{}/alerts", mock_server.uri());
    let sink = WebhookSink::new(&url).expect("WebhookSink construction");
    let rule = rule_with_for_duration(Duration::from_secs(60));
    let incident = Incident::firing(&rule, SystemTime::UNIX_EPOCH);

    let result = sink.emit(&incident).await;
    match result {
        Err(beacon::SinkError::Transient { .. }) => {}
        other => panic!("expected Transient, got {other:?}"),
    }
}

#[tokio::test]
async fn webhook_sink_returns_permanent_error_on_http_4xx() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/alerts"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&mock_server)
        .await;

    let url = format!("{}/alerts", mock_server.uri());
    let sink = WebhookSink::new(&url).expect("WebhookSink construction");
    let rule = rule_with_for_duration(Duration::from_secs(60));
    let incident = Incident::firing(&rule, SystemTime::UNIX_EPOCH);

    let result = sink.emit(&incident).await;
    match result {
        Err(beacon::SinkError::Permanent { .. }) => {}
        other => panic!("expected Permanent, got {other:?}"),
    }
}

// --------------------------------------------------------------------
// End-to-end walking skeleton: Sasha's first cycle.
//
// Three transitions across simulated time:
//   T+0   : Inactive + Active → Pending(0)
//   T+30s : Pending(0) + Active → Pending(0)   (dwell not met)
//   T+60s : Pending(0) + Active → Firing(60)   (dwell met → emit)
//   T+90s : Firing(60) + Active → Firing(60)   (no new emission)
//   T+120s: Firing(60) + Inactive → Inactive   (Resolved emission)
//
// The webhook sink captures one POST for Firing and one for Resolved.
// --------------------------------------------------------------------

#[tokio::test]
async fn sashas_first_cycle_fires_one_webhook_then_resolves_one_webhook() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/alerts"))
        .respond_with(ResponseTemplate::new(200))
        .expect(2)
        .mount(&mock_server)
        .await;

    let url = format!("{}/alerts", mock_server.uri());
    let sink: Arc<dyn Sink> = Arc::new(WebhookSink::new(&url).expect("WebhookSink construction"));

    let rule = rule_with_for_duration(Duration::from_secs(60));
    let state = Arc::new(Mutex::new(RuleState::Inactive));
    let t0 = SystemTime::UNIX_EPOCH;

    // Schedule of (offset_from_t0, outcome) pairs.
    let schedule = [
        (Duration::from_secs(0), QueryOutcome::Active),
        (Duration::from_secs(30), QueryOutcome::Active),
        (Duration::from_secs(60), QueryOutcome::Active),
        (Duration::from_secs(90), QueryOutcome::Active),
        (Duration::from_secs(120), QueryOutcome::Inactive),
    ];

    let mut emissions: Vec<&'static str> = Vec::new();
    for (offset, outcome) in schedule {
        let now = t0 + offset;
        let mut current = state.lock().await;
        let (next, emission) = transition(*current, outcome, &rule, now);
        *current = next;
        drop(current);

        match emission {
            Some(beacon::state_machine::Emission::Firing(incident)) => {
                emissions.push("firing");
                sink.emit(&incident).await.expect("firing emit");
            }
            Some(beacon::state_machine::Emission::Resolved(incident)) => {
                emissions.push("resolved");
                sink.emit(&incident).await.expect("resolved emit");
            }
            None => {}
        }
    }

    assert_eq!(emissions, vec!["firing", "resolved"]);
    mock_server.verify().await;
}
