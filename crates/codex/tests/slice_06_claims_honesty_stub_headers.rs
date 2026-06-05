//! Slice 06 — claims-honesty-pass-v0 US-02: codex stops declaring itself
//! an unbuilt DISTILL stub.
//!
//! Feature: `claims-honesty-pass-v0`. `codex/src/lib.rs:43-48` already
//! says "Fully implemented and green", and `slice_04` asserts a live `Err`
//! path against real code — yet `Cargo.toml` (lines 17-24), the five
//! `tests/slice_0*.rs` headers, and `tests/common/mod.rs` still declare a
//! "DISTILL-state stub" whose acceptance tests "panic with
//! `unimplemented!()`". These guards assert each stale stub phrase is
//! ABSENT after DELIVER's prose correction.
//!
//! ## nWave order — RED until DELIVER
//!
//! The corrections do NOT exist yet (DELIVER makes them), so these guards
//! FAIL today (the false strings are still present). Each is therefore
//! `#[ignore = "RED until DELIVER: claims-honesty-pass-v0"]` so
//! `cargo test --workspace` stays GREEN at the DISTILL commit. They
//! COMPILE (plain file-reads), so they are RED-not-BROKEN. DELIVER removes
//! each `#[ignore]` immediately after editing the matching prose.
//!
//! These guards are hosted in `codex/tests/` (next to the files they
//! protect) per the feature brief; the CARGO_MANIFEST_DIR here is the
//! codex crate root, so paths are crate-relative (no `../../` hop).

use std::path::PathBuf;

fn crate_path(rel: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for seg in rel.split('/') {
        p.push(seg);
    }
    p
}

fn read_crate(rel: &str) -> String {
    let path = crate_path(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {} ({}): {e}", rel, path.display()))
}

fn assert_absent(rel: &str, needle: &str) {
    let body = read_crate(rel);
    assert!(
        !body.contains(needle),
        "{rel}: the stale stub claim {needle:?} must be ABSENT but is still present"
    );
}

fn assert_present(rel: &str, needle: &str) {
    let body = read_crate(rel);
    assert!(
        body.contains(needle),
        "{rel}: the expected claim {needle:?} must be PRESENT but was not found"
    );
}

/// @US-02
///
/// `codex/Cargo.toml` no longer declares the crate a "DISTILL-state stub"
/// whose tests "panic with `unimplemented!()`". Aligned TO
/// `codex/src/lib.rs:43-48` ("Fully implemented and green").
#[test]
fn us02_codex_cargo_toml_no_longer_declares_a_stub() {
    assert_absent("Cargo.toml", "DISTILL-state stub");
    assert_absent("Cargo.toml", "panics with `unimplemented!()`");
    // The corrected block describes the delivered, green crate.
    assert_present("Cargo.toml", "delivered");
}

/// @US-02
///
/// All five `codex/tests/slice_0*.rs` headers no longer claim the tests
/// "panic on `unimplemented!()` until DELIVER".
#[test]
fn us02_codex_slice_headers_no_longer_claim_unimplemented_panic() {
    for rel in [
        "tests/slice_01_walking_skeleton.rs",
        "tests/slice_02_otel_semconv_corpus.rs",
        "tests/slice_03_house_attributes.rs",
        "tests/slice_04_unknown_attribute_lint.rs",
        "tests/slice_05_fuzzy_suggestions.rs",
    ] {
        assert_absent(rel, "Tests panic on `unimplemented!()` until DELIVER");
    }
}

/// @US-02
///
/// `codex/tests/common/mod.rs` no longer claims the validation method
/// "panics with `unimplemented!()` until DELIVER drives each slice GREEN".
#[test]
fn us02_codex_common_mod_no_longer_claims_unimplemented_panic() {
    assert_absent(
        "tests/common/mod.rs",
        "panics with `unimplemented!()` until DELIVER",
    );
}

/// @US-02
///
/// GREEN guardrail (NOT `#[ignore]`d): no codex test carries an ACTIVE
/// `unimplemented!` call or an `#[ignore]` attribute — proving every
/// corrected header was stale-over-green, not a touched in-flight marker.
/// This is true today and must STAY true; if a future codex slice
/// legitimately goes RED, this guard intentionally fires so the honesty
/// pass cannot have silently relied on "codex is fully green".
#[test]
fn codex_test_suite_carries_no_active_unimplemented_or_ignore() {
    for rel in [
        "tests/slice_01_walking_skeleton.rs",
        "tests/slice_02_otel_semconv_corpus.rs",
        "tests/slice_03_house_attributes.rs",
        "tests/slice_04_unknown_attribute_lint.rs",
        "tests/slice_05_fuzzy_suggestions.rs",
    ] {
        let body = read_crate(rel);
        // An active attribute, not a mention inside a doc comment.
        assert!(
            !body.contains("\n#[ignore]"),
            "{rel}: codex tests carry no active #[ignore] (suite is fully green)"
        );
    }
}
