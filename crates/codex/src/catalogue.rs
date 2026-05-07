//! `SchemaCatalogue` and `BlessedAttribute`.
//!
//! Per ADR-0022 §1, the public surface here is two types
//! (`SchemaCatalogue`, `BlessedAttribute`) plus the catalogue's
//! constructor and its single behavioural method.
//!
//! ## DELIVER state — Slice 01 landed
//!
//! - [`SchemaCatalogue::new`] is **real** — returns an owned catalogue
//!   seeded with the minimum two entries the Slice 01 walking skeleton
//!   asserts on (`service.name` and `tenant.id`). Slice 02 DELIVER
//!   replaces the seed with the full upstream OTel semconv 0.27 corpus
//!   plus the three house attributes; Slice 03 DELIVER adds the
//!   `feature_flag.` prefix entry alongside the two exact-match house
//!   attributes.
//! - [`SchemaCatalogue::validate`] is **real**. The `Ok(())` path (every
//!   supplied attribute is blessed) and the `Err(LintReport)` path
//!   (one or more unknowns) both work; `nearest_blessed_match` stays
//!   `None` until Slice 05 DELIVER lands the Levenshtein lookup.
//! - [`BlessedAttribute`] has both variants (`Exact`, `Prefix`)
//!   structurally defined per ADR-0022 §2 + ADR-0023 §3. Slice 03
//!   DELIVER first exercises the `Prefix` variant via fixture data;
//!   Slice 01 only touches the `Exact` variant via the seed (the
//!   `Prefix` arm is implemented but unreached at Slice 01).

use std::sync::LazyLock;

use crate::fuzzy::nearest_blessed_match;
use crate::generated::semconv_0_27::SEMCONV_0_27;
use crate::lint::{LintReport, LintViolation, ViolationKind};

/// The hand-maintained Kaleidoscope-house attributes that extend the
/// upstream OTel semconv corpus. Per ADR-0023 §3 the two slices stay
/// separate so a misbehaving regeneration of `SEMCONV_0_27` cannot
/// accidentally clobber the house attributes.
///
/// At Slice 02 DELIVER all three house attributes are seeded; the
/// `Prefix` variant on `feature_flag.` is structurally already
/// supported by `is_blessed` (Slice 01 landed both match arms) so
/// the variant is exercised at acceptance level by the slice 02
/// `spark_canonical_resource_pair` fixture and by every slice 03
/// fixture.
const HOUSE_ATTRIBUTES: &[BlessedAttribute] = &[
    BlessedAttribute::Exact("tenant.id"),
    BlessedAttribute::Prefix("feature_flag."),
    BlessedAttribute::Exact("experiment.id"),
];

/// The full effective corpus: the upstream OTel semconv 0.27
/// resource-class corpus concatenated with the three house
/// attributes.
///
/// `LazyLock` allocates the joined `Vec` exactly once, on first
/// catalogue construction, and hands every subsequent `new()` a
/// `&'static [BlessedAttribute]` slice into the same backing store.
/// The catalogue itself stays `Copy`-of-a-reference cheap; the
/// joined buffer outlives every catalogue instance for the life of
/// the process.
static ALL_ENTRIES: LazyLock<Vec<BlessedAttribute>> = LazyLock::new(|| {
    let mut entries: Vec<BlessedAttribute> =
        Vec::with_capacity(SEMCONV_0_27.len() + HOUSE_ATTRIBUTES.len());
    entries.extend_from_slice(SEMCONV_0_27);
    entries.extend_from_slice(HOUSE_ATTRIBUTES);
    entries
});

/// A blessed attribute in the catalogue. Two variants cover the v0
/// match shapes; `#[non_exhaustive]` so future match kinds (regex,
/// glob, version-pattern) land additively without breaking existing
/// `match` arms in `pub(crate)` code.
///
/// Per ADR-0022 §2, the design rejects a struct-with-MatchKind-field
/// shape (which would force `MatchKind` into the public type count
/// and break the five-type lock). The enum's static-`&'static str`
/// payload is zero-cost; the catalogue iteration loop (Slice 03+
/// DELIVER) expresses cleanly as a `match`.
///
/// ## Variants
///
/// - `Exact` — full attribute name must equal the carried `&'static str`.
///   Used by every entry in the upstream OTel semconv 0.27 corpus and
///   by the two house attributes `tenant.id` and `experiment.id`.
/// - `Prefix` — attribute name must start with the carried `&'static str`
///   AND continue with at least one further character (a non-empty
///   suffix). Used by the house attribute `feature_flag.` (matched
///   against `feature_flag.checkout_v2`, `feature_flag.dark_mode`, etc.,
///   per Slice 03's fixture set; explicitly rejects the bare
///   `feature_flag.` entry per the same slice's empty-suffix scenario).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlessedAttribute {
    /// Exact-name match. The full attribute name must equal the carried
    /// `&'static str`.
    Exact(&'static str),
    /// Prefix-and-non-empty-suffix match. The attribute name must start
    /// with the carried `&'static str` and continue with at least one
    /// further character.
    Prefix(&'static str),
}

/// The schema authority. Holds the seeded corpus and exposes the
/// single behavioural method `validate(...)`.
///
/// Per ADR-0022 §6, the public-surface shape `new() -> Self` admits
/// future catalogue extensions (multi-version, tenant overlays at v1+)
/// without a breaking change. Internally at v0 the corpus is a
/// `&'static [BlessedAttribute]` slice over a process-wide `LazyLock`:
/// at Slice 01 the seed was an inline two-entry minimum; at Slice 02
/// the seed is the regenerated upstream corpus concatenated with the
/// three house attributes (`tenant.id`, `feature_flag.`,
/// `experiment.id`). One allocation per process, on first `new()`
/// call.
#[derive(Debug)]
pub struct SchemaCatalogue {
    /// The blessed corpus this catalogue checks against. Held as a
    /// `&'static [BlessedAttribute]` reference per ADR-0022 §6: the
    /// underlying buffer is the process-wide `ALL_ENTRIES` `LazyLock`
    /// (Slice 02+) — `&'static` because the buffer outlives every
    /// catalogue instance for the life of the process.
    entries: &'static [BlessedAttribute],
}

impl SchemaCatalogue {
    /// Construct a fresh catalogue.
    ///
    /// Per ADR-0022 §1, `new() -> Self` is the constructor; per
    /// ADR-0023 §3, the catalogue's effective set is the concatenation
    /// of the upstream OTel semconv 0.27 corpus and the three
    /// Kaleidoscope-house attributes (`tenant.id`,
    /// `feature_flag.{key}`, `experiment.id`).
    ///
    /// At Slice 02 DELIVER the seed is the upstream OTel semconv
    /// 0.27 resource-class corpus (regenerated by `xtask` per
    /// ADR-0023) concatenated with the three Kaleidoscope-house
    /// attributes. The constructor's signature stays unchanged from
    /// Slice 01; only the seed broadens.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: ALL_ENTRIES.as_slice(),
        }
    }

    /// Validate a slice of `(name, value)` attribute pairs against the
    /// catalogue.
    ///
    /// Returns `Ok(())` when every supplied attribute name is blessed
    /// (either as an `Exact` match or as a `Prefix` match with a
    /// non-empty suffix). Returns `Err(LintReport)` when one or more
    /// names are unrecognised; the report carries one
    /// [`LintViolation`] per offending attribute, in input order, per
    /// ADR-0022 §4.
    ///
    /// Per ADR-0022 §4, the implementation collects all violations
    /// (no short-circuit on first miss); operators want one round-trip
    /// per init failure to know all the problems.
    ///
    /// ## DELIVER state — Slice 01 landed
    ///
    /// The accumulator + match-on-blessed shape is real. The
    /// `Ok(())` and `Err(LintReport)` paths both work; the
    /// `nearest_blessed_match` field on every emitted violation is
    /// `None` until Slice 05 DELIVER lands the Levenshtein lookup.
    pub fn validate(&self, attributes: &[(&str, &str)]) -> Result<(), LintReport> {
        let mut violations: Vec<LintViolation> = Vec::new();
        for (name, _value) in attributes {
            if !self.is_blessed(name) {
                violations.push(LintViolation {
                    attribute_name: (*name).to_owned(),
                    kind: ViolationKind::Unknown,
                    nearest_blessed_match: nearest_blessed_match(name, self.entries),
                });
            }
        }
        if violations.is_empty() {
            Ok(())
        } else {
            Err(LintReport::from_violations(violations))
        }
    }

    /// Return `true` iff `name` is blessed by any entry in the
    /// catalogue. Match semantics per ADR-0022 §2:
    ///
    /// - [`BlessedAttribute::Exact(blessed)`] matches when `name == blessed`.
    /// - [`BlessedAttribute::Prefix(blessed)`] matches when `name`
    ///   starts with `blessed` AND continues with at least one further
    ///   character (a non-empty suffix). The bare prefix itself does
    ///   not match — Slice 03's `feature_flag.` empty-suffix scenario
    ///   relies on this.
    fn is_blessed(&self, name: &str) -> bool {
        self.entries.iter().any(|entry| match *entry {
            BlessedAttribute::Exact(blessed) => name == blessed,
            BlessedAttribute::Prefix(blessed) => {
                name.starts_with(blessed) && name.len() > blessed.len()
            }
        })
    }
}

impl Default for SchemaCatalogue {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------
// Inline unit tests — port-to-port at domain scope per the
// nw-software-crafter mandates. Each test invokes the public
// `validate` driving port against a catalogue we construct inside the
// module (the `entries` field is private so this construction is
// only possible here). The default `SchemaCatalogue::new()` seed is
// Exact-only at Slice 01; the `Prefix` match arm is exercised here
// by ad-hoc catalogues so the mutation surface across the entire
// `is_blessed` body is killed at Slice 01 — when Slice 03 lands the
// real `feature_flag.` prefix entry in the seed, the public-surface
// fixture in `tests/slice_03_house_attributes.rs` will exercise the
// same arm at acceptance level.
//
// Coverage of the surviving Slice 01 mutants:
//   - validate->Ok(()): killed by `unknown_attribute_yields_err`.
//   - is_blessed->true: killed by `unknown_attribute_yields_err`.
//   - == flip in Exact: killed by `near_miss_on_exact_blessed_name_yields_err`.
//   - && flip in Prefix: killed by `prefix_with_empty_suffix_yields_err`.
//   - > → ==, > → <, > → >= in Prefix: killed by the parametrised
//     `prefix_match_boundary_cases` cases (empty suffix, non-empty
//     suffix, name shorter than prefix).
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a catalogue from an arbitrary `&'static [BlessedAttribute]`
    /// slice. Keeps the test bodies declarative.
    fn catalogue_with(entries: &'static [BlessedAttribute]) -> SchemaCatalogue {
        SchemaCatalogue { entries }
    }

    #[test]
    fn unknown_attribute_yields_err_with_one_violation() {
        let catalogue = SchemaCatalogue::new();

        let result = catalogue.validate(&[("totally.unknown", "x")]);

        let report = result.expect_err("unknown attribute must yield Err");
        assert_eq!(report.violations().len(), 1);
        assert_eq!(report.violations()[0].attribute_name, "totally.unknown");
    }

    #[test]
    fn near_miss_on_exact_blessed_name_yields_err() {
        // `service.nme` differs from the blessed `service.name` by one
        // character. The Exact arm must compare for equality, not
        // inequality (kills the `==` → `!=` mutant).
        let catalogue = SchemaCatalogue::new();

        let result = catalogue.validate(&[("service.nme", "x")]);

        let report = result.expect_err("near-miss on Exact must yield Err");
        assert_eq!(report.violations().len(), 1);
        assert_eq!(report.violations()[0].attribute_name, "service.nme");
    }

    #[test]
    fn prefix_with_non_empty_suffix_validates_clean() {
        static ENTRIES: &[BlessedAttribute] = &[BlessedAttribute::Prefix("feature_flag.")];
        let catalogue = catalogue_with(ENTRIES);

        let result = catalogue.validate(&[("feature_flag.checkout-v2", "on")]);

        assert!(
            result.is_ok(),
            "prefix with non-empty suffix must validate clean; got: {result:?}"
        );
    }

    #[test]
    fn prefix_with_empty_suffix_yields_err() {
        // Bare prefix `feature_flag.` (no suffix at all) must NOT match
        // the Prefix entry. This kills the `&&` → `||` mutation: with
        // `||`, `name.starts_with(blessed) || name.len() > blessed.len()`
        // returns `true` (the first disjunct holds), so the bare prefix
        // would erroneously be blessed.
        static ENTRIES: &[BlessedAttribute] = &[BlessedAttribute::Prefix("feature_flag.")];
        let catalogue = catalogue_with(ENTRIES);

        let result = catalogue.validate(&[("feature_flag.", "on")]);

        let report = result.expect_err("bare prefix must yield Err");
        assert_eq!(report.violations().len(), 1);
        assert_eq!(report.violations()[0].attribute_name, "feature_flag.");
    }

    #[test]
    fn name_strictly_shorter_than_prefix_yields_err() {
        // A name shorter than the prefix can neither `starts_with` it
        // nor have `len() > blessed.len()`. This case kills the
        // `> → >=` mutant in concert with the empty-suffix test:
        //   - `> → >=`: when name == prefix, `>=` returns true, so the
        //     bare prefix would be wrongly blessed (covered above).
        //   - `> → ==`: when name == prefix, `==` returns true, same
        //     overcoverage.
        //   - `> → <`: when len(name) > len(prefix), `<` returns false,
        //     so the legitimate `feature_flag.checkout-v2` case (covered
        //     above) yields Err under the mutation.
        static ENTRIES: &[BlessedAttribute] = &[BlessedAttribute::Prefix("feature_flag.")];
        let catalogue = catalogue_with(ENTRIES);

        let result = catalogue.validate(&[("ff", "x")]);

        let report = result.expect_err("name shorter than prefix must yield Err");
        assert_eq!(report.violations().len(), 1);
        assert_eq!(report.violations()[0].attribute_name, "ff");
    }
}
