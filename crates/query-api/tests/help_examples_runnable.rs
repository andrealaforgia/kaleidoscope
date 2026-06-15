// Kaleidoscope query-api — HELPRUN: the /help examples are runnable verbatim
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

//! HELPRUN — the getting-started `/help` examples are usable COLD.
//!
//! Observable contract: on a running, demo-seeded consolidated stack
//! (all three signals present via the `kaleidoscope-telemetrygen`
//! generator, exactly what `make demo` runs), starting ONLY from the text
//! of `GET /help` on the metrics query port, EACH printed example — run
//! VERBATIM, its exact path/params/window, nothing rewritten except the
//! host:port — returns that signal's data as JSON:
//!
//! - the metrics example -> metric data (>= 1 series);
//! - the logs example -> log data (>= 1 record);
//! - the traces-by-service-window example -> trace data (>= 1 trace);
//! - the error-find traces example (`error=true`) -> only the failed
//!   trace's spans, each carrying Error status (>= 1);
//! - the single-trace-by-id example -> that trace's spans (>= 1);
//! - and NO example returns a success-looking HTML page (a 200 rendering
//!   the Prism dashboard instead of the signal's data).
//!
//! ## Test architecture
//!
//! - REUSES the C1/C3 composition root
//!   (`kaleidoscope_runtime::spawn_consolidated`) to stand up a LIVE
//!   consolidated runtime IN THE TEST PROCESS on EPHEMERAL `127.0.0.1:0`
//!   ports — never the fixed 9090/9091/9092 defaults (the fixed-port
//!   flake). The actual bound ports are read back from `RunningRuntime`.
//!   A static Prism bundle IS mounted on the metrics router so the
//!   wrong-port failure mode is the REAL one make-up exhibits: a
//!   misaddressed read renders the dashboard HTML (a 200), not a 404.
//! - SEEDS via the COMPILED generator binary run as a SUBPROCESS (the same
//!   way `make demo` does). A subprocess is required: the generator's
//!   `spark::init` installs a process-global tracing subscriber to bridge
//!   the demo log to OTLP, which an in-process call could not install
//!   (the runtime already owns the process subscriber).
//! - Drives each example through the TRUE getting-started surface: it
//!   GETs `/help`, extracts the printed example commands, substitutes the
//!   ephemeral host:port for the printed fixed `localhost:PORT`, and runs
//!   each command VERBATIM via `sh -c` (so the `NOW=$(date +%s)` relative
//!   window and the per-signal port are exercised exactly as a user runs
//!   them). Real subprocess, real OTLP wire, real live store.
//!
//! ## Falsifiability (RED before / GREEN after)
//!
//! The fix this suite guards makes the examples (a) reach EACH signal's
//! own port and (b) use demo-matching values + a covering, within-cap
//! window. Before it, the metrics example names a metric/window the seed
//! does not match (0 series), and the logs/traces/by-id examples address
//! the metrics port, so they render the dashboard HTML instead of data.
//! Either way the per-example data assertions FAIL. A wrong port, a
//! non-covering window, or a wrong metric name re-breaks it — this is not
//! a tautology.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use kaleidoscope_runtime::{spawn_consolidated, ConsolidatedConfig, RunningRuntime};

/// The single local-experiment tenant the make-up stack runs under
/// (`KALEIDOSCOPE_TENANT=acme`, auth off, env-tenant mode).
const TENANT: &str = "acme";
/// The `service.name` the generator files the sample telemetry under.
const DEMO_SERVICE: &str = "kaleidoscope-demo";
/// The demo trace id the generator pins the sample span to.
const TRACE_ID_HEX: &str = "4bf92f3577b34da6a3ce929d0e0e4736";

/// How long the "see" half polls for the seeded telemetry to become
/// queryable (the live loop tolerates async ingest accept + batch flush).
const SEE_TIMEOUT: Duration = Duration::from_secs(15);

/// A marker-carrying SPA index document, so a misaddressed example that
/// falls through to the dashboard is observably HTML (not signal data).
const INDEX_HTML_BODY: &str =
    "<!doctype html><html><head><title>Prism</title></head><body><div id=\"root\"></div></body></html>";

/// A live consolidated runtime plus the temp dirs it owns (kept alive so
/// neither the pillar root nor the static bundle is reclaimed mid-test).
struct TestRuntime {
    runtime: RunningRuntime,
    _pillar_root: PathBuf,
    _bundle: PathBuf,
}

fn unique_tempdir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let mut path = std::env::temp_dir();
    path.push(format!(
        "query-api-helprun-{label}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).expect("create tempdir");
    path
}

/// Lay down a static bundle whose `index.html` carries [`INDEX_HTML_BODY`].
fn static_bundle(label: &str) -> PathBuf {
    let dir = unique_tempdir(label);
    std::fs::write(dir.join("index.html"), INDEX_HTML_BODY).expect("write index.html");
    dir
}

/// Spawn a consolidated runtime on EPHEMERAL `127.0.0.1:0` ports for
/// `TENANT`, with the Prism bundle mounted on the metrics router so a
/// misaddressed read renders the dashboard HTML (the make-up failure mode).
async fn spawn(label: &str) -> TestRuntime {
    let pillar_root = unique_tempdir(&format!("{label}-root"));
    let bundle = static_bundle(&format!("{label}-bundle"));
    let mut config = ConsolidatedConfig::for_ephemeral_test(pillar_root.clone(), TENANT);
    config.static_dir = Some(bundle.clone());
    let runtime = spawn_consolidated(config)
        .await
        .expect("consolidated runtime spawns on ephemeral ports");
    TestRuntime {
        runtime,
        _pillar_root: pillar_root,
        _bundle: bundle,
    }
}

/// Locate the compiled `kaleidoscope-telemetrygen` binary next to the test
/// binary (`target/<profile>/kaleidoscope-telemetrygen`). `CARGO_BIN_EXE_*`
/// is only exported to the binary's OWN crate, so this cross-crate suite
/// derives the path from `current_exe` instead. The gate runs
/// `cargo build --workspace` before the tests, so the binary is present; a
/// clear panic (not a silent skip) fires if it is not.
fn telemetrygen_bin() -> PathBuf {
    let mut dir = std::env::current_exe().expect("current_exe");
    dir.pop(); // drop the test binary file name -> .../deps
    if dir.ends_with("deps") {
        dir.pop(); // -> .../<profile>, where the workspace binaries live
    }
    let bin = dir.join(format!(
        "kaleidoscope-telemetrygen{}",
        std::env::consts::EXE_SUFFIX
    ));
    assert!(
        bin.exists(),
        "the kaleidoscope-telemetrygen binary was not found at {bin:?}; \
         run `cargo build --workspace` before this suite"
    );
    bin
}

/// Seed the running runtime by running the compiled generator once against
/// its ingest gRPC port, exactly as `make demo` does.
async fn seed(grpc_addr: SocketAddr) {
    let out = tokio::process::Command::new(telemetrygen_bin())
        .env("OTEL_EXPORTER_OTLP_ENDPOINT", format!("http://{grpc_addr}"))
        .env("KALEIDOSCOPE_TENANT", TENANT)
        .env("OTEL_SERVICE_NAME", DEMO_SERVICE)
        .output()
        .await
        .expect("run the kaleidoscope-telemetrygen binary");
    assert!(
        out.status.success(),
        "the generator must seed cleanly; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// GET `/help` from the metrics query port and return its body verbatim.
async fn fetch_help(metrics_addr: SocketAddr) -> String {
    reqwest::Client::new()
        .get(format!("http://{metrics_addr}/help"))
        .send()
        .await
        .expect("GET /help over loopback")
        .text()
        .await
        .expect("read /help body")
}

/// Extract the runnable example commands from the `/help` body: every line
/// whose trimmed form carries a `curl ` invocation.
fn example_commands(help: &str) -> Vec<String> {
    help.lines()
        .map(str::trim)
        .filter(|line| line.contains("curl "))
        .map(str::to_owned)
        .collect()
}

/// Rewrite ONLY the printed fixed `localhost:PORT` authorities to this
/// runtime's ephemeral signal ports — path, params and window are kept
/// VERBATIM. The printed metrics/logs/traces ports (9090/9091/9092) map to
/// the corresponding bound addresses; nothing else is touched.
fn substitute_ports(command: &str, rt: &RunningRuntime) -> String {
    command
        .replace("localhost:9090", &rt.metrics_query_addr.to_string())
        .replace("localhost:9091", &rt.logs_query_addr.to_string())
        .replace("localhost:9092", &rt.traces_query_addr.to_string())
}

/// Run a printed example command VERBATIM through `sh -c` (so its
/// `NOW=$(date +%s)` relative window resolves exactly as a user's would)
/// and return curl's stdout (the response body).
async fn run_example(command: &str) -> String {
    let out = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .await
        .expect("run the /help example via sh");
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Poll `run_example` until `done` holds or [`SEE_TIMEOUT`] elapses,
/// returning the final body.
async fn poll_example(command: &str, done: impl Fn(&str) -> bool) -> String {
    let start = Instant::now();
    loop {
        let body = run_example(command).await;
        if done(&body) || start.elapsed() >= SEE_TIMEOUT {
            return body;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// True when a `query_range` body reports `status: success` with >= 1 series.
fn metrics_has_series(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .map(|v| {
            v["status"] == "success"
                && v["data"]["result"]
                    .as_array()
                    .map(|a| !a.is_empty())
                    .unwrap_or(false)
        })
        .unwrap_or(false)
}

/// The length of a bare-JSON-array body (logs records, traces, or spans).
fn array_len(body: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.as_array().map(|a| a.len()))
        .unwrap_or(0)
}

/// Assert a response body is NOT an HTML page (the misaddressed-to-the-SPA
/// failure mode): no example may answer with the dashboard document.
fn assert_not_html(body: &str, command: &str) {
    let lower = body.to_ascii_lowercase();
    assert!(
        !lower.contains("<!doctype") && !lower.contains("<html"),
        "an example returned an HTML page, not signal data; command: {command}; body: {body}"
    );
}

/// HELPRUN — the printed `/help` examples, run verbatim against a
/// demo-seeded stack, each return that signal's data as JSON (not HTML).
///
/// ```gherkin
/// @driving_port @real-io @HELPRUN
/// Scenario: The getting-started help is usable cold
///   Given a demo-seeded consolidated runtime (all three signals present)
///   When each example printed by GET /help is run verbatim
///   Then the metrics example returns >= 1 series
///   And the logs example returns >= 1 record
///   And the traces service-window example returns >= 1 trace
///   And the single-trace-by-id example returns that trace's spans
///   And no example returns a dashboard HTML page
/// ```
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn help_examples_run_verbatim_and_return_each_signals_data() {
    let rt = spawn("helprun").await;
    seed(rt.runtime.ingest_grpc_addr).await;

    let help = fetch_help(rt.runtime.metrics_query_addr).await;
    let commands = example_commands(&help);
    assert_eq!(
        commands.len(),
        5,
        "GET /help must list five runnable examples; got:\n{help}"
    );

    let (mut saw_metrics, mut saw_logs, mut saw_traces, mut saw_error_traces, mut saw_by_id) =
        (false, false, false, false, false);

    for command in &commands {
        let runnable = substitute_ports(command, &rt.runtime);

        if runnable.contains("/api/v1/query_range") {
            saw_metrics = true;
            let body = poll_example(&runnable, metrics_has_series).await;
            assert!(
                metrics_has_series(&body),
                "the metrics example must return >= 1 series; command: {command}; body: {body}"
            );
            assert_not_html(&body, command);
        } else if runnable.contains("/api/v1/traces/by_id") {
            saw_by_id = true;
            let body =
                poll_example(&runnable, |b| array_len(b) >= 1 && b.contains(TRACE_ID_HEX)).await;
            assert!(
                array_len(&body) >= 1 && body.contains(TRACE_ID_HEX),
                "the by-id example must return the demo trace's spans; command: {command}; body: {body}"
            );
            assert_not_html(&body, command);
        } else if runnable.contains("/api/v1/traces") && runnable.contains("error=true") {
            // The error-find example: run VERBATIM it must surface the demo's
            // failed trace's spans, EACH carrying Error status — proving a
            // newcomer who copy-pastes it actually reaches the failures, and
            // that the advertised filter is honest (non-vacuous on the seed).
            saw_error_traces = true;
            let body = poll_example(&runnable, |b| array_len(b) >= 1).await;
            assert!(
                array_len(&body) >= 1,
                "the error-find example must return >= 1 failed trace; command: {command}; body: {body}"
            );
            let json: serde_json::Value =
                serde_json::from_str(&body).expect("the error-find body is a JSON span array");
            let spans = json.as_array().expect("bare span array");
            assert!(
                spans
                    .iter()
                    .all(|s| s["status"]["code"].as_str() == Some("Error")),
                "every span the error-find example returns must carry Error status; command: {command}; body: {body}"
            );
            assert_not_html(&body, command);
        } else if runnable.contains("/api/v1/traces") {
            saw_traces = true;
            let body = poll_example(&runnable, |b| array_len(b) >= 1).await;
            assert!(
                array_len(&body) >= 1,
                "the traces service-window example must return >= 1 trace; command: {command}; body: {body}"
            );
            assert_not_html(&body, command);
        } else if runnable.contains("/api/v1/logs") {
            saw_logs = true;
            let body = poll_example(&runnable, |b| array_len(b) >= 1).await;
            assert!(
                array_len(&body) >= 1,
                "the logs example must return >= 1 record; command: {command}; body: {body}"
            );
            assert_not_html(&body, command);
        } else {
            panic!("unrecognised /help example: {command}");
        }
    }

    assert!(
        saw_metrics && saw_logs && saw_traces && saw_error_traces && saw_by_id,
        "all five signal examples must be present (metrics={saw_metrics}, logs={saw_logs}, \
         traces={saw_traces}, error_traces={saw_error_traces}, by_id={saw_by_id})"
    );
}
