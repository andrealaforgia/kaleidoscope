// Kaleidoscope Beacon — rule-evaluation + alerting engine
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

//! Sink trait + the webhook adapter.
//!
//! Slice 01 ships [`WebhookSink`] only; the SMTP, Mattermost, Zulip,
//! and OnCall adapters arrive at slice 04. The trait abstracts every
//! adapter behind the same `emit(&Incident)` signature so the
//! evaluator never sees adapter-specific concerns.

use async_trait::async_trait;
use std::fmt;
use std::time::Duration;

use crate::types::Incident;

/// Discriminator used in telemetry and per-sink routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkKind {
    Webhook,
    Mattermost,
    Zulip,
    OnCall,
}

impl fmt::Display for SinkKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SinkKind::Webhook => f.write_str("webhook"),
            SinkKind::Mattermost => f.write_str("mattermost"),
            SinkKind::Zulip => f.write_str("zulip"),
            SinkKind::OnCall => f.write_str("oncall"),
        }
    }
}

/// Adapter-emitted failure classification. The orchestrator (binary)
/// uses the variant to decide whether to retry: Transient retries on
/// the configured schedule; Permanent records and moves on.
#[derive(Debug)]
pub enum SinkError {
    /// Network failure, HTTP 5xx, timeout: retry per ADR-0035 §retry.
    Transient {
        retry_after: Duration,
        message: String,
    },
    /// HTTP 4xx, configuration error: do not retry.
    Permanent { message: String },
}

impl fmt::Display for SinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SinkError::Transient { message, .. } => {
                write!(f, "transient sink error: {message}")
            }
            SinkError::Permanent { message } => {
                write!(f, "permanent sink error: {message}")
            }
        }
    }
}

impl std::error::Error for SinkError {}

/// Adapter interface. Implementations carry their own configuration
/// (URL, channel, credentials) constructed at orchestrator startup.
#[async_trait]
pub trait Sink: Send + Sync {
    async fn emit(&self, incident: &Incident) -> Result<(), SinkError>;

    fn kind(&self) -> SinkKind;
}

/// Webhook adapter. POSTs the canonical [`Incident`] JSON to the
/// configured URL. Transient failure on HTTP 5xx / network error;
/// permanent on HTTP 4xx.
#[derive(Debug, Clone)]
pub struct WebhookSink {
    url: String,
    client: reqwest::Client,
}

impl WebhookSink {
    /// Construct a webhook sink bound to a target URL. The HTTP client
    /// is created with the project default (rustls, 30 s connect timeout).
    pub fn new(url: impl Into<String>) -> Result<Self, SinkError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|err| SinkError::Permanent {
                message: format!("failed to build webhook HTTP client: {err}"),
            })?;
        Ok(Self {
            url: url.into(),
            client,
        })
    }
}

#[async_trait]
impl Sink for WebhookSink {
    async fn emit(&self, incident: &Incident) -> Result<(), SinkError> {
        let response = self
            .client
            .post(&self.url)
            .json(incident)
            .send()
            .await
            .map_err(|err| SinkError::Transient {
                retry_after: Duration::from_secs(1),
                message: format!("webhook POST failed: {err}"),
            })?;

        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        if status.is_server_error() {
            return Err(SinkError::Transient {
                retry_after: Duration::from_secs(1),
                message: format!("webhook returned HTTP {status}"),
            });
        }
        Err(SinkError::Permanent {
            message: format!("webhook returned HTTP {status}"),
        })
    }

    fn kind(&self) -> SinkKind {
        SinkKind::Webhook
    }
}

// ------------------------------------------------------------------
// Mattermost adapter.
//
// Posts a Markdown body to the configured Mattermost incoming
// webhook URL. The payload shape is
// `{ "text": "...", "channel": "..." }`; channel is optional and
// overrides the webhook's default channel binding.
// ------------------------------------------------------------------

/// Mattermost incoming-webhook adapter. Per-rule channel override
/// is supported through [`crate::SinkConfig::channel`].
#[derive(Debug, Clone)]
pub struct MattermostSink {
    url: String,
    channel: Option<String>,
    client: reqwest::Client,
}

impl MattermostSink {
    pub fn new(url: impl Into<String>, channel: Option<String>) -> Result<Self, SinkError> {
        let client = build_client("mattermost")?;
        Ok(Self {
            url: url.into(),
            channel,
            client,
        })
    }
}

#[async_trait]
impl Sink for MattermostSink {
    async fn emit(&self, incident: &Incident) -> Result<(), SinkError> {
        let body = MattermostPayload {
            text: format_markdown(incident),
            channel: self.channel.as_deref(),
        };
        post_json(&self.client, &self.url, &body, "mattermost").await
    }

    fn kind(&self) -> SinkKind {
        SinkKind::Mattermost
    }
}

#[derive(serde::Serialize)]
struct MattermostPayload<'a> {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    channel: Option<&'a str>,
}

fn format_markdown(incident: &Incident) -> String {
    let resolved = incident
        .resolved_at
        .map(|_| " (resolved)")
        .unwrap_or_default();
    format!(
        "**{name}**{resolved} — severity `{severity}`\n```\n{query}\n```",
        name = incident.name,
        severity = match incident.severity {
            crate::types::Severity::Info => "info",
            crate::types::Severity::Warning => "warning",
            crate::types::Severity::Critical => "critical",
        },
        query = incident.query,
    )
}

// ------------------------------------------------------------------
// Zulip adapter.
//
// Posts a topic-keyed plain-text message to the configured Zulip
// incoming webhook URL. The payload shape is
// `{ "topic": "...", "content": "..." }`.
// ------------------------------------------------------------------

/// Zulip incoming-webhook adapter. Topic is required (Zulip's
/// streams are topic-segmented).
#[derive(Debug, Clone)]
pub struct ZulipSink {
    url: String,
    topic: String,
    client: reqwest::Client,
}

impl ZulipSink {
    pub fn new(url: impl Into<String>, topic: impl Into<String>) -> Result<Self, SinkError> {
        let client = build_client("zulip")?;
        Ok(Self {
            url: url.into(),
            topic: topic.into(),
            client,
        })
    }
}

#[async_trait]
impl Sink for ZulipSink {
    async fn emit(&self, incident: &Incident) -> Result<(), SinkError> {
        let body = ZulipPayload {
            topic: &self.topic,
            content: format_plain(incident),
        };
        post_json(&self.client, &self.url, &body, "zulip").await
    }

    fn kind(&self) -> SinkKind {
        SinkKind::Zulip
    }
}

#[derive(serde::Serialize)]
struct ZulipPayload<'a> {
    topic: &'a str,
    content: String,
}

fn format_plain(incident: &Incident) -> String {
    let resolved = incident
        .resolved_at
        .map(|_| " (resolved)")
        .unwrap_or_default();
    format!(
        "{name}{resolved} severity={severity} query={query}",
        name = incident.name,
        severity = match incident.severity {
            crate::types::Severity::Info => "info",
            crate::types::Severity::Warning => "warning",
            crate::types::Severity::Critical => "critical",
        },
        query = incident.query,
    )
}

// ------------------------------------------------------------------
// Grafana OnCall adapter.
//
// Posts a payload conforming to OnCall's documented webhook schema:
// `{ "alert_uid", "title", "state", "message" }`. The `state` field
// maps Firing→"alerting", Resolved→"ok" per OnCall's contract.
// ------------------------------------------------------------------

/// Grafana OnCall webhook adapter. Optional bearer token from an
/// environment variable per ADR-0035 (secret material never inline
/// in CUE / TOML).
#[derive(Debug, Clone)]
pub struct OnCallSink {
    url: String,
    bearer_token: Option<String>,
    client: reqwest::Client,
}

impl OnCallSink {
    pub fn new(url: impl Into<String>, bearer_token: Option<String>) -> Result<Self, SinkError> {
        let client = build_client("oncall")?;
        Ok(Self {
            url: url.into(),
            bearer_token,
            client,
        })
    }
}

#[async_trait]
impl Sink for OnCallSink {
    async fn emit(&self, incident: &Incident) -> Result<(), SinkError> {
        let body = OnCallPayload {
            alert_uid: &incident.name,
            title: &incident.name,
            state: if incident.resolved_at.is_some() {
                "ok"
            } else {
                "alerting"
            },
            message: format!(
                "severity={} query={}",
                match incident.severity {
                    crate::types::Severity::Info => "info",
                    crate::types::Severity::Warning => "warning",
                    crate::types::Severity::Critical => "critical",
                },
                incident.query,
            ),
        };
        let mut req = self.client.post(&self.url).json(&body);
        if let Some(token) = &self.bearer_token {
            req = req.bearer_auth(token);
        }
        let response = req.send().await.map_err(|err| SinkError::Transient {
            retry_after: Duration::from_secs(1),
            message: format!("oncall POST failed: {err}"),
        })?;
        classify_response(response.status(), "oncall")
    }

    fn kind(&self) -> SinkKind {
        SinkKind::OnCall
    }
}

#[derive(serde::Serialize)]
struct OnCallPayload<'a> {
    alert_uid: &'a str,
    title: &'a str,
    state: &'a str,
    message: String,
}

// ------------------------------------------------------------------
// Shared HTTP helpers.
// ------------------------------------------------------------------

fn build_client(kind: &str) -> Result<reqwest::Client, SinkError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|err| SinkError::Permanent {
            message: format!("failed to build {kind} HTTP client: {err}"),
        })
}

async fn post_json<B: serde::Serialize>(
    client: &reqwest::Client,
    url: &str,
    body: &B,
    kind: &str,
) -> Result<(), SinkError> {
    let response =
        client
            .post(url)
            .json(body)
            .send()
            .await
            .map_err(|err| SinkError::Transient {
                retry_after: Duration::from_secs(1),
                message: format!("{kind} POST failed: {err}"),
            })?;
    classify_response(response.status(), kind)
}

fn classify_response(status: reqwest::StatusCode, kind: &str) -> Result<(), SinkError> {
    if status.is_success() {
        return Ok(());
    }
    if status.is_server_error() {
        return Err(SinkError::Transient {
            retry_after: Duration::from_secs(1),
            message: format!("{kind} returned HTTP {status}"),
        });
    }
    Err(SinkError::Permanent {
        message: format!("{kind} returned HTTP {status}"),
    })
}
