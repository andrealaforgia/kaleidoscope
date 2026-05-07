# Slice 07 — Codex schema lint integration

- **Wave**: post-DELIVER cross-feature integration (Spark consumes
  Codex)
- **Author**: Bea (orchestrator), executing the slice directly per
  the recurring agent-stall recovery pattern. The work is fully
  specified by ADR-0025 so the brief and the RED tests fold into
  one document.
- **Date**: 2026-05-07
- **Driven by**: ADR-0025 §1-6 (Codex–Spark integration shape)

## Goal

Wire Codex's `SchemaCatalogue` into Spark's `init` so that
misconfigured resource attributes surface either as a single
`tracing::warn!(target = "spark", ...)` event (default) or as
`Err(SparkError::SchemaValidation(report))` (opt-in via
`SparkConfig::with_strict_schema_lint(true)`).

## Scope (IN)

- Spark's `Cargo.toml` gains `codex = { path = "../codex", version = "0.1" }`
  as a runtime dep.
- `SparkError::SchemaValidation(codex::LintReport)` is added
  additively under the existing `#[non_exhaustive]` annotation.
- `SparkConfig` gains a `strict_schema_lint: bool` field (default
  `false`) and a `with_strict_schema_lint(bool) -> Self` builder.
- `crates/spark/src/init.rs` adds a `OnceLock<SchemaCatalogue>`
  (lazy build, once per process) and calls
  `catalogue().validate(&resource_attrs)` after the existing
  `lint(config)?` pass and before any OTel SDK type construction.
- On `Err(report)`: strict-mode returns
  `Err(SparkError::SchemaValidation(report))`; warn-mode emits one
  `tracing::warn!(target = "spark", "schema validation failed:\n{}", report)`
  and continues.
- `LintReport::Display` is rendered inline in the warn message
  body (not as a structured field) per ADR-0025 §3.
- ADR-0012 (Spark error type) gains a post-DELIVER amendment note
  documenting the new variant.
- ADR-0013 (Spark dependency pinning) gains a post-DELIVER amendment
  note documenting the Codex runtime dep.

## Scope (OUT)

- The strict-vs-warn knob as an env var (ADR-0025 alternative D,
  rejected at v0).
- A Spark-side newtype wrapper around `LintReport` (ADR-0025 §4
  defers this DELIVER decision; v0 takes the simpler re-export
  shape).
- Any change to Spark's existing four error variants. The new
  variant is purely additive.
- Any change to Codex's public API. The integration consumes
  `SchemaCatalogue::new()`, `SchemaCatalogue::validate(...)`, and
  `LintReport`'s `Display` impl — all already public per ADR-0022.

## Acceptance criteria

The slice ships with five integration tests in
`crates/spark/tests/slice_07_codex_schema_lint.rs`:

1. **happy_path_with_blessed_attributes_passes_warn_mode_silently**:
   `init` with only blessed attributes (`service.name`,
   `tenant.id`, `feature_flag.checkout_v2`, `experiment.id`)
   returns `Ok(SparkGuard)` and emits no warn event.
2. **mistyped_attribute_emits_warn_with_suggestion_in_default_mode**:
   `init` with a misspelled attribute (`feature_flg.x` say) returns
   `Ok(SparkGuard)`, AND a single warn event with target=`spark`
   appears whose body matches `"schema validation failed:"` and
   contains the suggestion text.
3. **mistyped_attribute_with_strict_mode_returns_err_schema_validation**:
   `init` with the same misspelled attribute and
   `with_strict_schema_lint(true)` returns
   `Err(SparkError::SchemaValidation(report))`. The report's
   `Display` rendering matches the warn-mode body byte-for-byte.
4. **with_strict_schema_lint_builder_round_trips_to_config**:
   `SparkConfig::default().with_strict_schema_lint(true)` produces
   a config whose strict-mode lint is observable through the
   ADR-0025 §3 Err contract above (proven by test 3).
5. **default_mode_does_not_construct_otel_sdk_when_lint_is_clean**:
   smoke test pinning the order: existing internal `lint(config)?`
   pass is unchanged; Codex lint runs after it; OTel SDK
   construction runs after both.

The tests use the existing `with_capture_subscriber` /
`spark_capture` test utilities (slice 06 precedent) for warn-event
observation. No new test infrastructure is required.

## Mutation testing scope

`cargo mutants -p spark --in-diff` against the slice 07 diff. Per
ADR-0005 Gate 5, 100% kill rate. Expected viable mutants:

- Strict-mode branch: the `if config.strict_schema_lint` test
  mutates to `true` / `false` — pinned by tests 2 and 3.
- Warn-mode branch: removing the `tracing::warn!` call — pinned by
  test 2's assertion that the event is observed.
- OnceLock initialisation: replacing
  `CATALOGUE.get_or_init(SchemaCatalogue::new)` with
  `&SchemaCatalogue::new()` produces a build-time mutant if the
  return type fails (the static path is the load-bearing one).

## Quality gates

The standard nine gates from `.github/workflows/ci.yml` apply
unchanged. No new gates introduced by this slice.

## Constraints

- Spark stays Apache-2.0; the new Codex runtime dep is
  AGPL-3.0-or-later. The licence asymmetry is acceptable per
  ADR-0025 §1's Sieve-Aperture precedent: both crates ship in a
  single Kaleidoscope deployment, AGPL on the platform side is
  structural, and downstream Spark consumers do not inherit AGPL
  because they consume Spark, not Codex.
- `#[non_exhaustive]` on `SparkError` keeps the new variant
  non-breaking. Cargo public-api Gate 2 confirms the addition is
  the only public-surface change. Cargo semver-checks Gate 3
  confirms it's non-breaking.
- The lint hook runs **before** any OTel SDK type is constructed.
  Strict-mode Err must not have side effects on OTel global state.

## Outputs

Produced by this slice landing:

- `crates/spark/Cargo.toml` — adds the Codex runtime dep.
- `crates/spark/src/error.rs` — adds the SchemaValidation variant.
- `crates/spark/src/config.rs` — adds the strict_schema_lint field
  and builder.
- `crates/spark/src/init.rs` — adds the OnceLock and the lint hook.
- `crates/spark/tests/slice_07_codex_schema_lint.rs` — five tests
  (RED first, GREEN after the implementation lands).
- `crates/spark/Cargo.toml` `[[test]]` declaration for the new test
  binary.
- `docs/product/architecture/adr-0012-spark-error-type.md` —
  post-DELIVER amendment naming the new variant.
- `docs/product/architecture/adr-0013-spark-dependency-pinning.md` —
  post-DELIVER amendment naming the Codex runtime dep.
- This slice brief in this file.

## Why direct Bea execution rather than crafter dispatch

The slice is fully specified by ADR-0025 — no design decisions are
delegated to the implementor. The crafter agent has stalled at
least four times across the project (Morgan twice, Scholar twice);
on a fully-specified slice, Bea-direct with tight verification per
step is faster and lower-risk than a dispatch-then-finalise cycle.

This is the same pattern Bea used for the env-override fix at
commit `c8d8a55` (Aperture issue 002): a small focused change
fully driven by an ADR clause, executed with RED→GREEN→commit
discipline.

## Closes

Codex's deferred slice 06: ADR-0025 promised "Codex side complete
at v0; Spark side lands as a separate Spark-side wave with
post-DELIVER amendments to ADR-0012 + ADR-0013 at that landing".
This slice IS that landing.

After this slice ships, every Spark `init` carrying a misspelled
resource-attribute key gets a clear warn message with a typo
suggestion at default rollout, AND CI integration tests can opt
into strict mode for fail-fast misconfiguration detection. The
five-feature v0 graduates a real cross-feature integration on top
of its individual graduations.
