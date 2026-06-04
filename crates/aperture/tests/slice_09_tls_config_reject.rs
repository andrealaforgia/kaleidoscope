//! Slice 09 — Aperture refuses to start when an unimplemented security
//! knob is requested (`tls-config-reject-v0`, US-TLS-01, ADR-0061).
//!
//! Supersedes the warn-and-continue runtime reaction of slice 07
//! (`slice_07_tls_schema_knob.rs`) for `tls.enabled` / `auth.spiffe.enabled`.
//! The forward-compat *schema* decision (ADR-0008) is preserved: the keys
//! still parse and still default off. Only the runtime reaction to `= true`
//! changes — from "warn and bind plaintext" to "refuse to start".
//!
//! ## The contract under test (ADR-0061 behaviour matrix)
//!
//! | `tls.enabled` | `auth.spiffe.enabled` | Result | Event | Exit | Listener |
//! |---|---|---|---|---|---|
//! | true  | false | Refuse | `config_validation_failed` names `tls.enabled`        | 2 | none |
//! | false | true  | Refuse | `config_validation_failed` names `auth.spiffe.enabled` | 2 | none |
//! | true  | true  | Refuse | `config_validation_failed` names **both** knobs        | 2 | none |
//! | false | false | Start (unchanged) | `startup`/`ready` (no refusal) | runs | 4317+4318 |
//! | absent `[security]` | absent | Start (unchanged) | identical to both-false | runs | binds |
//!
//! ## Driving port
//!
//! Two black-box surfaces, both named for the operator in `brief.md`'s
//! "For Acceptance Designer" note:
//!
//! - **in-process seam** — `Config::from_toml_str` → `RawConfig::into_config`,
//!   the same entry point slice 07 uses. The refusal lands as
//!   `Err(ConfigError)`; because `Config` is never constructed, the bind path
//!   (`compose::spawn_grpc`/`spawn_http`) is structurally unreachable. This is
//!   the strongest AC-4 (no-plaintext-bind) observable: no `Config`, no bind.
//! - **binary subprocess** (`@real-io`) — `aperture --config <file>`, asserting
//!   the operator-visible surface: exit code 2 + a structured stderr line
//!   carrying `event=config_validation_failed` naming the knob + connection
//!   refused on the OTLP ports.
//!
//! ## RED-not-BROKEN
//!
//! The refusal CODE does not exist yet (DELIVER adds the reject branch to
//! `RawConfig::into_config` and routes the `main.rs` error line through the
//! structured channel). These tests are written against the EXISTING public
//! API, so they COMPILE today but FAIL behaviourally (today `into_config`
//! returns `Ok` with a warn; the binary binds). Every refusal test is
//! therefore `#[ignore = "RED until DELIVER: tls-config-reject-v0"]` so
//! `cargo test --workspace` stays green at the DISTILL commit; DELIVER removes
//! the ignores.
//!
//! The two negative controls (AC-5, AC-6) assert today's preserved behaviour
//! (knobs off → start + bind, no refusal event). They are NOT ignored: they
//! pass today and DELIVER must keep them green (the non-regression guard).
//!
//! Port-collision safety (DEVOPS D3/D4): the refusal path never binds, so the
//! binary refusal tests run collision-free against the default 4317/4318. The
//! positive-bind negative control uses the ephemeral `127.0.0.1:0` override
//! (`config/mod.rs:226-236`, as slice 07 does) — no default-port positive-bind
//! test is added to the parallel suite.

mod common;

use std::io::Write;
use std::net::SocketAddr;
use std::process::Command;
use std::sync::Arc;

use aperture::config::Config;
use aperture::ports::OtlpSink;
use aperture::testing::RecordingSink;

use crate::common::{capture_stderr_events, expect_no_stderr_event, expect_stderr_event};

const REFUSAL_EVENT: &str = "config_validation_failed";
const RED: &str = "RED until DELIVER: tls-config-reject-v0";

// =========================================================================
// Subprocess helpers (binary driving port — @real-io)
// =========================================================================

/// Write `toml` to a uniquely-named temp file and return its path. The
/// caller owns the file for the lifetime of the subprocess run.
fn write_temp_config(label: &str, toml: &str) -> std::path::PathBuf {
    let unique = format!(
        "aperture-{label}-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    );
    let path = std::env::temp_dir().join(unique);
    let mut f = std::fs::File::create(&path).expect("create temp config");
    f.write_all(toml.as_bytes()).expect("write temp config");
    path
}

/// Run `aperture --config <path>` to completion and return its output.
/// Used only for the refusal rows, which exit immediately (no bind, no
/// long-running listener) — so a plain `.output()` terminates promptly.
fn run_aperture_with_config(path: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_aperture"))
        .args(["--config", path.to_str().expect("utf-8 path")])
        .output()
        .expect("spawn aperture binary")
}

// =========================================================================
// AC-1 — tls.enabled = true refuses (in-process seam + binary)
// =========================================================================

/// AC-1 (seam): `tls.enabled = true` → `into_config` returns `Err`.
/// Strongest AC-4 observable: no `Config`, so no bind path is reachable.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: tls-config-reject-v0"]
async fn ac1_tls_enabled_true_refuses_config_construction() {
    let toml = r#"
        [aperture.transport.grpc]
        bind_addr = "127.0.0.1:0"

        [aperture.transport.http]
        bind_addr = "127.0.0.1:0"

        [aperture.security.tls]
        enabled = true
        cert_path = "/nowhere/cert.pem"
        key_path  = "/nowhere/key.pem"
    "#;
    let result = Config::from_toml_str(toml);
    let err =
        result.expect_err("tls.enabled=true must refuse: into_config returns Err(ConfigError)");
    assert!(
        err.to_string().contains("tls.enabled"),
        "refusal must name tls.enabled; got: {err}"
    );
}

/// AC-1 + AC-4 (binary, @real-io): `aperture --config` with `tls.enabled=true`
/// exits 2, prints a stderr line carrying `event=config_validation_failed`
/// naming `tls.enabled`, and binds no plaintext listener.
#[test]
#[ignore = "RED until DELIVER: tls-config-reject-v0"]
fn ac1_tls_enabled_true_binary_exits_two_naming_tls_and_binds_nothing() {
    let toml = r#"
        [aperture.security.tls]
        enabled = true
    "#;
    let path = write_temp_config("ac1-tls", toml);
    let output = run_aperture_with_config(&path);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        output.status.code(),
        Some(2),
        "tls.enabled=true must exit 2; stderr: {stderr}"
    );
    assert!(
        stderr.contains(REFUSAL_EVENT),
        "stderr must carry event={REFUSAL_EVENT}; got: {stderr}"
    );
    assert!(
        stderr.contains("tls.enabled"),
        "refusal line must name tls.enabled; got: {stderr}"
    );
    // AC-4: the process exited (code 2) before constructing a Config, so no
    // listener was ever bound on the default OTLP ports.
    assert!(
        connect_refused_on_default_otlp_ports(),
        "no plaintext listener may be bound on 4317/4318 after a refusal"
    );
}

// =========================================================================
// AC-2 — auth.spiffe.enabled = true refuses (tls off)
// =========================================================================

/// AC-2 (seam): `auth.spiffe.enabled = true`, `tls.enabled = false` →
/// `into_config` returns `Err` naming `auth.spiffe.enabled`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: tls-config-reject-v0"]
async fn ac2_spiffe_enabled_true_refuses_config_construction() {
    let toml = r#"
        [aperture.transport.grpc]
        bind_addr = "127.0.0.1:0"

        [aperture.transport.http]
        bind_addr = "127.0.0.1:0"

        [aperture.security.tls]
        enabled = false

        [aperture.security.auth.spiffe]
        enabled = true
        trust_domain = "example.org"
    "#;
    let err = Config::from_toml_str(toml)
        .expect_err("auth.spiffe.enabled=true must refuse: into_config returns Err");
    assert!(
        err.to_string().contains("auth.spiffe.enabled"),
        "refusal must name auth.spiffe.enabled; got: {err}"
    );
    assert!(
        !err.to_string().contains("tls.enabled"),
        "spiffe-only refusal must not name tls.enabled; got: {err}"
    );
}

/// AC-2 + AC-4 (binary, @real-io): exits 2, stderr names
/// `auth.spiffe.enabled`, no listener bound.
#[test]
#[ignore = "RED until DELIVER: tls-config-reject-v0"]
fn ac2_spiffe_enabled_true_binary_exits_two_naming_spiffe_and_binds_nothing() {
    let toml = r#"
        [aperture.security.tls]
        enabled = false

        [aperture.security.auth.spiffe]
        enabled = true
    "#;
    let path = write_temp_config("ac2-spiffe", toml);
    let output = run_aperture_with_config(&path);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        output.status.code(),
        Some(2),
        "auth.spiffe.enabled=true must exit 2; stderr: {stderr}"
    );
    assert!(
        stderr.contains(REFUSAL_EVENT),
        "stderr must carry event={REFUSAL_EVENT}; got: {stderr}"
    );
    assert!(
        stderr.contains("auth.spiffe.enabled"),
        "refusal line must name auth.spiffe.enabled; got: {stderr}"
    );
    assert!(
        connect_refused_on_default_otlp_ports(),
        "no plaintext listener may be bound on 4317/4318 after a refusal"
    );
}

// =========================================================================
// AC-3 — both knobs true refuses, names the requested knob(s)
// =========================================================================

/// AC-3 (seam): both `true` → `into_config` returns `Err` naming BOTH
/// requested knobs; it does not silently pick one and proceed.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED until DELIVER: tls-config-reject-v0"]
async fn ac3_both_knobs_true_refuses_naming_both() {
    let toml = r#"
        [aperture.transport.grpc]
        bind_addr = "127.0.0.1:0"

        [aperture.transport.http]
        bind_addr = "127.0.0.1:0"

        [aperture.security.tls]
        enabled = true

        [aperture.security.auth.spiffe]
        enabled = true
    "#;
    let err = Config::from_toml_str(toml)
        .expect_err("both knobs true must refuse: into_config returns Err");
    let msg = err.to_string();
    assert!(
        msg.contains("tls.enabled") && msg.contains("auth.spiffe.enabled"),
        "both-true refusal must name BOTH requested knobs; got: {msg}"
    );
}

/// AC-3 + AC-4 (binary, @real-io): exits 2, stderr names both knobs,
/// no listener bound, no silent proceed.
#[test]
#[ignore = "RED until DELIVER: tls-config-reject-v0"]
fn ac3_both_knobs_true_binary_exits_two_naming_both_and_binds_nothing() {
    let toml = r#"
        [aperture.security.tls]
        enabled = true

        [aperture.security.auth.spiffe]
        enabled = true
    "#;
    let path = write_temp_config("ac3-both", toml);
    let output = run_aperture_with_config(&path);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        output.status.code(),
        Some(2),
        "both knobs true must exit 2; stderr: {stderr}"
    );
    assert!(
        stderr.contains(REFUSAL_EVENT),
        "stderr must carry event={REFUSAL_EVENT}; got: {stderr}"
    );
    assert!(
        stderr.contains("tls.enabled") && stderr.contains("auth.spiffe.enabled"),
        "both-true refusal line must name BOTH requested knobs; got: {stderr}"
    );
    assert!(
        connect_refused_on_default_otlp_ports(),
        "no plaintext listener may be bound on 4317/4318 after a refusal"
    );
}

// =========================================================================
// AC-4 — no plaintext bind on any refusal (structural seam assertion)
// =========================================================================
//
// AC-4 is asserted three ways: (a) at the seam, each refusal returns
// Err so no Config exists and the bind path is unreachable (the
// strongest, refactor-proof guarantee — covered by the ac{1,2,3}_*_refuses_
// config_construction tests above); (b) in the binary, the process exited
// (code 2) and a connect attempt to the default OTLP ports is refused
// (the ac{1,2,3}_*_binary_* tests). The helper below backs (b).

/// True iff a TCP connect to BOTH default OTLP ports on loopback is
/// refused — i.e. aperture bound no plaintext listener. A refused connect
/// is the black-box observable for "no listener". (The default bind is
/// `0.0.0.0:PORT`; loopback reaches it if it were bound.)
fn connect_refused_on_default_otlp_ports() -> bool {
    use std::net::TcpStream;
    use std::time::Duration;
    let grpc: SocketAddr = "127.0.0.1:4317".parse().expect("grpc addr");
    let http: SocketAddr = "127.0.0.1:4318".parse().expect("http addr");
    let refused =
        |addr: SocketAddr| TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_err();
    refused(grpc) && refused(http)
}

// =========================================================================
// AC-5 — NEGATIVE CONTROL: both knobs false → starts + binds (unchanged)
// =========================================================================
//
// NOT #[ignore]d: asserts today's preserved behaviour. Passes now; DELIVER
// must keep it green (the non-regression guard for the common case). Uses
// the ephemeral 127.0.0.1:0 override (config/mod.rs:226-236) so it binds
// collision-free in the parallel suite — NOT the default 4317/4318.

/// AC-5: `tls.enabled = false` and `auth.spiffe.enabled = false` →
/// `into_config` succeeds (config is constructable, so startup proceeds).
#[tokio::test(flavor = "multi_thread")]
async fn ac5_both_knobs_false_into_config_succeeds() {
    let toml = r#"
        [aperture.transport.grpc]
        bind_addr = "127.0.0.1:0"

        [aperture.transport.http]
        bind_addr = "127.0.0.1:0"

        [aperture.security.tls]
        enabled = false

        [aperture.security.auth.spiffe]
        enabled = false
    "#;
    let result = Config::from_toml_str(toml);
    assert!(
        result.is_ok(),
        "both knobs false must NOT refuse; got: {result:?}"
    );
}

/// AC-5: with both knobs false, aperture starts, binds both listeners, and
/// emits no refusal event — byte-for-byte today's behaviour. Ephemeral
/// ports keep the parallel suite collision-free.
#[tokio::test(flavor = "multi_thread")]
async fn ac5_both_knobs_false_starts_binds_and_emits_no_refusal_event() {
    let ((grpc_bound, http_bound), events) = capture_stderr_events(|| async {
        let toml = r#"
            [aperture.transport.grpc]
            bind_addr = "127.0.0.1:0"

            [aperture.transport.http]
            bind_addr = "127.0.0.1:0"

            [aperture.security.tls]
            enabled = false

            [aperture.security.auth.spiffe]
            enabled = false
        "#;
        let config = Config::from_toml_str(toml).expect("config parses and builds");
        let sink: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
        let handle = aperture::spawn(config, sink).await.expect("spawn");
        handle.wait_until_ready().await.expect("ready");
        (
            handle.grpc_addr().port() != 0,
            handle.http_addr().port() != 0,
        )
    })
    .await;

    assert!(grpc_bound, "gRPC listener must bind when knobs are off");
    assert!(http_bound, "HTTP listener must bind when knobs are off");
    expect_stderr_event(&events, "startup");
    expect_no_stderr_event(&events, REFUSAL_EVENT);
}

// =========================================================================
// AC-6 — NEGATIVE CONTROL: [security] tables absent ≡ both false
// =========================================================================
//
// NOT #[ignore]d: asserts today's preserved behaviour. serde defaults the
// knobs to false when the [security] tables are omitted entirely.

/// AC-6: config omitting the `[security]` tables behaves identically to
/// both-false — `into_config` succeeds.
#[tokio::test(flavor = "multi_thread")]
async fn ac6_security_tables_absent_into_config_succeeds() {
    let toml = r#"
        [aperture.transport.grpc]
        bind_addr = "127.0.0.1:0"

        [aperture.transport.http]
        bind_addr = "127.0.0.1:0"
    "#;
    let result = Config::from_toml_str(toml);
    assert!(
        result.is_ok(),
        "absent [security] tables must NOT refuse; got: {result:?}"
    );
}

/// AC-6: with the `[security]` tables absent, aperture starts, binds both
/// listeners, and emits no refusal event — identical to AC-5.
#[tokio::test(flavor = "multi_thread")]
async fn ac6_security_tables_absent_starts_binds_and_emits_no_refusal_event() {
    let ((grpc_bound, http_bound), events) = capture_stderr_events(|| async {
        let toml = r#"
            [aperture.transport.grpc]
            bind_addr = "127.0.0.1:0"

            [aperture.transport.http]
            bind_addr = "127.0.0.1:0"
        "#;
        let config = Config::from_toml_str(toml).expect("config parses and builds");
        let sink: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
        let handle = aperture::spawn(config, sink).await.expect("spawn");
        handle.wait_until_ready().await.expect("ready");
        (
            handle.grpc_addr().port() != 0,
            handle.http_addr().port() != 0,
        )
    })
    .await;

    assert!(
        grpc_bound,
        "gRPC listener must bind when [security] is absent"
    );
    assert!(
        http_bound,
        "HTTP listener must bind when [security] is absent"
    );
    expect_stderr_event(&events, "startup");
    expect_no_stderr_event(&events, REFUSAL_EVENT);
}

// =========================================================================
// AC-7 — comment correction at sinks.rs:94-95
// =========================================================================
//
// AC-7 is a DELIVER-VERIFIED criterion, NOT a runtime test. The brief's
// "For Acceptance Designer" note classifies it as "a code-review/lint
// observable, not a runtime one". Asserting the literal text of a source
// comment from an integration test would couple the suite to a source-line
// detail and break on any rewording. DELIVER verifies the corrected comment
// at code-review time (see distill/ac-coverage.md and wave-decisions.md).
//
// A `_ = RED;` reference keeps the ignore-reason constant in use so the
// shared constant documents intent for the refusal tests above.
#[test]
fn ac7_comment_correction_is_a_deliver_verified_criterion() {
    // Intentionally trivial: AC-7 is verified by DELIVER code review, not at
    // runtime. This test exists to document that decision in the suite.
    assert_eq!(RED, "RED until DELIVER: tls-config-reject-v0");
}
