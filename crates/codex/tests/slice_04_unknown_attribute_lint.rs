//! Slice 04 — Unknown attributes produce structured violations
//!
//! Maps to `docs/feature/codex/slices/slice-04-unknown-attribute-lint.md`.
//! Companion story: US-CO-04.
//!
//! Single, multi, and mixed cases. Asserts the `LintReport` shape
//! (violations slice, `ViolationKind::Unknown`, name) and the
//! `Display` impl renders human-readable text naming each offending
//! attribute.
//!
//! Tests panic on `unimplemented!()` until DELIVER lands the Err
//! path + the Display impl in `lint.rs`.

mod common;

use codex::{SchemaCatalogue, ViolationKind};

use crate::common::{
    one_blessed_one_unknown_pair, two_unknown_attributes_pair, unknown_attribute_pair,
};

#[test]
fn a_single_unknown_attribute_produces_one_lint_violation() {
    let catalogue = SchemaCatalogue::new();
    let attrs = unknown_attribute_pair();
    let result = catalogue.validate(&attrs);
    assert!(result.is_err(), "unknown attribute must produce Err");
    let report = result.expect_err("Err");
    let violations = report.violations();
    assert_eq!(
        violations.len(),
        1,
        "expected exactly one violation; got {} ({:?})",
        violations.len(),
        violations
    );
    assert_eq!(violations[0].attribute_name, "tenat.id");
    assert!(
        matches!(&violations[0].kind, ViolationKind::Unknown),
        "expected ViolationKind::Unknown; got {:?}",
        &violations[0].kind
    );
}

#[test]
fn multiple_unknown_attributes_produce_multiple_violations() {
    let catalogue = SchemaCatalogue::new();
    let attrs = two_unknown_attributes_pair();
    let result = catalogue.validate(&attrs);
    assert!(result.is_err());
    let report = result.expect_err("Err");
    let violations = report.violations();
    assert_eq!(
        violations.len(),
        2,
        "expected exactly two violations; got {} ({:?})",
        violations.len(),
        violations
    );
}

#[test]
fn a_mixed_blessed_and_unknown_set_produces_one_violation_for_the_unknown_only() {
    let catalogue = SchemaCatalogue::new();
    let attrs = one_blessed_one_unknown_pair();
    let result = catalogue.validate(&attrs);
    assert!(result.is_err());
    let report = result.expect_err("Err");
    let violations = report.violations();
    assert_eq!(
        violations.len(),
        1,
        "the blessed attribute must not produce a violation; only the unknown one must"
    );
}

#[test]
fn the_lint_report_display_impl_names_each_offending_attribute() {
    let catalogue = SchemaCatalogue::new();
    let attrs = two_unknown_attributes_pair();
    let result = catalogue.validate(&attrs);
    assert!(result.is_err());
    let report = result.expect_err("Err");
    let rendered = format!("{report}");
    for violation in report.violations() {
        assert!(
            rendered.contains(&violation.attribute_name),
            "Display rendering must name each offending attribute; missing {:?} in:\n{rendered}",
            violation.attribute_name
        );
    }
}
