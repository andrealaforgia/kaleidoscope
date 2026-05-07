//! In-tree Levenshtein helper and nearest-blessed-match lookup.
//!
//! Per ADR-0024 §2, Codex carries no `strsim` runtime dep at v0. The
//! Levenshtein implementation is a `pub(crate)` two-row dynamic-
//! programming matrix; the corpus is small enough (~120 entries at
//! v0.27, growing slowly with semconv minor bumps) that the
//! straightforward implementation is the right call.
//!
//! ## DELIVER state — Slice 05 landed
//!
//! Both functions are real. [`levenshtein`] is the two-row DP matrix
//! iterating over `char`s (Unicode-correct). [`nearest_blessed_match`]
//! walks the catalogue and returns `Some(closest)` when the minimum
//! distance is ≤ 2 (the `THRESHOLD` constant per DISCUSS Q5 and
//! ADR-0024 §2). Tie-breaking is deterministic by lexicographic order
//! of the blessed name so snapshot tests stay stable across runs.

#![allow(dead_code)]

use crate::catalogue::BlessedAttribute;

/// The Levenshtein-distance threshold below which a blessed entry is
/// surfaced as a `did you mean` suggestion. Locked at 2 per
/// DISCUSS Q5 + ADR-0024 §2: distance 2 covers single-character
/// substitution, deletion, insertion, and transposition (the common
/// typo classes) without dragging in absurdly-distant entries.
const THRESHOLD: usize = 2;

/// Compute the Levenshtein distance between two strings.
///
/// Per ADR-0024 §2: two-row dynamic-programming matrix, iterating over
/// `char`s (not bytes) so the function is Unicode-correct even though
/// the OTel semconv attribute names are ASCII in practice. The corpus
/// is small (~120 entries at v0.27); allocation cost of the two
/// `Vec<usize>` rows is bounded.
///
/// `pub(crate)` — used internally by [`nearest_blessed_match`] and not
/// part of Codex's public surface.
pub(crate) fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let (m, n) = (a_chars.len(), b_chars.len());

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr: Vec<usize> = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1) // deletion
                .min(curr[j - 1] + 1) // insertion
                .min(prev[j - 1] + cost); // substitution
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

/// Find the catalogue entry with the smallest Levenshtein distance to
/// `attribute_name`, returning `Some(closest_blessed_name)` when the
/// minimum distance is ≤ [`THRESHOLD`] (2 per DISCUSS Q5) and `None`
/// otherwise.
///
/// `pub(crate)` — used internally by `SchemaCatalogue::validate` to
/// populate `LintViolation::nearest_blessed_match` (Slice 05 DELIVER).
///
/// For [`BlessedAttribute::Prefix`] entries the comparison candidate is
/// the raw prefix string; for [`BlessedAttribute::Exact`] entries it is
/// the carried name. Tie-breaking is deterministic: when two entries
/// share the minimum distance the lexicographically smaller name wins,
/// so snapshot tests stay stable across catalogue reorderings.
pub(crate) fn nearest_blessed_match(
    attribute_name: &str,
    catalogue: &[BlessedAttribute],
) -> Option<String> {
    let (name, distance) = catalogue
        .iter()
        .map(|entry| match *entry {
            BlessedAttribute::Exact(blessed) => blessed,
            BlessedAttribute::Prefix(blessed) => blessed,
        })
        .map(|candidate| (candidate, levenshtein(attribute_name, candidate)))
        // Tuple comparison: first by distance ascending, then by name
        // ascending — the latter is the deterministic tie-break (the
        // lexicographically smaller blessed name wins).
        .min_by(|(left_name, left_distance), (right_name, right_distance)| {
            left_distance
                .cmp(right_distance)
                .then_with(|| left_name.cmp(right_name))
        })?;
    if distance <= THRESHOLD {
        Some(name.to_owned())
    } else {
        None
    }
}

// ---------------------------------------------------------------------
// Inline unit tests — port-to-port at domain scope. The two functions
// in this module are pure; their public signatures ARE the driving
// port. Each test calls the function under test with literal inputs
// and asserts on the return value (the only observable outcome).
//
// Coverage is sized to kill every mutant `cargo mutants` produces on
// the body of `levenshtein` and `nearest_blessed_match`:
//
//   levenshtein:
//     - the two early-return arms (m == 0, n == 0)         → empty-input cases
//     - the `cost` ternary (== flip / 0 vs 1)              → equal-vs-different cases
//     - the three `+ 1`s (deletion, insertion, sub)        → distance-1 cases of each kind
//     - the two `.min` calls                               → forced via asymmetric inputs
//     - the row-swap loop                                  → distance-2 (transposition)
//     - distance-3 boundary                                → confirms the algorithm scales
//
//   nearest_blessed_match:
//     - empty catalogue                                    → None branch on `best?`
//     - threshold pass (distance ≤ 2)                      → Some branch
//     - threshold fail (distance > 2)                      → None branch on `if`
//     - ties broken lexicographically                      → equality arm in the match
//     - Prefix variant carries through                     → kills `BlessedAttribute::Exact`
//                                                            being treated as the only path
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{levenshtein, nearest_blessed_match, BlessedAttribute};

    // ----- levenshtein -----

    #[test]
    fn equal_strings_have_distance_zero() {
        assert_eq!(levenshtein("service.name", "service.name"), 0);
    }

    #[test]
    fn both_empty_strings_have_distance_zero() {
        assert_eq!(levenshtein("", ""), 0);
    }

    #[test]
    fn empty_first_string_has_distance_equal_to_second_length() {
        assert_eq!(levenshtein("", "abc"), 3);
    }

    #[test]
    fn empty_second_string_has_distance_equal_to_first_length() {
        assert_eq!(levenshtein("abc", ""), 3);
    }

    #[test]
    fn single_substitution_has_distance_one() {
        // tenant.id vs tenant.iD — one char swapped at the end.
        assert_eq!(levenshtein("tenant.id", "tenant.iD"), 1);
    }

    #[test]
    fn single_deletion_has_distance_one() {
        // service.nme is service.name with the 'a' deleted.
        assert_eq!(levenshtein("service.nme", "service.name"), 1);
    }

    #[test]
    fn single_insertion_has_distance_one() {
        // service.namee is service.name with an extra 'e' at the end.
        assert_eq!(levenshtein("service.namee", "service.name"), 1);
    }

    #[test]
    fn transposition_has_distance_two() {
        // service.nmae is service.name with 'a' and 'm' transposed —
        // two single-character edits under plain Levenshtein.
        assert_eq!(levenshtein("service.nmae", "service.name"), 2);
    }

    #[test]
    fn three_unrelated_characters_have_distance_three() {
        // abc vs xyz — three substitutions, one per position.
        assert_eq!(levenshtein("abc", "xyz"), 3);
    }

    #[test]
    fn distance_is_symmetric() {
        // d(a, b) == d(b, a) is a Levenshtein invariant; this test
        // catches mutations that break the symmetry of the row swap
        // (e.g. mutating one of the `prev` indices to `curr`).
        assert_eq!(
            levenshtein("kitten", "sitting"),
            levenshtein("sitting", "kitten"),
        );
    }

    #[test]
    fn unicode_characters_count_as_single_chars() {
        // The implementation must iterate over `char`s, not bytes —
        // 'é' is a single char but two bytes in UTF-8. Mutating to
        // byte-iteration would report distance 2 instead of 1.
        assert_eq!(levenshtein("café", "cafe"), 1);
    }

    // ----- nearest_blessed_match -----

    #[test]
    fn empty_catalogue_yields_none() {
        let catalogue: &[BlessedAttribute] = &[];
        assert_eq!(nearest_blessed_match("tenat.id", catalogue), None);
    }

    #[test]
    fn close_match_within_threshold_is_returned() {
        let catalogue = &[BlessedAttribute::Exact("tenant.id")];
        assert_eq!(
            nearest_blessed_match("tenat.id", catalogue),
            Some("tenant.id".to_owned()),
        );
    }

    #[test]
    fn distance_exactly_at_threshold_is_returned() {
        // 'service.nmae' is distance 2 from 'service.name' — exactly at
        // the threshold. Must be Some. Kills the `<=` → `<` mutation.
        let catalogue = &[BlessedAttribute::Exact("service.name")];
        assert_eq!(
            nearest_blessed_match("service.nmae", catalogue),
            Some("service.name".to_owned()),
        );
    }

    #[test]
    fn distance_above_threshold_yields_none() {
        // 'acme.totally-custom' is far from any blessed entry.
        let catalogue = &[BlessedAttribute::Exact("tenant.id")];
        assert_eq!(
            nearest_blessed_match("acme.totally-custom", catalogue),
            None
        );
    }

    #[test]
    fn distance_three_yields_none() {
        // 'abc' vs 'wxyz' has distance 4; 'abc' vs 'wxy' has distance 3.
        // Both are above THRESHOLD = 2 and must yield None. Kills the
        // `<=` → `<=` (no-op) and `<= 2` → `<= 3` mutations.
        let catalogue = &[BlessedAttribute::Exact("wxy")];
        assert_eq!(nearest_blessed_match("abc", catalogue), None);
    }

    #[test]
    fn closest_among_many_entries_is_picked() {
        // 'tenat.id' is distance 1 from 'tenant.id' and far from the
        // others. The function must pick the closest, not the first.
        let catalogue = &[
            BlessedAttribute::Exact("service.name"),
            BlessedAttribute::Exact("tenant.id"),
            BlessedAttribute::Exact("experiment.id"),
        ];
        assert_eq!(
            nearest_blessed_match("tenat.id", catalogue),
            Some("tenant.id".to_owned()),
        );
    }

    #[test]
    fn ties_are_broken_lexicographically_when_smaller_name_appears_second() {
        // 'abc' is distance 1 from both 'abd' and 'abe'. The
        // lexicographically smaller blessed name ('abd') wins even
        // though it appears second — the tie-break is order-independent.
        let catalogue = &[
            BlessedAttribute::Exact("abe"),
            BlessedAttribute::Exact("abd"),
        ];
        assert_eq!(
            nearest_blessed_match("abc", catalogue),
            Some("abd".to_owned()),
        );
    }

    #[test]
    fn ties_are_broken_lexicographically_when_smaller_name_appears_first() {
        // The mirror of the test above: when the lex-smaller name is
        // already first, the function must still return it. Catches any
        // mutation that flips the tie-break direction (a `cmp` that
        // reverses to "lex-larger wins" would return 'abe' here).
        let catalogue = &[
            BlessedAttribute::Exact("abd"),
            BlessedAttribute::Exact("abe"),
        ];
        assert_eq!(
            nearest_blessed_match("abc", catalogue),
            Some("abd".to_owned()),
        );
    }

    #[test]
    fn prefix_entry_candidate_is_compared_against_the_prefix_string() {
        // The Prefix arm in the candidate-extraction match must yield
        // the prefix string itself; mutating it to ignore Prefix entries
        // would skip this case. Distance from 'feature_flg.' to
        // 'feature_flag.' is 1 (a single deletion of 'a').
        let catalogue = &[BlessedAttribute::Prefix("feature_flag.")];
        assert_eq!(
            nearest_blessed_match("feature_flg.", catalogue),
            Some("feature_flag.".to_owned()),
        );
    }
}
