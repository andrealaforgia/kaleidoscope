// Kaleidoscope log-query-api — slice 10 read-auth composition-root config reject
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

//! Slice 10 — read-auth composition-root config wiring (the closers).
//!
//! Feature: `read-path-query-api-auth-v0` (ADR-0074 DD1/DD4). This is the
//! load-bearing security control DISTILL deferred to DELIVER: it lives at the
//! `main`/composition-root boundary, NOT the `router()` seam the in-process
//! auth suites drive. It mirrors `query-api`'s
//! `slice_08_auth_config_reject.rs` refuse-to-start precedent on the logs
//! read binary.
//!
//! ## The matrix under test (DD1/DD4)
//!
//! | `KALEIDOSCOPE_LOG_QUERY_AUTH_*` | Result | Event | Exit | Listener |
//! |---|---|---|---|---|
//! | all four absent             | Start (env-tenant mode) | `listener_bound` | runs | bound |
//! | partial (some set)          | Refuse | `config_validation_failed` names the missing key | non-zero | none |
//! | secret_file unreadable      | Refuse | `config_validation_failed` names the PATH (no bytes) | non-zero | none |
//! | complete + readable         | Start (auth mode, no env tenant needed) | `listener_bound` | runs | bound |
//!
//! ## Driving port (black-box, @real-io)
//!
//! The real `log-query-api` binary subprocess, observed through its exit code
//! and structured stderr. EPHEMERAL ports only: every spawn sets
//! `KALEIDOSCOPE_LOG_QUERY_ADDR=127.0.0.1:0`, so the fixed `9091` default is
//! never bound (no fixed-port flake). The "no listener" observable on a
//! refusal is the non-zero exit BEFORE the bind step — the binary is gone,
//! nothing is serving.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::Value;

const CONFIG_REFUSAL_EVENT: &str = "config_validation_failed";
const PROBE_REFUSAL_EVENT: &str = "health.startup.refused";
const BOUND_EVENT: &str = "listener_bound";

/// A sentinel secret value written to the readable secret file. If this
/// string ever appears in a refusal's stderr, the never-logged invariant
/// (ADR-0074 DD1) has been violated.
const SENTINEL_SECRET: &str = "SUPERSECRET-read-auth-hs256-key-bytes";

fn log_query_api_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_log-query-api"))
}

/// A unique temp path under the system temp dir for this test run.
fn unique_path(label: &str, ext: &str) -> std::path::PathBuf {
    let stamp = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    );
    std::env::temp_dir().join(format!("kaleidoscope-lqa-authcfg-{label}-{stamp}.{ext}"))
}

/// Parse each stderr line as JSON; true iff ANY line carries the structured
/// `event` field equal to `event_name`. Non-JSON pre-init lines are skipped.
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

/// The `reason` field of the FIRST stderr line carrying `event_name`, if any.
fn event_reason(stderr: &str, event_name: &str) -> Option<String> {
    stderr.lines().find_map(|line| {
        let v: Value = serde_json::from_str(line).ok()?;
        if v.get("event").and_then(|e| e.as_str()) == Some(event_name) {
            v.get("reason")
                .and_then(|r| r.as_str())
                .map(|s| s.to_string())
        } else {
            None
        }
    })
}

/// A Drop-guard that always reaps the child on every exit path.
struct ChildReaper(Option<Child>);

impl Drop for ChildReaper {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Spawn the command, drain stderr on a thread, and wait up to `timeout`.
/// Breaks early if `stop_event` (when given) appears on stderr. Returns the
/// exit code (`None` if it was still running at the deadline) and all stderr
/// captured up to that point. The child is always reaped.
fn run_bounded(
    mut cmd: Command,
    timeout: Duration,
    stop_event: Option<&str>,
) -> (Option<i32>, String) {
    let child = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn log-query-api");
    let mut guard = ChildReaper(Some(child));
    let mut stderr_pipe = guard
        .0
        .as_mut()
        .expect("child present")
        .stderr
        .take()
        .expect("child stderr piped");

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let reader = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match stderr_pipe.read(&mut buf) {
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
    let mut exit_code = None;
    loop {
        while let Ok(chunk) = rx.try_recv() {
            captured.push_str(&String::from_utf8_lossy(&chunk));
        }
        if let Some(event) = stop_event {
            if stderr_has_event(&captured, event) {
                break;
            }
        }
        match guard
            .0
            .as_mut()
            .expect("child present")
            .try_wait()
            .expect("try_wait")
        {
            Some(status) => {
                exit_code = status.code();
                break;
            }
            None => {
                if Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }

    // Reap, then drain anything the reader still holds.
    if let Some(mut child) = guard.0.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    for chunk in rx {
        captured.push_str(&String::from_utf8_lossy(&chunk));
    }
    let _ = reader.join();
    (exit_code, captured)
}

// =========================================================================
// US-RAUTH-01 — partial auth config refuses to start (config_validation_failed)
// =========================================================================

/// A partial read-auth config (issuer + audience set, secret_file + catalogue
/// omitted) is the silent-downgrade trap: it MUST refuse to start with a
/// non-zero exit and `event=config_validation_failed` naming a missing key.
/// The env tenant is set so the ONLY reason to refuse is the partial auth
/// config (had the binary proceeded it would have bound the ephemeral port).
#[test]
fn partial_auth_config_refuses_to_start_naming_the_missing_key() {
    let pillar = unique_path("partial-pillar", "dir");
    std::fs::create_dir_all(&pillar).expect("pillar root");

    let (code, stderr) = run_bounded(
        {
            let mut cmd = log_query_api_bin();
            cmd.env("KALEIDOSCOPE_PILLAR_ROOT", &pillar)
                .env("KALEIDOSCOPE_LOG_QUERY_TENANT", "acme-prod")
                .env("KALEIDOSCOPE_LOG_QUERY_ADDR", "127.0.0.1:0")
                .env("KALEIDOSCOPE_LOG_QUERY_AUTH_ISSUER", "acme-observability")
                .env("KALEIDOSCOPE_LOG_QUERY_AUTH_AUDIENCE", "kaleidoscope-query")
                .env_remove("KALEIDOSCOPE_LOG_QUERY_AUTH_SECRET_FILE")
                .env_remove("KALEIDOSCOPE_LOG_QUERY_AUTH_CATALOGUE");
            cmd
        },
        Duration::from_secs(8),
        None,
    );
    let _ = std::fs::remove_dir_all(&pillar);

    assert_eq!(
        code,
        Some(2),
        "a partial read-auth config must exit 2 (refuse to start); stderr:\n{stderr}"
    );
    assert!(
        stderr_has_event(&stderr, CONFIG_REFUSAL_EVENT),
        "stderr must carry event={CONFIG_REFUSAL_EVENT}; got:\n{stderr}"
    );
    let reason = event_reason(&stderr, CONFIG_REFUSAL_EVENT).unwrap_or_default();
    assert!(
        reason.contains("SECRET_FILE")
            || reason.contains("CATALOGUE")
            || reason.contains("secret_file")
            || reason.contains("catalogue"),
        "the refusal must name a missing read-auth key; reason: {reason:?}"
    );
}

// =========================================================================
// US-RAUTH-01 — unreadable secret_file refuses, naming the PATH not the bytes
// =========================================================================

/// A complete read-auth config whose `secret_file` points at a non-existent
/// path refuses to start, naming the unreadable PATH — and NO secret bytes
/// appear anywhere on stderr (there are none to read; the assertion guards a
/// future inline-secret regression too).
#[test]
fn unreadable_secret_file_refuses_to_start_naming_the_path_not_the_bytes() {
    let pillar = unique_path("unreadable-pillar", "dir");
    std::fs::create_dir_all(&pillar).expect("pillar root");
    let catalogue = unique_path("unreadable-cat", "toml");
    std::fs::write(&catalogue, "[[tenants]]\nid = \"acme-prod\"\n").expect("write catalogue");
    let missing_secret = unique_path("NOTHERE-secret", "key");
    // Deliberately do NOT create `missing_secret`.

    let (code, stderr) = run_bounded(
        {
            let mut cmd = log_query_api_bin();
            cmd.env("KALEIDOSCOPE_PILLAR_ROOT", &pillar)
                .env("KALEIDOSCOPE_LOG_QUERY_TENANT", "acme-prod")
                .env("KALEIDOSCOPE_LOG_QUERY_ADDR", "127.0.0.1:0")
                .env("KALEIDOSCOPE_LOG_QUERY_AUTH_ISSUER", "acme-observability")
                .env("KALEIDOSCOPE_LOG_QUERY_AUTH_AUDIENCE", "kaleidoscope-query")
                .env("KALEIDOSCOPE_LOG_QUERY_AUTH_SECRET_FILE", &missing_secret)
                .env("KALEIDOSCOPE_LOG_QUERY_AUTH_CATALOGUE", &catalogue);
            cmd
        },
        Duration::from_secs(8),
        None,
    );
    let _ = std::fs::remove_dir_all(&pillar);
    let _ = std::fs::remove_file(&catalogue);

    assert_eq!(
        code,
        Some(2),
        "an unreadable secret_file must exit 2; stderr:\n{stderr}"
    );
    assert!(
        stderr_has_event(&stderr, CONFIG_REFUSAL_EVENT),
        "stderr must carry event={CONFIG_REFUSAL_EVENT}; got:\n{stderr}"
    );
    let reason = event_reason(&stderr, CONFIG_REFUSAL_EVENT).unwrap_or_default();
    assert!(
        reason.contains("secret_file") || reason.contains(&missing_secret.display().to_string()),
        "the refusal must name the unreadable secret source by reference; reason: {reason:?}"
    );
    assert!(
        !stderr.contains(SENTINEL_SECRET),
        "no secret bytes may appear on stderr; got:\n{stderr}"
    );
}

// =========================================================================
// US-RAUTH-01 — complete config STARTS and binds WITHOUT an env tenant
// (the positive control + the auth startup negative probe passes)
// =========================================================================

/// A complete, readable read-auth config starts the binary in auth mode and
/// binds the listener EVEN WITH THE ENV TENANT UNSET — because the per-request
/// tenant comes from the bearer (DD3 arm 1), so auth-on does not require an env
/// tenant. Neither refusal event appears.
#[test]
fn complete_auth_config_starts_and_binds_without_an_env_tenant() {
    let pillar = unique_path("complete-pillar", "dir");
    std::fs::create_dir_all(&pillar).expect("pillar root");
    let catalogue = unique_path("complete-cat", "toml");
    std::fs::write(&catalogue, "[[tenants]]\nid = \"acme-prod\"\n").expect("write catalogue");
    let secret = unique_path("complete-secret", "key");
    std::fs::write(&secret, SENTINEL_SECRET).expect("write secret");

    let (_, stderr) = run_bounded(
        {
            let mut cmd = log_query_api_bin();
            cmd.env("KALEIDOSCOPE_PILLAR_ROOT", &pillar)
                .env_remove("KALEIDOSCOPE_LOG_QUERY_TENANT")
                .env("KALEIDOSCOPE_LOG_QUERY_ADDR", "127.0.0.1:0")
                .env("KALEIDOSCOPE_LOG_QUERY_AUTH_ISSUER", "acme-observability")
                .env("KALEIDOSCOPE_LOG_QUERY_AUTH_AUDIENCE", "kaleidoscope-query")
                .env("KALEIDOSCOPE_LOG_QUERY_AUTH_SECRET_FILE", &secret)
                .env("KALEIDOSCOPE_LOG_QUERY_AUTH_CATALOGUE", &catalogue);
            cmd
        },
        Duration::from_secs(10),
        Some(BOUND_EVENT),
    );
    let _ = std::fs::remove_dir_all(&pillar);
    let _ = std::fs::remove_file(&catalogue);
    let _ = std::fs::remove_file(&secret);

    assert!(
        stderr_has_event(&stderr, BOUND_EVENT),
        "a complete read-auth config must start auth mode and bind even with the env tenant unset; stderr:\n{stderr}"
    );
    assert!(
        !stderr_has_event(&stderr, CONFIG_REFUSAL_EVENT),
        "a complete config must NOT emit {CONFIG_REFUSAL_EVENT}; stderr:\n{stderr}"
    );
    assert!(
        !stderr_has_event(&stderr, PROBE_REFUSAL_EVENT),
        "the auth startup negative probe must pass (no {PROBE_REFUSAL_EVENT}); stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains(SENTINEL_SECRET),
        "the secret bytes must never appear on stderr; got:\n{stderr}"
    );
}

// =========================================================================
// US-RAUTH-02 — absent auth config starts in env-tenant mode (backward compat)
// (GUARDRAIL — green before AND after DELIVER)
// =========================================================================

/// With NO read-auth env vars set and an env tenant, the binary starts in
/// today's env-tenant mode and binds — the additive opt-out is byte-for-byte
/// unchanged, and the new refuse-to-start path never fires.
#[test]
fn absent_auth_config_starts_in_env_tenant_mode() {
    let pillar = unique_path("absent-pillar", "dir");
    std::fs::create_dir_all(&pillar).expect("pillar root");

    let (_, stderr) = run_bounded(
        {
            let mut cmd = log_query_api_bin();
            cmd.env("KALEIDOSCOPE_PILLAR_ROOT", &pillar)
                .env("KALEIDOSCOPE_LOG_QUERY_TENANT", "acme-prod")
                .env("KALEIDOSCOPE_LOG_QUERY_ADDR", "127.0.0.1:0")
                .env_remove("KALEIDOSCOPE_LOG_QUERY_AUTH_ISSUER")
                .env_remove("KALEIDOSCOPE_LOG_QUERY_AUTH_AUDIENCE")
                .env_remove("KALEIDOSCOPE_LOG_QUERY_AUTH_SECRET_FILE")
                .env_remove("KALEIDOSCOPE_LOG_QUERY_AUTH_CATALOGUE");
            cmd
        },
        Duration::from_secs(10),
        Some(BOUND_EVENT),
    );
    let _ = std::fs::remove_dir_all(&pillar);

    assert!(
        stderr_has_event(&stderr, BOUND_EVENT),
        "an absent read-auth config must start in env-tenant mode and bind; stderr:\n{stderr}"
    );
    assert!(
        !stderr_has_event(&stderr, CONFIG_REFUSAL_EVENT),
        "an absent config must NOT emit {CONFIG_REFUSAL_EVENT} (additive opt-out); stderr:\n{stderr}"
    );
}
