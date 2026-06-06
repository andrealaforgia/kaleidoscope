//! Slice 07 — TLS / SPIFFE schema knob (forward-compat insurance).
//!
//! Maps to `docs/feature/aperture/slices/slice-07-tls-schema-knob.md`.
//! Companion stories: none (this is the only `@infrastructure` slice
//! in v0).
//!
//! ## Contract evolution (ADR-0061 supersedes ADR-0008's runtime reaction)
//!
//! ADR-0008 placed two forward-compat security knobs in the v0 schema
//! (`tls.enabled`, `auth.spiffe.enabled`) defaulting off, and originally
//! chose a *warn-and-continue* runtime reaction to `= true`. ADR-0061
//! supersedes ONLY that runtime reaction: setting either knob to `true`
//! now causes config validation to **refuse to start** (`event=
//! config_validation_failed`, exit 2, no listener bound) rather than
//! warning and binding plaintext. The forward-compat *schema* decision
//! is preserved unchanged: the keys still parse and still default off.
//!
//! The user-observable contract this file now pins:
//!
//! - The v0 config schema still accepts `tls.enabled`, `tls.cert_path`,
//!   `tls.key_path`, `auth.spiffe.enabled`, `auth.spiffe.trust_domain`
//!   without parse errors (schema preserved — ADR-0008).
//! - All five keys default to off / empty when omitted (schema preserved).
//! - Setting `tls.enabled = true` now **refuses** config construction
//!   (`into_config` → `Err(ConfigError)` naming `tls.enabled`) — the
//!   superseded warn-and-bind path is gone (ADR-0061).
//! - Setting `auth.spiffe.enabled = true` refuses analogously, naming
//!   `auth.spiffe.enabled`.
//! - Setting an unknown key is still rejected at config load
//!   (`deny_unknown_fields`).
//!
//! The full refusal truth table (both-true, exit codes, binary @real-io
//! surface, no-plaintext-bind) lives in `slice_09_tls_config_reject.rs`,
//! which owns the ADR-0061 contract. This file retains the schema-parse /
//! defaults-off scenarios (still valid) and flips the four scenarios that
//! encoded the superseded warn-and-continue reaction.
//!
//! These tests use the `Config::from_toml_str` entry point so the
//! schema is exercised verbatim, not via the typed builder.

mod common;

use std::sync::Arc;

use aperture::config::Config;
use aperture::ports::OtlpSink;
use aperture::testing::RecordingSink;

use crate::common::{capture_stderr_events, expect_no_stderr_event, expect_stderr_event};

const REFUSAL_EVENT: &str = "config_validation_failed";

/// Write a readable secret + tenant catalogue to temp files and return a
/// complete `[aperture.security.auth.jwt]` TOML block referencing them.
///
/// Since aegis-ingest-auth-v0 (ADR-0068 DD4) the TOML loader REFUSES TO
/// START without a complete, readable auth block. The schema tests below
/// that assert a config PARSES / STARTS append this block; it is the
/// auth precondition, orthogonal to the TLS/SPIFFE behaviour they test.
fn jwt_block(label: &str) -> String {
    let dir = std::env::temp_dir();
    let stamp = format!("{}-{label}", std::process::id());
    let secret = dir.join(format!("aperture-slice07-secret-{stamp}.key"));
    let catalogue = dir.join(format!("aperture-slice07-cat-{stamp}.toml"));
    std::fs::write(&secret, b"slice07-test-secret-bytes").expect("write secret");
    std::fs::write(&catalogue, b"[[tenants]]\nid = \"acme-prod\"\n").expect("write catalogue");
    format!(
        "\n[aperture.security.auth.jwt]\n\
         issuer = \"acme-observability\"\n\
         audience = \"kaleidoscope-ingest\"\n\
         secret_file = \"{}\"\n\
         catalogue_path = \"{}\"\n",
        secret.display(),
        catalogue.display()
    )
}

// =========================================================================
// Schema parses TLS + SPIFFE keys at default off (PRESERVED — ADR-0008)
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn config_with_all_security_keys_at_defaults_parses_without_error() {
    let toml = format!(
        r#"
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
    {}"#,
        jwt_block("defaults")
    );
    let result = Config::from_toml_str(&toml);
    assert!(
        result.is_ok(),
        "default-security config should parse; got: {result:?}"
    );
}

// =========================================================================
// tls.enabled=true — REFUSES (ADR-0061; supersedes warn-and-continue)
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn tls_enabled_true_refuses_config_construction_naming_tls() {
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
    let err = Config::from_toml_str(toml)
        .expect_err("tls.enabled=true must now refuse (ADR-0061), not warn-and-bind");
    assert!(
        err.to_string().contains("tls.enabled"),
        "refusal must name tls.enabled; got: {err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn tls_enabled_true_does_not_bind_or_emit_a_warn_line() {
    // The superseded contract bound a plaintext listener and emitted a
    // `tls_not_supported_in_v0` warn line. Under ADR-0061 the config is
    // never constructed, so `spawn` is never reached: no listener binds
    // and no warn line is emitted. We capture the (empty) event stream
    // around the refusing `from_toml_str` to prove no warn surfaces.
    let (refused, events) = capture_stderr_events(|| async {
        let toml = r#"
            [aperture.transport.grpc]
            bind_addr = "127.0.0.1:0"

            [aperture.transport.http]
            bind_addr = "127.0.0.1:0"

            [aperture.security.tls]
            enabled = true
        "#;
        Config::from_toml_str(toml).is_err()
    })
    .await;
    assert!(refused, "tls.enabled=true must refuse config construction");
    expect_no_stderr_event(&events, "tls_not_supported_in_v0");
}

// =========================================================================
// spiffe.enabled=true — REFUSES (ADR-0061)
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn spiffe_enabled_true_refuses_config_construction_naming_spiffe() {
    let toml = r#"
        [aperture.transport.grpc]
        bind_addr = "127.0.0.1:0"

        [aperture.transport.http]
        bind_addr = "127.0.0.1:0"

        [aperture.security.auth.spiffe]
        enabled = true
        trust_domain = "example.org"
    "#;
    let err = Config::from_toml_str(toml)
        .expect_err("auth.spiffe.enabled=true must now refuse (ADR-0061)");
    let msg = err.to_string();
    assert!(
        msg.contains("auth.spiffe.enabled"),
        "refusal must name auth.spiffe.enabled; got: {msg}"
    );
    assert!(
        !msg.contains("tls.enabled"),
        "spiffe-only refusal must not name tls.enabled; got: {msg}"
    );
}

// =========================================================================
// Defaults — config starts and binds, no refusal event (PRESERVED)
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn config_with_security_keys_omitted_starts_and_emits_no_refusal_event() {
    let ((), events) = capture_stderr_events(|| async {
        let toml = format!(
            r#"
            [aperture.transport.grpc]
            bind_addr = "127.0.0.1:0"

            [aperture.transport.http]
            bind_addr = "127.0.0.1:0"
        {}"#,
            jwt_block("omitted")
        );
        let config = Config::from_toml_str(&toml).expect("config parses");
        let sink: Arc<dyn OtlpSink> = Arc::new(RecordingSink::new());
        let handle = aperture::spawn(config, sink).await.expect("spawn");
        handle.wait_until_ready().await.expect("ready");
    })
    .await;
    expect_stderr_event(&events, "startup");
    expect_no_stderr_event(&events, REFUSAL_EVENT);
    expect_no_stderr_event(&events, "tls_not_supported_in_v0");
}

// =========================================================================
// Unknown keys — rejected at config load (PRESERVED — deny_unknown_fields)
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
