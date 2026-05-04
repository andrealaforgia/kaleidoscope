//! Slice 07 — TLS / SPIFFE schema knob (forward-compat insurance).
//!
//! Maps to `docs/feature/aperture/slices/slice-07-tls-schema-knob.md`.
//! Companion stories: none (this is the only `@infrastructure` slice
//! in v0).
//!
//! The user-observable contract:
//!
//! - The v0 config schema accepts `tls.enabled`, `tls.cert_path`,
//!   `tls.key_path`, `auth.spiffe.enabled`, `auth.spiffe.trust_domain`
//!   without parse errors.
//! - All five keys default to off / empty when omitted.
//! - Setting `tls.enabled = true` produces exactly one
//!   `event=tls_not_supported_in_v0` warn line at startup; listeners
//!   still bind plaintext; `/readyz` reaches 200.
//! - Setting `auth.spiffe.enabled = true` produces an analogous warn
//!   line.
//! - Setting an unknown key is rejected at config load
//!   (`event=config_validation_failed`).
//!
//! These tests use the `Config::from_toml_str` entry point so the
//! schema is exercised verbatim, not via the typed builder.

mod common;

use std::sync::Arc;

use aperture::config::Config;
use aperture::ports::OtlpSink;
use aperture::testing::RecordingSink;

use crate::common::{capture_stderr_events, expect_no_stderr_event, expect_stderr_event};

// =========================================================================
// Schema parses TLS + SPIFFE keys at default off
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn config_with_all_security_keys_at_defaults_parses_without_error() {
    let toml = r#"
        [aperture.transport.grpc]
        bind_addr = "127.0.0.1:0"

        [aperture.transport.http]
        bind_addr = "127.0.0.1:0"

        [aperture.security.tls]
        enabled = false
        cert_path = ""
        key_path = ""

        [aperture.security.auth.spiffe]
        enabled = false
        trust_domain = ""
    "#;
    let result = Config::from_toml_str(toml);
    assert!(
        result.is_ok(),
        "default-security config should parse; got: {result:?}"
    );
}

// =========================================================================
// tls.enabled=true — warn line, plaintext continues
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn tls_enabled_true_emits_tls_not_supported_in_v0_warn_line() {
    let ((), events) = capture_stderr_events(|| async {
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
        let config = Config::from_toml_str(toml).expect("config parses");
        let sink: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
        let handle = aperture::spawn(config, sink).await.expect("spawn");
        handle.wait_until_ready().await.expect("ready");
    })
    .await;
    let evt = expect_stderr_event(&events, "tls_not_supported_in_v0");
    assert_eq!(evt.level, "warn");
}

#[tokio::test(flavor = "multi_thread")]
async fn tls_enabled_true_emits_exactly_one_warn_line() {
    let ((), events) = capture_stderr_events(|| async {
        let toml = r#"
            [aperture.transport.grpc]
            bind_addr = "127.0.0.1:0"

            [aperture.transport.http]
            bind_addr = "127.0.0.1:0"

            [aperture.security.tls]
            enabled = true
        "#;
        let config = Config::from_toml_str(toml).expect("config parses");
        let sink: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
        let handle = aperture::spawn(config, sink).await.expect("spawn");
        handle.wait_until_ready().await.expect("ready");
    })
    .await;
    let count = events
        .iter()
        .filter(|e| e.event == "tls_not_supported_in_v0")
        .count();
    assert_eq!(count, 1, "exactly one warn line; got: {count}");
}

#[tokio::test(flavor = "multi_thread")]
async fn tls_enabled_true_listeners_still_bind_and_readyz_returns_ok() {
    let toml = r#"
        [aperture.transport.grpc]
        bind_addr = "127.0.0.1:0"

        [aperture.transport.http]
        bind_addr = "127.0.0.1:0"

        [aperture.security.tls]
        enabled = true
    "#;
    let config = Config::from_toml_str(toml).expect("config parses");
    let sink: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
    let handle = aperture::spawn(config, sink).await.expect("spawn");
    handle.wait_until_ready().await.expect("ready");
    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://{}/readyz", handle.http_addr()))
        .send()
        .await
        .expect("GET /readyz");
    assert_eq!(response.status().as_u16(), 200);
}

// =========================================================================
// spiffe.enabled=true — warn line, plaintext continues
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn spiffe_enabled_true_emits_warn_line() {
    let ((), events) = capture_stderr_events(|| async {
        let toml = r#"
            [aperture.transport.grpc]
            bind_addr = "127.0.0.1:0"

            [aperture.transport.http]
            bind_addr = "127.0.0.1:0"

            [aperture.security.auth.spiffe]
            enabled = true
            trust_domain = "example.org"
        "#;
        let config = Config::from_toml_str(toml).expect("config parses");
        let sink: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
        let handle = aperture::spawn(config, sink).await.expect("spawn");
        handle.wait_until_ready().await.expect("ready");
    })
    .await;
    // DESIGN/component-design names this `tls_not_supported_in_v0` for
    // both knobs (per the design's "spiffe.enabled = true on v0
    // produces exactly one analogous warn line"). DELIVER may choose
    // to namespace separately; this assertion captures that the
    // structural-equivalent warn surface is emitted.
    let any_warn = events
        .iter()
        .any(|e| e.level == "warn" && e.event == "tls_not_supported_in_v0");
    assert!(
        any_warn,
        "expected an analogous warn event for spiffe.enabled=true; got: {:?}",
        events
            .iter()
            .map(|e| (e.level.as_str(), e.event.as_str()))
            .collect::<Vec<_>>()
    );
}

// =========================================================================
// Defaults — no warn line when keys are absent or false
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn config_with_security_keys_omitted_does_not_emit_tls_warn_line() {
    let ((), events) = capture_stderr_events(|| async {
        let toml = r#"
            [aperture.transport.grpc]
            bind_addr = "127.0.0.1:0"

            [aperture.transport.http]
            bind_addr = "127.0.0.1:0"
        "#;
        let config = Config::from_toml_str(toml).expect("config parses");
        let sink: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
        let handle = aperture::spawn(config, sink).await.expect("spawn");
        handle.wait_until_ready().await.expect("ready");
    })
    .await;
    expect_no_stderr_event(&events, "tls_not_supported_in_v0");
}

// =========================================================================
// Unknown keys — rejected at config load
// =========================================================================
//
// DESIGN open-issue #3: figment must `deny_unknown_fields` so a
// misspelled key fails loud. Tests assert the user-observable
// behaviour: an unknown key produces a parse error, not a silent
// default-value-use.

#[tokio::test(flavor = "multi_thread")]
async fn config_with_unknown_key_is_rejected_at_load() {
    let toml = r#"
        [aperture.transport.grpc]
        bind_addr = "127.0.0.1:0"
        max_concurent_requests = 1024  # NB: typo (one 'r' missing)

        [aperture.transport.http]
        bind_addr = "127.0.0.1:0"
    "#;
    let result = Config::from_toml_str(toml);
    assert!(
        result.is_err(),
        "unknown key should be rejected; got: {result:?}"
    );
}
