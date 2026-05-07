//! Slice 01 — Walking skeleton
//!
//! Maps to `docs/feature/codex/slices/slice-01-walking-skeleton.md`.
//! Companion story: US-CO-01.
//!
//! The smallest unit of evidence that the catalogue type compiles, the
//! validate call compiles, and the happy path returns `Ok(())`. Two
//! tests: a canonical attribute pair validates clean, and an empty
//! attribute set validates clean.
//!
//! Tests panic on `unimplemented!()` until DELIVER lands
//! `SchemaCatalogue::validate`.

mod common;

use codex::SchemaCatalogue;

use crate::common::canonical_pair;

#[test]
fn a_canonical_attribute_pair_validates_clean() {
    let catalogue = SchemaCatalogue::new();
    let attrs = canonical_pair();
    let result = catalogue.validate(&attrs);
    assert!(
        result.is_ok(),
        "canonical attribute pair must validate clean; got: {result:?}"
    );
}

#[test]
fn an_empty_attribute_set_validates_clean() {
    let catalogue = SchemaCatalogue::new();
    let attrs: Vec<(&str, &str)> = Vec::new();
    let result = catalogue.validate(&attrs);
    assert!(
        result.is_ok(),
        "empty attribute set must validate clean; got: {result:?}"
    );
}
