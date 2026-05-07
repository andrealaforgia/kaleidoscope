# Codex v0 — outcome KPIs

Six measurable outcomes with numeric targets and CI-enforced
verification mechanisms. Each KPI is something Sasha or Riley can
verify is true today without reading source code.

---

## KPI 1 — Canonical Resources validate clean

- **Who**: Sasha, platform engineer.
- **Does what**: emits Spark's canonical Resource (service.name,
  tenant.id, feature_flag.*, experiment.id) and gets `Ok(())`.
- **By how much**: 100% of canonical-Resource fixtures yield
  `Ok(())`.
- **Measured by**: `slice_01_walking_skeleton`,
  `slice_03_house_attributes`, and the slice-06 Spark integration
  test all assert canonical Resources validate.
- **Baseline**: greenfield.
- **Guardrail**: any commit that flags a canonical Resource as
  unknown fails the gate.

## KPI 2 — Full OTel semconv 0.27 corpus blessed

- **Who**: Sasha, platform engineer.
- **Does what**: integrates an off-the-shelf OTel SDK and gets no
  false positives on standard semconv attributes.
- **By how much**: 100% of upstream
  `opentelemetry-semantic-conventions =0.27` resource attributes
  blessed.
- **Measured by**: `slice_02_otel_semconv_corpus` integration test
  exercises the full upstream corpus.
- **Baseline**: greenfield.
- **Guardrail**: a missing standard attribute fails the gate.

## KPI 3 — Unknown attributes produce structured violations

- **Who**: Sasha, platform engineer.
- **Does what**: receives a structured `LintReport` naming the
  offending attribute, the kind of violation, and (where possible)
  the nearest blessed match.
- **By how much**: 100% of unknown-attribute fixtures produce
  exactly one `LintViolation` per offending attribute.
- **Measured by**: `slice_04_unknown_attribute_lint` exercises
  single, multi, and prefix-empty cases.
- **Baseline**: greenfield.
- **Guardrail**: missing `LintViolation`s or wrong-shape reports
  fail the gate.

## KPI 4 — Close typos surface "did you mean" suggestions

- **Who**: Sasha, platform engineer.
- **Does what**: makes a typical typo (`tenat.id`, `service.nme`,
  etc.) and reads a clear suggestion in the violation message.
- **By how much**: 100% of close-typo fixtures (Levenshtein
  distance ≤ 2) yield a populated `nearest_blessed_match`.
- **Measured by**: `slice_05_fuzzy_suggestions` exercises a
  fixture set of common typos.
- **Baseline**: greenfield.
- **Guardrail**: a missing suggestion on a close typo fails the
  gate.

## KPI 5 — Spark integration surfaces violations at integration time

- **Who**: Sasha (developer), Riley (SRE).
- **Does what**: misnames an attribute somewhere in Spark's
  pipeline; sees the violation at `spark::init` rather than
  chasing it downstream.
- **By how much**: 100% of misconfigured Spark inits in warn mode
  emit exactly one `tracing::warn!(target = "spark", ...)` event;
  100% of misconfigured Spark inits in strict mode return
  `Err(SparkError::SchemaValidation(report))`.
- **Measured by**: `slice_07_codex_lint` integration test in Spark
  (or wherever the slice 06 integration test lives) asserts both
  modes.
- **Baseline**: greenfield. Spark today only checks `service.name`.
- **Guardrail**: a missing event in warn mode or a missing Err in
  strict mode fails the gate.

## KPI 6 — Codex library is fast enough to live in `init`

- **Who**: Sasha (developer experience).
- **Does what**: calls `spark::init` and gets back a result
  without Codex slowing the boot path noticeably.
- **By how much**: a typical Resource (~10 attributes) validates
  in under 1 ms on developer hardware. The full upstream corpus
  fixture validates in under 10 ms.
- **Measured by**: a small benchmark or wall-clock assertion in
  `slice_05_fuzzy_suggestions` (the Levenshtein loop is the
  hottest path).
- **Baseline**: greenfield.
- **Guardrail**: a regression that pushes validation past the
  budget fails the gate.

---

## Guardrail metrics (CI invariants per ADR-0005)

| Invariant | Mechanism |
|---|---|
| `forbid(unsafe_code)` | `forbid(unsafe_code)` in `crates/codex/src/lib.rs`; clippy gates verify on every commit. |
| 100% mutation kill rate | `cargo mutants --package codex --in-diff` per slice; per ADR-0005 Gate 5. |
| Apache + AGPL supply chain | `cargo deny check` (Gate 4); the in-tree Levenshtein avoids new licence-audit entries. |
| AGPL containment | Codex is `AGPL-3.0-or-later`; Spark consumes via runtime dep; symmetric with Aperture's case for Sieve. |
| Public-API lock | `cargo public-api -p codex` (Gate 2) keeps `SchemaCatalogue`, `BlessedAttribute`, `LintReport`, `LintViolation`, `ViolationKind` stable; semver-checks (Gate 3) confirms additive-only changes between releases. |

## What is NOT measured at v0

- Throughput / latency at scale (millions of attributes per
  second). Codex v0 lives in init paths, not in hot loops; the
  10 ms budget covers application-startup overhead.
- gRPC / HTTP latency. The library shape has no network surface.
- Multi-version migration semantics. Single pinned version at v0.
- Per-tenant overlay performance. Not in v0 scope.
