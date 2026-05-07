# Codex v0 — DISTILL wave decisions

- **Date**: 2026-05-07
- **Author**: Scholar (`nw-acceptance-designer`) for the source skeleton +
  fixture helpers; Bea (orchestrator) for the five slice tests + the
  invariant smoke + workspace integration after Scholar stalled at
  the same watchdog pattern that hit Morgan in DESIGN.

## Test posture

Strategy C "real local" — but Codex's "real local" is small. Codex
v0 has no I/O, no async, no tracing emit, no subprocess, no
external integrations. The slice tests are synchronous `cargo test`
invocations against literal `(&str, &str)` fixture pairs. Real
Aperture / RecordingSink fixtures from Sieve and Spark are
unnecessary here.

## Per-binary isolation

No process-global state in Codex. The five slice tests can run in
parallel without cross-binary interference. No `serial_test` mutex
needed.

The `[[test]]` declarations in `Cargo.toml` give Cargo per-binary
compile output, which mirrors the discipline established by Aperture
(ADR-0015) and Sieve (ADR-0018) without requiring the same isolation
machinery. The form is the same; the rationale is leaner.

## Public surface enforcement

`tests/invariant_public_api_smoke.rs` is a compile-time smoke test
that imports the five public types via `use codex::{...}`. If any of
the five is renamed or removed, Gate 1 fails to compile. This is
the cheapest in-process companion to `cargo public-api` (Gate 2)
and `cargo semver-checks` (Gate 3).

## Path A compliance

Not applicable — Codex has no `drained=` / `dropped=` event
contract or similar precision-deferral. The Spark integration at
slice 06 surfaces the `LintReport` via `Display` rendering inside
the `tracing::warn!` event body; the rendering format is locked in
ADR-0022 / ADR-0025 and tested by slice 04.

## Cross-feature integration test scope

Slice 06's test (`crates/spark/tests/slice_NN_codex_lint.rs`) lives
in Spark's test directory, not Codex's, per ADR-0025. The Codex
side is exercised by the unit-level slice 04-05 tests; the
end-to-end spark::init → catalogue.validate → tracing::warn!
round-trip belongs in Spark's test surface where the fixture
infrastructure (subscriber capture, etc.) already exists.

## Workspace integration

- `crates/codex` added to workspace members at this DISTILL.
- `scripts/hooks/pre-commit` adds `--exclude codex` to Gate 1
  during DISTILL/DELIVER (mirroring Aperture, Spark, Sieve
  precedent). Compile-only coverage via `cargo build -p codex
  --all-targets --locked`.
- `.github/workflows/ci.yml` Gate 1 same exclusion.
- DEVOPS hand-offs (Apex's wave): extend Gate 2 + Gate 3 to cover
  codex; add `gate-5-mutants-codex` parallel job; pre-push hook
  loop; no deny.toml edits required (Codex has zero runtime deps).

## Mandate compliance

- **CM-A — Hexagonal boundary**: every test imports `use codex::{...}`
  exclusively. Grep over `crates/codex/tests/*.rs` for
  `use codex::(catalogue|lint|fuzzy|generated)` returns zero hits.
- **CM-B — Business language purity**: test names read as user
  outcomes (`a_canonical_attribute_pair_validates_clean`,
  `a_close_typo_produces_a_suggestion`). No technical vocabulary
  in test names.
- **CM-C — User journey completeness**: walking skeleton (slice 01)
  plus four focused slices (02-05) plus the invariant binary.
  Slice 06 is in Spark.
- **CM-D — Pure function extraction**: `SchemaCatalogue::validate`
  is total (returns `Ok(())` or `Err(LintReport)`); `is_error_bearing`
  / `levenshtein` / `nearest_blessed_match` are pure functions.

## Stub posture

Per ADR-0022 / ADR-0024:

- `SchemaCatalogue::new()` — REAL with a minimal seed (slice 01).
- `SchemaCatalogue::validate(...)` — `unimplemented!()` until
  slice 01 DELIVER.
- `LintReport::from_violations`, `LintReport::violations` — REAL
  data accessors so tests can inspect.
- `LintReport::Display` impl — `unimplemented!()` until slice 04.
- `LintViolation::name`, `kind`, `nearest_blessed_match` — REAL
  accessors.
- `levenshtein`, `nearest_blessed_match` — `unimplemented!()` until
  slice 05.
- `BlessedAttribute`, `ViolationKind` — REAL enums.

## Counts

- Five slice integration test binaries (slice 01-05).
- One invariant binary (`invariant_public_api_smoke`).
- 13 `#[test]` functions across the six binaries (3 + 2 + 3 + 4 + 3
  for slices 01-05; 1 for the invariant).
- Plus optional inline `#[cfg(test)] mod tests` blocks the
  software-crafter may add inside `src/*.rs` modules during
  DELIVER for unit-level mutation coverage.

## Awaiting

Sentinel peer review. Apex DEVOPS work after that. Crafty starts
DELIVER once both close.
