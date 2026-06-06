// Kaleidoscope Beacon — beacon-server SLO operator-path + SIGHUP reload test
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

//! Acceptance test (DISTILL, feature `beacon-slo-operator-path-v0`).
//!
//! The OPERATOR-PATH + SIGHUP-RELOAD half of the SLO feature. The driving
//! ports (ADR-0067 "For Acceptance Designer") are: (a) the `--rules DIR`
//! TOML on disk (declare `[[slo]]`); (b) the real `beacon-server` binary
//! started against that dir; (c) the POSIX signal `kill -HUP <pid>` after
//! an edit. There is NO new CLI and NO new HTTP surface. Every assertion
//! is black-box: the synthesised rules' firing behaviour at the webhook
//! catcher, and the structured `beacon.reload.succeeded` /
//! `beacon.reload.refused` events on the child's stderr. Assertions pin
//! the REAL synthesised names `{service}_slo_{page|ticket}_{long}_{short}`
//! (e.g. `checkout_slo_page_1h_5m`) — NOT the DISCUSS illustrative names.
//!
//! # Strategy C (real I/O) — the harness is the beacon-sighup-reload-v0 one
//!
//! This module REUSES the `sighup_reload.rs` harness shape verbatim
//! (ADR-0067 / DEVOPS environments.yaml `slo_reload_test_environment`):
//! the real `beacon-server` binary as a real child process
//! (`CARGO_BIN_EXE_beacon-server`), a real POSIX `kill -HUP` by pid via
//! the SAFE `rustix::process::kill_process`, a real writable tmp
//! `--rules` dir, a real `wiremock` mock PromQL backend, and a real
//! `wiremock` webhook catcher. No InMemory double can catch the wiring
//! this proves: the loader's SLO synthesis reaching the live catalogue,
//! the expansion-aware reload count, the all-or-nothing refuse on a
//! malformed SLO edit, and the name-keyed state carryover. Tagged
//! `@real-io @driving_port`.
//!
//! # Determinism discipline (no wall-clock assertion)
//!
//! These tests NEVER assert a p95 or a wall-clock latency. The
//! happen-before anchor is the structured reload EVENT on stderr; the
//! awaited observable (a synthesised rule's firing, the still-alive
//! process) is reached by POLLING UNDER A GENEROUS BOUND and returning on
//! first appearance (presence-under-a-bound). The seeded short rule
//! `interval` is for test SPEED only and is never the thing asserted.
//! `synthesise_slo` has no clock and no RNG, so the firing pattern is a
//! function of the backend stub alone — the project's overnight
//! p95-flake class does not apply.
//!
//! # Portability — SIGHUP is POSIX-only
//!
//! The whole module is `#![cfg(unix)]`-gated. The signal is sent by pid
//! via `rustix::process::kill_process` (the crate forbids `unsafe_code`).
//!
//! # RED-not-BROKEN at the DISTILL commit (Mandate 7)
//!
//! The `[[slo]]` loader path does NOT exist yet (DELIVER adds it). Today a
//! file containing `[[slo]]` POISONS its file (`FileShape` is
//! `deny_unknown_fields` with only `rules`), so `beacon-server` skips it:
//! no synthesised rule ever reaches the live catalogue, so no
//! `checkout_slo_*` rule ever fires and a valid-SLO `beacon.reload.succeeded`
//! with `added=4` never appears. These tests therefore FAIL on BEHAVIOUR
//! (the awaited firing / event never arrives within the generous bound),
//! not on a missing symbol: they compile against the EXISTING surface only
//! (the binary via `CARGO_BIN_EXE`, `wiremock`, `rustix`). Each is
//! `#[ignore = "RED until DELIVER: beacon-slo-operator-path-v0"]` so
//! `cargo test --workspace` stays GREEN at this commit; DELIVER removes the
//! `#[ignore]`s once the loader wires the SLO path. Run with `--ignored`
//! to see them FAIL on the assertion. CLEAN-UP: every test reaps its child
//! with `shutdown` (kill + wait) so no beacon-server process leaks.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustix::process::{kill_process, Pid, Signal};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const GENEROUS_BOUND: Duration = Duration::from_secs(20);
const POLL_STEP: Duration = Duration::from_millis(50);

// The four REAL synthesised names for a `checkout` SLO (the `_slo_`
// infix is the shipped authority, ADR-0067 F2 / slo.rs:124-127).
const CHECKOUT_PAGE_1H_5M: &str = "checkout_slo_page_1h_5m";

fn send_sighup(child: &Child) {
    // Safe, no `unsafe` block: `rustix` does the FFI internally.
    let pid = Pid::from_child(child);
    kill_process(pid, Signal::HUP).expect("kill -HUP <child pid> failed");
}

/// A unique, writable tmp `--rules` directory (the beacon-sighup-reload
/// style; beacon-server writes its durable store under the dir, so it
/// must be writable + test-owned).
struct TmpRules {
    path: PathBuf,
}

impl TmpRules {
    fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let pid = std::process::id();
        let mut path = std::env::temp_dir();
        path.push(format!("beacon-slo-reload-{label}-{pid}-{nanos}"));
        std::fs::create_dir_all(&path).expect("mkdir tmp rules dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TmpRules {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Write a `[[slo]]` rule file. `interval` is not an SLO key (the engine
/// fixes 30s); for test SPEED we lean on the active backend so the
/// synthesised rule reaches Firing within a couple of evaluation ticks.
/// The intended ADR-0067 F1 schema: `service`, `good_events_query`,
/// `total_events_query`, `target_availability`, `error_budget_period`,
/// `[[slo.sinks]]`.
fn write_slo_file(
    rules_dir: &Path,
    file: &str,
    service: &str,
    target_availability: &str,
    budget: &str,
    sink_url: &str,
) {
    let toml = format!(
        r#"
[[slo]]
service = "{service}"
good_events_query = "rate(http_errors_total{{job=\"{service}\"}}[5m]) > 0.5"
total_events_query = "rate(http_requests_total{{job=\"{service}\"}}[5m])"
target_availability = {target_availability}
error_budget_period = "{budget}"

[[slo.sinks]]
kind = "webhook"
url = "{sink_url}"
"#
    );
    std::fs::write(rules_dir.join(file), toml).expect("write SLO file");
}

/// A plain hand-authored rule file (the existing reload harness shape).
fn write_rule_file(rules_dir: &Path, file: &str, name: &str, query: &str, sink_url: &str) {
    let toml = format!(
        r#"
[[rules]]
name = "{name}"
query = "{query}"
for_duration = "100ms"
interval = "100ms"
severity = "critical"

[[rules.sinks]]
kind = "webhook"
url = "{sink_url}"
"#
    );
    std::fs::write(rules_dir.join(file), toml).expect("write rule file");
}

fn spawn_beacon(rules_dir: &Path, backend: &str) -> Child {
    Command::new(env!("CARGO_BIN_EXE_beacon-server"))
        .arg("--rules")
        .arg(rules_dir)
        .arg("--backend")
        .arg(backend)
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn beacon-server")
}

/// A backend that always reports the queried metric Active, so any
/// well-formed rule firing against it reaches Firing within a couple of
/// ticks. (The synthesised SLO PromQL is a `>` comparison; the stub's
/// non-empty vector makes it evaluate truthy in the harness, exactly as
/// the existing reload harness drives a `> 0.05` rule.)
async fn mock_active_backend() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/query"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "success",
            "data": {
                "resultType": "vector",
                "result": [{
                    "metric": {"__name__": "up"},
                    "value": [1640000000, "1"]
                }]
            }
        })))
        .mount(&server)
        .await;
    server
}

async fn webhook_catcher() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    server
}

async fn received_incidents(sink: &MockServer) -> Vec<Value> {
    sink.received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter_map(|r: &Request| serde_json::from_slice(&r.body).ok())
        .collect()
}

async fn firing_count(sink: &MockServer, rule_name: &str) -> usize {
    received_incidents(sink)
        .await
        .iter()
        .filter(|inc| {
            inc.get("name").and_then(Value::as_str) == Some(rule_name)
                && inc.get("resolved_at").map(Value::is_null).unwrap_or(true)
        })
        .count()
}

async fn wait_for_firing(sink: &MockServer, rule_name: &str) -> bool {
    let deadline = Instant::now() + GENEROUS_BOUND;
    while Instant::now() < deadline {
        if firing_count(sink, rule_name).await >= 1 {
            return true;
        }
        tokio::time::sleep(POLL_STEP).await;
    }
    false
}

fn capture_stderr(child: &mut Child) -> std::sync::Arc<std::sync::Mutex<String>> {
    use std::io::Read;
    let buf = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let mut stderr = child.stderr.take().expect("child stderr piped");
    let sink = std::sync::Arc::clone(&buf);
    std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        loop {
            match stderr.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if let Ok(mut s) = sink.lock() {
                        s.push_str(&String::from_utf8_lossy(&chunk[..n]));
                    }
                }
            }
        }
    });
    buf
}

async fn wait_for_event(buf: &std::sync::Arc<std::sync::Mutex<String>>, event: &str) -> bool {
    let deadline = Instant::now() + GENEROUS_BOUND;
    while Instant::now() < deadline {
        if buf.lock().map(|s| s.contains(event)).unwrap_or(false) {
            return true;
        }
        tokio::time::sleep(POLL_STEP).await;
    }
    false
}

fn process_still_running(child: &mut Child) -> bool {
    matches!(child.try_wait(), Ok(None))
}

/// Reap the child at test end (clean-target shutdown — no leaked
/// beacon-server process).
fn shutdown(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

// ====================================================================
// US-01 (operator-path walking slice): the binary loads the SLO + a
// fast burn pages.
// ====================================================================

/// WALKING SLICE (US-01, @driving_adapter): the operator declares one
/// `[[slo]]` in a real `--rules` dir, starts the REAL beacon-server
/// against it, and a fast burn PAGES — the synthesised
/// `checkout_slo_page_1h_5m` rule fires a critical incident to the SLO's
/// sink. Demo-able to a stakeholder: "declare one SLO, the page rule goes
/// live." This is the binary/subprocess proof the loader test cannot give
/// (the engine reaching the live catalogue + the evaluator + the sink
/// wiring). RED today: `[[slo]]` poisons its file, so no synthesised rule
/// ever fires.
#[tokio::test]
#[ignore = "RED until DELIVER: beacon-slo-operator-path-v0"]
async fn declared_slo_loads_and_a_fast_burn_pages() {
    // @walking_skeleton @driving_port @real-io  (US-01)
    let backend = mock_active_backend().await;
    let sink = webhook_catcher().await;
    let rules = TmpRules::new("ws");
    write_slo_file(
        rules.path(),
        "checkout.toml",
        "checkout",
        "0.999",
        "30d",
        &sink.uri(),
    );
    let mut child = spawn_beacon(rules.path(), &backend.uri());

    // The synthesised page rule reaches Firing against the active
    // backend: the operator's declared SLO is live and paging.
    let paged = wait_for_firing(&sink, CHECKOUT_PAGE_1H_5M).await;
    assert!(
        paged,
        "a declared [[slo]] must synthesise checkout_slo_page_1h_5m and page on a fast burn"
    );
    assert!(
        process_still_running(&mut child),
        "the daemon must stay up serving the synthesised SLO rules"
    );
    shutdown(child);
}

/// US-01 (startup count is expansion-aware): starting the binary against
/// one `[[slo]]` reports a `rules_loaded` count reflecting the four-rule
/// expansion. RED today (the file is poisoned; rules_loaded does not
/// reflect four synthesised rules).
#[tokio::test]
#[ignore = "RED until DELIVER: beacon-slo-operator-path-v0"]
async fn startup_rules_loaded_count_reflects_four_rule_expansion() {
    // @driving_port @real-io  (US-01)
    let backend = mock_active_backend().await;
    let sink = webhook_catcher().await;
    let rules = TmpRules::new("startup-count");
    write_slo_file(
        rules.path(),
        "checkout.toml",
        "checkout",
        "0.999",
        "30d",
        &sink.uri(),
    );
    let mut child = spawn_beacon(rules.path(), &backend.uri());
    let stderr = capture_stderr(&mut child);

    assert!(
        wait_for_firing(&sink, CHECKOUT_PAGE_1H_5M).await,
        "the synthesised SLO must be live before asserting the startup count"
    );
    let log = stderr.lock().unwrap().clone();
    assert!(
        log.contains("rules_loaded=4")
            || log.contains("rules_loaded\":4")
            || log.contains("rules_loaded = 4"),
        "startup must report rules_loaded reflecting the four-rule expansion; got: {log}"
    );
    shutdown(child);
}

// ====================================================================
// US-05 — an SLO edit hot-reloads under SIGHUP.
// ====================================================================

/// US-05 happy path (a valid SLO edit hot-reloads atomically): the
/// operator edits the `checkout` SLO target on disk, runs `kill -HUP`,
/// and `beacon.reload.succeeded` is emitted in the SAME process, with the
/// synthesised rules re-applied. RED today (no SLO path → no SLO reload).
#[tokio::test]
#[ignore = "RED until DELIVER: beacon-slo-operator-path-v0"]
async fn valid_slo_edit_hot_reloads_under_sighup() {
    // @driving_port @real-io  (US-05 happy path)
    let backend = mock_active_backend().await;
    let sink = webhook_catcher().await;
    let rules = TmpRules::new("valid-edit");
    write_slo_file(
        rules.path(),
        "checkout.toml",
        "checkout",
        "0.999",
        "30d",
        &sink.uri(),
    );
    let mut child = spawn_beacon(rules.path(), &backend.uri());
    let stderr = capture_stderr(&mut child);
    assert!(
        wait_for_firing(&sink, CHECKOUT_PAGE_1H_5M).await,
        "the initial SLO must be live before the edit"
    );
    let pid_before = child.id();

    // Tighten the target and signal. No restart.
    write_slo_file(
        rules.path(),
        "checkout.toml",
        "checkout",
        "0.9995",
        "30d",
        &sink.uri(),
    );
    send_sighup(&child);

    assert!(
        wait_for_event(&stderr, "beacon.reload.succeeded").await,
        "a valid SLO edit must emit beacon.reload.succeeded"
    );
    assert_eq!(
        child.id(),
        pid_before,
        "the SLO reload must not restart the process"
    );
    assert!(
        process_still_running(&mut child),
        "the process must stay alive across a successful SLO reload"
    );
    shutdown(child);
}

/// US-05 (the expansion-aware added count): adding a SECOND, unrelated
/// `search` SLO and reloading reports `added=4` (the four synthesised
/// `search` rules), the honest count of new evaluators (ADR-0067 F4). RED
/// today.
#[tokio::test]
#[ignore = "RED until DELIVER: beacon-slo-operator-path-v0"]
async fn adding_an_slo_reports_added_four_on_reload() {
    // @driving_port @real-io  (US-05)
    let backend = mock_active_backend().await;
    let sink = webhook_catcher().await;
    let rules = TmpRules::new("added-four");
    write_slo_file(
        rules.path(),
        "checkout.toml",
        "checkout",
        "0.999",
        "30d",
        &sink.uri(),
    );
    let mut child = spawn_beacon(rules.path(), &backend.uri());
    let stderr = capture_stderr(&mut child);
    assert!(wait_for_firing(&sink, CHECKOUT_PAGE_1H_5M).await);

    // Add an unrelated second SLO and signal.
    write_slo_file(
        rules.path(),
        "search.toml",
        "search",
        "0.99",
        "30d",
        &sink.uri(),
    );
    send_sighup(&child);

    assert!(wait_for_event(&stderr, "beacon.reload.succeeded").await);
    let log = stderr.lock().unwrap().clone();
    assert!(
        log.contains("added=4") || log.contains("added\":4") || log.contains("added = 4"),
        "adding one SLO must report the expansion-aware added=4; got: {log}"
    );
    shutdown(child);
}

/// US-05 error path (a malformed SLO edit is refused, previous catalogue
/// kept): the operator fat-fingers `target = 1.0` and reloads;
/// `beacon.reload.refused` is emitted naming the file +
/// previous-catalogue-retained, the daemon does NOT exit, and no
/// degenerate always-fire rule reaches evaluation. The primary reload
/// safety property. RED today (no SLO path → the malformed edit is not
/// even recognised as an SLO edit).
#[tokio::test]
#[ignore = "RED until DELIVER: beacon-slo-operator-path-v0"]
async fn malformed_slo_edit_is_refused_and_previous_catalogue_is_kept() {
    // @driving_port @real-io  (US-05 error path)
    let backend = mock_active_backend().await;
    let sink = webhook_catcher().await;
    let rules = TmpRules::new("malformed-edit");
    write_slo_file(
        rules.path(),
        "checkout.toml",
        "checkout",
        "0.999",
        "30d",
        &sink.uri(),
    );
    let mut child = spawn_beacon(rules.path(), &backend.uri());
    let stderr = capture_stderr(&mut child);
    assert!(
        wait_for_firing(&sink, CHECKOUT_PAGE_1H_5M).await,
        "the valid SLO must be live before the malformed edit"
    );

    // Fat-finger target = 1.0 (the always-fire gun) and signal.
    write_slo_file(
        rules.path(),
        "checkout.toml",
        "checkout",
        "1.0",
        "30d",
        &sink.uri(),
    );
    send_sighup(&child);

    assert!(
        wait_for_event(&stderr, "beacon.reload.refused").await,
        "a malformed SLO edit must emit beacon.reload.refused"
    );
    assert!(
        process_still_running(&mut child),
        "a malformed SLO reload must not crash the daemon"
    );
    let log = stderr.lock().unwrap().clone();
    assert!(
        log.contains("checkout.toml"),
        "the refusal must name the offending file; got: {log}"
    );
    assert!(
        log.contains("previous_catalogue_retained") || log.contains("previous catalogue retained"),
        "the refusal must state the previous catalogue was retained; got: {log}"
    );
    // The previous, valid catalogue is retained: the page rule keeps
    // being evaluated (no degenerate rule replaced it).
    assert!(
        firing_count(&sink, CHECKOUT_PAGE_1H_5M).await >= 1,
        "the previous valid SLO must keep being evaluated after the refused edit"
    );
    shutdown(child);
}

/// US-05 carryover (a firing synthesised rule survives an unrelated SLO
/// add without re-paging): `checkout_slo_page_1h_5m` is Firing; the
/// operator adds an unrelated `search` SLO and reloads; the checkout page
/// rule keeps its state by stable synthesised name (no SECOND Firing
/// incident) and the four `search` rules are added (ADR-0067 F4 / ADR-0063
/// sub-decision 2). RED today.
#[tokio::test]
#[ignore = "RED until DELIVER: beacon-slo-operator-path-v0"]
async fn firing_synthesised_rule_survives_unrelated_slo_add_without_repaging() {
    // @driving_port @real-io @property  (US-05 carryover)
    let backend = mock_active_backend().await;
    let sink = webhook_catcher().await;
    let rules = TmpRules::new("carryover");
    write_slo_file(
        rules.path(),
        "checkout.toml",
        "checkout",
        "0.999",
        "30d",
        &sink.uri(),
    );
    let mut child = spawn_beacon(rules.path(), &backend.uri());
    let stderr = capture_stderr(&mut child);
    assert!(wait_for_firing(&sink, CHECKOUT_PAGE_1H_5M).await);
    let first_since = received_incidents(&sink)
        .await
        .into_iter()
        .find(|inc| inc.get("name").and_then(Value::as_str) == Some(CHECKOUT_PAGE_1H_5M))
        .and_then(|inc| inc.get("started_at").cloned())
        .expect("first firing carries started_at");

    // Add an unrelated SLO; leave checkout untouched. Signal.
    write_slo_file(
        rules.path(),
        "search.toml",
        "search",
        "0.99",
        "30d",
        &sink.uri(),
    );
    send_sighup(&child);
    assert!(wait_for_event(&stderr, "beacon.reload.succeeded").await);
    // Let the swapped generation run several ticks.
    assert!(wait_for_firing(&sink, "search_slo_page_1h_5m").await);
    tokio::time::sleep(Duration::from_millis(500)).await;

    assert_eq!(
        firing_count(&sink, CHECKOUT_PAGE_1H_5M).await,
        1,
        "a surviving synthesised rule must not re-page across an unrelated SLO add"
    );
    let still_since = received_incidents(&sink)
        .await
        .into_iter()
        .find(|inc| inc.get("name").and_then(Value::as_str) == Some(CHECKOUT_PAGE_1H_5M))
        .and_then(|inc| inc.get("started_at").cloned());
    assert_eq!(
        still_since,
        Some(first_since),
        "the surviving synthesised rule keeps its original since across the swap"
    );
    shutdown(child);
}

// ====================================================================
// US-04 (binary coexistence negative control — PASSES TODAY, guardrail).
// ====================================================================

/// US-04 negative control (PASSING TODAY): a rules-only `--rules` dir
/// (no `[[slo]]`) drives the real beacon-server exactly as before this
/// feature — the hand-authored rule fires. This is the byte-identical
/// rules-only-path guardrail at the BINARY level (KPI 3). UN-ignored: it
/// must STAY GREEN through DELIVER, proving the SLO wiring did not regress
/// the existing operator path.
#[tokio::test]
async fn rules_only_directory_drives_the_binary_as_before() {
    // @driving_port @real-io  (US-04 negative control — PASSES TODAY)
    let backend = mock_active_backend().await;
    let sink = webhook_catcher().await;
    let rules = TmpRules::new("rules-only-binary");
    write_rule_file(
        rules.path(),
        "service-down.toml",
        "service-down",
        "up == 0",
        &sink.uri(),
    );
    let mut child = spawn_beacon(rules.path(), &backend.uri());

    assert!(
        wait_for_firing(&sink, "service-down").await,
        "a hand-authored rule must keep firing exactly as before the SLO feature"
    );
    assert!(process_still_running(&mut child));
    shutdown(child);
}
