# Slice 01 — Walking skeleton

## Outcome added

`crates/codex` exists as an AGPL-3.0-or-later workspace member. The
crate exposes a `SchemaCatalogue` type with `new()` constructor and a
`validate(resource_attributes)` method. The catalogue contains exactly
two attributes at this slice: one OTel semconv attribute
(`service.name`) and one Kaleidoscope-house attribute (`tenant.id`).
A Resource carrying both validates clean.

## What it lights up

- The crate skeleton: `Cargo.toml`, lib root, public API surface.
- The minimal type vocabulary: `SchemaCatalogue`, `BlessedAttribute`,
  `LintReport`, `LintViolation`, `ViolationKind`. Bodies stay
  intentionally thin — just enough to make the two-attribute validation
  pass.
- The integration surface Spark will eventually call:
  `SchemaCatalogue::validate(&[(name, value)]) -> Result<(), LintReport>`.
- The mutation-testing baseline (Gate 5, 100% kill rate) on the new
  crate.

## Demo command

```sh
cargo test -p codex --test slice_01_walking_skeleton
```

The test instantiates `SchemaCatalogue::new()`, calls
`validate(&[("service.name", "payments-api"), ("tenant.id", "acme-prod")])`,
and asserts the result is `Ok(())`.

## Acceptance summary

- `crates/codex/Cargo.toml` declares AGPL-3.0-or-later, MSRV inherited
  from workspace, exact-patch pin on
  `opentelemetry-semantic-conventions = "=0.27.x"` (unused at this
  slice but pinned to lock the family).
- `SchemaCatalogue::new()` returns a catalogue containing the two
  named attributes.
- `validate(...)` with both attributes returns `Ok(())`.
- `validate(...)` with neither attribute returns `Ok(())` (presence is
  not enforced at v0; only that supplied attributes are blessed).
- The mutation suite for the slice's modified files reaches 100% kill
  rate.

## Complexity drivers

- Choosing the internal storage for the catalogue (`&'static [...]` vs
  `phf` vs `BTreeMap`). Recommendation: a sorted `&'static [BlessedAttribute]`
  with binary-search lookup; trivial, allocation-free, and lets later
  slices grow the corpus without changing the type.
- The `LintReport` / `LintViolation` shape needs to be right at this
  slice because Slice 04 and 05 build on it. Worth landing the full
  shape (with `nearest_blessed_match: Option<String>`) even though
  v01's tests will only exercise the empty-report case.

## Out of scope

- The full OTel semconv 0.27 corpus (Slice 02).
- The other two house attributes `feature_flag.*` and `experiment.id`
  (Slice 03).
- Lint diagnostics for unknown attributes (Slice 04).
- Fuzzy "did you mean" suggestions (Slice 05).
- Spark integration (Slice 06).
- Any service / network shape — Codex stays a library throughout v0.
