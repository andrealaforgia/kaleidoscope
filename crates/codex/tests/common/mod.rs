//! Shared fixture helpers for Codex's slice-level acceptance tests.
//!
//! Mirrors Sieve's and Spark's `tests/common/mod.rs` shape, but smaller:
//! Codex has no I/O, no async, no subprocess, no tracing surface, so the
//! helper module is just a handful of pair-builders the slice tests
//! consume. Every test imports `use codex::{...}` exclusively — the
//! hexagonal-boundary contract per `journey-codex.yaml >
//! mandate_compliance > hexagonal_boundary`.
//!
//! ## DISTILL state
//!
//! Every helper is **real** — these are pure-function pair-builders
//! that hand back `&'static`-lifetimed `(&str, &str)` slices the slice
//! tests pass into `SchemaCatalogue::validate`. The validation method
//! itself panics with `unimplemented!()` until DELIVER drives each
//! slice GREEN.
//!
//! ## Why no test infrastructure
//!
//! Sieve's helper carries Tokio fixtures, capture layers, mutex-based
//! serialisation, and `Arc<RecordingSink>` newtypes because Sieve has
//! a timer task, process-global counters, and a tracing emit surface.
//! Spark's helper carries an `ApertureFixture` because Spark drives
//! real OTLP/gRPC traffic at a live Aperture instance. Codex has none
//! of those: no async, no tracing emit, no global state, no I/O. The
//! slice tests are synchronous `cargo test` invocations against
//! literal pair fixtures, per ADR-0022 §"Quality attribute alignment
//! / Testability".

#![allow(dead_code)]

// ---------------------------------------------------------------------
// Canonical attribute values shared across slices.
//
// These mirror Spark's `tests/common/mod.rs` constants verbatim so
// the cross-crate journey vocabulary stays consistent. The same
// literal strings appear in `docs/feature/codex/discuss/user-stories.md`
// example values and in `docs/feature/codex/discuss/journey-codex.yaml`
// shared-artefacts.
// ---------------------------------------------------------------------

/// The canonical `service.name` value used in the walking skeleton and
/// downstream slices (per `journey-codex.yaml > scenarios >
/// walking_skeleton` and the matching Spark `tests/common/mod.rs`
/// entry).
pub const CANONICAL_SERVICE_NAME: &str = "payments-api";

/// The canonical `tenant.id` value used in the walking skeleton and
/// downstream slices.
pub const CANONICAL_TENANT_ID: &str = "acme-prod";

/// The canonical `experiment.id` value used in Slice 03 onward.
pub const CANONICAL_EXPERIMENT_ID: &str = "exp-2026-Q2-pricing";

/// The canonical `feature_flag.{key}` key + value pair used in Slice 03
/// onward. The key string `feature_flag.checkout-v2` is built inline at
/// each call site so the slice fixtures keep the prefix shape visible
/// to the reader.
pub const CANONICAL_FEATURE_FLAG_KEY: &str = "feature_flag.checkout-v2";
pub const CANONICAL_FEATURE_FLAG_VALUE: &str = "on";

// ---------------------------------------------------------------------
// Pair-builders. Each returns an owned `Vec<(&'static str, &'static str)>`
// because the slice tests pass `&attrs` to `validate`; the static
// lifetime keeps the borrowed slice ergonomic for the assertions below.
// ---------------------------------------------------------------------

/// The Slice 01 walking-skeleton canonical pair: one OTel semconv
/// attribute (`service.name`) and one Kaleidoscope-house attribute
/// (`tenant.id`). Per `docs/feature/codex/discuss/user-stories.md >
/// US-CO-01 > Domain examples`.
#[must_use]
pub fn canonical_pair() -> Vec<(&'static str, &'static str)> {
    vec![
        ("service.name", CANONICAL_SERVICE_NAME),
        ("tenant.id", CANONICAL_TENANT_ID),
    ]
}

/// The Slice 03 Spark-canonical-Resource fixture: every house attribute
/// at once, with realistic values. Per `docs/feature/codex/discuss/
/// user-stories.md > US-CO-03 > Domain examples`.
#[must_use]
pub fn spark_canonical_resource_pair() -> Vec<(&'static str, &'static str)> {
    vec![
        ("service.name", CANONICAL_SERVICE_NAME),
        ("tenant.id", CANONICAL_TENANT_ID),
        (CANONICAL_FEATURE_FLAG_KEY, CANONICAL_FEATURE_FLAG_VALUE),
        ("experiment.id", CANONICAL_EXPERIMENT_ID),
    ]
}

/// The Slice 02 OTel-semconv-corpus fixture: a representative spread of
/// the upstream OTel semconv 0.27 resource attributes. Drawn from the
/// `slice-02-otel-semconv-corpus.md` brief (the test exercises `service.*`,
/// `host.*`, `os.*`, `process.*`, `deployment.*`, `telemetry.sdk.*`).
///
/// At Slice 02 DELIVER, the catalogue's seed is the full upstream corpus
/// per `generated/semconv_0_27.rs`; this fixture exercises a meaningful
/// spread without enumerating every entry (the full-corpus invariant is
/// proved by the catalogue construction itself, not by re-listing every
/// blessed name in the test). Per `slice-02-otel-semconv-corpus.md >
/// Acceptance summary`.
#[must_use]
pub fn otel_semconv_resource_spread() -> Vec<(&'static str, &'static str)> {
    vec![
        ("service.name", CANONICAL_SERVICE_NAME),
        ("service.version", "1.4.2"),
        ("service.namespace", "checkout"),
        ("service.instance.id", "instance-7f3a"),
        ("deployment.environment", "production"),
        ("host.name", "node-01"),
        ("host.arch", "amd64"),
        ("os.type", "linux"),
        ("process.pid", "12345"),
        ("process.runtime.name", "rust"),
        ("telemetry.sdk.language", "rust"),
        ("telemetry.sdk.name", "opentelemetry"),
        ("telemetry.sdk.version", "0.27.0"),
    ]
}

/// A single unknown-attribute pair for Slice 04's typo scenarios. The
/// canonical typo `tenat.id` (one transposition / deletion away from
/// the blessed `tenant.id`) is the running example throughout the
/// stories and journey YAML.
#[must_use]
pub fn unknown_attribute_pair() -> Vec<(&'static str, &'static str)> {
    vec![("tenat.id", CANONICAL_TENANT_ID)]
}

/// A multi-typo pair for Slice 04's "two violations from one
/// `validate` call" scenario. Per `docs/feature/codex/discuss/
/// user-stories.md > US-CO-04 > Domain examples > 2`.
#[must_use]
pub fn two_unknown_attributes_pair() -> Vec<(&'static str, &'static str)> {
    vec![
        ("tenat.id", CANONICAL_TENANT_ID),
        ("svc.name", CANONICAL_SERVICE_NAME),
    ]
}

/// A blessed attribute mixed with an unknown one — Slice 04's "blessed
/// entries are silently accepted; only the unknowns produce
/// violations" scenario. Per `slice-04-unknown-attribute-lint.md >
/// Acceptance summary`.
#[must_use]
pub fn one_blessed_one_unknown_pair() -> Vec<(&'static str, &'static str)> {
    vec![
        ("service.name", CANONICAL_SERVICE_NAME),
        ("tenat.id", CANONICAL_TENANT_ID),
    ]
}

/// The Slice 03 empty-suffix-rejection pair: `feature_flag.` with no
/// suffix. Per `docs/feature/codex/discuss/user-stories.md > US-CO-03
/// > Scenario: A feature_flag prefix with empty suffix is rejected`.
#[must_use]
pub fn feature_flag_empty_suffix_pair() -> Vec<(&'static str, &'static str)> {
    vec![("feature_flag.", "on")]
}

/// The Slice 05 fuzzy-suggestion close-typo fixture. Per
/// `docs/feature/codex/discuss/user-stories.md > US-CO-05 > Scenario:
/// A close typo produces a suggestion`.
#[must_use]
pub fn close_typo_pair() -> Vec<(&'static str, &'static str)> {
    vec![("tenat.id", CANONICAL_TENANT_ID)]
}

/// The Slice 05 fuzzy-suggestion no-suggestion fixture: a name far
/// enough from every blessed entry that no suggestion fires (distance
/// > 2). Per `docs/feature/codex/discuss/user-stories.md > US-CO-05 >
/// Scenario: A far-distance attribute produces no suggestion`.
#[must_use]
pub fn far_distance_pair() -> Vec<(&'static str, &'static str)> {
    vec![("acme.totally-custom", "x")]
}
