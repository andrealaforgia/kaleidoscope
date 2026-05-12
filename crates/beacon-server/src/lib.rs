// Kaleidoscope Beacon — server library
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

//! beacon-server orchestrator primitives.
//!
//! Split out from `main.rs` so the orchestration logic
//! (`fetch_query`, `evaluate_once`, sink construction) can be tested
//! against `wiremock` without spawning the binary.
//!
//! Per ADR-0033 the runtime choice still lives in the `tokio::main`
//! entry point; this library is executor-agnostic via `async fn`.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use beacon::{
    transition, Emission, MattermostSink, OnCallSink, QueryOutcome, Rule, RuleState, Sink,
    SinkConfig, WebhookSink, ZulipSink,
};
use serde::Deserialize;

/// Build the sink adapters for one rule. Failure on any adapter is
/// fatal for that rule — the orchestrator skips it and continues
/// with the rest.
pub fn build_sinks(rule: &Rule) -> Result<Vec<Arc<dyn Sink>>, String> {
    let mut out: Vec<Arc<dyn Sink>> = Vec::with_capacity(rule.sinks.len());
    for cfg in &rule.sinks {
        out.push(adapter_from_config(cfg, &rule.name)?);
    }
    Ok(out)
}

fn adapter_from_config(cfg: &SinkConfig, rule_name: &str) -> Result<Arc<dyn Sink>, String> {
    let url = cfg
        .url
        .as_ref()
        .ok_or_else(|| format!("sink for rule \"{rule_name}\" missing url"))?;
    match cfg.kind.as_str() {
        "webhook" => {
            let sink = WebhookSink::new(url)
                .map_err(|err| format!("webhook sink construction failed: {err}"))?;
            Ok(Arc::new(sink))
        }
        "mattermost" => {
            let sink = MattermostSink::new(url, cfg.channel.clone())
                .map_err(|err| format!("mattermost sink construction failed: {err}"))?;
            Ok(Arc::new(sink))
        }
        "zulip" => {
            let topic = cfg.topic.clone().ok_or_else(|| {
                format!("zulip sink for rule \"{rule_name}\" missing \"topic\"")
            })?;
            let sink = ZulipSink::new(url, topic)
                .map_err(|err| format!("zulip sink construction failed: {err}"))?;
            Ok(Arc::new(sink))
        }
        "oncall" => {
            // Per ADR-0035: secrets via environment variable named in
            // CUE / TOML. Missing env var is not fatal at startup —
            // the adapter will simply not attach the bearer header.
            let token = cfg
                .auth_token_env
                .as_ref()
                .and_then(|name| std::env::var(name).ok());
            let sink = OnCallSink::new(url, token)
                .map_err(|err| format!("oncall sink construction failed: {err}"))?;
            Ok(Arc::new(sink))
        }
        other => Err(format!(
            "unsupported sink kind \"{other}\" for rule \"{rule_name}\" (slice 04 supports: webhook, mattermost, zulip, oncall)"
        )),
    }
}

/// Fetch one PromQL instant query and classify the result. Empty
/// result set → Inactive. Non-empty → Active. Any HTTP or shape
/// failure surfaces as `Err` so the caller can choose to treat it as
/// Inactive (slice 02b default) or as a backoff trigger (later slice).
pub async fn fetch_query(
    backend: &str,
    query: &str,
    client: &reqwest::Client,
) -> Result<QueryOutcome, FetchError> {
    let url = format!("{}/query", backend.trim_end_matches('/'));
    let response = client
        .get(&url)
        .query(&[("query", query)])
        .send()
        .await
        .map_err(|err| FetchError::Network(err.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        return Err(FetchError::HttpStatus(status.as_u16()));
    }
    let body: PromResponse = response
        .json()
        .await
        .map_err(|err| FetchError::InvalidJson(err.to_string()))?;
    if body.status != "success" {
        return Err(FetchError::PromError(
            body.error.unwrap_or_else(|| "unknown".to_string()),
        ));
    }
    if body.data.result.is_empty() {
        Ok(QueryOutcome::Inactive)
    } else {
        Ok(QueryOutcome::Active)
    }
}

/// One evaluator cycle for a rule. Returns the next state and the
/// emission (Firing or Resolved) if a transition fired. The caller
/// is expected to drive this in a ticker loop and feed the emission
/// to an [`InhibitionResolver`](beacon::InhibitionResolver) before
/// forwarding to sinks.
pub fn evaluate_once(
    rule: &Rule,
    state: RuleState,
    outcome: QueryOutcome,
    now: SystemTime,
) -> (RuleState, Option<Emission>) {
    transition(state, outcome, rule, now)
}

/// Build a production-shaped HTTP client. 30 s total timeout,
/// rustls, JSON+gzip features baked into the binary's Cargo.toml.
pub fn build_http_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
}

#[derive(Debug, Deserialize)]
struct PromResponse {
    status: String,
    #[serde(default)]
    data: PromData,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct PromData {
    #[serde(default)]
    result: Vec<serde_json::Value>,
}

/// Failure surface for the PromQL HTTP fetch. The orchestrator turns
/// every variant into a warn-level log and a degraded `Inactive`
/// outcome at slice 02b; later slices add backoff classification.
#[derive(Debug)]
pub enum FetchError {
    Network(String),
    HttpStatus(u16),
    InvalidJson(String),
    PromError(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Network(m) => write!(f, "network: {m}"),
            FetchError::HttpStatus(s) => write!(f, "HTTP {s}"),
            FetchError::InvalidJson(m) => write!(f, "invalid JSON: {m}"),
            FetchError::PromError(m) => write!(f, "prom error: {m}"),
        }
    }
}

impl std::error::Error for FetchError {}
