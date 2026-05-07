# Slice 05 — Fuzzy "did you mean" suggestions

## Outcome added

Each `LintViolation { kind: Unknown }` carries a populated
`nearest_blessed_match: Option<String>` when the offending attribute
is within Levenshtein distance ≤ 2 of any blessed name in the
catalogue. Common typos surface their fix:

- `service.nme` → `Some("service.name")`
- `tennt.id` → `Some("tenant.id")`
- `feature_flg.checkout_v2` → `Some("feature_flag.checkout_v2")`
  (matched against the prefix rule by reconstructing the candidate)
- `wholly_unrelated_string` → `None`

The `Display` output appends the suggestion when present:
"attribute `srvce.name` is not in the blessed set; did you mean
`service.name`?".

## What it lights up

- The fuzzy-match algorithm. Recommendation: an in-tree Levenshtein
  implementation (≈ 30 lines) rather than a new dependency. Bound
  candidates by length (skip blessed names whose length differs by
  more than 2 from the input) for cheap pruning.
- The prefix-rule interaction. For a `feature_flag.*` typo, the slice
  needs to compute a sensible candidate. The recommended approach: if
  the blessed prefix `feature_flag.` is within distance 2 of the
  input's prefix portion, reconstruct the suggestion as
  `feature_flag.{input_suffix}`.
- The threshold: distance ≤ 2 catches most realistic typos
  (transposition, single-character substitution / deletion / insertion)
  without surfacing absurd "did you mean" suggestions.

## Demo command

```sh
cargo test -p codex --test slice_05_fuzzy_suggestions
```

The test exercises a table of typo → expected-suggestion pairs across
exact-match house attributes, exact-match OTel semconv attributes, the
prefix-rule house attribute, and a "no plausible suggestion" case
(distance > 2 from every blessed name).

## Acceptance summary

- Single-character substitutions (`service.nzme`) suggest the correct
  blessed name.
- Single-character deletions (`service.nme`) suggest the correct
  blessed name.
- Single-character insertions (`service.namee`) suggest the correct
  blessed name.
- Transpositions (`service.nmae`) suggest the correct blessed name
  (Levenshtein distance 2 covers a transposition).
- A typo on the prefix-rule house attribute suggests the corrected
  prefix with the original suffix preserved.
- A wholly unrelated string (distance > 2 from every blessed name)
  yields `nearest_blessed_match: None`.
- The `Display` output appends the suggestion when present, omits the
  clause when absent. Snapshot-tested.
- 100% mutation kill rate on the modified files.

## Complexity drivers

- Performance: a naïve "compute Levenshtein against every blessed name"
  is O(corpus × |input|²). At v0 the corpus is small enough that this
  is fine; the length-bound pruning is cheap insurance.
- Tie-breaking: if two blessed names are equidistant from the input,
  the slice picks the lexicographically smaller name and documents
  this so the snapshot tests stay deterministic.
- The prefix-rule reconstruction is the subtle piece. Worth its own
  small unit test alongside the table-driven suite.

## Out of scope

- Suggestions for non-Unknown violation kinds (Deprecated, Misnamed)
  — those variants are still unreachable at v0.
- Multi-suggestion lists ("did you mean A or B or C?") — v0 returns at
  most one suggestion per violation.
- Spark integration (Slice 06).
