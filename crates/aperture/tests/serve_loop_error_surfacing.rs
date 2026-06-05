//! Acceptance tests — aperture-serve-loop-error-surfacing-v0.
//!
//! Feature: when aperture's gRPC or HTTP serving loop dies AFTER the
//! socket is bound (the accept loop errors out post-bind), the death
//! must surface honestly instead of being swallowed at
//! `transport.rs:93` (`let _ = server.await;`, gRPC) and `:153-157`
//! (`let _ = axum::serve(...).await;`, HTTP). The operator-visible
//! contract (ADR-0066, brief.md "For Acceptance Designer"):
//!
//!   1. one structured stderr event `event=serve_loop_failed
//!      transport=grpc|http error=<reason>` at error level;
//!   2. `/readyz` flips to 503 `"failed"` (was 200 `"ready"`);
//!   3. `/healthz` stays 200 (liveness; never the lever);
//!   4. the process verdict folds to exit code `3` (distinct from
//!      clean-drain 0 / deadline 1 / config 2);
//!   5. a NORMAL graceful SIGTERM stays a byte-for-byte clean no-op:
//!      NO `serve_loop_failed`, the existing slice-08 drain sequence,
//!      exit 0 (the false-alarm guard, D3).
//!
//! ## Driving port (black-box)
//!
//! The running `aperture` instance, observed through its stderr
//! (`testing::stderr_capture`), its `/readyz` + `/healthz` probes over
//! real `reqwest`, and the process exit code. No internal type is
//! reached: `ServeOutcome` / `ReadinessPhase::Failed` / `ShutdownBundle`
//! are `pub(crate)` and intentionally unreachable from this crate.
//!
//! ## The injection seam (ADR-0066 Test seam (ii))
//!
//! A real accept loop rarely dies on command, so the post-bind death is
//! injected in-process behind the spawn helper, the aperture analogue of
//! cinder's `FailingFsyncBackend`. The seam is
//! `aperture::testing::spawn_with_injected_serve_failure(config, sink,
//! InjectServeFailure::{Grpc|Http|GrpcEarlyOk})`. It does not exist on
//! the public surface today (the serve future is built inside the spawn
//! helper, and the readiness/outcome types are crate-private), so DISTILL
//! ships a minimal `unimplemented!` scaffold and these failure scenarios
//! are `#[ignore]`-d RED until DELIVER lands the real injection.
//!
//! ## RED-not-BROKEN classification (Mandate 7)
//!
//! aperture already exists, so the harness, the stderr-capture seam, the
//! HTTP probes, and the public `spawn`/`Handle` surface all resolve and
//! compile. The serve-failure scenarios are RED because the seam they
//! drive is `unimplemented!` (they panic when run with `--ignored`) and,
//! once DELIVER implements the seam, they would still FAIL against the
//! present swallow (no event, `/readyz` stays 200, exit 0) — that is the
//! falsifiability guard. The negative controls (graceful drain stays
//! clean, a healthy instance reports ready) PASS today and are left
//! un-ignored: they are the guardrails the change must not regress.

mod common;

use std::sync::Arc;
use std::time::Duration;

use aperture::config::Config;
use aperture::ports::OtlpSink;
use aperture::testing::{InjectServeFailure, RecordingSink};

use crate::common::{
    capture_stderr_events, expect_no_stderr_event, expect_stderr_event, start_default,
};

// =========================================================================
// Local helpers
// =========================================================================

/// A default ephemeral-port config plus a fresh recording sink. The
/// serve-failure seam binds real loopback listeners, so tests can probe
/// `/readyz` / `/healthz` over the wire before and after the death.
fn default_config_and_sink() -> (Config, Arc<RecordingSink>, Arc<dyn OtlpSink>) {
    let sink = Arc::new(RecordingSink::new());
    let sink_dyn: Arc<dyn OtlpSink> = sink.clone();
    let config = Config::builder()
        .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
        .http_bind_addr("127.0.0.1:0".parse().unwrap())
        .drain_deadline(Duration::from_secs(5))
        .build()
        .unwrap();
    (config, sink, sink_dyn)
}

// =========================================================================
// US-01 — a serving loop that dies post-bind names the transport on stderr
// =========================================================================

/// US-01 / KPI-2 / KPI-5. The gRPC arm.
///
/// FALSIFIABILITY: against today's `let _ = server.await;` swallow the
/// death emits nothing, so `expect_stderr_event(.., "serve_loop_failed")`
/// panics — the test cannot pass on the bug. It passes only once the
/// serve task emits exactly one named event at error level.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: serve-failure injection seam is unimplemented; \
            against today's `let _ = server.await;` swallow the gRPC death emits no \
            serve_loop_failed event"]
async fn grpc_serving_loop_death_after_bind_is_named_on_stderr() {
    let ((), events) = capture_stderr_events(|| async {
        let (config, _sink, sink_dyn) = default_config_and_sink();
        let handle = aperture::testing::spawn_with_injected_serve_failure(
            config,
            sink_dyn,
            InjectServeFailure::Grpc,
        )
        .await
        .expect("spawn with injected gRPC serve failure");
        // Give the injected death a moment to self-react.
        tokio::time::sleep(Duration::from_millis(100)).await;
        drop(handle);
    })
    .await;

    let evt = expect_stderr_event(&events, "serve_loop_failed");
    assert_eq!(
        evt.level, "error",
        "a serving-loop death must be loud (error level), never silent"
    );
    assert_eq!(
        evt.fields.get("transport").and_then(|v| v.as_str()),
        Some("grpc"),
        "the event must name the gRPC transport"
    );
    let error_field = evt.fields.get("error").and_then(|v| v.as_str());
    assert!(
        error_field.is_some_and(|s| !s.is_empty()),
        "the event must carry a non-empty error reason"
    );
    // Exactly one — not zero, not two (KPI-2).
    let count = events
        .iter()
        .filter(|e| e.event == "serve_loop_failed")
        .count();
    assert_eq!(count, 1, "exactly one serve_loop_failed event per death");
}

/// US-01 / US-03 scenario 2 / KPI-5. The HTTP arm — the previously
/// SILENT half (no disclosing comment at `transport.rs:153`), proven by
/// its OWN scenario, never implied by the gRPC test.
///
/// FALSIFIABILITY: against today's silent `let _ = axum::serve(...)`
/// swallow the death emits nothing; this test panics on the missing
/// event. It passes only when the HTTP arm surfaces identically to gRPC.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: serve-failure injection seam is unimplemented; the \
            previously-silent HTTP arm emits no serve_loop_failed event on today's swallow"]
async fn http_serving_loop_death_after_bind_is_named_on_stderr() {
    let ((), events) = capture_stderr_events(|| async {
        let (config, _sink, sink_dyn) = default_config_and_sink();
        let handle = aperture::testing::spawn_with_injected_serve_failure(
            config,
            sink_dyn,
            InjectServeFailure::Http,
        )
        .await
        .expect("spawn with injected HTTP serve failure");
        tokio::time::sleep(Duration::from_millis(100)).await;
        drop(handle);
    })
    .await;

    let evt = expect_stderr_event(&events, "serve_loop_failed");
    assert_eq!(evt.level, "error");
    assert_eq!(
        evt.fields.get("transport").and_then(|v| v.as_str()),
        Some("http"),
        "the event must name the HTTP transport — the transport field is the only \
         difference between the two arms"
    );
    let count = events
        .iter()
        .filter(|e| e.event == "serve_loop_failed")
        .count();
    assert_eq!(
        count, 1,
        "exactly one serve_loop_failed event for the HTTP death"
    );
}

/// US-01 scenario 1 / US-03 scenario 1 / KPI-4 — NEGATIVE CONTROL.
///
/// A normal graceful shutdown (`Handle::shutdown`, the documented
/// SIGTERM equivalent) emits the existing slice-08 drain sequence and
/// NO `serve_loop_failed` line. This PASSES today and must stay green:
/// it is the false-alarm guard. A fix that fires on a graceful return
/// (D3 mis-implemented) would make `expect_no_stderr_event` panic here.
#[tokio::test(flavor = "multi_thread")]
async fn graceful_shutdown_emits_no_serve_loop_failed_event() {
    let ((), events) = capture_stderr_events(|| async {
        let (sink, _release): (Arc<RecordingSink>, ()) = (Arc::new(RecordingSink::new()), ());
        let sink_dyn: Arc<dyn OtlpSink> = sink.clone();
        let config = Config::builder()
            .grpc_bind_addr("127.0.0.1:0".parse().unwrap())
            .http_bind_addr("127.0.0.1:0".parse().unwrap())
            .drain_deadline(Duration::from_secs(5))
            .build()
            .unwrap();
        let handle = aperture::spawn(config, sink_dyn).await.expect("spawn");
        handle.wait_until_ready().await.expect("ready");
        // A clean graceful drain: the serve futures resolve Ok with
        // shutdown requested.
        let _ = handle.shutdown().await;
    })
    .await;

    // The existing drain narrative fired ...
    let _ = expect_stderr_event(&events, "shutdown_initiated");
    let _ = expect_stderr_event(&events, "shutdown_complete");
    // ... and NO serve-failure false alarm appeared.
    expect_no_stderr_event(&events, "serve_loop_failed");
}

// =========================================================================
// US-02 — a dead serving loop stops reporting healthy/ready
// =========================================================================

/// US-02 scenario 1 / KPI-3 — NEGATIVE CONTROL.
///
/// A healthy instance with both serving loops running reports `/readyz`
/// ready (200) and `/healthz` ok (200). PASSES today; this is the state
/// the feature must NOT disturb.
#[tokio::test(flavor = "multi_thread")]
async fn healthy_instance_reports_ready_and_alive() {
    let instance = start_default().await;
    let base = instance.http_base_url();
    let client = reqwest::Client::new();

    let readyz = client
        .get(format!("{base}/readyz"))
        .send()
        .await
        .expect("GET /readyz");
    assert_eq!(readyz.status().as_u16(), 200, "a healthy instance is ready");
    assert_eq!(readyz.text().await.unwrap_or_default().trim(), "ready");

    let healthz = client
        .get(format!("{base}/healthz"))
        .send()
        .await
        .expect("GET /healthz");
    assert_eq!(
        healthz.status().as_u16(),
        200,
        "a healthy instance is alive"
    );
    assert_eq!(healthz.text().await.unwrap_or_default().trim(), "ok");

    let _ = instance.handle.shutdown().await;
}

/// US-02 scenario 2 / KPI-3. A dead serving loop stops reporting ready.
///
/// FALSIFIABILITY: against today's swallow `/readyz` stays 200 `"ready"`
/// (there is no `Failed` phase a dead loop can flip to,
/// `readiness.rs:37-41`). The 503 `"failed"` assertion FAILS on the bug
/// and passes only once `flip_to_failed()` lands. `/healthz` stays 200
/// throughout (liveness; the zombie's process is still up).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: serve-failure injection seam is unimplemented; on today's \
            swallow /readyz stays 200 ready (no Failed phase) after a serving-loop death"]
async fn dead_serving_loop_stops_reporting_ready_but_stays_alive() {
    let (config, _sink, sink_dyn) = default_config_and_sink();
    let handle = aperture::testing::spawn_with_injected_serve_failure(
        config,
        sink_dyn,
        InjectServeFailure::Grpc,
    )
    .await
    .expect("spawn with injected gRPC serve failure");
    handle.wait_until_ready().await.expect("ready before death");
    let http_addr = handle.http_addr();
    let client = reqwest::Client::new();

    // Poll up to 1 s for the readiness flip; the injected death
    // self-reacts asynchronously.
    let mut saw_failed = false;
    let started = std::time::Instant::now();
    while started.elapsed() < Duration::from_secs(1) {
        let resp = client
            .get(format!("http://{http_addr}/readyz"))
            .send()
            .await
            .expect("GET /readyz");
        if resp.status().as_u16() == 503 {
            let body = resp.text().await.unwrap_or_default();
            if body.trim() == "failed" {
                saw_failed = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(
        saw_failed,
        "/readyz must flip to 503 \"failed\" after a serving-loop death — a zombie \
         that serves nothing must never report ready"
    );

    // Liveness stays true: the process is up, only the listener is dead.
    let healthz = client
        .get(format!("http://{http_addr}/healthz"))
        .send()
        .await
        .expect("GET /healthz");
    assert_eq!(
        healthz.status().as_u16(),
        200,
        "/healthz stays 200 — readiness, not liveness, is the lever"
    );

    drop(handle);
}

/// US-02 / KPI-3 — readiness is STICKY: once `/readyz` reports failed it
/// never flaps back to ready (a dead listener never recovers). Property-
/// shaped invariant: regardless of how many times it is probed, `/readyz`
/// stays 503 after the death.
///
/// FALSIFIABILITY: today there is no Failed phase at all, so the initial
/// flip never happens; once DELIVER lands the sticky phase, a mutation
/// that lets `Failed` demote back to `Ready` is caught here.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: serve-failure injection seam is unimplemented; the sticky \
            Failed readiness phase does not exist yet"]
async fn readyz_failed_phase_is_sticky_and_never_flaps_back_to_ready() {
    let (config, _sink, sink_dyn) = default_config_and_sink();
    let handle = aperture::testing::spawn_with_injected_serve_failure(
        config,
        sink_dyn,
        InjectServeFailure::Grpc,
    )
    .await
    .expect("spawn with injected gRPC serve failure");
    let http_addr = handle.http_addr();
    let client = reqwest::Client::new();

    // Wait for the flip.
    let started = std::time::Instant::now();
    loop {
        let resp = client
            .get(format!("http://{http_addr}/readyz"))
            .send()
            .await
            .expect("GET /readyz");
        if resp.status().as_u16() == 503 && resp.text().await.unwrap_or_default().trim() == "failed"
        {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "/readyz never flipped to failed"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    // Probe repeatedly: it must stay 503 "failed" forever.
    for _ in 0..5 {
        let resp = client
            .get(format!("http://{http_addr}/readyz"))
            .send()
            .await
            .expect("GET /readyz");
        assert_eq!(
            resp.status().as_u16(),
            503,
            "Failed is sticky: /readyz must never flap back to ready"
        );
        assert_eq!(resp.text().await.unwrap_or_default().trim(), "failed");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    drop(handle);
}

// =========================================================================
// US-03 scenario 3 — the unexpected-early-Ok case (D3, fatal at v0)
// =========================================================================

/// US-03 scenario 3 / D3. A serving loop that returns `Ok` on its own,
/// with NO shutdown requested, is treated as a fatal post-bind death
/// (the listener silently stopped). It surfaces identically to an `Err`.
///
/// FALSIFIABILITY: today the discarded `Ok` is indistinguishable from a
/// graceful return and emits nothing. The discriminator is "was shutdown
/// requested?", not the `Ok`/`Err` tag; this test pins the
/// not-requested-early-Ok → fatal leg of D3.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: serve-failure injection seam is unimplemented; the \
            shutdown_requested discriminator (D3) does not exist yet, so an early Ok \
            without a shutdown request emits nothing on today's swallow"]
async fn early_ok_without_shutdown_request_is_treated_as_fatal() {
    let ((), events) = capture_stderr_events(|| async {
        let (config, _sink, sink_dyn) = default_config_and_sink();
        let handle = aperture::testing::spawn_with_injected_serve_failure(
            config,
            sink_dyn,
            InjectServeFailure::GrpcEarlyOk,
        )
        .await
        .expect("spawn with injected early-Ok serve return");
        tokio::time::sleep(Duration::from_millis(100)).await;
        drop(handle);
    })
    .await;

    let evt = expect_stderr_event(&events, "serve_loop_failed");
    assert_eq!(
        evt.level, "error",
        "an unexpected early Ok with no shutdown requested is a fatal post-bind death"
    );
    assert_eq!(
        evt.fields.get("transport").and_then(|v| v.as_str()),
        Some("grpc")
    );
}

// =========================================================================
// US-02 / US-03 — process exit code (driving-adapter, real binary)
// =========================================================================

/// US-02 scenario 3 / KPI-3 — driving adapter, real subprocess. The
/// REGRESSION GUARD half: the actual `aperture` binary, run with a
/// malformed config, exits with the EXISTING config-error code `2` (and
/// a healthy run is terminated cleanly). This exercises the genuine
/// process boundary (a real child process + a real exit code) that the
/// in-process harness cannot, proving the established exit-code map
/// (0 clean / 1 deadline / 2 config) is preserved while the new `3`
/// arm is added.
///
/// This PASSES today (exit 2 on a bad config already works) and is the
/// driving-adapter walking skeleton: it answers "can a supervisor read
/// aperture's exit code over a real process boundary?". The exit-3 leg
/// (a serve death drives exit 3) is the in-process Layer-1 seam,
/// DELIVER-owned (lib.rs:379-430, `ServeOutcome` is pub(crate) and
/// unreachable here) — see the companion below.
#[test]
fn binary_preserves_config_error_exit_code_two() {
    use std::process::Command;

    let bin = env!("CARGO_BIN_EXE_aperture");
    let output = Command::new(bin)
        .arg("--config")
        .arg("/nonexistent/path/aperture.toml")
        .output()
        .expect("run the aperture binary");

    let code = output.status.code().expect("process produced an exit code");
    assert_eq!(
        code, 2,
        "a config error must exit 2 — the established exit map (0/1/2) is preserved \
         beneath the new serve-failure exit code 3"
    );
}

/// US-02 scenario 3 / KPI-3 — driving adapter, real subprocess, exit 3.
///
/// The real binary, with an injected post-bind serve death (no SIGTERM),
/// exits `3`. This requires a binary-level injection trigger DELIVER must
/// add (e.g. a test-only env var read only when compiled for the test
/// matrix, gating the `testing::spawn_with_injected_serve_failure` path
/// inside `run`). It does not exist today, so this scenario is RED:
/// against today's swallow the binary would exit 0 (the serve error is
/// discarded and the process keeps running until killed).
///
/// FALSIFIABILITY: a passing test requires exit 3, distinct from the
/// clean-drain 0 a normal run produces; today the injected death has no
/// path into the exit code.
#[cfg(unix)]
#[test]
#[ignore = "RED until DELIVER: the binary has no serve-failure injection trigger; on \
            today's swallow an injected serve death exits 0, not 3"]
fn binary_exits_three_on_injected_serve_death() {
    use std::process::Command;

    let bin = env!("CARGO_BIN_EXE_aperture");
    let output = Command::new(bin)
        // The trigger DELIVER wires: a test-only env var that drives the
        // injected post-bind serve death inside `run`.
        .env("APERTURE_TEST_INJECT_SERVE_FAILURE", "grpc")
        .output()
        .expect("run the aperture binary with an injected serve death");

    let code = output.status.code().expect("process produced an exit code");
    assert_eq!(
        code, 3,
        "an injected post-bind serve death must exit 3 — distinct from clean-drain 0, \
         deadline 1, config 2"
    );
}

// =========================================================================
// US-03 scenario 1 — graceful drain stays clean (driving adapter, exit 0)
// =========================================================================

/// US-03 scenario 1 / KPI-4 — driving adapter, real subprocess, exit 0.
///
/// A real `aperture` child process, sent SIGTERM, drains and exits `0`
/// with NO `serve_loop_failed` on its stderr. This is the false-alarm
/// guard at the process boundary: a routine restart must never page Sam.
///
/// DELIVER lands the process-spawning SIGTERM fixture (mirrors the
/// existing slice-08 `#[ignore]` precedent
/// `sigterm_and_handle_shutdown_produce_the_same_drain_sequence`). Marked
/// RED-by-pending-fixture, not by behaviour: the in-process negative
/// control (`graceful_shutdown_emits_no_serve_loop_failed_event`, above)
/// already proves the behaviour green today.
#[cfg(unix)]
#[test]
#[ignore = "RED until DELIVER: lands the process-spawning SIGTERM fixture (slice-08 \
            precedent); the in-process graceful negative control already proves the \
            behaviour green"]
fn binary_exits_zero_and_silent_on_real_sigterm() {
    // DELIVER fixture: spawn `aperture` as a child bound to ephemeral
    // ports; once ready, send SIGTERM; assert exit code 0 and that the
    // captured child stderr contains the slice-08 drain sequence ending
    // `shutdown_complete exit_code=0` and NO `serve_loop_failed` line.
    //
    // Explicit RED so this placeholder cannot masquerade as a pass when
    // forced with `--ignored`: an empty body would trivially succeed and
    // hide the missing fixture. The behaviour itself is already proven
    // green in-process by `graceful_shutdown_emits_no_serve_loop_failed_event`.
    panic!(
        "RED until DELIVER: process-spawning SIGTERM fixture not yet landed; \
         assert the real binary exits 0 with no serve_loop_failed on its stderr"
    );
}
