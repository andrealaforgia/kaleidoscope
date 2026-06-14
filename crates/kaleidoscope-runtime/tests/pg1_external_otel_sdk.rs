// Kaleidoscope consolidated runtime — PG-1: a real external OTel SDK app
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

//! # PG-1 — "a third party's official OpenTelemetry SDK app just works".
//!
//! The NORTH STAR is interoperability with the wider ecosystem: an app that
//! depends on NOTHING of ours — only the official `opentelemetry`,
//! `opentelemetry.sdk` and `opentelemetry.exporter.otlp.*` Python packages —
//! emits ONE trace (a parent span and a child span) over real OTLP/HTTP to a
//! RUNNING consolidated runtime, and that trace becomes faithfully QUERYABLE,
//! down to the exact nanosecond timestamps the SDK recorded.
//!
//! ## Test architecture (mirrors `kaleidoscope-telemetrygen`'s slice 2)
//!
//! - The suite REUSES the C1 composition root (`kaleidoscope_runtime::
//!   spawn_consolidated`) to stand up a live consolidated runtime IN THE TEST
//!   PROCESS on EPHEMERAL `127.0.0.1:0` ports — NEVER the fixed
//!   4317/4318/9090/9091/9092 defaults (the fixed-port flake, project memory
//!   `aperture_fixed_port_4317_flake`). The actual bound ports are read back
//!   from the `RunningRuntime`.
//! - The external app is driven through its TRUE driving port: the readable
//!   demo `examples/otel-external-demo/app.py` run as a SUBPROCESS inside a
//!   FRESH, self-bootstrapped virtualenv whose ONLY installed dependencies are
//!   the two pinned official OpenTelemetry packages. The app is pointed at the
//!   runtime's bound ingest HTTP port via `OTEL_EXPORTER_OTLP_ENDPOINT` and
//!   files its spans under tenant "acme" via `KALEIDOSCOPE_TENANT`. This is
//!   real I/O: real venv, real third-party SDK, real OTLP/HTTP wire, real live
//!   store (`@real-io`).
//! - The "see" half GETs the runtime's traces query router over loopback and
//!   asserts the BUSINESS OUTCOME (the parent/child spans, their linkage, the
//!   child's span-level attribute, and the EXACT nanos round-tripping), never
//!   transport details.
//!
//! ## Falsifiability (RED before GREEN)
//!
//! Before `app.py` emits any telemetry, the subprocess prints nothing on
//! stdout, so the single JSON handshake line fails to parse and the test
//! PANICS — a genuine RED, never a false green. The timing assertions assert
//! EXACT `u64` nano equality between what the SDK printed and what the gateway
//! returned: that is only green if the SDK's recorded nanos truly round-trip
//! through the real OTLP wire and store. No fudge is possible.
//!
//! ## Graceful degradation (a LOUD skip, never a silent green)
//!
//! If `python3` is absent, or venv creation fails, or the pinned packages
//! cannot be installed (e.g. offline), the test prints a clear
//! `PG-1 SKIPPED: <reason>` on stderr and returns early. A skip is loud; it is
//! never reported as a pass of the business assertions.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use kaleidoscope_runtime::{spawn_consolidated, ConsolidatedConfig, RunningRuntime};

// =========================================================================
// Vocabulary
// =========================================================================

/// The single local-experiment tenant the spans are filed under.
const TENANT_ACME: &str = "acme";
/// The span-level attribute the app stamps on the CHILD span (NOT a resource
/// attribute) and that the retrieved child span must carry back.
const CUSTOMER_ID_KEY: &str = "customer.id";
const CUSTOMER_ID_VALUE: &str = "bea-test";
/// The unique log body the (bonus) external log carries, used to find it in
/// the logs query result.
const LOG_BODY: &str = "pg1 external sdk log inside span";

/// How long the "see" half polls for telemetry to appear (the live loop
/// tolerates async accept + batch flush).
const SEE_TIMEOUT: Duration = Duration::from_secs(10);
/// The query routers parse start/end as epoch SECONDS and cap the window at
/// `query_http_common::MAX_WINDOW_SECONDS` (86 400 s). A +/- 1 h bracket round
/// "now" contains the app's just-stamped telemetry while staying within the cap.
const WINDOW_HALF_SPAN_SECS: u64 = 3_600;

// =========================================================================
// Runtime lifecycle (REUSE the C1 composition root on EPHEMERAL ports)
// =========================================================================

/// A live consolidated runtime plus the pillar root it owns (kept alive so the
/// temp dir is not reclaimed mid-test).
struct TestRuntime {
    runtime: RunningRuntime,
    _pillar_root: PathBuf,
}

fn fresh_pillar_root(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let pid = std::process::id();
    let mut path = std::env::temp_dir();
    path.push(format!("kal-pg1-{label}-{pid}-{nanos}"));
    std::fs::create_dir_all(&path).expect("mkdir pillar root");
    path
}

/// Spawn a consolidated runtime on EPHEMERAL `127.0.0.1:0` ports whose four
/// tenant roles are all `tenant`, with a fresh empty pillar root.
async fn spawn_runtime(label: &str, tenant: &str) -> TestRuntime {
    let pillar_root = fresh_pillar_root(label);
    let config = ConsolidatedConfig::for_ephemeral_test(pillar_root.clone(), tenant);
    let runtime = spawn_consolidated(config)
        .await
        .expect("consolidated runtime spawns on ephemeral ports");
    TestRuntime {
        runtime,
        _pillar_root: pillar_root,
    }
}

// =========================================================================
// The external demo app dir + its self-bootstrapped virtualenv
// =========================================================================

/// Absolute path to `examples/otel-external-demo`, resolved from this crate's
/// manifest dir (`crates/kaleidoscope-runtime`) up two levels.
fn app_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dir = manifest
        .join("..")
        .join("..")
        .join("examples")
        .join("otel-external-demo");
    dir.canonicalize().unwrap_or(dir)
}

/// The interpreter to bootstrap the venv from: `PG1_PYTHON3` override else
/// `python3`.
fn host_python3() -> String {
    std::env::var("PG1_PYTHON3").unwrap_or_else(|_| "python3".to_string())
}

/// Emit a LOUD skip line on stderr and signal early-return to the caller.
fn skip(reason: &str) -> Option<PathBuf> {
    eprintln!("PG-1 SKIPPED: {reason}");
    None
}

/// Create a fresh virtualenv in a temp dir and install ONLY the two pinned
/// official OpenTelemetry packages from `requirements.txt`. Returns the path to
/// the venv's `python` interpreter, or `None` (after a loud skip) when the host
/// has no usable `python3`, venv creation fails, or the install fails (offline).
async fn build_venv() -> Option<PathBuf> {
    let python3 = host_python3();

    // 1. Probe the host interpreter.
    let probe = tokio::process::Command::new(&python3)
        .arg("--version")
        .output()
        .await;
    match probe {
        Ok(out) if out.status.success() => {}
        _ => return skip(&format!("host '{python3}' is absent or not runnable")),
    }

    // 2. Create the venv in a fresh temp dir (leaked: kept alive for the test).
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let mut venv_dir = std::env::temp_dir();
    venv_dir.push(format!("kal-pg1-venv-{}-{}", std::process::id(), nanos));
    let venv_out = tokio::process::Command::new(&python3)
        .arg("-m")
        .arg("venv")
        .arg(&venv_dir)
        .output()
        .await;
    match venv_out {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            return skip(&format!(
                "venv creation failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ))
        }
        Err(e) => return skip(&format!("venv creation could not run: {e}")),
    }

    let venv_python = venv_dir.join("bin").join("python");

    // 3. Install the pinned official OTel packages (and nothing else).
    let requirements = app_dir().join("requirements.txt");
    let pip_out = tokio::process::Command::new(&venv_python)
        .arg("-m")
        .arg("pip")
        .arg("install")
        .arg("--quiet")
        .arg("--disable-pip-version-check")
        .arg("-r")
        .arg(&requirements)
        .output()
        .await;
    match pip_out {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            return skip(&format!(
                "pip install of the pinned OTel packages failed (offline?): {}",
                String::from_utf8_lossy(&out.stderr)
            ))
        }
        Err(e) => return skip(&format!("pip could not run: {e}")),
    }

    Some(venv_python)
}

/// Run `app.py` once as a subprocess inside the bootstrapped venv, pointed at
/// `ingest_http` for `tenant`. Real subprocess, real third-party SDK, real
/// OTLP/HTTP wire.
async fn run_app(venv_python: &Path, ingest_http: SocketAddr, tenant: &str) -> Output {
    tokio::process::Command::new(venv_python)
        .arg(app_dir().join("app.py"))
        .env(
            "OTEL_EXPORTER_OTLP_ENDPOINT",
            format!("http://{ingest_http}"),
        )
        .env("KALEIDOSCOPE_TENANT", tenant)
        .output()
        .await
        .expect("run the external otel-external-demo app.py")
}

/// The handshake the app prints on stdout: the exact ids and nanos the SDK
/// recorded, so the test can assert an EXACT round-trip.
struct DemoHandshake {
    trace_id: String,
    parent_span_id: String,
    parent_start: u64,
    parent_end: u64,
    child_span_id: String,
    child_start: u64,
    child_end: u64,
}

fn parse_handshake(stdout: &str) -> DemoHandshake {
    // The app prints exactly one parseable JSON line on stdout (human notes go
    // to stderr). RED discriminator: before the app emits, this is empty and
    // the parse PANICS — never a false green.
    let line = stdout
        .lines()
        .find(|l| l.trim_start().starts_with('{'))
        .unwrap_or_else(|| {
            panic!("app must print one JSON handshake line on stdout; got: {stdout:?}")
        });
    let v: serde_json::Value =
        serde_json::from_str(line).expect("the app's stdout handshake line parses as JSON");
    let hex = |x: &serde_json::Value| x.as_str().expect("hex id is a string").to_string();
    let nano = |x: &serde_json::Value| x.as_u64().expect("nano is an unsigned integer");
    DemoHandshake {
        trace_id: hex(&v["trace_id"]),
        parent_span_id: hex(&v["parent"]["span_id"]),
        parent_start: nano(&v["parent"]["start_unix_nano"]),
        parent_end: nano(&v["parent"]["end_unix_nano"]),
        child_span_id: hex(&v["child"]["span_id"]),
        child_start: nano(&v["child"]["start_unix_nano"]),
        child_end: nano(&v["child"]["end_unix_nano"]),
    }
}

// =========================================================================
// The "see" half — GET the query routers, assert business outcomes
// =========================================================================

async fn get(url: &str) -> (u16, String) {
    let resp = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .expect("GET query endpoint over loopback");
    let status = resp.status().as_u16();
    let body = resp.text().await.expect("read query response body");
    (status, body)
}

async fn trace_by_id(addr: SocketAddr, trace_id: &str) -> (u16, String) {
    get(&format!(
        "http://{addr}/api/v1/traces/by_id?trace_id={trace_id}"
    ))
    .await
}

fn query_window() -> (u64, u64) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    (
        now.saturating_sub(WINDOW_HALF_SPAN_SECS),
        now + WINDOW_HALF_SPAN_SECS,
    )
}

async fn logs_query(addr: SocketAddr) -> (u16, String) {
    let (start, end) = query_window();
    get(&format!(
        "http://{addr}/api/v1/logs?start={start}&end={end}"
    ))
    .await
}

fn as_array(body: &str) -> Vec<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
}

/// Poll `f` until `done` holds or `timeout` elapses. Returns the final
/// `(status, body)`.
async fn poll_until<F, Fut>(
    timeout: Duration,
    mut f: F,
    done: impl Fn(u16, &str) -> bool,
) -> (u16, String)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = (u16, String)>,
{
    let start = Instant::now();
    loop {
        let (status, body) = f().await;
        if done(status, &body) || start.elapsed() >= timeout {
            return (status, body);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

/// `customer.id` -style accessor: the serialised `Span` carries
/// `attributes` as a string->string object.
fn span_attr<'a>(span: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    span["attributes"][key].as_str()
}

fn span_field_str<'a>(span: &'a serde_json::Value, key: &str) -> &'a str {
    span[key].as_str().unwrap_or("")
}

fn span_field_u64(span: &serde_json::Value, key: &str) -> u64 {
    span[key].as_u64().unwrap_or(0)
}

// =========================================================================
// Criterion 1 — the "official SDK only" structural guard
// =========================================================================

/// The whole external demo dir must depend on NOTHING first-party. The
/// criterion's primary definition is "ZERO imports of anything named
/// 'kaleidoscope' (or any of our crate names, all prefixed `kaleidoscope`); no
/// dependency on our code whatsoever". So:
///
///   * `requirements.txt` must not mention "kaleidoscope" at all (it pins ONLY
///     the official OpenTelemetry packages) -- a blanket case-insensitive guard.
///   * No `import` / `from ... import` line in `app.py` may reference
///     "kaleidoscope" -- this is the actual "no first-party code" guarantee.
///     (The `KALEIDOSCOPE_TENANT` env-var NAME the app reads is vendor input
///     data, not a code dependency, so it is permitted; the gateway only ever
///     sees the vendor-neutral `tenant.id` resource attribute the app sets.)
///   * `app.py` MUST import from `opentelemetry` -- positive evidence that it
///     uses the official SDK and only the official SDK.
///
/// This proves the app uses ONLY the official OpenTelemetry SDK.
fn assert_official_sdk_only() {
    let requirements = std::fs::read_to_string(app_dir().join("requirements.txt"))
        .expect("read requirements.txt for the structural guard");
    assert!(
        !requirements.to_lowercase().contains("kaleidoscope"),
        "criterion 1 (official SDK only): requirements.txt must pin only official \
         OpenTelemetry packages, never any first-party 'kaleidoscope' package"
    );

    let app = std::fs::read_to_string(app_dir().join("app.py"))
        .expect("read app.py for the structural guard");
    let mut imports_opentelemetry = false;
    for line in app.lines() {
        let trimmed = line.trim_start();
        let is_import = trimmed.starts_with("import ") || trimmed.starts_with("from ");
        if !is_import {
            continue;
        }
        assert!(
            !trimmed.to_lowercase().contains("kaleidoscope"),
            "criterion 1 (official SDK only): app.py must not import any first-party \
             'kaleidoscope' code; offending import: {trimmed}"
        );
        if trimmed.contains("opentelemetry") {
            imports_opentelemetry = true;
        }
    }
    assert!(
        imports_opentelemetry,
        "criterion 1 (official SDK only): app.py must import from the official \
         'opentelemetry' SDK"
    );
}

// =========================================================================
// PG-1 CORE — the external SDK trace round-trips faithfully
// @real-io @driving_port @adapter-integration @PG-1
// =========================================================================

/// PG-1 core: an external app built on ONLY the official OpenTelemetry SDK
/// emits one parent+child trace that round-trips faithfully through the
/// consolidated runtime.
///
/// ```gherkin
/// @real-io @driving_port @PG-1
/// Scenario: An external official-SDK app's trace is faithfully queryable
///   Given a consolidated runtime is running for tenant "acme"
///   And an app that depends only on the official OpenTelemetry SDK
///   When the app emits one trace with a parent span and a child span
///   Then the trace is retrievable by its trace id with both spans
///   And exactly one span is the root and its id matches the app's parent
///   And the child's parent is the root and its id matches the app's child
///   And the child carries the span-level attribute customer.id = "bea-test"
///   And every span's start/end nanos exactly equal the SDK-recorded nanos
/// ```
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_official_sdk_trace_round_trips_faithfully() {
    // Criterion 1: the structural "official SDK only" guard (no first-party dep).
    assert_official_sdk_only();

    let Some(venv) = build_venv().await else {
        return; // a loud skip already printed; never a silent green.
    };

    let rt = spawn_runtime("trace", TENANT_ACME).await;
    let out = run_app(&venv, rt.runtime.ingest_http_addr, TENANT_ACME).await;
    assert!(
        out.status.success(),
        "the external app exits cleanly against a running stack; stderr: {}",
        stderr_of(&out)
    );

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let demo = parse_handshake(&stdout);

    // Criterion 4 (retrieval): the trace is queryable by id; poll for both spans.
    let (status, body) = poll_until(
        SEE_TIMEOUT,
        || trace_by_id(rt.runtime.traces_query_addr, &demo.trace_id),
        |s, b| s == 200 && as_array(b).len() >= 2,
    )
    .await;
    assert_eq!(status, 200, "by-id query answers 200; body: {body}");
    let spans = as_array(&body);
    assert!(
        spans.len() >= 2,
        "both the parent and the child span come back live; body: {body}"
    );

    // Criterion 2 (linkage): EXACTLY one root (parent_span_id == null), and it
    // is the app's parent.
    let roots: Vec<&serde_json::Value> = spans
        .iter()
        .filter(|s| s["parent_span_id"].is_null())
        .collect();
    assert_eq!(
        roots.len(),
        1,
        "exactly one span is the trace root; body: {body}"
    );
    let root = roots[0];
    assert_eq!(
        span_field_str(root, "span_id"),
        demo.parent_span_id,
        "the root's span id matches the app's parent span id; body: {body}"
    );

    // The child is the span whose parent is the root; its id matches the app's
    // child.
    let child = spans
        .iter()
        .find(|s| span_field_str(s, "parent_span_id") == demo.parent_span_id)
        .unwrap_or_else(|| panic!("a span whose parent is the root must exist; body: {body}"));
    assert_eq!(
        span_field_str(child, "span_id"),
        demo.child_span_id,
        "the child's span id matches the app's child span id; body: {body}"
    );

    // Criterion 3 (attribute): the child carries the SPAN-level customer.id.
    assert_eq!(
        span_attr(child, CUSTOMER_ID_KEY),
        Some(CUSTOMER_ID_VALUE),
        "the child span carries the span-level {CUSTOMER_ID_KEY} = {CUSTOMER_ID_VALUE}; body: {body}"
    );

    // Criterion 4 (timing): the retrieved nanos EXACTLY equal the SDK-recorded
    // nanos (no fuzz), and the window encloses the child.
    assert_eq!(
        span_field_u64(root, "start_time_unix_nano"),
        demo.parent_start,
        "the root start nano round-trips exactly; body: {body}"
    );
    assert_eq!(
        span_field_u64(root, "end_time_unix_nano"),
        demo.parent_end,
        "the root end nano round-trips exactly; body: {body}"
    );
    assert_eq!(
        span_field_u64(child, "start_time_unix_nano"),
        demo.child_start,
        "the child start nano round-trips exactly; body: {body}"
    );
    assert_eq!(
        span_field_u64(child, "end_time_unix_nano"),
        demo.child_end,
        "the child end nano round-trips exactly; body: {body}"
    );
    assert!(
        demo.parent_start <= demo.child_start
            && demo.child_start <= demo.child_end
            && demo.child_end <= demo.parent_end,
        "the parent window encloses the child: \
         parent.start={} child.start={} child.end={} parent.end={}",
        demo.parent_start,
        demo.child_start,
        demo.child_end,
        demo.parent_end
    );
}

// =========================================================================
// PG-1 BONUS (PG-2 seed) — the external SDK log inside the span is queryable
// @real-io @driving_port @PG-1 @bonus
// =========================================================================

/// PG-1 bonus: the SAME external app also emits one log record inside the
/// child's active span context; that log's unique body is retrievable from the
/// logs query router. Kept as its OWN test so a bonus flake never reds the core
/// PG-1 round-trip.
///
/// ```gherkin
/// @real-io @driving_port @PG-1 @bonus
/// Scenario: An external official-SDK app's in-span log is queryable
///   Given a consolidated runtime is running for tenant "acme"
///   When the app emits a log record inside the child span's context
///   Then a logs query returns the log's unique body
/// ```
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_official_sdk_log_inside_span_is_queryable() {
    let Some(venv) = build_venv().await else {
        return; // a loud skip already printed; never a silent green.
    };

    let rt = spawn_runtime("log", TENANT_ACME).await;
    let out = run_app(&venv, rt.runtime.ingest_http_addr, TENANT_ACME).await;
    assert!(
        out.status.success(),
        "the external app exits cleanly against a running stack; stderr: {}",
        stderr_of(&out)
    );

    // If the bonus log was skipped in app.py, there is nothing to retrieve;
    // surface that as a loud skip rather than a misleading red.
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let demo = parse_handshake(&stdout);

    let (status, body) = poll_until(
        SEE_TIMEOUT,
        || logs_query(rt.runtime.logs_query_addr),
        |s, b| s == 200 && b.contains(LOG_BODY),
    )
    .await;
    assert_eq!(status, 200, "logs query answers 200; body: {body}");
    assert!(
        body.contains(LOG_BODY),
        "the external in-span log body \"{LOG_BODY}\" is retrievable; body: {body}"
    );

    // If the lumen log JSON exposes a hex trace_id string, it must match the
    // app's trace id. (lumen serialises trace_id as a byte array, not a hex
    // string, so this is a best-effort cross-check, not the primary assertion.)
    if let Some(rec) = as_array(&body)
        .into_iter()
        .find(|r| r["body"].as_str() == Some(LOG_BODY))
    {
        if let Some(tid) = rec["trace_id"].as_str() {
            assert_eq!(
                tid, demo.trace_id,
                "the in-span log carries the app's trace id; record: {rec}"
            );
        }
    }
}
