// Kaleidoscope Beacon — beacon-server smoke test
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

//! Smoke test for `beacon-server`'s orchestrator primitives.
//!
//! Exercises `fetch_query` and `evaluate_once` against a wiremock
//! Prometheus HTTP API. The walking-skeleton + loader tests already
//! cover the library; this test pins the *binary's* JSON contract
//! with the Prometheus backend.

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};

use beacon::{Emission, Rule, RuleState, Severity};
use beacon_server::{build_http_client, evaluate_once, fetch_query, FetchError};
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn rule() -> Rule {
    Rule {
        name: "service_down".to_string(),
        query: "up == 0".to_string(),
        for_duration: Duration::from_secs(60),
        interval: Duration::from_secs(30),
        severity: Severity::Critical,
        labels: BTreeMap::new(),
        sinks: Vec::new(),
        inhibits: Vec::new(),
    }
}

#[tokio::test]
async fn fetch_query_returns_active_when_prom_returns_non_empty_vector() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/query"))
        .and(query_param("query", "up == 0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "success",
            "data": {
                "resultType": "vector",
                "result": [{
                    "metric": {"__name__": "up", "instance": "localhost:9090"},
                    "value": [1640000000, "0"]
                }]
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = build_http_client().expect("client");
    let outcome = fetch_query(&server.uri(), "up == 0", &client)
        .await
        .expect("fetch_query");
    assert_eq!(outcome, beacon::QueryOutcome::Active);
}

#[tokio::test]
async fn fetch_query_returns_inactive_when_prom_returns_empty_vector() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/query"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "success",
            "data": {"resultType": "vector", "result": []}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = build_http_client().expect("client");
    let outcome = fetch_query(&server.uri(), "up", &client)
        .await
        .expect("fetch");
    assert_eq!(outcome, beacon::QueryOutcome::Inactive);
}

#[tokio::test]
async fn fetch_query_surfaces_http_5xx_as_http_status_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/query"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;

    let client = build_http_client().expect("client");
    let err = fetch_query(&server.uri(), "up", &client).await.unwrap_err();
    match err {
        FetchError::HttpStatus(503) => {}
        other => panic!("expected HttpStatus(503), got {other:?}"),
    }
}

#[tokio::test]
async fn fetch_query_surfaces_prom_status_error_with_error_field() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/query"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "error",
            "errorType": "bad_data",
            "error": "1:5: parse error: unexpected ..."
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = build_http_client().expect("client");
    let err = fetch_query(&server.uri(), "rate(metric[5m", &client)
        .await
        .unwrap_err();
    match err {
        FetchError::PromError(msg) => assert!(msg.contains("parse error")),
        other => panic!("expected PromError, got {other:?}"),
    }
}

#[tokio::test]
async fn fetch_query_surfaces_non_json_body_as_invalid_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/query"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .expect(1)
        .mount(&server)
        .await;

    let client = build_http_client().expect("client");
    let err = fetch_query(&server.uri(), "up", &client).await.unwrap_err();
    match err {
        FetchError::InvalidJson(_) => {}
        other => panic!("expected InvalidJson, got {other:?}"),
    }
}

#[test]
fn evaluate_once_progresses_state_machine_with_no_emission_when_dwell_not_met() {
    let rule = rule();
    let t0 = SystemTime::UNIX_EPOCH;
    let (next, emission) =
        evaluate_once(&rule, RuleState::Inactive, beacon::QueryOutcome::Active, t0);
    assert_eq!(next, RuleState::Pending { since: t0 });
    assert!(emission.is_none());
}

#[test]
fn evaluate_once_emits_firing_emission_when_dwell_met() {
    let rule = rule();
    let t0 = SystemTime::UNIX_EPOCH;
    let dwell_met = t0 + Duration::from_secs(60);
    let (next, emission) = evaluate_once(
        &rule,
        RuleState::Pending { since: t0 },
        beacon::QueryOutcome::Active,
        dwell_met,
    );
    assert_eq!(next, RuleState::Firing { since: dwell_met });
    match emission.expect("firing emission") {
        Emission::Firing(inc) => {
            assert_eq!(inc.name, "service_down");
            assert!(inc.resolved_at.is_none());
        }
        other => panic!("expected Firing, got {other:?}"),
    }
}

#[test]
fn evaluate_once_emits_resolved_emission_on_recovery() {
    let rule = rule();
    let t0 = SystemTime::UNIX_EPOCH;
    let later = t0 + Duration::from_secs(120);
    let (next, emission) = evaluate_once(
        &rule,
        RuleState::Firing { since: t0 },
        beacon::QueryOutcome::Inactive,
        later,
    );
    assert_eq!(next, RuleState::Inactive);
    match emission.expect("resolved emission") {
        Emission::Resolved(inc) => assert_eq!(inc.resolved_at, Some(later)),
        other => panic!("expected Resolved, got {other:?}"),
    }
}
