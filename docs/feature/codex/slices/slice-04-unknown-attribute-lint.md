# Slice 04 — Unknown attribute lint

## Outcome added

`SchemaCatalogue::validate(...)` with an unrecognised attribute returns
`Err(LintReport)` whose body contains one `LintViolation` per offending
attribute. Each violation carries:

- `attribute_name: String` — the offending attribute as supplied.
- `kind: ViolationKind::Unknown` — at this slice the only populated
  variant.
- `nearest_blessed_match: None` — populated by Slice 05.

`LintReport` implements `Display` for an operator-friendly message
naming each offending attribute on its own line, and
`std::error::Error` so the report propagates cleanly through `?`.

## What it lights up

- The error path through `validate(...)`. Slices 01–03 only exercised
  the `Ok` path; this slice fills in the `Err` shape end-to-end.
- The multi-violation case: a Resource carrying three typos returns one
  `LintReport` containing three `LintViolation` entries, not three
  separate calls.
- The `Display` formatting of `LintReport` and `LintViolation`.
  British-English wording, plain operator language: "attribute
  `srvce.name` is not in the blessed set" rather than "ERR0042 schema
  violation detected".

## Demo command

```sh
cargo test -p codex --test slice_04_unknown_attribute_lint
```

The test feeds a Resource carrying one valid attribute
(`service.name`) and two unknown attributes (`srvce.name`,
`tennt.id`), asserts the result is `Err(report)`, asserts the report
contains exactly two violations, and snapshots the `Display` output.

## Acceptance summary

- A Resource with one unknown attribute returns `Err(LintReport)` with
  one violation; `kind` is `Unknown`; `nearest_blessed_match` is
  `None`.
- A Resource with multiple unknown attributes returns one
  `LintReport` listing all of them.
- A Resource mixing blessed and unknown attributes returns
  `Err(LintReport)` listing only the unknown ones; the blessed entries
  are silently accepted.
- `LintReport` implements `Display` and `std::error::Error`.
- The `Display` output is snapshot-tested (insta or equivalent) so
  that wording changes are deliberate.
- 100% mutation kill rate on the modified files.

## Complexity drivers

- Display formatting choices — pluralisation ("1 attribute is not
  blessed" vs "3 attributes are not blessed"), ordering (preserve
  input order vs sort alphabetically — recommendation: preserve input
  order so the report mirrors the Resource composition).
- Whether `LintReport` is `#[non_exhaustive]`. Recommendation: yes,
  because Slice 05 adds the fuzzy-match field meaning and a future
  slice may add a deprecation kind. Non-exhaustive lets that grow
  without a breaking change.

## Out of scope

- Fuzzy "did you mean" suggestions (Slice 05) — the field exists from
  Slice 01 but stays `None` until Slice 05 populates it.
- The `Deprecated` and `Misnamed` variants of `ViolationKind` —
  defined in the enum but unreachable until v1+ slices populate them.
- Spark integration (Slice 06).
