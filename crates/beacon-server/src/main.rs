// Kaleidoscope Beacon — server binary
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

//! beacon-server entry point.
//!
//! Thin shell: CLI parsing, runtime construction, signal handling.
//! All orchestration logic lives in `beacon_server` (lib.rs).

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::SystemTime;

use beacon::{load_rules, Rule, RuleState, Sink};
use beacon_server::{build_http_client, build_sinks, evaluate_once, fetch_query};
use clap::Parser;
use tokio::signal;
use tracing::{debug, error, info, warn};

#[derive(Debug, Parser)]
#[command(
    name = "beacon-server",
    about = "Kaleidoscope Beacon — alerting engine over any OTel-compatible PromQL backend",
    version
)]
struct Args {
    /// Directory tree of rule TOML files. Loaded once at startup;
    /// SIGHUP reload arrives at slice 03.
    #[arg(long, value_name = "DIR")]
    rules: PathBuf,
    /// PromQL HTTP backend base URL (e.g.
    /// `http://localhost:9090/api/v1`). The trailing
    /// `/query?query=...` path is appended per request.
    #[arg(long, value_name = "URL")]
    backend: String,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let outcome = match load_rules(&args.rules) {
        Ok(o) => o,
        Err(err) => {
            error!(error = %err, "failed to read rule directory");
            return ExitCode::from(2);
        }
    };

    for diag in &outcome.diagnostics {
        warn!(diagnostic = %diag.display(), "rule load diagnostic");
    }

    if !outcome.has_any_rules() {
        error!(
            rules_dir = %args.rules.display(),
            diagnostics = outcome.diagnostics.len(),
            "no rules loaded; refusing to start"
        );
        return ExitCode::from(1);
    }

    info!(
        rules_loaded = outcome.rules.len(),
        diagnostics = outcome.diagnostics.len(),
        backend = %args.backend,
        "beacon-server starting"
    );

    let client = match build_http_client() {
        Ok(c) => c,
        Err(err) => {
            error!(error = %err, "failed to construct HTTP client");
            return ExitCode::from(3);
        }
    };

    let backend = Arc::new(args.backend);
    let mut handles = Vec::with_capacity(outcome.rules.len());
    for rule in outcome.rules {
        let backend = Arc::clone(&backend);
        let client = client.clone();
        handles.push(tokio::spawn(async move {
            run_rule(rule, backend, client).await;
        }));
    }

    let shutdown = tokio::signal::ctrl_c();
    #[cfg(unix)]
    let mut term = match signal::unix::signal(signal::unix::SignalKind::terminate()) {
        Ok(s) => s,
        Err(err) => {
            error!(error = %err, "failed to install SIGTERM handler");
            return ExitCode::from(4);
        }
    };
    #[cfg(unix)]
    tokio::select! {
        _ = shutdown => info!("SIGINT received; shutting down"),
        _ = term.recv() => info!("SIGTERM received; shutting down"),
    }
    #[cfg(not(unix))]
    {
        let _ = shutdown.await;
        info!("SIGINT received; shutting down");
    }

    for handle in handles {
        handle.abort();
    }
    ExitCode::SUCCESS
}

/// Per-rule loop: tick → fetch → transition → emit.
async fn run_rule(rule: Rule, backend: Arc<String>, client: reqwest::Client) {
    let mut state = RuleState::Inactive;
    let sinks: Vec<Arc<dyn Sink>> = match build_sinks(&rule) {
        Ok(s) => s,
        Err(err) => {
            error!(rule = %rule.name, error = %err, "failed to build sinks; rule disabled");
            return;
        }
    };

    let mut ticker = tokio::time::interval(rule.interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;
        let outcome = match fetch_query(&backend, &rule.query, &client).await {
            Ok(o) => o,
            Err(err) => {
                warn!(rule = %rule.name, error = %err, "PromQL fetch failed; treating as Inactive");
                beacon::QueryOutcome::Inactive
            }
        };
        let now = SystemTime::now();
        let (next, incident) = evaluate_once(&rule, state, outcome, now);
        if state != next {
            debug!(rule = %rule.name, from = ?state, to = ?next, "state transition");
        }
        state = next;
        if let Some(incident) = incident {
            info!(rule = %rule.name, "emitting incident");
            for sink in &sinks {
                if let Err(err) = sink.emit(&incident).await {
                    warn!(
                        rule = %rule.name,
                        sink_kind = %sink.kind(),
                        error = %err,
                        "sink emission failed"
                    );
                }
            }
        }
    }
}
