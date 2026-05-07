//! Slice 03 — Kaleidoscope-house attributes are first-class
//!
//! Maps to `docs/feature/codex/slices/slice-03-house-attribute-completeness.md`.
//! Companion story: US-CO-03.
//!
//! Three exact-match attributes (`tenant.id`, `experiment.id`, plus the
//! prefix-bearing `feature_flag.{key}`) need to be blessed in the
//! catalogue. Tests cover: all three present together, the
//! `feature_flag.{key}` shape with arbitrary suffix, and the negative
//! case `feature_flag.` (empty suffix) being rejected.
//!
//! Tests panic on `unimplemented!()` until DELIVER lands
//! house-attribute support in `validate`.

mod common;

use codex::SchemaCatalogue;

use crate::common::{
    feature_flag_empty_suffix_pair, spark_canonical_resource_pair, CANONICAL_FEATURE_FLAG_KEY,
    CANONICAL_FEATURE_FLAG_VALUE,
};

#[test]
fn all_three_house_attributes_validate_clean() {
    let catalogue = SchemaCatalogue::new();
    let attrs = spark_canonical_resource_pair();
    let result = catalogue.validate(&attrs);
    assert!(
        result.is_ok(),
        "all three house attributes (tenant.id, feature_flag.{{key}}, experiment.id) must validate clean together; got: {result:?}"
    );
}

#[test]
fn a_feature_flag_with_arbitrary_non_empty_suffix_validates_clean() {
    let catalogue = SchemaCatalogue::new();
    let attrs = vec![
        ("service.name", "payments-api"),
        (CANONICAL_FEATURE_FLAG_KEY, CANONICAL_FEATURE_FLAG_VALUE),
        ("feature_flag.dark-mode", "off"),
        ("feature_flag.experimental.new-pricing", "on"),
    ];
    let result = catalogue.validate(&attrs);
    assert!(
        result.is_ok(),
        "feature_flag.{{any-non-empty-suffix}} must validate clean; got: {result:?}"
    );
}

#[test]
fn a_feature_flag_with_empty_suffix_is_rejected_as_unknown() {
    let catalogue = SchemaCatalogue::new();
    let attrs = feature_flag_empty_suffix_pair();
    let result = catalogue.validate(&attrs);
    assert!(
        result.is_err(),
        "feature_flag. (no suffix) must be rejected; got: {result:?}"
    );
    let report = result.expect_err("Err on empty-suffix feature_flag");
    assert_eq!(
        report.violations().len(),
        1,
        "expected exactly one violation for the empty-suffix feature_flag; got: {:?}",
        report.violations()
    );
}
