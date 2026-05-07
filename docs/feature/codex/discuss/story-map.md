# Codex v0 — story map

## Backbone

The single user activity Codex v0 supports is **resource-attribute
schema validation at telemetry-emission time**. The backbone steps
Sasha (platform engineer) walks through:

1. **Configure** — Spark's `SparkConfig` builds the Resource
   attributes the application emits (`service.name`, `tenant.id`,
   `feature_flag.*`, `experiment.id`, plus any standard OTel
   semconv attributes the OTel SDK supplies).
2. **Validate** — `spark::init` calls
   `SchemaCatalogue::validate(resource_attributes)` after Resource
   composition, before the OTel SDK is wired.
3. **Surface** — on a `LintReport`: in warn mode (the default), one
   `tracing::warn!(target = "spark")` event carries the report's
   text via `Display`; in strict mode, init returns
   `Err(SparkError::SchemaValidation(report))`.
4. **Correct** — Sasha reads the report (or the `Err` variant in
   strict CI), uses the `nearest_blessed_match` suggestion, fixes
   the typo, re-runs.

## Walking skeleton

The thinnest end-to-end Codex v0 installation: a Rust integration
test that builds a `SchemaCatalogue::new()`, runs
`validate(&[("service.name", "payments-api"), ("tenant.id",
"acme-prod")])` and asserts `Ok(())`. No Spark integration yet
(slice 06 lands that), no full corpus yet (slice 02 lands that), no
suggestions yet (slice 05). One trait + one impl + one assertion;
the contract has shape.

## Slices

Six elephant-carpaccio slices, each ≤1 day, each demoable as a
`cargo test` invocation that returns GREEN.

| Slice | Story | Demo command | Learning hypothesis |
|---|---|---|---|
| 01 walking-skeleton | US-CO-01 | `cargo test -p codex --test slice_01_walking_skeleton` | The `SchemaCatalogue::validate` shape is right. If extending it in slice 02 feels awkward, the API is wrong. |
| 02 OTel semconv 0.27 corpus | US-CO-02 | `cargo test -p codex --test slice_02_otel_semconv_corpus` | The generated-from-upstream corpus regeneration ritual is workable; if a maintainer cannot regenerate cleanly, the ritual needs documentation upgrade. |
| 03 house attribute completeness | US-CO-03 | `cargo test -p codex --test slice_03_house_attributes` | The `feature_flag.{key}` prefix-match shape composes with exact-match attributes cleanly; if the catalogue iteration loop becomes ugly, the data model is wrong. |
| 04 unknown attribute lint | US-CO-04 | `cargo test -p codex --test slice_04_unknown_attribute_lint` | Multi-violation reports are useful; if developers prefer fail-fast on the first violation, this slice surfaces the preference. |
| 05 fuzzy suggestions | US-CO-05 | `cargo test -p codex --test slice_05_fuzzy_suggestions` | Levenshtein distance ≤ 2 is the right "close typo" threshold; if it produces false-positive suggestions, this slice surfaces the calibration. |
| 06 Spark integration | US-CO-06 | `cargo test -p spark --test slice_07_codex_lint` (or similar; the test lives in spark, not codex) | Default-warn / opt-in-strict is the right ergonomic; if developers always set strict, the default is wrong. |

## Carpaccio taste tests

Each slice has been checked against the elephant-carpaccio
discipline:

1. **End-to-end value** — every slice closes with a Rust
   integration test that returns GREEN. The walking skeleton is
   the smallest unit of value; subsequent slices add capability.
2. **≤1 day ship** — each slice is bounded by one Rust crate change
   plus its integration test (slice 06 also touches Spark for the
   integration). Mutation testing on the diff brings the kill-rate
   to 100%.
3. **Named learning hypothesis** — every slice has a what-could-go
   -wrong hypothesis (column above); the slice fails fast if the
   hypothesis does not hold.
4. **Production-shape data** — fixture attribute pairs are real
   names with realistic values (`payments-api`, `acme-prod`,
   `checkout-v2`, `exp-2026-Q2-pricing`). No `user123` or `foo/bar`
   placeholders.
5. **Dogfood moment** — slice 01 ships a working catalogue; slice
   06 ships the lint integrated into Spark, the moment Codex
   becomes operationally visible. Each closes with a demoable
   artefact.
6. **IN/OUT scope** — each slice brief at `slices/slice-NN-*.md`
   has explicit IN scope and OUT scope.

No slice "ships 4+ new components". No two slices are
"identical-except-for-scale". No slice runs only on synthetic data.
The carpaccio discipline holds.

## Prioritisation

Execution order is the order listed (01 → 06). Rationale:

- **Learning leverage first**. Slice 01 locks the
  `SchemaCatalogue::validate` shape; if it's wrong, the cost of
  reshaping cascades across slices 02-06.
- **Dependency chain**. Slice 02 extends slice 01's catalogue with
  the upstream corpus. Slice 03 extends with the house attributes.
  Slice 04 needs the catalogue to be complete (slices 02-03) before
  the unknown-attribute path is meaningful. Slice 05 builds on
  slice 04's `LintViolation` shape. Slice 06 depends on 04-05 for
  the report quality.
- **Dogfood cadence**. After slice 01 the catalogue exists; after
  slice 03 the catalogue is complete; after slice 04 the lint is
  useful; after slice 05 the suggestions land; after slice 06 the
  lint is integrated.

## Slice briefs

Per-slice IN/OUT scope, demo command, complexity drivers in:

- `../slices/slice-01-walking-skeleton.md`
- `../slices/slice-02-otel-semconv-corpus.md`
- `../slices/slice-03-house-attribute-completeness.md`
- `../slices/slice-04-unknown-attribute-lint.md`
- `../slices/slice-05-fuzzy-suggestions.md`
- `../slices/slice-06-spark-integration.md`
