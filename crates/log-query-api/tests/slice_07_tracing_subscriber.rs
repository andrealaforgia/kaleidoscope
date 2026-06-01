// Kaleidoscope log-query-api — slice 07 tracing-subscriber acceptance suite
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

//! Black-box subprocess acceptance suite for read-api-tracing-subscriber-v0.
//!
//! Feature: install a tracing subscriber in the read binaries so the
//! operator (Priya Nair) can read startup and refusal lifecycle events
//! off container stderr.
//!
//! Driving port (DD5, the PINNED verification strategy): the operator's
//! actual invocation path — the COMPILED BINARY launched as a child
//! process, with controlled environment, whose stderr is captured and
//! grepped for structured JSON `event` lines. This is exactly the shape
//! the EDD black-box verifier uses (LQ02/LQ03 captured empty stderr
//! today). An in-process test cannot assert this: the subscriber is
//! process-global and `try_init`-guarded and writes to the real stderr
//! fd, so only a spawned process can observe it. The in-process
//! idempotence contract (Option B) lives as a unit test in
//! `crates/query-http-common/src/lib.rs`.
//!
//! ## RED-not-BROKEN posture (Mandate 7)
//!
//! At DISTILL close `query_http_common::init_tracing()` is a wired NO-OP:
//! it is called as the first statement of `log-query-api`'s `main` but
//! installs no subscriber, so the binary boots exactly as it does today
//! and every existing slice (01..06) stays GREEN. The fail-closed
//! scenario below (NOT `#[ignore]`d) therefore RUNS under `cargo test`
//! and is RED: the binary refuses to start (tenant unset), but no
//! `health.startup.refused` JSON line reaches stderr because the
//! subscriber is absent. DELIVER (Crafty) fills the `init_tracing` body
//! with aperture's JSON-to-stderr `EnvFilter("RUST_LOG")` builder and
//! this scenario turns GREEN. The test is RED because the behaviour is
//! unimplemented, never BROKEN (no panic, no missing symbol).
//!
//! The clean-start and filter scenarios spawn a server that binds a
//! socket and blocks on `axum::serve`, so they are polled-then-killed.
//! DELIVER de-ignored them (no `#[ignore]`) because they bind an EPHEMERAL
//! OS-assigned port (`127.0.0.1:0`, never a fixed port) and the
//! poll-then-kill drains stderr on a dedicated thread under a robust
//! wall-clock `recv_timeout` deadline that fires even when the child
//! emits no output at all (the `RUST_LOG=warn` case). With no fixed port
//! and no timing-fragile read, they run stably inside the deterministic
//! pre-commit hook — the same determinism bar perf-kpi-ci-gating-v0
//! established. The fail-closed scenario needs no socket and no kill (the
//! process exits non-zero on its own), and remains the always-run RED
//! anchor that pins the core refusal observability.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::Value;

/// The compiled `log-query-api` binary, as launched by an operator.
/// Cargo guarantees this path points at the freshly built artefact for
/// the crate under test.
fn log_query_api_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_log-query-api"))
}

/// Parse each stderr line as JSON and return whether ANY line carries
/// the given structured `event` field value. Non-JSON lines (the
/// pre-init `eprintln!` window, US-06) are skipped without failing the
/// scan.
fn stderr_has_event(stderr: &str, event_name: &str) -> bool {
    stderr.lines().any(|line| {
        serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|v| {
                v.get("event")
                    .and_then(|e| e.as_str())
                    .map(|e| e == event_name)
            })
            .unwrap_or(false)
    })
}

/// Spawn the binary, read its stderr until a line carrying `event_name`
/// appears or `timeout` elapses, then kill the child. Returns the
/// captured stderr so far. Used for the clean-start / filter scenarios
/// where the process binds a socket and never exits on its own.
///
/// The child's stderr is drained on a dedicated thread that forwards each
/// chunk over a channel. The main thread enforces a WALL-CLOCK deadline
/// via `recv_timeout`, so the bound is honoured even when the child
/// produces NO stderr at all — which is precisely the `RUST_LOG=warn`
/// filter case, where every info-level line is dropped and a plain
/// blocking `read` on the pipe would otherwise hang until the child is
/// killed (the read never returns, so a deadline tested only between
/// reads is never reached). Draining on a thread decouples the deadline
/// from the blocking read.
fn capture_stderr_until_event(
    mut cmd: Command,
    event_name: &str,
    timeout: Duration,
) -> (String, bool) {
    let mut child = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn log-query-api");
    let mut stderr = child.stderr.take().expect("child stderr piped");

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let reader = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match stderr.read(&mut buf) {
                Ok(0) => break, // child closed stderr / exited
                Ok(n) => {
                    // If the receiver has gone away (deadline hit, child
                    // about to be killed) stop draining.
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let deadline = Instant::now() + timeout;
    let mut captured = String::new();
    let mut seen = false;

    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(remaining) {
            Ok(chunk) => {
                captured.push_str(&String::from_utf8_lossy(&chunk));
                if stderr_has_event(&captured, event_name) {
                    seen = true;
                    break;
                }
            }
            // Deadline reached, or the reader thread finished (channel
            // closed) — either way we stop waiting.
            Err(_) => break,
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    // Dropping `rx` lets the reader thread observe a closed channel and
    // exit; join so no thread leaks past the test.
    drop(rx);
    let _ = reader.join();
    (captured, seen)
}

// =========================================================================
// US-02: fail-closed refusal is visible before exit (the RED anchor)
// =========================================================================
//
// Always-run. Deterministic: tenant unset -> probe refuses -> the binary
// emits `health.startup.refused` then exits non-zero, with no socket
// bound and no kill needed. `.output()` blocks until the child exits.

/// Scenario: Operator learns why the service refused to start.
///
/// Given Priya starts log-query-api with the tenant unset
/// When the startup probe refuses the service
/// Then stderr contains a `health.startup.refused` event naming the reason
/// And the process exits non-zero
#[test]
fn fail_closed_startup_writes_health_startup_refused_to_stderr_before_nonzero_exit() {
    let tmp = std::env::temp_dir().join(format!(
        "kaleidoscope-lqa-tracing-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));

    let output = log_query_api_bin()
        .env("KALEIDOSCOPE_PILLAR_ROOT", &tmp)
        // KALEIDOSCOPE_LOG_QUERY_TENANT deliberately UNSET -> fail-closed.
        .env_remove("KALEIDOSCOPE_LOG_QUERY_TENANT")
        .stdout(Stdio::null())
        .output()
        .expect("spawn log-query-api");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Observable outcome 1: the refusal reason is visible on stderr as a
    // structured event. RED against the no-op subscriber (stderr empty).
    assert!(
        stderr_has_event(&stderr, "health.startup.refused"),
        "operator must see the structured refusal event on stderr; got:\n{stderr}"
    );

    // Observable outcome 2: the service genuinely refused (non-zero exit).
    assert!(
        !output.status.success(),
        "a fail-closed startup must exit non-zero; status was {:?}",
        output.status.code()
    );
}

/// Scenario: Refusal survives a strict log filter.
///
/// Given Priya sets `RUST_LOG=error` and starts with the tenant unset
/// When the startup probe refuses the service
/// Then the `health.startup.refused` event is still present on stderr
#[test]
fn refusal_event_survives_rust_log_error_filter() {
    let tmp = std::env::temp_dir().join(format!(
        "kaleidoscope-lqa-tracing-err-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));

    let output = log_query_api_bin()
        .env("KALEIDOSCOPE_PILLAR_ROOT", &tmp)
        .env_remove("KALEIDOSCOPE_LOG_QUERY_TENANT")
        .env("RUST_LOG", "error")
        .stdout(Stdio::null())
        .output()
        .expect("spawn log-query-api");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // The error-level refusal must survive an `error`-floor filter, so a
    // stricter operator filter never hides the reason. RED against no-op.
    assert!(
        stderr_has_event(&stderr, "health.startup.refused"),
        "the error-level refusal must survive RUST_LOG=error; got:\n{stderr}"
    );
    assert!(!output.status.success(), "fail-closed must exit non-zero");
}

// =========================================================================
// US-01: clean startup lifecycle is visible (poll-then-kill scenarios)
// =========================================================================
//
// These bind a socket and block on `axum::serve`, so they are polled for
// the awaited event then killed. NOT `#[ignore]`d: they bind an ephemeral
// `127.0.0.1:0` port and the poll-then-kill uses a robust wall-clock
// deadline, so they run deterministically in the pre-commit hook.

/// Scenario: Operator sees the service announce itself at startup.
///
/// Given Priya runs log-query-api with tenant `acme` and a readable store
/// When the process starts
/// Then its stderr contains a structured `log_query_api_starting` event
#[test]
fn clean_startup_announces_log_query_api_starting_on_stderr() {
    let tmp = std::env::temp_dir().join(format!(
        "kaleidoscope-lqa-tracing-start-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    std::fs::create_dir_all(&tmp).expect("tmp pillar root");

    let mut cmd = log_query_api_bin();
    cmd.env("KALEIDOSCOPE_PILLAR_ROOT", &tmp)
        .env("KALEIDOSCOPE_LOG_QUERY_TENANT", "acme")
        // Ephemeral OS-assigned port: no collisions, no fixed-port flake.
        .env("KALEIDOSCOPE_LOG_QUERY_ADDR", "127.0.0.1:0");

    let (stderr, seen) =
        capture_stderr_until_event(cmd, "log_query_api_starting", Duration::from_secs(5));

    assert!(
        seen,
        "operator must see the service announce startup on stderr; got:\n{stderr}"
    );
}

/// Scenario: Operator sees the bound listener address.
///
/// Given Priya runs log-query-api with tenant `acme` and a readable store
/// When the listener binds
/// Then its stderr contains a `listener_bound` event naming the address
#[test]
fn clean_startup_reports_bound_listener_address_on_stderr() {
    let tmp = std::env::temp_dir().join(format!(
        "kaleidoscope-lqa-tracing-bound-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    std::fs::create_dir_all(&tmp).expect("tmp pillar root");

    let mut cmd = log_query_api_bin();
    cmd.env("KALEIDOSCOPE_PILLAR_ROOT", &tmp)
        .env("KALEIDOSCOPE_LOG_QUERY_TENANT", "acme")
        .env("KALEIDOSCOPE_LOG_QUERY_ADDR", "127.0.0.1:0");

    let (stderr, seen) = capture_stderr_until_event(cmd, "listener_bound", Duration::from_secs(5));

    assert!(
        seen,
        "operator must see the bound listener on stderr; got:\n{stderr}"
    );
    // The bound event must carry an `addr` field so the operator can
    // confirm an override took effect from stderr alone (US-01 example 2).
    let bound_line_has_addr = stderr.lines().any(|line| {
        serde_json::from_str::<Value>(line)
            .ok()
            .map(|v| {
                v.get("event").and_then(|e| e.as_str()) == Some("listener_bound")
                    && v.get("addr").is_some()
            })
            .unwrap_or(false)
    });
    assert!(
        bound_line_has_addr,
        "the listener_bound event must name the bound address; got:\n{stderr}"
    );
}

/// Scenario: Startup chatter respects the log-level filter.
///
/// Given Priya sets `RUST_LOG=warn` before starting log-query-api
/// When the process starts cleanly
/// Then the info-level startup events are absent from stderr
#[test]
fn rust_log_warn_suppresses_info_startup_events() {
    let tmp = std::env::temp_dir().join(format!(
        "kaleidoscope-lqa-tracing-warn-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    std::fs::create_dir_all(&tmp).expect("tmp pillar root");

    let mut cmd = log_query_api_bin();
    cmd.env("KALEIDOSCOPE_PILLAR_ROOT", &tmp)
        .env("KALEIDOSCOPE_LOG_QUERY_TENANT", "acme")
        .env("KALEIDOSCOPE_LOG_QUERY_ADDR", "127.0.0.1:0")
        .env("RUST_LOG", "warn");

    // Give the process a fixed window to boot and bind, then inspect what
    // it wrote. We do NOT wait for an info event (the filter should drop
    // it); we wait for the listener to bind by polling on the warn-filtered
    // run is not possible, so capture a fixed window then assert absence.
    let (stderr, _) = capture_stderr_until_event(cmd, "__never__", Duration::from_secs(2));

    assert!(
        !stderr_has_event(&stderr, "log_query_api_starting"),
        "RUST_LOG=warn must suppress the info-level startup event; got:\n{stderr}"
    );
}
