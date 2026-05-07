//! In-tree Levenshtein helper and nearest-blessed-match lookup.
//!
//! Per ADR-0024 §2, Codex carries no `strsim` runtime dep at v0. The
//! Levenshtein implementation is a `pub(crate)` two-row dynamic-
//! programming matrix; the corpus is small enough (~120 entries at
//! v0.27, growing slowly with semconv minor bumps) that the
//! straightforward implementation is the right call.
//!
//! ## DISTILL state
//!
//! Both functions panic with `unimplemented!()`. Slice 05 DELIVER
//! lands both: the Levenshtein function with the two-row DP matrix,
//! and the `nearest_blessed_match` lookup that walks the catalogue and
//! returns `Some(closest)` when the minimum distance is ≤ 2 (per
//! DISCUSS Q5 + ADR-0024 §2). Until then, the slice 05 acceptance
//! tests panic at the production-method panic, which is the canonical
//! RED state.

#![allow(dead_code)]

use crate::catalogue::BlessedAttribute;

/// Compute the Levenshtein distance between two strings.
///
/// Per ADR-0024 §2: two-row dynamic-programming matrix, iterating over
/// `char`s (not bytes) so the function is Unicode-correct even though
/// the OTel semconv attribute names are ASCII in practice.
///
/// `pub(crate)` — used internally by [`nearest_blessed_match`] and not
/// part of Codex's public surface.
///
/// ## DISTILL state
///
/// Panics with `unimplemented!()`. Slice 05 DELIVER lands the body.
///
/// # Panics
///
/// Always panics at DISTILL state.
pub(crate) fn levenshtein(_a: &str, _b: &str) -> usize {
    unimplemented!(
        "fuzzy::levenshtein is RED at DISTILL state — Slice 05 DELIVER lands the \
         two-row DP matrix per ADR-0024 §2"
    )
}

/// Find the catalogue entry with the smallest Levenshtein distance to
/// `attribute_name`, returning `Some(closest_blessed_name)` when the
/// minimum distance is ≤ 2 (per DISCUSS Q5) and `None` otherwise.
///
/// `pub(crate)` — used internally by `SchemaCatalogue::validate` to
/// populate `LintViolation::nearest_blessed_match` (Slice 05 DELIVER).
///
/// For `BlessedAttribute::Prefix` entries, the comparison candidate
/// is the prefix-pattern reconstructed against the input's suffix
/// portion (Slice 05's brief documents the reconstruction shape).
///
/// ## DISTILL state
///
/// Panics with `unimplemented!()`. Slice 05 DELIVER lands the body.
///
/// # Panics
///
/// Always panics at DISTILL state.
pub(crate) fn nearest_blessed_match(
    _attribute_name: &str,
    _catalogue: &[BlessedAttribute],
) -> Option<String> {
    unimplemented!(
        "fuzzy::nearest_blessed_match is RED at DISTILL state — Slice 05 DELIVER \
         lands the corpus walk + threshold check per DISCUSS Q5 + ADR-0024 §2"
    )
}
