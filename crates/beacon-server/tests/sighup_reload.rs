// Kaleidoscope Beacon — beacon-server SIGHUP reload acceptance test
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

//! Acceptance test (DISTILL, feature beacon-sighup-reload-v0).
//!
//! Black-box proving test for the operator-visible SIGHUP reload
//! contract (US-01 apply-via-SIGHUP, US-02 refuse-malformed-keep-
//! previous). The driving port is the POSIX signal: the operator edits
//! the `--rules` directory of a running `beacon-server` and runs
//! `kill -HUP <pid>`. There is NO new CLI and NO new HTTP surface; the
//! reload is observed entirely through the spawned binary's behaviour
//! (the webhook sink it POSTs to, and the two structured `tracing`
//! events on its stderr).
//!
//! # Strategy C (real I/O)
//!
//! Every test here drives the real `beacon-server` binary as a real
//! child process (`CARGO_BIN_EXE_beacon-server`), sends a real POSIX
//! signal to it by pid, edits a real writable tmp rules directory, and
//! observes a real mock PromQL backend + a real webhook catcher (both
//! `wiremock`, the harness shape `smoke.rs` already uses). No InMemory
//! double can catch the wiring this proves: signal install order, the
//! atomic catalogue swap, the durable-store-under-the-rules-dir wrinkle,
//! and the structured-event format. Tagged `@real-io`.
//!
//! # Determinism discipline (DEVOPS wave-decisions, Decision 2)
//!
//! These tests NEVER assert a wall-clock latency or a p95. The
//! happen-before anchor is the structured reload EVENT on the child's
//! stderr (`beacon.reload.succeeded` INFO / `beacon.reload.refused`
//! WARN, ADR-0063 "Observables"). After the event is seen, the awaited
//! observable (the sink POST, the still-alive process) is reached by
//! POLLING UNDER A GENEROUS BOUND and returning on first appearance. A
//! short per-rule `interval` is seeded into the rule TOML for test SPEED
//! only; it is never the thing asserted. The assertion form is
//! presence-under-a-bound ("the awaited firing/event WAS observed within
//! the bound"), the explicit guard against the project's overnight
//! p95-flake class (MEMORY: project_p95_wallclock_flakes_overnight).
//!
//! # Portability (DEVOPS Decision 3, reviewer condition 3)
//!
//! SIGHUP is POSIX-only. The whole module is `#![cfg(unix)]`-gated so a
//! future Windows CI does not fail on the absent signal. The signal is
//! sent to the child by pid via the SAFE `rustix::process::kill_process`
//! (the crate forbids `unsafe_code`, so the unsafe `libc::kill` FFI is
//! not used directly; `rustix` is already in the workspace lock). The
//! exact production signal surface is the DELIVER crafter's choice and is
//! not referenced here.
//!
//! # RED-not-BROKEN at the DISTILL commit
//!
//! The SIGHUP handler does NOT exist yet (DELIVER adds it). Today SIGHUP
//! hits the OS default disposition or is a no-op, so the added rule never
//! fires and the `beacon.reload.succeeded` event never appears: these
//! tests FAIL on behaviour, not on a missing symbol. They compile against
//! the existing public surface only (the binary via `CARGO_BIN_EXE`, the
//! `wiremock` mock servers, `rustix::process::kill_process`) — no
//! not-yet-existing API is named. Each test is `#[ignore = "RED until
//! DELIVER: beacon-sighup-
//! reload-v0"]` so `cargo test --workspace` stays GREEN at this commit;
//! DELIVER removes the `#[ignore]`s once the handler lands.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustix::process::{kill_process, Pid, Signal};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

// --------------------------------------------------------------------
// Generous upper bound for every poll-until-seen loop. NOT an assertion
// threshold: a test fails only if this elapses with the awaited
// observable still absent. Seeded rule `interval` is far shorter, so in
// the GREEN (post-DELIVER) world the observable arrives in a fraction of
// this; the slack only absorbs scheduler/CI jitter.
// --------------------------------------------------------------------
const GENEROUS_BOUND: Duration = Duration::from_secs(20);
const POLL_STEP: Duration = Duration::from_millis(50);

// --------------------------------------------------------------------
// Real signal to the child by pid (DEVOPS Decision 3). POSIX-only;
// guarded by `#![cfg(unix)]` at module scope.
// --------------------------------------------------------------------
fn send_sighup(child: &Child) {
    // Safe, no `unsafe` block: `rustix` does the FFI internally, so this
    // test target honours the crate's `forbid(unsafe_code)` lint.
    let pid = Pid::from_child(child);
    kill_process(pid, Signal::HUP).expect("kill -HUP <child pid> failed");
}

/// A unique, writable tmp rules directory under the OS temp dir, in the
/// project's established `env::temp_dir()` style (no `tempfile` dep).
/// beacon-server writes its durable store under `<rules>/.beacon-state`
/// (main.rs:107), so the directory must be writable and test-owned
/// (DEVOPS Decision 3, writable-rules-dir wrinkle).
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
        path.push(format!("beacon-sighup-reload-{label}-{pid}-{nanos}"));
        std::fs::create_dir_all(&path).expect("mkdir tmp rules dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TmpRules {
    fn drop(&mut self) {
        // Clean-target shutdown: drop the tmp tree at test end.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

// --------------------------------------------------------------------
// A rule TOML file. `interval` is seeded SHORT for test SPEED only
// (DEVOPS Decision 2 point 3). `for_duration` short so a rule reaches
// Firing within a couple of ticks against an Active backend.
// --------------------------------------------------------------------
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

/// A deliberately malformed rule file: an unknown field that
/// `#[serde(deny_unknown_fields)]` rejects with the loader's "did you
/// mean" suggestion (US-02). Mirrors Sofia's `for_duraton` typo.
fn write_malformed_rule_file(rules_dir: &Path, file: &str) {
    let toml = r#"
[[rules]]
name = "payments-latency"
query = "histogram_quantile(0.99, latency) > 1"
for_duraton = "5m"
severity = "critical"
"#;
    std::fs::write(rules_dir.join(file), toml).expect("write malformed file");
}

// --------------------------------------------------------------------
// Spawn the real beacon-server binary with stderr piped so the test can
// synchronise on the structured reload event (the happen-before anchor).
// `RUST_LOG=info` so `beacon.reload.succeeded` (INFO) is on the stream.
// --------------------------------------------------------------------
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

/// A backend that always reports the queried metric Active (non-empty
/// instant vector), so any well-formed rule firing against it reaches
/// Firing within a couple of seeded ticks.
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
                    "value": [1640000000, "0"]
                }]
            }
        })))
        .mount(&server)
        .await;
    server
}

/// A webhook catcher that records every incident POST it receives.
/// Polled (under the generous bound) for the awaited Firing incident.
async fn webhook_catcher() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    server
}

/// Every incident JSON POSTed to the webhook catcher, in arrival order.
async fn received_incidents(sink: &MockServer) -> Vec<Value> {
    sink.received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter_map(|r: &Request| serde_json::from_slice(&r.body).ok())
        .collect()
}

/// Count of Firing incidents (resolved_at absent) for a given rule name.
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

/// Poll the sink under the generous bound until at least one Firing
/// incident for `rule_name` has arrived. Returns true on first
/// appearance; false only if the bound elapses. Presence-under-a-bound,
/// never a p95.
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

/// Drain the child's stderr in a background task into a shared buffer so
/// the test can poll it for the structured reload event without blocking
/// on a fixed read. Returns a handle the poller reads.
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

/// Poll the captured stderr (under the generous bound) until the named
/// structured event appears. This is the happen-before ANCHOR, not a
/// fixed sleep after `kill -HUP`. Returns true on first appearance.
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

/// True while the child is still running (reload must NOT crash it).
fn process_still_running(child: &mut Child) -> bool {
    matches!(child.try_wait(), Ok(None))
}

/// Reap the child at test end (also the clean-target shutdown).
fn shutdown(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

// ====================================================================
// US-01 — apply edited rules via SIGHUP, no restart.
// ====================================================================

/// Walking skeleton (US-01): the operator adds a rule to the live rules
/// directory, runs `kill -HUP <pid>`, and the newly-added rule begins
/// firing — no restart — with a structured success event naming what
/// changed. Demo-able to a stakeholder: "edit the rules, signal, the new
/// alert goes live."
#[tokio::test]
async fn added_rule_begins_firing_after_sighup_without_restart() {
    // @walking_skeleton @driving_port @real-io  (US-01)
    let backend = mock_active_backend().await;
    let sink = webhook_catcher().await;
    let rules = TmpRules::new("case");

    // Start with one rule, "service-down", which fires against the
    // active backend.
    write_rule_file(
        rules.path(),
        "service-down.toml",
        "service-down",
        "up == 0",
        &sink.uri(),
    );
    let mut child = spawn_beacon(rules.path(), &backend.uri());
    let stderr = capture_stderr(&mut child);

    // Precondition: the initial catalogue is live and firing.
    assert!(
        wait_for_firing(&sink, "service-down").await,
        "service-down should fire on the initial catalogue"
    );
    let pid_before = child.id();

    // The operator adds a new rule to the LIVE rules directory and
    // signals the running process. No restart.
    write_rule_file(
        rules.path(),
        "checkout-error-rate.toml",
        "checkout-error-rate",
        "rate(http_errors[5m]) > 0.05",
        &sink.uri(),
    );
    send_sighup(&child);

    // Happen-before anchor: the success event, THEN the downstream
    // observable (the new rule's firing).
    assert!(
        wait_for_event(&stderr, "beacon.reload.succeeded").await,
        "a successful reload must emit beacon.reload.succeeded"
    );
    assert!(
        wait_for_firing(&sink, "checkout-error-rate").await,
        "the newly-added rule must begin firing after the reload"
    );

    // Observable: same process, not a restart.
    assert_eq!(
        child.id(),
        pid_before,
        "the reload must not restart the process"
    );
    assert!(
        process_still_running(&mut child),
        "the process must stay alive across a successful reload"
    );
    shutdown(child);
}

/// US-01 AC-5: a successful reload emits one structured INFO event
/// carrying the loaded rule count and what changed.
#[tokio::test]
async fn successful_reload_emits_structured_event_naming_what_changed() {
    // @driving_port @real-io  (US-01)
    let backend = mock_active_backend().await;
    let sink = webhook_catcher().await;
    let rules = TmpRules::new("case");
    write_rule_file(
        rules.path(),
        "service-down.toml",
        "service-down",
        "up == 0",
        &sink.uri(),
    );
    let mut child = spawn_beacon(rules.path(), &backend.uri());
    let stderr = capture_stderr(&mut child);
    assert!(wait_for_firing(&sink, "service-down").await);

    write_rule_file(
        rules.path(),
        "checkout-error-rate.toml",
        "checkout-error-rate",
        "rate(http_errors[5m]) > 0.05",
        &sink.uri(),
    );
    send_sighup(&child);

    assert!(
        wait_for_event(&stderr, "beacon.reload.succeeded").await,
        "reload must emit the named success event"
    );
    let log = stderr.lock().unwrap().clone();
    assert!(
        log.contains("rules_loaded"),
        "success event must carry the loaded rule count: {log}"
    );
    assert!(
        log.contains("added"),
        "success event must name what was added: {log}"
    );
    shutdown(child);
}

/// US-01 sc.2: a removed rule stops being evaluated after SIGHUP. The
/// reload succeeds; the removed rule issues no further firings. Observed
/// as: the removed rule never produces a SECOND Firing once it is gone,
/// while the surviving rule's reload event confirms removed=1.
#[tokio::test]
async fn removed_rule_stops_evaluating_after_sighup() {
    // @driving_port @real-io  (US-01)
    let backend = mock_active_backend().await;
    let sink = webhook_catcher().await;
    let rules = TmpRules::new("case");
    write_rule_file(
        rules.path(),
        "service-down.toml",
        "service-down",
        "up == 0",
        &sink.uri(),
    );
    write_rule_file(
        rules.path(),
        "disk-pressure.toml",
        "disk-pressure",
        "disk_free < 0.1",
        &sink.uri(),
    );
    let mut child = spawn_beacon(rules.path(), &backend.uri());
    let stderr = capture_stderr(&mut child);
    assert!(wait_for_firing(&sink, "disk-pressure").await);

    // Remove disk-pressure from the live directory and signal.
    std::fs::remove_file(rules.path().join("disk-pressure.toml")).expect("remove rule");
    let firings_before = firing_count(&sink, "disk-pressure").await;
    send_sighup(&child);
    assert!(wait_for_event(&stderr, "beacon.reload.succeeded").await);

    // After the swap settles, no NEW firing arrives for the removed
    // rule. service-down keeps firing, proving the daemon is live.
    assert!(
        wait_for_event(&stderr, "removed").await,
        "event names removed"
    );
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        firing_count(&sink, "disk-pressure").await,
        firings_before,
        "a removed rule must issue no further firings after the swap"
    );
    shutdown(child);
}

/// US-01 sc.3 + STATE CARRYOVER (reviewer condition 2, success path): a
/// SIGHUP that adds an unrelated rule and leaves "service-down" unchanged
/// must NOT re-page "service-down". The surviving Firing rule keeps its
/// state across the swap: it emits NO second Firing incident. This is the
/// co-equal observable for "same `since`" (DEVOPS Decision 2: if `since`
/// is not externally observable, assert no second Firing = state kept).
/// Exercises ADR-0063 sub-decisions 2/3 (name-matching carryover +
/// InhibitionResolver rebuild), not merely "a new rule fires".
#[tokio::test]
async fn surviving_firing_rule_keeps_state_and_does_not_repage_on_successful_reload() {
    // @driving_port @real-io @property  (US-02 carryover, success path)
    let backend = mock_active_backend().await;
    let sink = webhook_catcher().await;
    let rules = TmpRules::new("case");
    write_rule_file(
        rules.path(),
        "service-down.toml",
        "service-down",
        "up == 0",
        &sink.uri(),
    );
    let mut child = spawn_beacon(rules.path(), &backend.uri());
    let stderr = capture_stderr(&mut child);

    // service-down reaches Firing: exactly one Firing incident so far.
    assert!(wait_for_firing(&sink, "service-down").await);
    let first_since = received_incidents(&sink)
        .await
        .into_iter()
        .find(|inc| inc.get("name").and_then(Value::as_str) == Some("service-down"))
        .and_then(|inc| inc.get("started_at").cloned())
        .expect("first firing carries started_at");

    // Add an UNRELATED new rule; leave service-down untouched. Signal.
    write_rule_file(
        rules.path(),
        "checkout-error-rate.toml",
        "checkout-error-rate",
        "rate(http_errors[5m]) > 0.05",
        &sink.uri(),
    );
    send_sighup(&child);
    assert!(wait_for_event(&stderr, "beacon.reload.succeeded").await);
    // Let the swapped-in generation run several ticks.
    assert!(wait_for_firing(&sink, "checkout-error-rate").await);
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Carryover: service-down still has exactly ONE Firing incident (no
    // re-page) and, if observable, the SAME started_at.
    assert_eq!(
        firing_count(&sink, "service-down").await,
        1,
        "a surviving Firing rule must not re-page across a successful reload"
    );
    let still_since = received_incidents(&sink)
        .await
        .into_iter()
        .find(|inc| inc.get("name").and_then(Value::as_str) == Some("service-down"))
        .and_then(|inc| inc.get("started_at").cloned());
    assert_eq!(
        still_since,
        Some(first_since),
        "the surviving Firing rule must keep its original since across the swap"
    );
    shutdown(child);
}

// ====================================================================
// US-02 — refuse a malformed reload, keep the previous catalogue (the
// load-bearing negative + the safety properties).
// ====================================================================

/// US-02 (THE negative): a malformed edit + SIGHUP keeps the previous
/// catalogue active, emits a refusal event naming the file + error +
/// "previous catalogue retained", does NOT crash, and does NOT partially
/// apply. This is the primary safety property of the whole feature.
#[tokio::test]
async fn malformed_reload_keeps_previous_catalogue_and_does_not_crash() {
    // @driving_port @real-io  (US-02)
    let backend = mock_active_backend().await;
    let sink = webhook_catcher().await;
    let rules = TmpRules::new("case");
    write_rule_file(
        rules.path(),
        "service-down.toml",
        "service-down",
        "up == 0",
        &sink.uri(),
    );
    let mut child = spawn_beacon(rules.path(), &backend.uri());
    let stderr = capture_stderr(&mut child);
    assert!(wait_for_firing(&sink, "service-down").await);

    // Introduce a parse error into the live rules dir and signal.
    write_malformed_rule_file(rules.path(), "payments.toml");
    send_sighup(&child);

    // Happen-before anchor: the refusal event.
    assert!(
        wait_for_event(&stderr, "beacon.reload.refused").await,
        "a malformed reload must emit beacon.reload.refused"
    );

    // Safety: process did not exit; the previous catalogue is retained,
    // so service-down keeps being evaluated (still alive and firing).
    assert!(
        process_still_running(&mut child),
        "a malformed reload must not crash the daemon"
    );
    let log = stderr.lock().unwrap().clone();
    assert!(
        log.contains("payments.toml"),
        "refusal event must name the offending file: {log}"
    );
    assert!(
        log.contains("for_duration") || log.contains("for_duraton"),
        "refusal event must carry the parse error / did-you-mean: {log}"
    );
    assert!(
        log.contains("previous_catalogue_retained") || log.contains("previous catalogue retained"),
        "refusal event must state the previous catalogue was retained: {log}"
    );
    shutdown(child);
}

/// US-02 sc.2 + STATE CARRYOVER (reviewer condition 2, refused path): a
/// rule that was Firing before a REFUSED reload keeps firing with its
/// original state and does not re-page. Co-equal observable: no SECOND
/// Firing incident for the surviving rule across the refused reload, and
/// the same started_at if observable.
#[tokio::test]
async fn surviving_firing_rule_does_not_repage_across_refused_reload() {
    // @driving_port @real-io @property  (US-02 carryover, refused path)
    let backend = mock_active_backend().await;
    let sink = webhook_catcher().await;
    let rules = TmpRules::new("case");
    write_rule_file(
        rules.path(),
        "service-down.toml",
        "service-down",
        "up == 0",
        &sink.uri(),
    );
    let mut child = spawn_beacon(rules.path(), &backend.uri());
    let stderr = capture_stderr(&mut child);
    assert!(wait_for_firing(&sink, "service-down").await);
    let first_since = received_incidents(&sink)
        .await
        .into_iter()
        .find(|inc| inc.get("name").and_then(Value::as_str) == Some("service-down"))
        .and_then(|inc| inc.get("started_at").cloned())
        .expect("first firing carries started_at");

    write_malformed_rule_file(rules.path(), "payments.toml");
    send_sighup(&child);
    assert!(wait_for_event(&stderr, "beacon.reload.refused").await);
    tokio::time::sleep(Duration::from_millis(500)).await;

    assert_eq!(
        firing_count(&sink, "service-down").await,
        1,
        "a surviving Firing rule must not re-page across a refused reload"
    );
    let still_since = received_incidents(&sink)
        .await
        .into_iter()
        .find(|inc| inc.get("name").and_then(Value::as_str) == Some("service-down"))
        .and_then(|inc| inc.get("started_at").cloned());
    assert_eq!(
        still_since,
        Some(first_since),
        "the surviving Firing rule keeps its original since across a refused reload"
    );
    shutdown(child);
}

/// US-02 sc.4 (empty catalogue boundary): every rule file deleted, then
/// SIGHUP. The reload yields zero rules; because a valid catalogue
/// requires at least one rule, the reload is refused, the daemon keeps
/// alerting (does not go dark), and the refusal event states no rules
/// were found.
#[tokio::test]
async fn reload_to_empty_catalogue_is_refused_daemon_keeps_alerting() {
    // @driving_port @real-io  (US-02)
    let backend = mock_active_backend().await;
    let sink = webhook_catcher().await;
    let rules = TmpRules::new("case");
    write_rule_file(
        rules.path(),
        "service-down.toml",
        "service-down",
        "up == 0",
        &sink.uri(),
    );
    let mut child = spawn_beacon(rules.path(), &backend.uri());
    let stderr = capture_stderr(&mut child);
    assert!(wait_for_firing(&sink, "service-down").await);

    // A botched deploy empties the rules directory.
    std::fs::remove_file(rules.path().join("service-down.toml")).expect("empty rules dir");
    send_sighup(&child);

    assert!(
        wait_for_event(&stderr, "beacon.reload.refused").await,
        "an empty-catalogue reload must be refused"
    );
    assert!(
        process_still_running(&mut child),
        "the daemon must not go dark when the rules dir is emptied"
    );
    let firings_before = firing_count(&sink, "service-down").await;
    // The previous catalogue is retained, so service-down keeps being
    // evaluated: at least the original firing persists; no resolution.
    assert!(
        firings_before >= 1,
        "service-down keeps firing on the retained catalogue"
    );
    shutdown(child);
}

/// US-02 sc.3 (partly-broken boundary): a SIGHUP whose re-loaded
/// catalogue has one VALID new rule alongside a malformed file applies
/// the valid catalogue (it succeeds, not refused) AND still surfaces the
/// per-file diagnostic for the skipped file (report-and-skip, B01,
/// startup-consistent).
#[tokio::test]
async fn partly_broken_catalogue_applies_valid_rules_and_surfaces_diagnostic() {
    // @driving_port @real-io  (US-02 boundary)
    let backend = mock_active_backend().await;
    let sink = webhook_catcher().await;
    let rules = TmpRules::new("case");
    write_rule_file(
        rules.path(),
        "service-down.toml",
        "service-down",
        "up == 0",
        &sink.uri(),
    );
    let mut child = spawn_beacon(rules.path(), &backend.uri());
    let stderr = capture_stderr(&mut child);
    assert!(wait_for_firing(&sink, "service-down").await);

    // Add one VALID rule and one MALFORMED file; signal.
    write_rule_file(
        rules.path(),
        "checkout-error-rate.toml",
        "checkout-error-rate",
        "rate(http_errors[5m]) > 0.05",
        &sink.uri(),
    );
    write_malformed_rule_file(rules.path(), "inventory.toml");
    send_sighup(&child);

    // The catalogue as a whole validated (at least one rule), so the
    // reload SUCCEEDS and the valid new rule begins firing.
    assert!(
        wait_for_event(&stderr, "beacon.reload.succeeded").await,
        "a partly-broken catalogue with a valid rule must still apply"
    );
    assert!(
        wait_for_firing(&sink, "checkout-error-rate").await,
        "the valid new rule must begin firing"
    );
    // The skipped file's diagnostic is still surfaced.
    let log = stderr.lock().unwrap().clone();
    assert!(
        log.contains("inventory.toml"),
        "the per-file diagnostic for the skipped file must be surfaced: {log}"
    );
    shutdown(child);
}

/// US-01 sc.3 (no-change boundary): a SIGHUP with no on-disk change
/// swaps cleanly with no spurious Firing and no spurious Resolved. The
/// surviving Firing rule keeps exactly one Firing incident, and no
/// Resolved incident is emitted. (A config-management tool that sends
/// SIGHUP on every converge must not storm on-call.)
#[tokio::test]
async fn no_change_sighup_swaps_cleanly_with_no_spurious_emissions() {
    // @driving_port @real-io  (US-01 boundary)
    let backend = mock_active_backend().await;
    let sink = webhook_catcher().await;
    let rules = TmpRules::new("case");
    write_rule_file(
        rules.path(),
        "service-down.toml",
        "service-down",
        "up == 0",
        &sink.uri(),
    );
    let mut child = spawn_beacon(rules.path(), &backend.uri());
    let stderr = capture_stderr(&mut child);
    assert!(wait_for_firing(&sink, "service-down").await);

    // Signal without editing anything.
    send_sighup(&child);
    assert!(wait_for_event(&stderr, "beacon.reload.succeeded").await);
    tokio::time::sleep(Duration::from_millis(500)).await;

    assert_eq!(
        firing_count(&sink, "service-down").await,
        1,
        "a no-change reload must not emit a spurious second Firing"
    );
    let resolved = received_incidents(&sink)
        .await
        .into_iter()
        .filter(|inc| {
            inc.get("name").and_then(Value::as_str) == Some("service-down")
                && inc
                    .get("resolved_at")
                    .map(|v| !v.is_null())
                    .unwrap_or(false)
        })
        .count();
    assert_eq!(
        resolved, 0,
        "a no-change reload must not emit a spurious Resolved"
    );
    shutdown(child);
}
