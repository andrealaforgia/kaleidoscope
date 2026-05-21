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

use beacon::{
    load_rules, Emission, FileBackedRuleStateStore, InhibitionResolver, Rule, RuleState,
    RuleStateStore, Sink,
};
use beacon_server::{build_http_client, build_sinks, evaluate_once, fetch_query};
use clap::Parser;
use tokio::signal;
use tokio::sync::Mutex;
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

    // Open the durable rule-state store. The base path is derived from
    // the rules directory (no new CLI surface): a `.beacon-state/store`
    // sibling that inherits the operator's filesystem permissions. A
    // corrupt or unreadable state file makes `open()` fail; we refuse
    // to start rather than silently reset firing alerts to Inactive
    // (recover-then-refuse, ADR-0040 decision 3).
    let state_base = args.rules.join(".beacon-state").join("store");
    if let Some(dir) = state_base.parent() {
        if let Err(err) = std::fs::create_dir_all(dir) {
            error!(error = %err, dir = %dir.display(), "failed to create rule-state directory");
            return ExitCode::from(5);
        }
    }
    let store: Arc<dyn RuleStateStore> = match FileBackedRuleStateStore::open(&state_base) {
        Ok(s) => Arc::new(s),
        Err(err) => {
            error!(
                error = %err,
                state_base = %state_base.display(),
                "durable rule state is corrupt; refusing to start (recover-then-refuse)"
            );
            return ExitCode::from(5);
        }
    };

    // Recover persisted state once at startup. States for rules no
    // longer in config are dropped and logged, not resurrected
    // (US-02 sc.4). Each surviving rule is seeded with its recovered
    // state; absent rules default to Inactive.
    let mut recovered = match store.load_all() {
        Ok(map) => map,
        Err(err) => {
            error!(error = %err, "failed to recover rule state; refusing to start");
            return ExitCode::from(5);
        }
    };
    let live_names: std::collections::HashSet<&str> =
        outcome.rules.iter().map(|r| r.name.as_str()).collect();
    for dropped in recovered
        .keys()
        .filter(|name| !live_names.contains(name.as_str()))
    {
        warn!(rule = %dropped, "recovered state for a rule no longer in config; dropping it");
    }
    let firing = recovered
        .values()
        .filter(|s| matches!(s, RuleState::Firing { .. }))
        .count();
    let pending = recovered
        .values()
        .filter(|s| matches!(s, RuleState::Pending { .. }))
        .count();
    info!(
        rules_recovered = recovered.len(),
        firing, pending, "recovered alert state"
    );

    let backend = Arc::new(args.backend);
    // The InhibitionResolver is shared across all per-rule tasks. A
    // Tokio Mutex is appropriate because the .observe() call is
    // synchronous (no .await inside) and the lock is held briefly.
    let resolver = Arc::new(Mutex::new(InhibitionResolver::new(&outcome.rules)));
    let mut handles = Vec::with_capacity(outcome.rules.len());
    for rule in outcome.rules {
        let backend = Arc::clone(&backend);
        let client = client.clone();
        let resolver = Arc::clone(&resolver);
        let store = Arc::clone(&store);
        // Seed from the recovered value for this rule; a rule absent
        // from the recovered map starts Inactive, as a fresh rule would.
        let seeded = recovered.remove(&rule.name).unwrap_or(RuleState::Inactive);
        handles.push(tokio::spawn(async move {
            run_rule(rule, seeded, backend, client, resolver, store).await;
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

/// Per-rule loop: tick → fetch → transition → inhibition → emit.
async fn run_rule(
    rule: Rule,
    seeded: RuleState,
    backend: Arc<String>,
    client: reqwest::Client,
    resolver: Arc<Mutex<InhibitionResolver>>,
    store: Arc<dyn RuleStateStore>,
) {
    // Seed from the recovered state so a firing alert survives a
    // restart and does not re-page on-call (US-02).
    let mut state = seeded;
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
        let (next, emission) = evaluate_once(&rule, state, outcome, now);
        if state != next {
            debug!(rule = %rule.name, from = ?state, to = ?next, "state transition");
            // Persist the new state (latest-wins, DD4). A transient WAL
            // write failure degrades to in-memory rather than silencing
            // the alert: warn and continue, do not kill the loop.
            if let Err(err) = store.put(&rule.name, next) {
                warn!(rule = %rule.name, error = %err, "failed to persist rule state; continuing in-memory");
            }
        }
        state = next;

        // Hand the emission to the shared inhibition resolver. It may
        // suppress this rule's Firing (storm collapse) or release
        // pending Firings from previously-inhibited rules whose
        // inhibitor just resolved. The returned list is what actually
        // reaches the sinks.
        let to_emit = {
            let mut guard = resolver.lock().await;
            guard.observe(&rule.name, emission)
        };

        for ev in to_emit {
            let (incident, kind) = match ev {
                Emission::Firing(i) => (i, "firing"),
                Emission::Resolved(i) => (i, "resolved"),
            };
            info!(rule = %rule.name, transition = kind, "emitting incident");
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
