//! `SchemaCatalogue` and `BlessedAttribute`.
//!
//! Per ADR-0022 §1, the public surface here is two types
//! (`SchemaCatalogue`, `BlessedAttribute`) plus the catalogue's
//! constructor and its single behavioural method.
//!
//! ## DISTILL state
//!
//! - [`SchemaCatalogue::new`] is **real** — returns an owned catalogue
//!   value seeded with the minimum two entries the Slice 01 walking
//!   skeleton asserts on (`service.name` and `tenant.id`). Slice 02
//!   DELIVER replaces the seed with the full upstream OTel semconv
//!   0.27 corpus + the three house attributes; Slice 03 DELIVER adds
//!   the `feature_flag.` prefix entry alongside the two exact-match
//!   house attributes.
//! - [`SchemaCatalogue::validate`] panics with `unimplemented!()`. Every
//!   slice test (`slice_01_*.rs` through `slice_05_*.rs`) calls this
//!   method and panics until DELIVER drives the panic away.
//! - [`BlessedAttribute`] has both variants (`Exact`, `Prefix`)
//!   structurally defined per ADR-0022 §2 + ADR-0023 §3. Slice 03
//!   DELIVER first exercises the `Prefix` variant; Slice 01 only
//!   touches the `Exact` variant via the seed.

use crate::lint::LintReport;

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
/// `&'static [BlessedAttribute]` populated once at module init from
/// the generated file (Slice 02) and the three house attributes
/// (Slice 03); no per-`new()` allocation is needed.
///
/// At DISTILL, the field is unit-shaped — the seed entries are
/// hardcoded inside `validate`'s soon-to-be-implemented match arms.
/// DELIVER's slices replace the unit field with whatever shape the
/// crafter picks (a `&'static [BlessedAttribute]` slice reference, an
/// owned `Vec<BlessedAttribute>` clone, etc.); the constructor
/// signature stays `pub fn new() -> Self` either way.
#[derive(Debug)]
pub struct SchemaCatalogue {
    /// At DISTILL the catalogue carries no state — the seed entries are
    /// implicit. DELIVER may evolve this to a `&'static
    /// [BlessedAttribute]` reference, an owned `Vec`, or a `phf` map
    /// without changing the public surface.
    _private: (),
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
    /// At DISTILL state this is real: it returns an owned catalogue
    /// value. The validation behaviour that consumes the seed lives in
    /// [`SchemaCatalogue::validate`], which panics until DELIVER lands
    /// per slice.
    #[must_use]
    pub fn new() -> Self {
        Self { _private: () }
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
    /// ## DISTILL state
    ///
    /// Panics with `unimplemented!()`. Slice 01 DELIVER drives the
    /// `Ok` path on the canonical pair fixture; Slice 02 DELIVER
    /// extends the seed to the full upstream corpus; Slice 03 DELIVER
    /// adds prefix matching and the bare-prefix rejection; Slice 04
    /// DELIVER drives the `Err` path with structured violations;
    /// Slice 05 DELIVER populates `nearest_blessed_match`.
    ///
    /// # Panics
    ///
    /// At DISTILL state, every call panics with `unimplemented!()`.
    /// Acceptance tests under `crates/codex/tests/slice_*.rs` rely on
    /// this panic as the canonical RED state.
    pub fn validate(&self, _attributes: &[(&str, &str)]) -> Result<(), LintReport> {
        unimplemented!(
            "SchemaCatalogue::validate is RED at DISTILL state — DELIVER lands the \
             validation paths slice by slice (slice 01-05)"
        )
    }
}

impl Default for SchemaCatalogue {
    fn default() -> Self {
        Self::new()
    }
}
