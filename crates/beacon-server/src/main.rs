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
    load_rules, Emission, FileBackedRuleStateStore, InhibitionResolver, LoadOutcome, Rule,
    RuleState, RuleStateStore, Sink,
};
use beacon_server::{build_http_client, build_sinks, evaluate_once, fetch_query};
use clap::Parser;
use tokio::signal;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

#[derive(Debug, Parser)]
#[command(
    name = "beacon-server",
    about = "Kaleidoscope Beacon — alerting engine over any OTel-compatible PromQL backend",
    version
)]
struct Args {
    /// Directory tree of rule TOML files. Loaded once at startup and
    /// re-read on SIGHUP (atomic all-or-nothing reload, ADR-0063).
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
    // Structured diagnostics, including the SIGHUP reload events
    // (`beacon.reload.succeeded` / `beacon.reload.refused`), go to STDERR:
    // logs are diagnostics, not the program's data output, and operators
    // (and the reload acceptance suite) read them on stderr (ADR-0063
    // "Observables").
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
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
    // synchronous (no .await inside) and the lock is held briefly. On a
    // SIGHUP reload the whole `Arc<Mutex<>>` is REPLACED wholesale (see
    // the reload sequence below): surviving new tasks get the new Arc;
    // old tasks keep the old Arc until aborted, so no task ever observes
    // a partially-rebuilt resolver (ADR-0063 sub-decision 3).
    let mut resolver = Arc::new(Mutex::new(InhibitionResolver::new(&outcome.rules)));
    // The orchestrator (this task) is the SOLE writer of `handles`,
    // `resolver`, and the live rule-name set. The per-rule `run_rule`
    // loops are pure evaluators (ADR-0037) and are never touched by a
    // reload; they only ever READ the resolver under its Mutex and WRITE
    // the durable store.
    let mut live_names: std::collections::HashSet<String> =
        outcome.rules.iter().map(|r| r.name.clone()).collect();
    let mut handles = spawn_generation(
        outcome.rules,
        &mut recovered,
        &backend,
        &client,
        &resolver,
        &store,
    );

    // Install SIGHUP BEFORE entering the loop so an early reload signal
    // during startup does not hit the OS default disposition (terminate);
    // mirrors the SIGTERM install order (ADR-0063 sub-decision 1).
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    #[cfg(unix)]
    let mut term = match signal::unix::signal(signal::unix::SignalKind::terminate()) {
        Ok(s) => s,
        Err(err) => {
            error!(error = %err, "failed to install SIGTERM handler");
            return ExitCode::from(4);
        }
    };
    #[cfg(unix)]
    let mut hangup = match signal::unix::signal(signal::unix::SignalKind::hangup()) {
        Ok(s) => s,
        Err(err) => {
            error!(error = %err, "failed to install SIGHUP handler");
            return ExitCode::from(4);
        }
    };

    // The select loop owns shutdown AND reload. SIGINT/SIGTERM break the
    // loop and shut down cleanly; SIGHUP runs the reload sequence inline
    // and continues. Because the orchestrator is the only writer of
    // `handles`/`resolver`, there is no data race with an in-flight tick.
    #[cfg(unix)]
    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("SIGINT received; shutting down");
                break;
            }
            _ = term.recv() => {
                info!("SIGTERM received; shutting down");
                break;
            }
            _ = hangup.recv() => {
                reload(
                    &args.rules,
                    &mut handles,
                    &mut resolver,
                    &mut live_names,
                    &backend,
                    &client,
                    &store,
                )
                .await;
            }
        }
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

/// Spawn one per-rule evaluation task per rule, seeding each with its
/// recovered/carried-over [`RuleState`] by name. A rule absent from
/// `seed` starts `Inactive`, exactly as a fresh rule would. Returns the
/// new generation's `JoinHandle`s. Shared by startup and reload so both
/// seed identically (ADR-0063 sub-decision 2).
fn spawn_generation(
    rules: Vec<Rule>,
    seed: &mut std::collections::HashMap<String, RuleState>,
    backend: &Arc<String>,
    client: &reqwest::Client,
    resolver: &Arc<Mutex<InhibitionResolver>>,
    store: &Arc<dyn RuleStateStore>,
) -> Vec<JoinHandle<()>> {
    let mut handles = Vec::with_capacity(rules.len());
    for rule in rules {
        let backend = Arc::clone(backend);
        let client = client.clone();
        let resolver = Arc::clone(resolver);
        let store = Arc::clone(store);
        let seeded = seed.remove(&rule.name).unwrap_or(RuleState::Inactive);
        handles.push(tokio::spawn(async move {
            run_rule(rule, seeded, backend, client, resolver, store).await;
        }));
    }
    handles
}

/// The SIGHUP reload sequence (ADR-0063).
///
/// ORDERING INVARIANT (sub-decision 4, the all-or-nothing guarantee):
/// the NEW generation is built COMPLETELY and VALIDATED *before* any old
/// state is touched. Either the new catalogue becomes fully live (with
/// carried-over alert state) or the previous catalogue is fully retained.
/// There is NO partial-apply path: a refusal touches neither `handles`
/// nor `resolver`. The atomic swap is build-new -> make-new-live ->
/// abort-old, so a surviving rule never has zero evaluators (no missed
/// transition) and an overlapping tick is idempotent (no double-fire).
#[cfg(unix)]
#[allow(clippy::too_many_arguments)]
async fn reload(
    rules_dir: &std::path::Path,
    handles: &mut Vec<JoinHandle<()>>,
    resolver: &mut Arc<Mutex<InhibitionResolver>>,
    live_names: &mut std::collections::HashSet<String>,
    backend: &Arc<String>,
    client: &reqwest::Client,
    store: &Arc<dyn RuleStateStore>,
) {
    // (a) Re-read the rules dir (loader reused verbatim).
    let outcome: LoadOutcome = match load_rules(rules_dir) {
        Ok(o) => o,
        Err(err) => {
            // Directory unreadable: refuse, keep the previous catalogue.
            warn!(
                event = "beacon.reload.refused",
                file = %rules_dir.display(),
                error = %err,
                previous_catalogue_retained = true,
                "beacon.reload.refused"
            );
            return;
        }
    };

    // (b) VALIDATE before touching anything old. A refusal touches
    // neither `handles` nor `resolver`, so the previous catalogue stays
    // fully live (all-or-nothing). The catalogue is REFUSED when:
    //
    //   1. zero rules loaded (`!has_any_rules()`) — an emptied or
    //      wholly-broken rules directory; the daemon must not go dark
    //      (US-02 Domain Example 2); identical to the startup
    //      `has_any_rules()` refusal; OR
    //   2. a file the operator clearly intended as a rule failed to parse
    //      AND the edit added NO new valid rule (`has_diagnostics()` with
    //      no name added vs the live set) — the operator's intended edit
    //      was the broken file and nothing valid came of it, so applying
    //      it would silently swallow their change (US-02 Domain Example 1,
    //      the load-bearing negative).
    //
    // A partly-broken catalogue that STILL adds a valid rule SUCCEEDS:
    // report-and-skip applies, the valid rules go live, and each per-file
    // diagnostic is surfaced (US-02 Domain Example 3, startup-consistent
    // B01). This is the boundary that separates "your edit was all broken,
    // I kept you safe" from "I applied your good rules and told you which
    // file I skipped".
    let new_names: std::collections::HashSet<String> =
        outcome.rules.iter().map(|r| r.name.clone()).collect();
    let added_count = new_names.difference(live_names).count();
    let no_rules = !outcome.has_any_rules();
    let broken_edit_added_nothing = outcome.has_diagnostics() && added_count == 0;
    if no_rules || broken_edit_added_nothing {
        let file = outcome
            .diagnostics
            .first()
            .map(|d| d.file.display().to_string())
            .unwrap_or_else(|| rules_dir.display().to_string());
        let error = outcome
            .diagnostics
            .first()
            .map(|d| d.display())
            .unwrap_or_else(|| "rules directory yielded no rules".to_string());
        warn!(
            event = "beacon.reload.refused",
            file = %file,
            error = %error,
            previous_catalogue_retained = true,
            "beacon.reload.refused"
        );
        return;
    }

    // A partly-broken catalogue with at least one valid rule SUCCEEDS:
    // surface each per-file diagnostic via the existing report-and-skip
    // path, exactly as startup does, then apply the valid rules.
    for diag in &outcome.diagnostics {
        warn!(diagnostic = %diag.display(), "rule load diagnostic");
    }

    // (c) Recover the current durable state to seed carried-over rules
    // by name. A removed rule's state is dropped-and-logged exactly as
    // startup recovery drops state for rules no longer in config.
    let mut recovered = match store.load_all() {
        Ok(map) => map,
        Err(err) => {
            warn!(
                event = "beacon.reload.refused",
                file = %rules_dir.display(),
                error = %err,
                previous_catalogue_retained = true,
                "beacon.reload.refused: failed to recover durable state"
            );
            return;
        }
    };
    // A rule present only in the OLD catalogue (removed) gets no task in
    // the new generation: `spawn_generation` only spawns the new rules,
    // so a removed rule simply stops being evaluated, and its durable
    // state, left behind in `recovered`, is never seeded into a task (it
    // is dropped, exactly as startup recovery drops state for rules no
    // longer in config). The count of such removals is surfaced as the
    // `removed` field on the `beacon.reload.succeeded` event below.

    // Snapshot the OLD resolver's live carry-over (firing flags + pending
    // suppressed incidents) under one brief lock, then drop the lock
    // BEFORE building the new generation. Build the NEW resolver from the
    // new rule set with the both-ends-survival carryover applied.
    let carried = {
        let guard = resolver.lock().await;
        guard.carryover()
    };
    let new_resolver = Arc::new(Mutex::new(InhibitionResolver::rebuild_from(
        &outcome.rules,
        carried,
    )));

    let added = added_count;
    let removed = live_names.difference(&new_names).count();
    let rules_loaded = outcome.rules.len();
    let diagnostics = outcome.diagnostics.len();

    // Spawn the NEW task set, each seeded by name from the durable store.
    let new_handles = spawn_generation(
        outcome.rules,
        &mut recovered,
        backend,
        client,
        &new_resolver,
        store,
    );

    // ATOMIC SWAP: make the new generation live FIRST, then abort the old
    // tasks. New-live-before-old-aborted: a surviving rule has continuous
    // evaluation coverage across the swap (never zero evaluators), and the
    // old/new task for the same rule never double-fire because both seed
    // from the same durable `since` and an overlapping tick is idempotent
    // (ADR-0063 sub-decision 4 + review clarification 1).
    let old_handles = std::mem::replace(handles, new_handles);
    *resolver = new_resolver;
    *live_names = new_names;
    for handle in old_handles {
        handle.abort();
    }

    info!(
        event = "beacon.reload.succeeded",
        rules_loaded, added, removed, diagnostics, "beacon.reload.succeeded"
    );
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
