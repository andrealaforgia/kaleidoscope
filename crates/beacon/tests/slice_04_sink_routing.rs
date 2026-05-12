// Kaleidoscope Beacon — slice 04 multi-sink routing acceptance test
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

//! Slice 04 — multi-sink routing
//!
//! Maps to `docs/feature/beacon-v0/slices/slice-04-sink-routing.md`.
//! Companion story: US-BE-04.
//!
//! Sasha has the team's notification topology: Mattermost for
//! low-severity, OnCall for paging, Zulip for postmortem feeds.
//! Each rule routes to one or more sinks via the per-rule `sinks`
//! list. Slice 04 ships four HTTP-based adapters behind one trait:
//! Webhook (slice 01), Mattermost, Zulip, OnCall. SMTP arrives at v1.
//!
//! Each sink's emission carries the same canonical Incident,
//! formatted appropriately for that sink (Markdown for Mattermost,
//! plain text for Zulip, OnCall's JSON schema for OnCall, canonical
//! JSON for webhook). Adapter failures are independent — a slow
//! Mattermost does not block delivery to Zulip.

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};

use beacon::{
    Incident, MattermostSink, OnCallSink, Severity, Sink, SinkError, SinkKind, WebhookSink,
    ZulipSink,
};
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn firing_incident() -> Incident {
    let mut labels = BTreeMap::new();
    labels.insert("service".to_string(), "payments_api".to_string());
    Incident {
        name: "payments_api_down".to_string(),
        query: "up{service=\"payments_api\"} == 0".to_string(),
        severity: Severity::Critical,
        labels,
        started_at: SystemTime::UNIX_EPOCH,
        resolved_at: None,
    }
}

fn resolved_incident() -> Incident {
    let mut labels = BTreeMap::new();
    labels.insert("service".to_string(), "payments_api".to_string());
    Incident {
        name: "payments_api_down".to_string(),
        query: "up{service=\"payments_api\"} == 0".to_string(),
        severity: Severity::Critical,
        labels,
        started_at: SystemTime::UNIX_EPOCH,
        resolved_at: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(120)),
    }
}

// --------------------------------------------------------------------
// MattermostSink — Markdown body, optional channel override.
// --------------------------------------------------------------------

#[tokio::test]
async fn mattermost_posts_markdown_body_with_rule_name_and_severity() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/hook"))
        .and(body_partial_json(serde_json::json!({
            "text": "**payments_api_down** — severity `critical`\n```\nup{service=\"payments_api\"} == 0\n```"
        })))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let sink =
        MattermostSink::new(format!("{}/hook", server.uri()), None).expect("mattermost ctor");
    assert_eq!(sink.kind(), SinkKind::Mattermost);
    sink.emit(&firing_incident()).await.expect("emit");
    server.verify().await;
}

#[tokio::test]
async fn mattermost_includes_channel_when_configured() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/hook"))
        .and(body_partial_json(serde_json::json!({"channel": "#alerts"})))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let sink = MattermostSink::new(
        format!("{}/hook", server.uri()),
        Some("#alerts".to_string()),
    )
    .expect("mattermost ctor");
    sink.emit(&firing_incident()).await.expect("emit");
    server.verify().await;
}

#[tokio::test]
async fn mattermost_marks_resolved_incident_with_suffix() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/hook"))
        .and(body_partial_json(serde_json::json!({
            "text": "**payments_api_down** (resolved) — severity `critical`\n```\nup{service=\"payments_api\"} == 0\n```"
        })))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let sink =
        MattermostSink::new(format!("{}/hook", server.uri()), None).expect("mattermost ctor");
    sink.emit(&resolved_incident()).await.expect("emit");
    server.verify().await;
}

#[tokio::test]
async fn mattermost_classifies_5xx_as_transient() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/hook"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    let sink =
        MattermostSink::new(format!("{}/hook", server.uri()), None).expect("mattermost ctor");
    let err = sink.emit(&firing_incident()).await.unwrap_err();
    assert!(matches!(err, SinkError::Transient { .. }));
}

// --------------------------------------------------------------------
// ZulipSink — topic-keyed plain text.
// --------------------------------------------------------------------

#[tokio::test]
async fn zulip_posts_topic_and_content() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/zulip"))
        .and(body_partial_json(serde_json::json!({
            "topic": "alerts",
            "content": "payments_api_down severity=critical query=up{service=\"payments_api\"} == 0"
        })))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let sink = ZulipSink::new(format!("{}/zulip", server.uri()), "alerts").expect("zulip ctor");
    assert_eq!(sink.kind(), SinkKind::Zulip);
    sink.emit(&firing_incident()).await.expect("emit");
    server.verify().await;
}

#[tokio::test]
async fn zulip_classifies_4xx_as_permanent() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/zulip"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    let sink = ZulipSink::new(format!("{}/zulip", server.uri()), "alerts").expect("zulip ctor");
    let err = sink.emit(&firing_incident()).await.unwrap_err();
    assert!(matches!(err, SinkError::Permanent { .. }));
}

// --------------------------------------------------------------------
// OnCallSink — OnCall webhook JSON shape + optional bearer token.
// --------------------------------------------------------------------

#[tokio::test]
async fn oncall_posts_alert_uid_title_state_alerting_for_firing() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oncall/webhook"))
        .and(body_partial_json(serde_json::json!({
            "alert_uid": "payments_api_down",
            "title": "payments_api_down",
            "state": "alerting"
        })))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let sink =
        OnCallSink::new(format!("{}/oncall/webhook", server.uri()), None).expect("oncall ctor");
    assert_eq!(sink.kind(), SinkKind::OnCall);
    sink.emit(&firing_incident()).await.expect("emit");
    server.verify().await;
}

#[tokio::test]
async fn oncall_posts_state_ok_for_resolved() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oncall/webhook"))
        .and(body_partial_json(serde_json::json!({
            "alert_uid": "payments_api_down",
            "state": "ok"
        })))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let sink =
        OnCallSink::new(format!("{}/oncall/webhook", server.uri()), None).expect("oncall ctor");
    sink.emit(&resolved_incident()).await.expect("emit");
    server.verify().await;
}

#[tokio::test]
async fn oncall_attaches_bearer_authorization_when_token_provided() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oncall/webhook"))
        .and(header("Authorization", "Bearer SECRET_TOKEN"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let sink = OnCallSink::new(
        format!("{}/oncall/webhook", server.uri()),
        Some("SECRET_TOKEN".to_string()),
    )
    .expect("oncall ctor");
    sink.emit(&firing_incident()).await.expect("emit");
    server.verify().await;
}

// --------------------------------------------------------------------
// Header / body redaction by construction — none of the adapters
// can leak the operator's auth token into the JSON body, because
// the body is constructed from Incident fields, not from headers.
// --------------------------------------------------------------------

#[tokio::test]
async fn oncall_bearer_token_value_does_not_appear_in_request_body() {
    use std::sync::{Arc, Mutex};

    // Capture the request body so we can assert the token is NOT there.
    let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_clone = Arc::clone(&captured);

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oncall/webhook"))
        .respond_with(move |req: &wiremock::Request| {
            *captured_clone.lock().unwrap() = req.body.clone();
            ResponseTemplate::new(200)
        })
        .expect(1)
        .mount(&server)
        .await;

    let token = "SUPER_SECRET_BEACON_TOKEN_VALUE_THAT_MUST_NEVER_LEAK";
    let sink = OnCallSink::new(
        format!("{}/oncall/webhook", server.uri()),
        Some(token.to_string()),
    )
    .expect("oncall ctor");
    sink.emit(&firing_incident()).await.expect("emit");

    let body = captured.lock().unwrap().clone();
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        !body_str.contains(token),
        "OnCall bearer token must never appear in the request body; got body: {body_str}"
    );
}

#[tokio::test]
async fn webhook_sink_serialises_full_incident_as_json() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/alerts"))
        .and(body_partial_json(serde_json::json!({
            "name": "payments_api_down",
            "query": "up{service=\"payments_api\"} == 0",
            "severity": "critical"
        })))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let sink = WebhookSink::new(format!("{}/alerts", server.uri())).expect("webhook ctor");
    sink.emit(&firing_incident()).await.expect("emit");
    server.verify().await;
}
