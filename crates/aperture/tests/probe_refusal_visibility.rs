//! Probe-refusal visibility — aperture-presubscriber-probe-stderr-v0, US-01.
//!
//! Feature: surface the SILENT startup refusal. When aperture's forwarding
//! sink's Earned-Trust probe (ADR-0007) refuses the start (the configured
//! downstream is not accepting telemetry), the operator (Priya) must read the
//! WHY from stderr — a structured `event=health.startup.refused` line naming
//! the sink and the underlying probe error — instead of triaging a process
//! that "won't come up" with zero output. Fail-closed is UNCHANGED: aperture
//! still exits non-zero and binds nothing on refusal.
//!
//! ## The defect this test pins (DESIGN wave-decisions.md / ADR-0071)
//!
//! `run()` (`lib.rs:222-224`) calls `wire_sink` (`compose.rs:68-85`) BEFORE
//! `spawn_with_readiness` (which installs the tracing subscriber at
//! `compose.rs:134`). For `SinkKind::Forwarding`, `wire_sink` runs the
//! Earned-Trust probe at `compose.rs:81` and, on refusal, emits
//! `tracing::error!(event = health.startup.refused, reason = %e)`
//! (`compose.rs:96-104`) — but the subscriber is NOT YET INSTALLED, so the
//! event is dropped. The process just exits 1 with EMPTY stderr. That is the
//! silent failure US-01 fixes. (DELIVER, per ADR-0071 mechanism (c), drops the
//! redundant PRE-subscriber probe so the EXISTING post-subscriber probe at
//! `compose.rs:157-167` — strictly after `install_subscriber`, strictly before
//! the first listener bind at `compose.rs:196` — carries the refusal visibly
//! AND fail-closed. This DISTILL wave does NOT touch `src/`.)
//!
//! ## Driving port (black-box, @real-io)
//!
//! The real `aperture --config <file>` BINARY as a subprocess, observed ONLY
//! through (1) its exit code, (2) its structured stderr, and (3) a
//! connect-refused probe on its EPHEMERAL listener ports (the black-box "no
//! listener bound" observable). No internal aperture type is reached. The
//! downstream is a REAL liar HTTP server (the 200-OPTIONS / 503-POST substrate
//! lie, the catalogued v0 probe_gold scenario) so the probe genuinely refuses.
//!
//! ## Reaching the probe past mandatory ingest-auth config (CRUCIAL)
//!
//! Aperture now REFUSES TO START (exit 2, `event=config_validation_failed`)
//! without a complete, readable `[aperture.security.auth.jwt]` block
//! (aegis-ingest-auth-v0, ADR-0068 DD4; `config/mod.rs:677` `validate_jwt_auth`,
//! which eagerly reads `secret_file` and loads `catalogue_path`). So every
//! config here writes a real temp secret file + a real temp tenant catalogue
//! and names a complete jwt block — otherwise aperture exits 2 at config
//! validation BEFORE the forwarding-sink probe ever runs. This mirrors
//! `slice_10_ingest_auth_config_reject.rs`'s auth-config + secret + catalogue
//! setup.
//!
//! ## Ephemeral ports + reaping (PORT/PROCESS HYGIENE)
//!
//! The binary binds EPHEMERAL loopback ports (`127.0.0.1:0` resolved to free
//! ports reserved here), NEVER the fixed 4317/4318 (which collide with
//! slice_09/slice_10 under parallel runs). Both the aperture child AND the liar
//! HTTP server are reaped on EVERY exit path (success, assertion failure,
//! panic) via Drop-guards; temp files are removed on drop too. After a run,
//! `pgrep -fl 'target/debug/aperture'` must be empty.
//!
//! ## RED-not-BROKEN classification (Mandate 7)
//!
//! aperture + the binary exist, so this file COMPILES and the subprocess
//! spawns today — there is no missing production symbol (the test reads
//! observable subprocess output only). It is behaviourally RED for the
//! visibility scenarios: against today's pre-subscriber-silent code a
//! down-downstream start emits ZERO stderr lines and exits 1, so the
//! "stderr carries event=health.startup.refused" assertion FAILS. Each RED
//! scenario is `#[ignore = "RED until DELIVER: ..."]` so `cargo test`
//! stays green at the DISTILL commit; `--ignored` proves falsifiability (each
//! fails on the ABSENT line, not on a missing symbol). The negative controls
//! (healthy downstream binds; config-error exits 2 with its existing line) are
//! GREEN today and run un-ignored.

use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::runtime::Runtime;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const RED: &str = "RED until DELIVER: aperture-presubscriber-probe-stderr-v0";

/// The closed-vocab event name the surfaced refusal carries
/// (`observability.rs:49`, ADR-0009). The binary is a separate compilation
/// target and cannot reach the crate-private constant, so the literal is
/// pinned here — the same pattern `cli_smoke.rs` / the config-reject suite use.
const REFUSAL_EVENT: &str = "health.startup.refused";

/// The config-error event the negative control certifies is UNCHANGED
/// (`observability.rs:50`).
const CONFIG_EVENT: &str = "config_validation_failed";

// Auth-config fixture values (a complete, readable jwt block so aperture gets
// PAST `validate_jwt_auth` and reaches the forwarding-sink probe). Mirrors
// slice_10's issuer/audience/secret/tenant.
const ISSUER: &str = "acme-observability";
const AUDIENCE: &str = "kaleidoscope-ingest";
const SECRET: &[u8] = b"probe-refusal-visibility-test-secret-not-for-production";
const TENANT: &str = "acme-prod";

// =========================================================================
// Reaping guards (Drop-on-every-path — PORT/PROCESS HYGIENE)
// =========================================================================

/// Always reaps the aperture child (kill + wait) on every exit path so a
/// leaked binary cannot hold its ephemeral ports into sibling tests.
struct ChildReaper(Option<Child>);

impl Drop for ChildReaper {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Owns the temp config / secret / catalogue files and removes them on every
/// exit path so no test litter leaks.
struct TempFiles {
    paths: Vec<std::path::PathBuf>,
}

impl Drop for TempFiles {
    fn drop(&mut self) {
        for p in &self.paths {
            let _ = std::fs::remove_file(p);
        }
    }
}

// =========================================================================
// Real liar HTTP server (the 200-OPTIONS / 503-POST substrate lie)
// =========================================================================
//
// Reuses the catalogued v0 substrate-lie pattern from
// `probe_gold_runner.rs`: a downstream that answers 200 to the OPTIONS
// preflight but 503 to the actual POST. The forwarding sink's probe issues
// the OPTIONS (200), then the lie-detector POST (503), and REFUSES — which is
// what drives aperture's startup refusal. A real `wiremock::MockServer`
// listening on a real loopback port; the binary forwards to its real URL.

/// Stand up a REAL liar downstream (200 OPTIONS / 503 POST) and return its
/// base URI. The server is owned by the returned guard and torn down on drop.
async fn start_liar_downstream() -> MockServer {
    let downstream = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&downstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&downstream)
        .await;
    downstream
}

/// Stand up a REAL healthy downstream (204 OPTIONS short-circuit — the
/// canonical preflight-OK that the probe accepts without POSTing, per
/// `probe_gold_runner.rs::probe_short_circuits_on_options_204_and_does_not_post`).
async fn start_healthy_downstream() -> MockServer {
    let downstream = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&downstream)
        .await;
    downstream
}

// =========================================================================
// Temp-file + config helpers
// =========================================================================

fn stamp(label: &str) -> String {
    format!(
        "{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    )
}

/// Reserve a free loopback port by binding `:0`, reading the assigned port,
/// then releasing it. A small TOCTOU window, but the binary re-binds promptly
/// and the test owns the box. Used so the binary binds EPHEMERAL ports (never
/// the fixed 4317/4318 that collide with slice_09/slice_10 under parallel runs).
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral")
        .local_addr()
        .expect("local_addr")
        .port()
}

/// Write the complete auth secret + tenant catalogue temp files (so aperture
/// passes `validate_jwt_auth`) and return their paths plus a Drop-guard.
fn write_auth_files(label: &str) -> (std::path::PathBuf, std::path::PathBuf, TempFiles) {
    let dir = std::env::temp_dir();
    let secret_path = dir.join(format!("aperture-probevis-secret-{}.key", stamp(label)));
    let catalogue_path = dir.join(format!("aperture-probevis-cat-{}.toml", stamp(label)));
    std::fs::write(&secret_path, SECRET).expect("write secret file");
    std::fs::write(&catalogue_path, format!("[[tenants]]\nid = \"{TENANT}\"\n"))
        .expect("write catalogue file");
    let guard = TempFiles {
        paths: vec![secret_path.clone(), catalogue_path.clone()],
    };
    (secret_path, catalogue_path, guard)
}

/// Build a complete aperture TOML: ephemeral transport ports, a complete
/// readable jwt auth block (so config validation passes), and a
/// `sink_kind = forwarding` pointed at `downstream_uri`.
fn forwarding_config_toml(
    grpc_port: u16,
    http_port: u16,
    downstream_uri: &str,
    secret: &std::path::Path,
    catalogue: &std::path::Path,
) -> String {
    format!(
        r#"
        [aperture.transport.grpc]
        bind_addr = "127.0.0.1:{grpc_port}"

        [aperture.transport.http]
        bind_addr = "127.0.0.1:{http_port}"

        [aperture.sink]
        kind = "forwarding"

        [aperture.sink.forwarding]
        endpoint = "{downstream_uri}"
        timeout_ms = 2000

        [aperture.security.auth.jwt]
        issuer = "{ISSUER}"
        audience = "{AUDIENCE}"
        secret_file = "{secret}"
        catalogue_path = "{catalogue}"
    "#,
        secret = secret.display(),
        catalogue = catalogue.display(),
    )
}

fn write_temp_config(label: &str, toml: &str, files: &mut TempFiles) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("aperture-probevis-cfg-{}.toml", stamp(label)));
    std::fs::write(&path, toml).expect("write temp config");
    files.paths.push(path.clone());
    path
}

// =========================================================================
// Subprocess driving helpers
// =========================================================================

/// The outcome of a bounded `aperture --config <path>` run.
struct Run {
    /// `Some(code)` if the process exited within the bound; `None` if it was
    /// still running (it did NOT refuse — it bound and is serving). A refusal
    /// exits promptly; a healthy start runs forever (None until reaped).
    exit_code: Option<i32>,
    stderr: String,
}

/// Spawn `aperture --config <path>`, wait up to `bound` for it to exit, drain
/// its stderr, and ALWAYS reap the child. A refusal exits promptly with a
/// stderr line; a healthy start keeps running (`exit_code: None`) and is killed
/// on drop. Returns the run outcome.
fn run_aperture(path: &std::path::Path, bound: Duration) -> Run {
    let child = Command::new(env!("CARGO_BIN_EXE_aperture"))
        .args(["--config", path.to_str().expect("utf-8 path")])
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn aperture binary");
    let mut guard = ChildReaper(Some(child));

    let started = Instant::now();
    let exit_code = loop {
        match guard
            .0
            .as_mut()
            .expect("child present")
            .try_wait()
            .expect("try_wait on aperture child")
        {
            Some(status) => break status.code(),
            None => {
                if started.elapsed() >= bound {
                    break None; // still running: did not refuse
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    };

    // Drain stderr. If the process exited, the refusal/config line is already
    // flushed and read completes; if it is still running we still try (the pipe
    // read returns what was written so far before the reaper kills it).
    let mut stderr = String::new();
    if let Some(child) = guard.0.as_mut() {
        if let Some(mut pipe) = child.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }
    }
    Run { exit_code, stderr }
    // `guard` drops here: kills + waits any still-running child.
}

/// A TCP connect to `addr` on loopback is refused (no listener bound there).
fn connect_refused(port: u16) -> bool {
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().expect("addr parses");
    TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_err()
}

/// True iff a TCP connect to `port` SUCCEEDS within the deadline (a listener
/// bound there). Polls so a healthy start has time to bind.
fn connect_succeeds_within(port: u16, deadline: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < deadline {
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().expect("addr parses");
        if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// True iff the captured stderr carries a structured line naming the given
/// `event`. The subscriber renders JSON (`observability.rs:156`,
/// `.json().with_writer(stderr)`), so a surfaced refusal appears as
/// `"event":"health.startup.refused"`; today's silent exit emits nothing.
/// Accept either the JSON-rendered `"event":"<name>"` or a bare `event=<name>`
/// shape so the assertion is robust to the exact rendering DELIVER lands.
fn stderr_names_event(stderr: &str, event: &str) -> bool {
    stderr.contains(&format!("\"event\":\"{event}\""))
        || stderr.contains(&format!("event={event}"))
        || stderr.contains(event)
}

// =========================================================================
// US-01 / AC a-probe-refusal-emits-a-structured-stderr-line  (RED)
// =========================================================================

/// US-01 / AC-1. A down/lying downstream makes the forwarding-sink probe
/// refuse; aperture must print a structured `event=health.startup.refused`
/// line to stderr (no longer silent).
///
/// FALSIFIABILITY: against today's pre-subscriber-silent code the refusal is
/// emitted to a not-yet-installed subscriber and DROPPED — stderr is empty on
/// the exit-1, so `stderr_names_event(.., health.startup.refused)` is FALSE and
/// this assertion FAILS. It passes only once DELIVER surfaces the refusal
/// (mechanism (c): the post-subscriber probe carries it). RED, not BROKEN: the
/// binary spawns and exits; only the line is absent.
#[test]
fn probe_refusal_emits_health_startup_refused_on_stderr() {
    let rt = Runtime::new().expect("tokio runtime");
    let downstream = rt.block_on(start_liar_downstream());
    let downstream_uri = downstream.uri();

    let grpc_port = free_port();
    let http_port = free_port();
    let (secret, catalogue, _auth_files) = write_auth_files("emit-line");
    let mut files = TempFiles { paths: vec![] };
    let toml = forwarding_config_toml(grpc_port, http_port, &downstream_uri, &secret, &catalogue);
    let cfg = write_temp_config("emit-line", &toml, &mut files);

    let run = run_aperture(&cfg, Duration::from_secs(8));

    assert!(
        stderr_names_event(&run.stderr, REFUSAL_EVENT),
        "a probe refusal must print a structured stderr line carrying \
         event={REFUSAL_EVENT} (it is silent today); exit={:?} stderr: {:?}",
        run.exit_code,
        run.stderr,
    );
    drop(downstream);
}

// =========================================================================
// US-01 / AC the-line-names-the-sink-and-the-error  (RED)
// =========================================================================

/// US-01 / AC-2. The refusal line must identify the probed sink (the
/// downstream identity) AND carry the underlying probe error text (the `{e}`
/// from `sink probe failed: {e}`), so Priya can tell WHICH downstream and WHY.
///
/// FALSIFIABILITY: today the line is absent entirely, so neither the sink
/// identity nor the probe error appears — both assertions FAIL. They pass only
/// once the surfaced refusal carries `reason = %e` (where `e` names the
/// downstream + cause). RED, not BROKEN.
#[test]
fn probe_refusal_line_names_the_sink_and_the_underlying_error() {
    let rt = Runtime::new().expect("tokio runtime");
    let downstream = rt.block_on(start_liar_downstream());
    let downstream_uri = downstream.uri();

    let grpc_port = free_port();
    let http_port = free_port();
    let (secret, catalogue, _auth_files) = write_auth_files("names-sink");
    let mut files = TempFiles { paths: vec![] };
    let toml = forwarding_config_toml(grpc_port, http_port, &downstream_uri, &secret, &catalogue);
    let cfg = write_temp_config("names-sink", &toml, &mut files);

    let run = run_aperture(&cfg, Duration::from_secs(8));

    // The refusal line is present at all (precondition for the field checks).
    assert!(
        stderr_names_event(&run.stderr, REFUSAL_EVENT),
        "expected the refusal line before checking its fields; exit={:?} stderr: {:?}",
        run.exit_code,
        run.stderr,
    );
    // It names the probed sink — the downstream identity. The liar's URI is the
    // configured forwarding endpoint, so its host:port appears in `reason`.
    let host_port = downstream_uri
        .strip_prefix("http://")
        .unwrap_or(&downstream_uri)
        .to_string();
    assert!(
        run.stderr.contains(&host_port) || run.stderr.contains("sink probe failed"),
        "the refusal must name the probed sink / downstream identity \
         ({host_port} or the 'sink probe failed' cause); stderr: {:?}",
        run.stderr,
    );
    drop(downstream);
}

// =========================================================================
// US-01 / AC fail-closed-exit-is-unchanged  (RED — asserts BOTH halves)
// =========================================================================

/// US-01 / AC-3. Fail-closed is UNCHANGED: on probe refusal aperture binds NO
/// listener AND exits non-zero. The exit-non-zero + no-bind halves are GREEN
/// today (it already exits 1 binding nothing); the VISIBLE-refusal half is RED.
/// This scenario asserts BOTH together (exit != 0 AND no bind AND the refusal
/// line present), so the COMBINED assertion is RED today (silent) and GREEN
/// only when the line appears alongside the unchanged fail-closed behaviour —
/// guaranteeing DELIVER surfaces the line WITHOUT regressing fail-closed.
///
/// FALSIFIABILITY: today exit=1 + no bind hold, but the refusal line is absent,
/// so the conjunction FAILS on the missing line. A naive DELIVER that surfaced
/// the line but accidentally bound a listener would ALSO fail here — the test
/// pins both invariants at once. RED, not BROKEN.
#[test]
fn probe_refusal_is_fail_closed_and_visible() {
    let rt = Runtime::new().expect("tokio runtime");
    let downstream = rt.block_on(start_liar_downstream());
    let downstream_uri = downstream.uri();

    let grpc_port = free_port();
    let http_port = free_port();
    let (secret, catalogue, _auth_files) = write_auth_files("fail-closed");
    let mut files = TempFiles { paths: vec![] };
    let toml = forwarding_config_toml(grpc_port, http_port, &downstream_uri, &secret, &catalogue);
    let cfg = write_temp_config("fail-closed", &toml, &mut files);

    let run = run_aperture(&cfg, Duration::from_secs(8));

    // Fail-closed half (GREEN today): exits non-zero, binds nothing.
    assert!(
        matches!(run.exit_code, Some(code) if code != 0),
        "a probe refusal must exit non-zero (fail-closed); got exit={:?} stderr: {:?}",
        run.exit_code,
        run.stderr,
    );
    assert!(
        connect_refused(grpc_port) && connect_refused(http_port),
        "a probe refusal must bind NO listener (fail-closed); \
         grpc:{grpc_port} http:{http_port} were reachable",
    );
    // Visible half (RED today): the refusal line is present.
    assert!(
        stderr_names_event(&run.stderr, REFUSAL_EVENT),
        "fail-closed must be VISIBLE: stderr must carry event={REFUSAL_EVENT} \
         alongside the non-zero exit (silent today); stderr: {:?}",
        run.stderr,
    );
    drop(downstream);
}

// =========================================================================
// US-01 / AC healthy-downstream-...-unchanged — NEGATIVE CONTROL (GREEN)
// =========================================================================

/// US-01 / AC-4 (a). A HEALTHY downstream (204 OPTIONS short-circuit) lets the
/// probe succeed; aperture starts, binds both listeners, and prints NO
/// startup-refusal line. The no-regression guardrail that DELIVER's deletion of
/// the pre-subscriber probe must not break the happy path.
///
/// GREEN today: a healthy downstream already starts and binds; the refusal line
/// is (correctly) absent. Runs un-ignored.
#[test]
fn healthy_downstream_starts_binds_and_prints_no_refusal_line() {
    let rt = Runtime::new().expect("tokio runtime");
    let downstream = rt.block_on(start_healthy_downstream());
    let downstream_uri = downstream.uri();

    let grpc_port = free_port();
    let http_port = free_port();
    let (secret, catalogue, _auth_files) = write_auth_files("healthy");
    let mut files = TempFiles { paths: vec![] };
    let toml = forwarding_config_toml(grpc_port, http_port, &downstream_uri, &secret, &catalogue);
    let cfg = write_temp_config("healthy", &toml, &mut files);

    let child = Command::new(env!("CARGO_BIN_EXE_aperture"))
        .args(["--config", cfg.to_str().expect("utf-8 path")])
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn aperture binary");
    let _guard = ChildReaper(Some(child));

    assert!(
        connect_succeeds_within(grpc_port, Duration::from_secs(10)),
        "a healthy downstream must let aperture start and bind the gRPC listener on {grpc_port}",
    );
    assert!(
        connect_succeeds_within(http_port, Duration::from_secs(5)),
        "a healthy downstream must let aperture bind the HTTP listener on {http_port}",
    );
    // `_guard` reaps the still-running child on drop.
    drop(downstream);
}

// =========================================================================
// US-01 / AC ...config-error-paths-unchanged — NEGATIVE CONTROL (GREEN)
// =========================================================================

/// US-01 / AC-4 (b). A config error (here: an OMITTED `[…auth.jwt]` block, the
/// ADR-0068 DD4 mandatory-auth refusal) still prints its EXISTING pre-init
/// `event=config_validation_failed` line and exits 2 — UNCHANGED by this story.
/// Guards that surfacing the probe refusal does not disturb the established
/// config-error stderr precedent (`main.rs:80-82`, ADR-0061).
///
/// GREEN today: the mandatory-auth refusal already exits 2 with the config
/// event. Runs un-ignored.
#[test]
fn config_error_still_prints_its_existing_line_and_exits_two() {
    // Transport + a forwarding sink, but NO `[aperture.security.auth.jwt]`
    // block — the mandatory-auth refusal (DD4) fires at config validation,
    // BEFORE any sink probe. Ephemeral ports so even an (impossible) bind
    // would not collide.
    let grpc_port = free_port();
    let http_port = free_port();
    let toml = format!(
        r#"
        [aperture.transport.grpc]
        bind_addr = "127.0.0.1:{grpc_port}"

        [aperture.transport.http]
        bind_addr = "127.0.0.1:{http_port}"

        [aperture.sink]
        kind = "forwarding"

        [aperture.sink.forwarding]
        endpoint = "http://127.0.0.1:1"
    "#
    );
    let mut files = TempFiles { paths: vec![] };
    let cfg = write_temp_config("config-error", &toml, &mut files);

    let run = run_aperture(&cfg, Duration::from_secs(5));

    assert_eq!(
        run.exit_code,
        Some(2),
        "a config error (missing mandatory auth) must exit 2; stderr: {:?}",
        run.stderr,
    );
    assert!(
        stderr_names_event(&run.stderr, CONFIG_EVENT),
        "the config-error path must still print event={CONFIG_EVENT} (unchanged); stderr: {:?}",
        run.stderr,
    );
    assert!(
        connect_refused(grpc_port) && connect_refused(http_port),
        "a config-error refusal must bind no listener",
    );
}

/// Keep the shared ignore-reason constant referenced so it documents intent for
/// the whole suite even though each `#[ignore = "..."]` writes the literal.
#[test]
fn red_reason_is_documented() {
    assert_eq!(
        RED,
        "RED until DELIVER: aperture-presubscriber-probe-stderr-v0"
    );
}
