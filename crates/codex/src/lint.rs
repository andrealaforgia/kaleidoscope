//! `LintReport`, `LintViolation`, `ViolationKind`.
//!
//! Per ADR-0022 §1, the public surface here is three types. The
//! `Display` impl on `LintReport` is the operator-readable rendering
//! per ADR-0025 §6 — the warn-mode message body Spark emits is the
//! Display output verbatim.
//!
//! ## DISTILL state
//!
//! - `LintReport`, `LintViolation`, and `ViolationKind` are **real**
//!   (struct/enum definitions and the violations accessor compile and
//!   are visible to the slice tests).
//! - The `Display` impl on `LintReport` panics with `unimplemented!()`.
//!   Slice 04 DELIVER lands the rendering; the same slice's snapshot
//!   tests lock the wording.
//! - `std::error::Error` is implemented via the `Display` panic at
//!   DISTILL — DELIVER's Slice 04 fills in the body. Spark's
//!   `SparkError::SchemaValidation(LintReport)` variant (added at
//!   Slice 06 DELIVER per ADR-0025 §4) consumes the `Error` impl via
//!   the `?` operator in callers.

use std::fmt;

/// The kind of a [`LintViolation`].
///
/// `#[non_exhaustive]` because v1+ slices may add `Deprecated`,
/// `Misnamed`, etc. without breaking matchers in `pub(crate)` code or
/// in downstream consumers.
///
/// At v0 the only populated variant is `Unknown` — an attribute name
/// that is not in the catalogue (per Slice 04). Slice 05 leaves the
/// kind as `Unknown` and populates [`LintViolation::nearest_blessed_match`]
/// instead; the kind expresses the catalogue's verdict, the suggestion
/// expresses recovery.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViolationKind {
    /// The attribute name is not in the catalogue. v0's only populated
    /// variant.
    Unknown,
}

/// A single offending attribute as supplied to
/// [`crate::SchemaCatalogue::validate`].
///
/// Per ADR-0022 §4 + Slice 04's brief, the field set is fixed at v0:
///
/// - `attribute_name` — the offending attribute as supplied. Owned
///   (`String`) because the report outlives the borrow `validate(...)`
///   was called with.
/// - `kind` — the catalogue's verdict (Slice 04 only populates
///   `Unknown`).
/// - `nearest_blessed_match` — populated by Slice 05's Levenshtein
///   suggestion when the offending name is within distance ≤ 2 of any
///   blessed entry; `None` otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintViolation {
    /// The offending attribute as supplied to `validate`. Owned so the
    /// report can outlive the borrow `validate` was called with.
    pub attribute_name: String,
    /// The catalogue's verdict. v0 populates only `ViolationKind::Unknown`.
    pub kind: ViolationKind,
    /// The nearest blessed catalogue entry within Levenshtein distance
    /// ≤ 2 of `attribute_name`, if any. Populated by Slice 05;
    /// `None` at Slice 04 and below.
    pub nearest_blessed_match: Option<String>,
}

/// The report `validate` returns on `Err`. Carries one or more
/// [`LintViolation`] entries in input order (no sort, no deduplication
/// — the report mirrors the Resource composition the caller passed in,
/// per ADR-0022 §4).
///
/// `LintReport` implements [`std::fmt::Display`] for the
/// operator-readable rendering Spark's warn event surfaces (per
/// ADR-0025 §6). It also implements [`std::error::Error`] so the
/// report propagates cleanly through the `?` operator inside
/// `spark::init` callers.
///
/// ## DISTILL state
///
/// The struct compiles and `violations()` is real (returns the
/// underlying slice). The `Display` impl panics with `unimplemented!()`
/// — Slice 04 DELIVER lands the rendering. The `Error` impl is
/// derived-trivial; its `source()` returns `None` (Codex does not
/// chain to an underlying error at v0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintReport {
    violations: Vec<LintViolation>,
}

impl LintReport {
    /// Construct a `LintReport` from a non-empty `Vec<LintViolation>`.
    ///
    /// Per ADR-0022 §4: `validate` calls this only when the
    /// accumulator is non-empty; an empty `violations` would represent
    /// "the report exists but says nothing", which is a contradiction
    /// — `Ok(())` is the right shape for the no-violations case.
    ///
    /// At DISTILL, this constructor is **real** so the test fixtures
    /// in `tests/common/mod.rs` can build sample reports for snapshot
    /// or assertion seeding (slice tests do not call this — they call
    /// `validate` and assert on the returned report — but the slice
    /// 06 Spark integration test in `crates/spark/tests/` may
    /// construct a synthetic report for a fast comparison path).
    #[must_use]
    pub fn from_violations(violations: Vec<LintViolation>) -> Self {
        debug_assert!(
            !violations.is_empty(),
            "LintReport must carry at least one violation; the no-violations case is Ok(()) per ADR-0022 §4"
        );
        Self { violations }
    }

    /// View the contained violations in input order.
    #[must_use]
    pub fn violations(&self) -> &[LintViolation] {
        &self.violations
    }
}

impl fmt::Display for LintReport {
    /// Operator-readable rendering. One line per violation, prefixed
    /// by a header line. Per ADR-0025 §6 the contract is:
    ///
    /// ```text
    /// schema validation failed:
    ///   - tenat.id (Unknown; did you mean tenant.id?)
    ///   - svc.name (Unknown; no close match)
    /// ```
    ///
    /// ## DISTILL state
    ///
    /// Panics with `unimplemented!()`. Slice 04 DELIVER lands the
    /// rendering; the snapshot test at the same slice locks the
    /// wording.
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unimplemented!(
            "LintReport::fmt is RED at DISTILL state — Slice 04 DELIVER lands \
             the operator-readable rendering"
        )
    }
}

impl std::error::Error for LintReport {}
