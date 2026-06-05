//! Slice 05 — Fuzzy "did you mean" suggestions
//!
//! Maps to `docs/feature/codex/slices/slice-05-fuzzy-suggestions.md`.
//! Companion story: US-CO-05.
//!
//! Asserts that close typos (Levenshtein distance ≤ 2) populate
//! `LintViolation::nearest_blessed_match`, and far-distance unknown
//! attributes do not.
//!
//! Delivered and green: these tests exercise the live Levenshtein loop
//! and the suggestion-population logic.

mod common;

use codex::SchemaCatalogue;

use crate::common::{close_typo_pair, far_distance_pair};

#[test]
fn a_close_typo_produces_a_suggestion() {
    let catalogue = SchemaCatalogue::new();
    let attrs = close_typo_pair();
    let result = catalogue.validate(&attrs);
    assert!(result.is_err(), "close-typo attribute must produce Err");
    let report = result.expect_err("Err");
    let violations = report.violations();
    assert_eq!(violations.len(), 1);
    let violation = &violations[0];
    let suggestion = violation
        .nearest_blessed_match
        .as_deref()
        .expect("close typo must populate nearest_blessed_match");
    assert_eq!(
        suggestion, "tenant.id",
        "tenat.id should suggest tenant.id; got: {suggestion:?}"
    );
}

#[test]
fn a_far_distance_unknown_attribute_produces_no_suggestion() {
    let catalogue = SchemaCatalogue::new();
    let attrs = far_distance_pair();
    let result = catalogue.validate(&attrs);
    assert!(result.is_err());
    let report = result.expect_err("Err");
    let violations = report.violations();
    assert_eq!(violations.len(), 1);
    assert!(
        violations[0].nearest_blessed_match.is_none(),
        "far-distance attribute must not populate nearest_blessed_match; got: {:?}",
        violations[0].nearest_blessed_match
    );
}

#[test]
fn the_display_rendering_includes_the_suggestion_when_present() {
    let catalogue = SchemaCatalogue::new();
    let attrs = close_typo_pair();
    let result = catalogue.validate(&attrs);
    assert!(result.is_err());
    let report = result.expect_err("Err");
    let rendered = format!("{report}");
    assert!(
        rendered.contains("tenant.id"),
        "Display must include the suggested name when populated; got:\n{rendered}"
    );
}
