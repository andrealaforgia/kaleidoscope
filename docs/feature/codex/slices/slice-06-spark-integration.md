# Slice 06 — Spark integration

## Outcome added

Spark adds a runtime dependency on `codex` and calls
`SchemaCatalogue::validate(...)` inside `spark::init` after the
Resource has been composed. Behaviour is configurable:

- **Default (warn)**: a `LintReport` is emitted as a `tracing::warn!`
  event carrying the report's `Display` output; `spark::init` returns
  `Ok`. Existing deployments are unaffected.
- **Opt-in (strict)**: `spark::init` returns
  `Err(SparkError::SchemaValidation(LintReport))`. The new variant
  is added under the existing `#[non_exhaustive]` annotation on
  `SparkError`, so the addition is non-breaking.

A small additive ADR amendment records the new `SparkError` variant
and the warn / strict configuration knob; Bea will route the ADR
post-DELIVER.

## What it lights up

- The first cross-crate consumer of Codex. The lint loop with Spark
  is closed: Resource composition is now schema-checked at the moment
  it would matter.
- The non-exhaustive `SparkError` shape proves out as a real
  extension point (it has been declared non-exhaustive since Spark's
  walking skeleton; this slice is the first additive variant).
- The Spark configuration surface gains a `schema_validation:
  SchemaValidationMode` field with `Warn` (default) and `Strict`
  variants. Default-Warn means rolling Codex into Spark causes no
  behaviour change for existing callers.

## Demo command

```sh
cargo test -p spark --test schema_validation_init
```

Two scenarios: (a) Resource with only blessed attributes — `init`
succeeds in both modes, no warn event emitted. (b) Resource with one
unknown attribute — in Warn mode `init` succeeds and a `tracing::warn!`
event carries the report; in Strict mode `init` returns
`Err(SparkError::SchemaValidation(_))`. The test uses a tracing
subscriber to capture the warn event.

## Acceptance summary

- Spark's `Cargo.toml` gains `codex = { path = "../codex" }` (or
  workspace-style equivalent) as a runtime dep.
- `spark::init` calls `SchemaCatalogue::validate` after Resource
  composition.
- `SchemaValidationMode::{Warn, Strict}` is exposed on Spark's
  config; default is `Warn`.
- `SparkError::SchemaValidation(LintReport)` exists and is reachable
  in Strict mode.
- A `tracing::warn!` event carrying the `LintReport` is emitted in
  Warn mode when violations are present.
- The ADR amendment is drafted (not necessarily merged in this slice)
  noting the new variant and the default-warn posture.
- 100% mutation kill rate on the modified files (Spark's init path
  and config types).

## Complexity drivers

- Where in `spark::init` to call `validate`. Recommendation:
  immediately after the Resource is finalised but before the
  TracerProvider / MeterProvider builders consume it. This means a
  schema violation surfaces *before* any spans or metrics are
  attributed to a malformed Resource.
- The tracing-event shape needs to round-trip the `LintReport`
  faithfully. Recommendation: emit one `tracing::warn!` per
  `LintViolation` (so each violation is a structured event), plus a
  summary event. Allows operators to set up a count-by-violation
  dashboard.
- Test ergonomics: capturing a `tracing::warn!` in a unit test wants
  a thread-local subscriber (`tracing-subscriber`'s test utilities or
  a small custom layer). Worth landing this test harness shape now;
  v1+ schema-related events will reuse it.

## Out of scope

- Telemetry on the lint check itself (a metric counting violations
  per process lifetime) — useful but post-v0.
- Migrating any existing Spark resource-attribute lint code into
  Codex — that lint currently checks only `service.name` and is
  effectively replaced wholesale by the Codex call. The slice
  removes the old check.
- Aperture-side integration — Aperture composes its own Resource;
  bringing the same Codex check into Aperture is a follow-up feature,
  not part of v0.
- Any service / network shape — Codex remains a library throughout.
