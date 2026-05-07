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

use crate::lint::{LintReport, LintViolation, ViolationKind};

/// The Slice 01 seed corpus: the minimum two entries the walking
/// skeleton asserts on (one OTel semconv resource attribute, one
/// Kaleidoscope-house attribute). Slice 02 DELIVER replaces this
/// inline seed with the concatenation of the generated upstream
/// corpus and the hand-maintained house-attributes slice (per
/// ADR-0023 §3); Slice 03 DELIVER adds the `feature_flag.` prefix
/// entry. The shape stays `&'static [BlessedAttribute]` either way.
const SLICE_01_SEED: &[BlessedAttribute] = &[
    BlessedAttribute::Exact("service.name"),
    BlessedAttribute::Exact("tenant.id"),
];

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
/// without a breaking change. Internally at v0, the corpus is a
/// `&'static [BlessedAttribute]` populated once at module init: at
/// Slice 01 from the inline two-entry seed; at Slice 02 from the
/// regenerated upstream corpus; at Slice 03 with the
/// `feature_flag.` prefix entry added. No per-`new()` allocation is
/// needed.
#[derive(Debug)]
pub struct SchemaCatalogue {
    /// The blessed corpus this catalogue checks against. Held as a
    /// `&'static [BlessedAttribute]` reference per ADR-0022 §6: the
    /// underlying corpus is a static slice, populated at module init
    /// from inline seed (Slice 01) → generated upstream + house
    /// (Slice 02-03+); no per-`new()` allocation is needed.
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
    /// At Slice 01 DELIVER the seed is the inline two-entry minimum
    /// (`service.name`, `tenant.id`); subsequent slices broaden the
    /// seed without touching this constructor's signature.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: SLICE_01_SEED,
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
                    nearest_blessed_match: None,
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
