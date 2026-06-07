// Kaleidoscope gateway — slice 01 tracing-subscriber acceptance suite
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

//! Black-box subprocess acceptance suite for gateway-tracing-subscriber-v0.
//!
//! Feature: install a tracing subscriber EARLY in the gateway's `main` so
//! the operator (Priya Nair, platform SRE) can read the gateway's own
//! startup and fail-closed-refusal lifecycle events off container stderr.
//! The gateway is the write/ingest-side binary, so it aligns to the
//! aperture posture, NOT to the read tier's `query-http-common` (US-03,
//! anti-coupling invariant).
//!
//! Driving port (the PINNED verification strategy, DESIGN Verification
//! section): the operator's actual invocation path — the COMPILED BINARY
//! launched as a child process with a controlled environment, whose
//! stderr is captured and grepped for structured JSON `event` lines. This
//! is exactly the black-box shape the operability verifier (verifier-007,
//! issue 005) uses. An in-process test cannot assert this: the subscriber
//! is process-global, `try_init`-guarded, and writes to the real stderr
//! fd, so only a spawned process can observe it. The in-process
//! idempotence contract (the double-install / OnceLock guard) lives as a
//! unit test in the crate, mirroring the read tier's
//! `test_init_tracing_is_idempotent_and_never_panics`.
//!
//! ## Delivered posture (GREEN)
//!
//! `init_tracing()` in `main.rs` installs the real JSON-to-stderr
//! subscriber (registry + JSON stderr layer, `EnvFilter`,
//! `OnceLock` + `try_init`-guarded) as the first statement of the
//! gateway's `main`. The fail-closed scenario below (AC-02, NOT
//! `#[ignore]`d) therefore RUNS under `cargo test` and is GREEN: the
//! gateway refuses to start (the sink probe's snapshot create fails on a
//! read-only pillar root), exits non-zero, AND emits the
//! `health.startup.refused` JSON line to stderr because the subscriber is
//! installed and active. The test asserts that JSON line IS present and
//! PASSES.
//!
//! ## Why AC-02 (fail-closed) is the primary always-run anchor
//!
//! The gateway's `main` builds its aperture `Config` with
//! `Config::builder().build()` and reads NO env override for the listener
//! bind address (`crates/kaleidoscope-gateway/src/main.rs`); the defaults
//! are the FIXED operator ports `0.0.0.0:4317` (grpc) and `0.0.0.0:4318`
//! (http). A clean-start scenario (AC-01) therefore cannot bind an
//! ephemeral `127.0.0.1:0` the way the read tier's `log-query-api` could,
//! so it binds fixed ports and is fixed-port-flake-prone inside the
//! deterministic pre-commit hook. AC-02 needs no socket and no kill: the
//! sink probe refuses, the process exits non-zero on its own, and
//! `.output()` blocks until it does. It is the deterministic RED anchor
//! that pins the core refusal observability — the half of issue 005 the
//! operator most needs (understanding WHY the gateway refused to boot).
//! AC-01 (clean start) and the `listener_bound` regression guard are kept
//! `#[ignore]`d RED-ready below; DELIVER decides whether the fixed-port
//! bind can run deterministically in the hook or stays an explicit
//! `cargo test -- --ignored` check.

#![cfg(unix)]

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

use serde_json::Value;

/// The compiled `kaleidoscope-gateway` binary, as launched by an
/// operator. Cargo guarantees this path points at the freshly built
/// artefact for the crate under test.
fn gateway_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_kaleidoscope-gateway"))
}

/// Parse each stderr line as JSON and return whether ANY line carries the
/// given structured `event` field value. Non-JSON lines are skipped
/// without failing the scan.
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

/// Return the JSON value of the first stderr line whose `event` field
/// matches `event_name`, if any. Lets a scenario assert the line carries
/// the expected payload fields (`substrate`, `reason`, `pillar_root`).
fn first_event_line(stderr: &str, event_name: &str) -> Option<Value> {
    stderr.lines().find_map(|line| {
        serde_json::from_str::<Value>(line)
            .ok()
            .filter(|v| v.get("event").and_then(|e| e.as_str()) == Some(event_name))
    })
}

/// Build a unique temporary pillar root path (not yet created).
fn unique_pillar_root(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "kaleidoscope-gw-tracing-{tag}-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ))
}

/// Stage a pillar root that opens but cannot accept a fresh snapshot
/// file, so the storage-sink active-write probe (DD5 / ADR-0041) refuses
/// and the gateway emits `event=health.startup.refused` with
/// `substrate=sink`, then exits non-zero — WITHOUT binding any socket.
///
/// Mechanics (read from source, deterministic and infra-free):
/// - `FileBacked*Store::open` opens each WAL with
///   `OpenOptions::create(true).append(true)` on a path SIBLING to the
///   pillar root (`<root>/lumen.wal`, `<root>/ray.wal`, `<root>/pulse.wal`).
///   Pre-creating those files means the append-open finds an existing
///   file and succeeds even when the parent directory is read-only (Unix
///   needs write on the FILE for append, write on the DIRECTORY only to
///   CREATE a new entry).
/// - The sink probe then ingests one sentinel (WAL append — succeeds) and
///   calls `snapshot()`, whose `File::create(<root>/lumen.snapshot)` needs
///   to CREATE a new directory entry. On a `0o555` (read+execute, no
///   write) pillar root that create fails with permission denied, so the
///   probe returns `ProbeError::Unreachable` -> `CompositionError::SinkProbe`
///   -> `substrate=sink`. This is exactly the catalogued "opens but is not
///   writable" substrate lie the snapshot check exists to catch.
///
/// `main` runs `std::fs::create_dir_all(root)` first; that returns Ok on
/// an already-existing directory regardless of its mode, so the read-only
/// root survives to the store opens and the probe.
fn stage_unwritable_after_open_pillar_root(root: &Path) {
    use std::os::unix::fs::PermissionsExt;

    std::fs::create_dir_all(root).expect("create pillar root");
    // Pre-create the three WAL files so the store opens (append) succeed
    // on the soon-to-be read-only directory.
    for wal in ["lumen.wal", "ray.wal", "pulse.wal"] {
        std::fs::write(root.join(wal), b"").expect("pre-create WAL file");
    }
    // Read + execute, NO write: existing-file append still works; creating
    // the snapshot file (a new directory entry) does not.
    std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o555))
        .expect("chmod pillar root read-only");
}

/// Restore write permission so the temp dir can be cleaned up.
fn restore_and_cleanup(root: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o755));
    let _ = std::fs::remove_dir_all(root);
}

// =========================================================================
// AC-02: fail-closed refusal is visible before the non-zero exit
//        (the PRIMARY always-run RED anchor)
// =========================================================================
//
// Deterministic: the sink probe's snapshot create fails on a read-only
// pillar root -> the gateway emits `health.startup.refused` then exits
// non-zero, with no socket bound and no kill needed. `.output()` blocks
// until the child exits.

/// Scenario: Operator learns why the gateway refused to start.
///
/// Given Priya starts the gateway against a substrate that fails the
///       Earned-Trust composition probe (a pillar root that opens but
///       cannot accept a fresh snapshot write)
/// When the startup probe refuses the gateway
/// Then stderr contains a `health.startup.refused` event naming the
///      `substrate` class and the `reason`
/// And the process exits non-zero
/// And the refusal line precedes the exit (it is on the captured stderr,
///     which `.output()` returns only after the process has exited)
#[test]
fn fail_closed_startup_writes_health_startup_refused_to_stderr_before_nonzero_exit() {
    let root = unique_pillar_root("refused");
    stage_unwritable_after_open_pillar_root(&root);

    let output = gateway_bin()
        .arg(&root)
        .env("KALEIDOSCOPE_DEFAULT_TENANT", "acme")
        .stdout(Stdio::null())
        .output()
        .expect("spawn kaleidoscope-gateway");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let status = output.status;
    restore_and_cleanup(&root);

    // Observable outcome 1: the refusal reason is visible on stderr as a
    // structured event. GREEN: `init_tracing` installs the real
    // JSON-to-stderr subscriber, so stderr carries the JSON
    // `health.startup.refused` line.
    let refusal = first_event_line(&stderr, "health.startup.refused");
    assert!(
        refusal.is_some(),
        "operator must see the structured refusal event on stderr; got:\n{stderr}"
    );

    // Observable outcome 2: the refusal names the substrate class and a
    // reason, so the operator can act on it (US-02 ACs).
    let refusal = refusal.unwrap();
    assert_eq!(
        refusal.get("substrate").and_then(|v| v.as_str()),
        Some("sink"),
        "the refusal must name substrate=sink for a failed sink probe; got:\n{stderr}"
    );
    assert!(
        refusal.get("reason").is_some(),
        "the refusal must carry a reason field; got:\n{stderr}"
    );

    // Observable outcome 3: the gateway genuinely refused (non-zero exit).
    assert!(
        !status.success(),
        "a fail-closed startup must exit non-zero; status was {:?}",
        status.code()
    );
}

/// Scenario: Refusal survives a strict log filter.
///
/// Given Priya sets `RUST_LOG=warn` and starts the gateway against a
///       substrate that fails the Earned-Trust composition probe
/// When the startup probe refuses the gateway
/// Then the error-level `health.startup.refused` event is still present
///      on stderr (an error survives any floor at warn or laxer), so a
///      stricter operator filter never hides the reason
#[test]
fn refusal_event_survives_rust_log_warn_filter() {
    let root = unique_pillar_root("refused-warn");
    stage_unwritable_after_open_pillar_root(&root);

    let output = gateway_bin()
        .arg(&root)
        .env("KALEIDOSCOPE_DEFAULT_TENANT", "acme")
        .env("RUST_LOG", "warn")
        .stdout(Stdio::null())
        .output()
        .expect("spawn kaleidoscope-gateway");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let status = output.status;
    restore_and_cleanup(&root);

    assert!(
        stderr_has_event(&stderr, "health.startup.refused"),
        "the error-level refusal must survive RUST_LOG=warn; got:\n{stderr}"
    );
    assert!(!status.success(), "fail-closed must exit non-zero");
}

// =========================================================================
// AC-01: clean startup lifecycle is visible
//        (RED-ready, `#[ignore]`d — fixed-port bind, see module docs)
// =========================================================================
//
// These bind the gateway's FIXED default ports (0.0.0.0:4317 grpc,
// 0.0.0.0:4318 http) because the gateway's `main` reads no bind-address
// override. They are `#[ignore]`d so the always-run suite stays
// deterministic in the pre-commit hook (no fixed-port collision risk).
// They remain an explicit `cargo test -- --ignored` operator check until
// deterministic binding is guaranteed (e.g. by serialising or by a future
// ephemeral-port knob). Their `#[ignore]` is port-flake determinism, NOT
// an absent subscriber: `init_tracing` installs the real JSON-to-stderr
// subscriber, so when these run `gateway_starting` renders (GREEN).

/// Spawn the gateway, drain stderr on a dedicated thread until a line
/// carrying `event_name` appears or `timeout` elapses, then kill the
/// child. Mirrors the read tier's poll-then-kill harness
/// (`log-query-api/tests/slice_07_tracing_subscriber.rs`): a WALL-CLOCK
/// deadline via `recv_timeout` so the bound is honoured even when the
/// child emits no stderr at all (the `RUST_LOG=warn` case).
fn capture_stderr_until_event(
    mut cmd: Command,
    event_name: &str,
    timeout: std::time::Duration,
) -> (String, bool) {
    use std::io::Read;
    use std::sync::mpsc;

    let mut child = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn kaleidoscope-gateway");
    let mut stderr = child.stderr.take().expect("child stderr piped");

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let reader = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match stderr.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
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
            Err(_) => break,
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    drop(rx);
    let _ = reader.join();
    (captured, seen)
}

/// Scenario: Operator sees the gateway announce itself at startup.
///
/// Given Priya runs the gateway with an explicit pillar root and a
///       default tenant set
/// When the process starts
/// Then its stderr contains a structured `gateway_starting` event naming
///      the `pillar_root`
#[test]
#[ignore = "binds the gateway's FIXED default ports (4317/4318); RED-ready, see module docs"]
fn clean_startup_announces_gateway_starting_on_stderr() {
    let root = unique_pillar_root("starting");
    std::fs::create_dir_all(&root).expect("tmp pillar root");

    let mut cmd = gateway_bin();
    cmd.arg(&root).env("KALEIDOSCOPE_DEFAULT_TENANT", "acme");

    let (stderr, seen) =
        capture_stderr_until_event(cmd, "gateway_starting", std::time::Duration::from_secs(5));
    let has_pillar_root = first_event_line(&stderr, "gateway_starting")
        .and_then(|v| v.get("pillar_root").cloned())
        .is_some();
    restore_and_cleanup(&root);

    assert!(
        seen,
        "operator must see the gateway announce startup on stderr; got:\n{stderr}"
    );
    assert!(
        has_pillar_root,
        "the gateway_starting event must name the pillar_root; got:\n{stderr}"
    );
}

/// Scenario: Operator sees the bound listener address (regression guard).
///
/// Given Priya runs the gateway with an explicit pillar root and a
///       default tenant set
/// When the listeners bind
/// Then its stderr contains a `listener_bound` event naming the
///      `transport` and the bound `addr`
///
/// `listener_bound` already renders today (aperture emits it inside
/// `spawn`, after its own install). This is the US-01 regression guard
/// that the early install preserves that stream and shape.
#[test]
#[ignore = "binds the gateway's FIXED default ports (4317/4318); regression guard, see module docs"]
fn clean_startup_reports_bound_listener_address_on_stderr() {
    let root = unique_pillar_root("bound");
    std::fs::create_dir_all(&root).expect("tmp pillar root");

    let mut cmd = gateway_bin();
    cmd.arg(&root).env("KALEIDOSCOPE_DEFAULT_TENANT", "acme");

    let (stderr, seen) =
        capture_stderr_until_event(cmd, "listener_bound", std::time::Duration::from_secs(5));
    let bound_line_has_fields = stderr.lines().any(|line| {
        serde_json::from_str::<Value>(line)
            .ok()
            .map(|v| {
                v.get("event").and_then(|e| e.as_str()) == Some("listener_bound")
                    && v.get("transport").is_some()
                    && v.get("addr").is_some()
            })
            .unwrap_or(false)
    });
    restore_and_cleanup(&root);

    assert!(
        seen,
        "operator must see the bound listener on stderr; got:\n{stderr}"
    );
    assert!(
        bound_line_has_fields,
        "listener_bound must name transport and addr; got:\n{stderr}"
    );
}
