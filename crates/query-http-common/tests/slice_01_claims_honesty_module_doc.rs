//! Slice 01 — claims-honesty-pass-v0 US-03 (query-http-common half):
//! the module doc stops claiming its bodies are unimplemented scaffolds.
//!
//! Feature: `claims-honesty-pass-v0`. `query-http-common/src/lib.rs:30-42`
//! still says "DISTILL scaffold — DELIVER fills the bodies … All free
//! functions are `unimplemented!("__SCAFFOLD__ query-http-common-v0
//! RED")`" — directly above FULLY-LIVE bodies, each of which already says
//! "DELIVER state: implemented" (`parse_time_range`,
//! `resolve_tenant_or_refuse`, `error_response`, `init_tracing`). This
//! guard asserts the stale module-doc scaffold claim is ABSENT after
//! DELIVER's correction and the implemented-helper wording is PRESENT.
//!
//! ## Bidirectional context
//!
//! This is ONE stale-over-green half of the US-03 bidirectional guard.
//! The OTHER half — that the genuinely-RED in-flight `__SCAFFOLD__` /
//! `#[ignore]` markers across the workspace REMAIN PRESENT — lives in
//! `otlp-conformance-harness/tests/slice_08_claims_honesty_doc_guards.rs`
//! (`us03_in_flight_scaffold_markers_remain_present`, GREEN today), so the
//! prose pass cannot over-reach and silence an honest in-flight marker.
//!
//! ## nWave order — RED until DELIVER
//!
//! The correction does not exist yet, so this guard FAILS today (the
//! stale string is still present) and is `#[ignore = "RED until DELIVER:
//! claims-honesty-pass-v0"]` to keep `cargo test --workspace` GREEN at the
//! DISTILL commit. It COMPILES (plain file-read), so it is RED-not-BROKEN.

use std::path::PathBuf;

fn lib_rs() -> String {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("src");
    p.push("lib.rs");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// @US-03
///
/// The `query-http-common` module doc no longer claims the free functions
/// are `unimplemented!("__SCAFFOLD__ … RED")` DISTILL scaffolds; it
/// describes the four implemented helpers, matching the live bodies and
/// each fn's own "DELIVER state: implemented" note.
#[test]
fn us03_module_doc_no_longer_claims_unimplemented_scaffold() {
    let body = lib_rs();
    assert!(
        !body.contains("DISTILL scaffold — DELIVER fills the bodies"),
        "the stale module-doc scaffold claim must be ABSENT but is still present"
    );
    assert!(
        !body.contains(
            "All free functions are `unimplemented!(\"__SCAFFOLD__ query-http-common-v0 RED\")`"
        ),
        "the stale `unimplemented!` __SCAFFOLD__ claim must be ABSENT but is still present"
    );
}

/// @US-03
///
/// GREEN guardrail (NOT `#[ignore]`d): each fn's own "DELIVER state:
/// implemented" note is PRESENT today — proving the module-doc claim was
/// stale-over-green, not a description of a true RED state. Must STAY
/// true after the correction.
#[test]
fn per_function_implemented_notes_are_present_today() {
    let body = lib_rs();
    assert!(
        body.contains("DELIVER state: implemented"),
        "the per-fn implemented notes prove the bodies are live (stale-over-green)"
    );
}
