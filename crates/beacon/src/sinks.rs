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

/// Discriminator used in telemetry and (later) per-sink routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkKind {
    Webhook,
}

impl fmt::Display for SinkKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SinkKind::Webhook => f.write_str("webhook"),
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
