# Shared Artefacts Registry — Spark v0

> **Wave**: DISCUSS — Phase 2 (Journey Visualisation, integration check).
> **Author**: Luna (`nw-product-owner`).
> **Date**: 2026-05-06.
> **Companion documents**: `journey-spark.yaml`, `journey-spark-visual.md`, `journey-spark.feature`, `user-stories.md`.

Every `${variable}` referenced in any of Spark's discuss artefacts has a single source of truth recorded in this file. Drift between source and consumer is the primary failure mode this registry exists to prevent.

The registry is grouped by integration risk. **HIGH** items break the wire contract or the contract Spark has with the OTel SDK or with Aperture; **MEDIUM** items break developer expectations or schema forward-compatibility; **LOW** items are cosmetic but worth tracking.

---

## HIGH-risk artefacts (wire / OTel SDK / Aperture contracts)

### `spark_init_function`

| Field | Value |
|---|---|
| Source of truth | `crates/spark/src/lib.rs` :: `pub fn init(config: SparkConfig) -> Result<SparkGuard, SparkError>` |
| Displayed as | `spark::init` |
| Consumers | every Spark-instrumented application; Spark crate's own integration tests; `journey-spark.yaml` step 2 + step 3; `user-stories.md` US-SP-01..US-SP-06 |
| Owner | `spark` (this feature) |
| Integration risk | HIGH — `spark::init` is the only public entry point; any breaking signature change forces every consumer to rebuild. `#[non_exhaustive]` postures locked in DESIGN protect against this. |
| Validation | `cargo public-api` over the spark crate (Gate 2 of the inherited harness ADR-0005 pattern) catches signature drift. |

### `spark_config_builder`

| Field | Value |
|---|---|
| Source of truth | `crates/spark/src/config.rs` (DESIGN-locked module path) :: `pub struct SparkConfig` and its builder methods |
| Displayed as | `SparkConfig::for_service`, `.require_tenant_id()`, `.with_tenant_id(...)`, `.with_feature_flags(...)`, `.with_experiment_id(...)`, `.with_endpoint(...)`, `.with_flush_timeout(...)` |
| Consumers | every application call site that constructs a config; `user-stories.md` US-SP-01, US-SP-02, US-SP-03, US-SP-04 |
| Owner | `spark` |
| Integration risk | HIGH — the builder pattern is the v0 contract. Adding a builder method is non-breaking; renaming or removing one is. |
| Validation | `cargo public-api` over the spark crate. DESIGN ADR locks `#[must_use]` on the builder return type. |

### `spark_error_variants`

| Field | Value |
|---|---|
| Source of truth | DESIGN-wave ADR (Morgan locks the exact variant set). DISCUSS specifies the closed set: `MissingRequiredAttribute { name }`, `InvalidEndpoint { endpoint, reason }`, `ExporterInitFailed { reason }`, `GlobalAlreadyInitialised`. |
| Displayed as | `SparkError::{MissingRequiredAttribute,InvalidEndpoint,ExporterInitFailed,GlobalAlreadyInitialised}` |
| Consumers | every application's error-handling path; `user-stories.md` US-SP-02, US-SP-04 |
| Owner | `spark` |
| Integration risk | HIGH — pattern matching on the variants is what application authors write to decide fatal-or-recoverable. Renames break consumers. |
| Validation | DESIGN locks `#[non_exhaustive]` on the enum. UAT scenarios in `journey-spark.feature` assert each variant by structural pattern (variant name and the named field). |

### `spark_guard_type`

| Field | Value |
|---|---|
| Source of truth | DESIGN-wave ADR. DISCUSS specifies: a value type returned from `spark::init`, opaque (no public fields), implements `Drop` synchronously calling `force_flush` with `flush_timeout` deadline. |
| Displayed as | `SparkGuard` |
| Consumers | application's `main` function (binds the guard to a local variable so the `Drop` runs at scope exit); `user-stories.md` US-SP-01, US-SP-06 |
| Owner | `spark` |
| Integration risk | HIGH — losing the guard (binding to `_` instead of `_guard`) drops it immediately, flushing nothing useful and stopping the OTel pipeline before the application has emitted anything. The crate's docs MUST warn against this. |
| Validation | DESIGN-wave ADR documents the `#[must_use]` posture. The doc-test for `spark::init` shows the canonical pattern (`let _guard = spark::init(config)?;`). |

### `house_attribute_set`

| Field | Value |
|---|---|
| Source of truth | `wave-decisions.md > Q5` plus this file |
| Displayed as | `service.name` (always required), `tenant.id` (opt-in required), `feature_flag.{key}` (optional, namespace-prefixed), `experiment.id` (optional) |
| Consumers | OTel `Resource` constructed at `spark::init`; every emitted `ExportTraceServiceRequest` / `ExportLogsServiceRequest` / `ExportMetricsServiceRequest`; Aperture's `request_received` and `sink_accepted` events; future Aegis (Phase 2) for `tenant.id`; future Loom (Phase 2) for `feature_flag.*` and `experiment.id`; future Codex for semconv validation |
| Owner | `spark` (the v0 schema); future Codex (Phase 0+) for the published-semconv version |
| Integration risk | HIGH — these names are the contract every Phase-2 component will read. Renaming any is a breaking change across the entire Kaleidoscope integration plane. |
| Validation | UAT scenarios in `journey-spark.feature` ("A traces export carries all four house attributes...", "A logs export carries the same four house attributes...", "A metrics export carries the same four house attributes...") assert the names verbatim. The CI invariant `house_attribute_completeness` defends presence on every emitted signal. |

### `feature_flag_namespace`

| Field | Value |
|---|---|
| Source of truth | `wave-decisions.md > Q5` and the architecture document's roadmap C.2 verbiage (`feature_flag.*`) |
| Displayed as | `feature_flag.{key}` — namespace prefix `feature_flag.` followed by the developer-supplied key |
| Consumers | OTel Resource attribute keys; Aperture's downstream consumers; user-stories.md US-SP-03 |
| Owner | `spark` (the v0 namespace); future Codex if OTel semconv stabilises on a different prefix |
| Integration risk | HIGH — the prefix is what makes house attributes searchable in operator log aggregators. Drift would split a query into two prefixes. |
| Validation | UAT scenario in `journey-spark.feature` ("A traces export carries all four house attributes on the Resource") asserts `feature_flag.checkout-v2` (with the `.` separator) verbatim. |

### `otlp_wire_pin`

| Field | Value |
|---|---|
| Source of truth | DESIGN-wave ADR (Morgan picks the `opentelemetry-otlp` minor version). The constraint inherited from harness ADR-0003: `opentelemetry-proto =0.27.0` exactly. |
| Displayed as | `opentelemetry-otlp = "<DESIGN-locked minor>"` plus `opentelemetry-proto = "=0.27.0"` (transitive, but Spark MUST be compatible) |
| Consumers | Spark's `Cargo.toml`; the wire bytes Spark emits; Aperture (which validates them via the harness); the harness corpus that defends the wire shape |
| Owner | DESIGN-wave for Spark; harness for the proto version |
| Integration risk | HIGH — if Spark's `opentelemetry-otlp` produces wire bytes that the harness's pinned `opentelemetry-proto` cannot decode, the entire Kaleidoscope integration plane breaks. |
| Validation | Spark's integration tests MUST run a real `ExportTraceServiceRequest` through a real Aperture instance (running the real harness) and assert acceptance. KPI 1 and 4 defend this end-to-end. |

### `otel_global_provider`

| Field | Value |
|---|---|
| Source of truth | `opentelemetry::global::set_tracer_provider` (the upstream OTel SDK function) |
| Displayed as | `opentelemetry::global::tracer(...)` (and `logger_provider`, `meter_provider`) |
| Consumers | every application call site that records a span / log / metric via the standard OTel API |
| Owner | upstream `opentelemetry` crate |
| Integration risk | HIGH — Spark's job is to set this provider exactly once with the configured Resource. A second call to `spark::init` MUST return `GlobalAlreadyInitialised` so the global state stays coherent. |
| Validation | Unit test in `crates/spark/tests/` enforces the single-init invariant. UAT scenario "spark::init refuses a second call in the same process". |

---

## MEDIUM-risk artefacts (developer-facing schema and observability)

### `spark_version`

| Field | Value |
|---|---|
| Source of truth | `crates/spark/Cargo.toml` :: `package.version` |
| Displayed as | `${spark_version}` |
| Consumers | doc-tests; the `tracing` INFO event written by `spark::init`; future Codex (when Spark advertises its semconv-version pin) |
| Owner | `spark` |
| Integration risk | MEDIUM — drift between Cargo metadata and the runtime version string makes incident triage harder when an operator wonders "which Spark released this telemetry?" |
| Validation | Spark reads `env!("CARGO_PKG_VERSION")` once at startup and includes the value in the resolved-config tracing event. Hand-written test asserts the runtime value matches the build-time value. |

### `otlp_endpoint`

| Field | Value |
|---|---|
| Source of truth | Resolution chain: `SparkConfig::with_endpoint` (highest) > `OTEL_EXPORTER_OTLP_ENDPOINT` env var > default `http://localhost:4317` |
| Displayed as | `${otlp_endpoint}` |
| Consumers | `opentelemetry-otlp` exporter target; Spark's resolved-config tracing event; Aperture's `listener_bound` addr (matches when the operator runs Aperture locally); user-stories.md US-SP-04 |
| Owner | `spark` (the resolution chain); operator (the actual value) |
| Integration risk | MEDIUM — wrong endpoint sends valid records into a black hole. Spark mitigates by writing the resolved value to `tracing` so the developer can see which endpoint was chosen. |
| Validation | UAT scenarios in `journey-spark.feature`: "SparkConfig::with_endpoint takes precedence over OTEL_EXPORTER_OTLP_ENDPOINT" and "OTEL_EXPORTER_OTLP_ENDPOINT is honoured when SparkConfig::with_endpoint is not called". |

### `flush_timeout_ms`

| Field | Value |
|---|---|
| Source of truth | `SparkConfig::with_flush_timeout` > default `5000` (5 s) |
| Displayed as | `${flush_timeout_ms}` |
| Consumers | `SparkGuard::Drop` deadline; Spark's resolved-config tracing event; the deadline-exceeded WARN event; user-stories.md US-SP-06 |
| Owner | `spark` |
| Integration risk | MEDIUM — too short drops in-flight exports on every clean exit; too long blocks the application's exit indefinitely (in practice). The default 5 s matches the OTel SDK's recommended exporter timeout. |
| Validation | UAT scenarios "SparkGuard drop flushes pending exports within the configured deadline" and "SparkGuard drop emits a deadline-exceeded warning when downstream is slow". |

### `required_attribute_lint_set`

| Field | Value |
|---|---|
| Source of truth | `wave-decisions.md > Q2 + Q3` and this file |
| Displayed as | `{service.name (always), tenant.id (when require_tenant_id() is called)}` |
| Consumers | `spark::init`'s lint pass; the `MissingRequiredAttribute` error variant; user-stories.md US-SP-02 |
| Owner | `spark` v0; future Codex once published semconv stabilises |
| Integration risk | MEDIUM — adding a new required attribute is a behavioural change every consumer must adapt to. Defaulted-off via opt-in builder methods (like `require_tenant_id`) preserves backwards compatibility. |
| Validation | UAT scenarios in `journey-spark.feature`: "spark::init refuses missing required tenant.id with a precise error", "spark::init refuses empty-string tenant.id with the same error as missing". |

### `transport_default`

| Field | Value |
|---|---|
| Source of truth | `wave-decisions.md > Q1`. Default: gRPC. Override via `OTEL_EXPORTER_OTLP_PROTOCOL` env var (which `opentelemetry-otlp` honours upstream). |
| Displayed as | `grpc` (the literal protocol identifier OTel uses) |
| Consumers | `opentelemetry-otlp` exporter constructor; Spark's resolved-config tracing event; Aperture's gRPC listener (the matching transport); user-stories.md US-SP-04 |
| Owner | `spark` (the default); upstream `opentelemetry-otlp` (the env var contract) |
| Integration risk | MEDIUM — wrong transport pairs Spark with the wrong Aperture listener (gRPC port 4317 vs HTTP port 4318). Matching the OTel-canonical default avoids surprise. |
| Validation | Slice 01 demo uses gRPC (the default); the integration test asserts the wire reaches Aperture's gRPC listener. |

---

## LOW-risk artefacts (vocabulary and convention)

### `spark_log_event_vocabulary`

| Field | Value |
|---|---|
| Source of truth | `journey-spark.yaml` step 3 + step 5 `tui_mockup` blocks; `wave-decisions.md > D5` |
| Displayed as | The closed set on the application's `tracing` facade: `{spark::init succeeded, spark: shutdown initiated, spark: shutdown complete drained=N, spark: flush deadline exceeded dropped=N, spark: exporter initialisation failed}` |
| Consumers | application's tracing-facade subscribers (whatever the application configured — `tracing-subscriber`, `slog`, etc.); spark crate's unit tests asserting tracing event capture |
| Owner | `spark` |
| Integration risk | LOW — adding a new event message is additive. The risk is renaming an existing one (operator queries break). |
| Validation | DESIGN-wave: a static set of `tracing` macro invocations in `crates/spark/src/`. Renames require a version bump documented in the changelog. |

### `walking_skeleton_demo`

| Field | Value |
|---|---|
| Source of truth | `slices/slice-01-walking-skeleton.md` :: Demo command section |
| Displayed as | `cargo run --example send_one_span_grpc` (against an Aperture instance running locally) |
| Consumers | every developer reading the README "getting started" section; CI (which runs the same demo as a regression test); user-stories.md US-SP-01 |
| Owner | `spark` |
| Integration risk | LOW — the demo is the public-facing proof. If it stops working, the README's first-impression value is broken. |
| Validation | KPI 1 — the slice-01 demo command runs end-to-end in CI without manual intervention. |

### `otel_canonical_env_vars`

| Field | Value |
|---|---|
| Source of truth | OpenTelemetry specification (`OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_EXPORTER_OTLP_PROTOCOL`, `OTEL_SERVICE_NAME`, etc.) |
| Displayed as | the env-var names exactly as the OTel spec publishes them |
| Consumers | `opentelemetry-otlp`'s upstream env-var resolver; Spark's resolved-config tracing event; user-stories.md US-SP-04 |
| Owner | upstream OpenTelemetry; `spark` v0 honours them but does not redefine them |
| Integration risk | LOW — Spark's contract is "we honour what OTel publishes". If OTel renames an env var, Spark inherits the change via the `opentelemetry-otlp` minor version bump. |
| Validation | UAT scenario "OTEL_EXPORTER_OTLP_ENDPOINT is honoured when SparkConfig::with_endpoint is not called" asserts the canonical name (with the `OTEL_` prefix). |

---

## CI invariants enforced by this registry

The registry is not just a document — it names five CI-enforced invariants. All five are reiterated in `journey-spark.yaml > integration_validation > ci_invariants` for the DEVOPS wave to pick up:

| Invariant | Mechanism | Owner |
|---|---|---|
| `single_init_call` | Unit test in `crates/spark/tests/` asserting that a second `spark::init` in the same process returns `Err(SparkError::GlobalAlreadyInitialised)`. | spark crate's CI |
| `no_telemetry_on_telemetry` | Integration test that subscribes to the application's `tracing` facade AND plugs a `RecordingSink` behind Aperture; after `spark::init`, the tracing subscriber MUST capture exactly one INFO event with target=`spark`, and the `RecordingSink` MUST NOT have received any `ExportTraceServiceRequest` with `service.name="spark"` or any other Spark-internal identifier. | spark crate's CI |
| `house_attribute_completeness` | Integration test asserts that for the canonical SparkConfig (all four house attributes set), every emitted signal carries all four on its Resource — in the wire bytes Aperture's harness sees. | spark crate's CI |
| `no_unsafe_code` | Crate-root `#![forbid(unsafe_code)]` attribute; `cargo deny check` at workspace level. | spark crate |
| `mutation_kill_rate_100_percent` | `cargo mutants` in CI (gate-5-mutants-spark) inheriting Aperture's `--in-diff` approach; every modified file must reach 100% mutation kill rate per ADR-0005 of the harness. | DEVOPS workflow YAML |

---

## How to add a shared artefact to this registry

When a new `${variable}` enters any DISCUSS artefact:

1. Add a section above with all six fields (Source of truth, Displayed as, Consumers, Owner, Integration risk, Validation).
2. Cross-reference any UAT scenario or CI invariant that defends the artefact.
3. If the artefact is HIGH-risk, surface the dependency to Morgan (DESIGN) explicitly in `wave-decisions.md`.

When an existing `${variable}` is renamed or its source moves:

1. Update this registry first.
2. Walk the consumers list and update each.
3. Update the corresponding UAT scenarios.
4. Re-run peer review before handoff to DESIGN.
