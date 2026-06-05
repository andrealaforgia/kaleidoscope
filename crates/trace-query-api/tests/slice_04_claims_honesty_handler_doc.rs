//! Slice 04 — claims-honesty-pass-v0 US-03 (trace-query-api half):
//! the handler doc stops claiming the live handler is an unimplemented
//! scaffold.
//!
//! Feature: `claims-honesty-pass-v0`. `trace-query-api/src/lib.rs:207-209`
//! and `:228-232` still call `handle_traces_by_id` (and its
//! `TracesByIdParams`) a "`unimplemented!` scaffold … DELIVER implements
//! the body" — yet the body at `:233-292` is a fully-live
//! resolve->parse->get_trace->cap->serialise orchestration, and
//! `parse_trace_id` (`:304-320`) is real too. This guard asserts the
//! stale "`unimplemented!` scaffold" claim is ABSENT after DELIVER's
//! correction.
//!
//! ## Bidirectional context
//!
//! This is the SECOND stale-over-green half of the US-03 bidirectional
//! guard (the first is `query-http-common/tests/slice_01_*`). The
//! IN-FLIGHT half — genuinely-RED `__SCAFFOLD__` / `#[ignore]` markers
//! across the workspace REMAIN PRESENT — lives in
//! `otlp-conformance-harness/tests/slice_08_claims_honesty_doc_guards.rs`
//! (GREEN today), so the prose pass cannot over-reach.
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
/// The `handle_traces_by_id` doc (and the `TracesByIdParams` doc above it)
/// no longer call the live handler an "`unimplemented!` scaffold" for
/// DISTILL Mandate 7. The corrected doc describes the implemented
/// lookup-by-id orchestration, matching the body.
#[test]
fn us03_handler_doc_no_longer_claims_unimplemented_scaffold() {
    let body = lib_rs();
    assert!(
        !body.contains("Scaffold for DISTILL Mandate 7 RED-not-BROKEN: the handler is"),
        "the stale handler-scaffold claim must be ABSENT but is still present"
    );
    assert!(
        !body.contains("the handler is\n    /// `unimplemented!`")
            && !body.contains("the handler is\n/// `unimplemented!`"),
        "no residual `unimplemented!` scaffold phrasing over the live handler"
    );
    // The corrected doc describes the live orchestration.
    assert!(
        body.contains("get_trace"),
        "the corrected doc names the live resolve->parse->get_trace->cap->serialise flow"
    );
}

/// @US-03
///
/// GREEN guardrail (NOT `#[ignore]`d): the live handler body IS present
/// today — proving the scaffold doc was stale-over-green. The async
/// handler `handle_traces_by_id` and the `parse_trace_id` helper are real.
/// Must STAY true after the correction.
#[test]
fn live_handler_body_is_present_today() {
    let body = lib_rs();
    assert!(
        body.contains("async fn handle_traces_by_id"),
        "the live handler must exist (stale-over-green precondition)"
    );
    assert!(
        body.contains("fn parse_trace_id"),
        "the live parse helper must exist (stale-over-green precondition)"
    );
}
