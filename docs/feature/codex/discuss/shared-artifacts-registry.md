# Codex v0 — shared artefacts registry

Every `${variable}` referenced in stories or scenarios is registered
here with source-of-truth, consumers, integration risk, and CI
validation per the nWave DISCUSS mandate.

## High-risk artefacts

| Artefact | Source of truth | Displayed as | Consumers | Integration risk | Validation |
|---|---|---|---|---|---|
| Pinned OTel semconv version | workspace `Cargo.lock` pin on `opentelemetry-semantic-conventions =0.27` | a Rust crate version | Codex's corpus generator, Spark's Resource composer, Aperture's harness validation | HIGH — corpus regeneration must match pin movement; if Codex's catalogue lags behind Spark's emitter, the lint produces false positives | `slice_02_otel_semconv_corpus` integration test enforces the catalogue covers every upstream attribute |
| `SchemaCatalogue` public type | `crates/codex/src/lib.rs` (DESIGN's call on exact shape) | Rust struct with `new()` and `validate(...) -> Result<(), LintReport>` | `crates/spark` slice 06 integration; future Aperture lint integration | HIGH — public API surface; ADR-equivalent lock | `cargo public-api -p codex` (Gate 2); `cargo semver-checks` (Gate 3) |
| `LintReport` and `LintViolation` types | `crates/codex/src/lib.rs` | Rust types implementing `Display` for human-readable output | Spark's warn-mode tracing emit; Spark's strict-mode `SparkError::SchemaValidation(...)` | HIGH — observable contract shape; consumers depend on `Display` rendering format | slice 04 asserts `Display` text; slice 06 asserts the warn message |

## Medium-risk artefacts

| Artefact | Source of truth | Displayed as | Consumers | Integration risk | Validation |
|---|---|---|---|---|---|
| Kaleidoscope-house attribute set (`tenant.id`, `feature_flag.*`, `experiment.id`) | Codex's hand-written part of the catalogue plus Spark's existing emission code | three exact-match plus one prefix-match `BlessedAttribute` entries | Codex catalogue, Spark Resource composer | MEDIUM — adding a new house attribute requires updating both Codex's catalogue and Spark's emitter; mismatch is a silent regression | slice 03 asserts the four house attributes blessed; the slice-06 Spark integration test asserts a canonical Resource validates clean |
| Generated semconv corpus (`crates/codex/src/generated/{...}.rs`) | the maintainer regeneration ritual; output is checked into git | a static slice of `BlessedAttribute` records | Codex's `SchemaCatalogue::new` constructor | MEDIUM — out-of-date generated file ⇒ catalogue lags semconv pin ⇒ false positives | the regeneration ritual is documented in slice 02; future drift is caught by slice 02's full-corpus test |
| Levenshtein implementation | in-tree at `crates/codex/src/{...}.rs` (DESIGN-named module) | a private function `pub(crate) fn levenshtein(a: &str, b: &str) -> usize` | `LintViolation::nearest_blessed_match` population at validate time | MEDIUM — wrong distance threshold ⇒ either false-positive suggestions or missed close-typo helpfulness | slice 05 asserts known-typo fixtures resolve to expected suggestions |

## Low-risk artefacts (namespacing, conventions)

| Artefact | Source of truth | Displayed as | Consumers | Integration risk | Validation |
|---|---|---|---|---|---|
| `target = "spark"` tracing target | Spark's existing observability vocabulary | the `target` field on tracing events | operator's tracing_subscriber; log aggregator filter | LOW — Spark's existing namespace; Codex doesn't introduce a new target | slice 06 integration test asserts the warn event arrives with `target = "spark"` |
| `nearest_blessed_match` string template for prefix families | Codex's suggestion logic | e.g. `"feature_flag.{key}"` literal | operator reading the warn line | LOW — string formatting; readable to humans either way | slice 05 acceptance criteria name the format |
| Strict-vs-warn knob name | `SparkConfig::with_strict_schema_lint(bool)` builder method | a Rust method on `SparkConfig` | Spark consumers configuring the lint posture | LOW — additive API on an existing builder pattern | slice 06 acceptance criteria name the method; documented in the post-DELIVER ADR amendment |

## CI invariants protecting these artefacts

| Invariant | Owner | Mechanism |
|---|---|---|
| Canonical Resources validate clean | Codex crate | `slice_01_walking_skeleton`, `slice_03_house_attributes`, slice-06 Spark integration |
| Full OTel semconv 0.27 corpus blessed | Codex crate | `slice_02_otel_semconv_corpus` exercises the upstream corpus |
| Unknown attributes produce structured violations | Codex crate | `slice_04_unknown_attribute_lint` |
| Close typos surface "did you mean" suggestions | Codex crate | `slice_05_fuzzy_suggestions` |
| Spark integration surfaces violations at integration time | Spark crate | slice-06 integration test in Spark |
| Public surface stable | DEVOPS workflow | `cargo public-api -p codex` Gate 2; `cargo semver-checks` Gate 3 |
| Apache + AGPL supply chain | DEVOPS workflow | `cargo deny check` Gate 4 |
| 100% mutation kill rate per slice | DEVOPS workflow | `cargo mutants --package codex --in-diff` Gate 5 |
| AGPL containment | DEVOPS workflow | the crate's `license = "AGPL-3.0-or-later"`; symmetric AGPL runtime dep on Spark mirrors Sieve precedent |
