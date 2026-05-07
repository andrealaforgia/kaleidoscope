//! Slice 02 — Full OTel semconv 0.27 corpus blessed
//!
//! Maps to `docs/feature/codex/slices/slice-02-otel-semconv-corpus.md`.
//! Companion story: US-CO-02.
//!
//! Asserts that every resource attribute in upstream
//! `opentelemetry-semantic-conventions =0.27` validates clean against
//! the catalogue. The fixture is `otel_semconv_resource_spread()` in
//! `common/mod.rs`, which selects representative attributes from the
//! full corpus (host.name, process.pid, telemetry.sdk.language, etc.).
//!
//! Tests panic on `unimplemented!()` until DELIVER lands the
//! generated corpus + `validate`.

mod common;

use codex::SchemaCatalogue;

use crate::common::{otel_semconv_resource_spread, spark_canonical_resource_pair};

#[test]
fn a_complete_otel_semconv_resource_attribute_set_validates_clean() {
    let catalogue = SchemaCatalogue::new();
    let attrs = otel_semconv_resource_spread();
    let result = catalogue.validate(&attrs);
    assert!(
        result.is_ok(),
        "every OTel semconv 0.27 resource attribute must be blessed; got: {result:?}"
    );
}

#[test]
fn a_mixed_standard_and_house_attribute_set_validates_clean() {
    let catalogue = SchemaCatalogue::new();
    let attrs = spark_canonical_resource_pair();
    let result = catalogue.validate(&attrs);
    assert!(
        result.is_ok(),
        "Spark's canonical Resource (mixing OTel standard plus three Kaleidoscope-house attributes) must validate clean; got: {result:?}"
    );
}
