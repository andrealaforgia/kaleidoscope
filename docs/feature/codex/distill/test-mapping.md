# Codex v0 — DISTILL test mapping

Per-slice mapping from BDD scenario in `user-stories.md` to
`#[test]` function in `crates/codex/tests/`. Slice 06's test lives
in Spark, not Codex, per ADR-0025.

## Slice 01 — Walking skeleton (US-CO-01)

| BDD scenario | Test binary | Function | Asserted public-API touchpoint |
|---|---|---|---|
| A canonical attribute pair validates clean | `slice_01_walking_skeleton` | `a_canonical_attribute_pair_validates_clean` | `SchemaCatalogue::new()` + `SchemaCatalogue::validate()` returning `Ok(())` |
| An empty attribute set validates clean | `slice_01_walking_skeleton` | `an_empty_attribute_set_validates_clean` | same surface; empty input is the boundary case |

## Slice 02 — OTel semconv 0.27 corpus (US-CO-02)

| BDD scenario | Test binary | Function | Asserted public-API touchpoint |
|---|---|---|---|
| A complete OTel semconv resource attribute set validates clean | `slice_02_otel_semconv_corpus` | `a_complete_otel_semconv_resource_attribute_set_validates_clean` | `validate(...)` returns `Ok(())` over the upstream-spread fixture |
| A mixed standard + house attribute set validates clean | `slice_02_otel_semconv_corpus` | `a_mixed_standard_and_house_attribute_set_validates_clean` | `validate(...)` returns `Ok(())` over Spark's canonical Resource |

## Slice 03 — House attribute completeness (US-CO-03)

| BDD scenario | Test binary | Function | Asserted public-API touchpoint |
|---|---|---|---|
| All three house attributes validate clean | `slice_03_house_attributes` | `all_three_house_attributes_validate_clean` | `validate(...)` returns `Ok(())` |
| A feature_flag with arbitrary non-empty suffix validates clean | `slice_03_house_attributes` | `a_feature_flag_with_arbitrary_non_empty_suffix_validates_clean` | prefix-match logic in `BlessedAttribute::Prefix` |
| A feature_flag with empty suffix is rejected | `slice_03_house_attributes` | `a_feature_flag_with_empty_suffix_is_rejected_as_unknown` | `validate(...)` returns `Err(LintReport)` with one violation |

## Slice 04 — Unknown attribute lint (US-CO-04)

| BDD scenario | Test binary | Function | Asserted public-API touchpoint |
|---|---|---|---|
| A single unknown attribute produces one LintViolation | `slice_04_unknown_attribute_lint` | `a_single_unknown_attribute_produces_one_lint_violation` | `LintReport.violations()`, `LintViolation::name()`, `ViolationKind::Unknown` |
| Multiple unknown attributes produce multiple violations | `slice_04_unknown_attribute_lint` | `multiple_unknown_attributes_produce_multiple_violations` | multi-violation collection |
| A mixed blessed and unknown set produces one violation for the unknown only | `slice_04_unknown_attribute_lint` | `a_mixed_blessed_and_unknown_set_produces_one_violation_for_the_unknown_only` | per-attribute discrimination |
| The LintReport's Display impl renders human-readable text | `slice_04_unknown_attribute_lint` | `the_lint_report_display_impl_names_each_offending_attribute` | `Display` impl on `LintReport` |

## Slice 05 — Fuzzy suggestions (US-CO-05)

| BDD scenario | Test binary | Function | Asserted public-API touchpoint |
|---|---|---|---|
| A close typo produces a suggestion | `slice_05_fuzzy_suggestions` | `a_close_typo_produces_a_suggestion` | `LintViolation::nearest_blessed_match()` populated |
| A far-distance unknown attribute produces no suggestion | `slice_05_fuzzy_suggestions` | `a_far_distance_unknown_attribute_produces_no_suggestion` | `nearest_blessed_match()` returns `None` |
| The Display rendering includes the suggestion when present | `slice_05_fuzzy_suggestions` | `the_display_rendering_includes_the_suggestion_when_present` | `Display` includes nearest_blessed_match text |

## Slice 06 — Spark integration (US-CO-06)

Test lives in `crates/spark/tests/slice_NN_codex_lint.rs` per
ADR-0025. Crafty implements at DELIVER. Coverage:

- Warn mode emits one `tracing::warn!` event per misconfigured init.
- Strict mode returns `Err(SparkError::SchemaValidation(report))`.
- A clean canonical Resource validates without warning.

## Cross-cutting traceability

| User story | Test binary | Coverage |
|---|---|---|
| US-CO-01 walking skeleton | `slice_01_walking_skeleton` | 2 tests, full coverage |
| US-CO-02 OTel semconv corpus | `slice_02_otel_semconv_corpus` | 2 tests, full coverage |
| US-CO-03 house attributes | `slice_03_house_attributes` | 3 tests, full coverage including negative |
| US-CO-04 unknown attribute lint | `slice_04_unknown_attribute_lint` | 4 tests, full coverage including Display |
| US-CO-05 fuzzy suggestions | `slice_05_fuzzy_suggestions` | 3 tests, full coverage |
| US-CO-06 Spark integration | (in Spark's test surface) | DELIVER scope |

Plus `invariant_public_api_smoke` (1 test) covering the five-type
public-surface lock per ADR-0022.

## Total

- 6 test binaries (5 slices + 1 invariant) on Codex's side
- 13 `#[test]` functions
- Plus the slice-06 test in Spark (Crafty's scope)
