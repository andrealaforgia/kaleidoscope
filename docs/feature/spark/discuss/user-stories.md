<!-- markdownlint-disable MD024 -->

# User Stories — `spark` v0

> **Wave**: DISCUSS — Phase 3 (Requirements crafting).
> **Author**: Luna (`nw-product-owner`).
> **Date**: 2026-05-06.
> **Companion documents**: `journey-spark.yaml`, `journey-spark.feature`, `journey-spark-visual.md`, `story-map.md`, `outcome-kpis.md`, `dor-validation.md`, `wave-decisions.md`, `shared-artifacts-registry.md`.

> Persona note: Spark's primary consumer is a **Rust developer** instrumenting an application with the OTel SDK; secondary consumers are operators redirecting Spark's traffic via env vars, future Aegis (Phase 2) for `tenant.id` per-request derivation, future Loom (Phase 2) for `feature_flag.*` and `experiment.id` consumption, and Kaleidoscope CI. House style: British English, no human-effort estimation.

---

## System Constraints

These constraints apply to every story below and are not repeated in each. They are the load-bearing decisions Bea (and Andrea-by-delegation) locked before this DISCUSS round.

1. **Library, not service**: Spark is a Rust crate at `crates/spark/`. It exposes a public API. It is not a service, has no network surface of its own, opens no listening ports, runs no daemon. The application that links Spark may itself be a service; Spark is not.
2. **Apache-2.0 licence**: Spark is in the SDK class per `LICENSING.md`; it must remain embeddable in proprietary application code without copyleft contamination.
3. **`opentelemetry` Rust SDK is the substrate**: Spark depends on the upstream `opentelemetry`, `opentelemetry-otlp`, and (transitively) `opentelemetry-proto` crates. It does not redefine, wrap, or re-export OTLP message types under a Spark-local name; consumers import OTel SDK types directly via the standard `opentelemetry` crate.
4. **No `unsafe` code**: `#![forbid(unsafe_code)]` at the crate root. Idiomatic Rust: data + free functions, `dyn Trait` only where polymorphism is genuinely needed, no class-style inheritance hierarchies.
5. **No telemetry from Spark itself through the OTel pipeline Spark configured** (D5): Spark's own diagnostic events (init success, flush success, flush deadline, exporter init failure) go to the application's `tracing` facade, NEVER through `opentelemetry::global::*`. The CI invariant `no_telemetry_on_telemetry` defends this.
6. **No panic on user input**: every error path returns a `Result::Err` variant. Panicking is reserved for true invariant violations of Spark's own internal logic, not for handling misconfigured `SparkConfig` values.
7. **Closed `SparkError` variant set at v0** (D2): `MissingRequiredAttribute`, `InvalidEndpoint`, `ExporterInitFailed`, `GlobalAlreadyInitialised`. Adding variants is non-breaking (`#[non_exhaustive]`); renaming requires a version bump.
8. **Single global init** (D7): a second call to `spark::init` in the same process returns `GlobalAlreadyInitialised`. Mirrors `opentelemetry::global::set_tracer_provider`'s own guard.
9. **`OTEL_*` environment-variable contract is upstream's** (D6): Spark honours `OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_EXPORTER_OTLP_PROTOCOL`, `OTEL_SERVICE_NAME`, etc. Spark does NOT introduce `SPARK_*` env vars. Configuration precedence: `SparkConfig` builder values > `OTEL_*` env vars > Spark defaults.
10. **No auto-instrumentation at v0**: Spark wraps the OTel SDK; the application calls Spark explicitly via `spark::init`, then uses the standard OTel API for span/log/metric emission.
11. **No CI lint that fails the build for missing semantic-conventions attributes at v0**: that is Codex's job in Phase 0+. Spark v0 does the runtime check at `init` time.
12. **British English**, **no human-effort estimation**, **trunk-based development**, **mutation testing per-feature with 100% kill rate** (per harness ADR-0005 Gate 5).

---

## US-SP-01 — Initialise Spark and round-trip a span end-to-end

### Elevator Pitch

- **Before**: A Rust developer wanting to instrument their service for Kaleidoscope has to wire up `opentelemetry`, `opentelemetry-otlp`, and `opentelemetry_sdk::Resource` by hand, juggle the right `tonic` version against the right `opentelemetry-proto` version, remember which fields the OTel SDK considers required, and hope they configured the exporter the same way Aperture's listener expects. Until that wiring is done, no telemetry reaches Aperture and no integration confidence exists.
- **After**: The developer adds `spark = "0.1"` to `Cargo.toml`, writes `let _guard = spark::init(SparkConfig::for_service("payments-api").with_tenant_id("acme-prod"))?;` in `main`, calls `opentelemetry::global::tracer("checkout-service").in_span("checkout.complete", |_| {...})`, and sees Aperture's `RecordingSink` (in the integration test) capture exactly one `ExportTraceServiceRequest` with `service.name="payments-api"` and `tenant.id="acme-prod"` on its `Resource`. The runtime entry point is `spark::init(SparkConfig::for_service("payments-api"))`; the test entry point is `cargo test -p spark slice_01_walking_skeleton`.
- **Decision enabled**: The developer decides Spark's value proposition holds. They have seen one real OTLP export round-trip end to end, with the house attributes attached, against a real Aperture instance running the real harness. Every subsequent slice adds capability without re-litigating the integration boundary.

### Problem

Spark's value proposition is "one function call replaces a page of OTel SDK setup". Until one valid export round-trips successfully — through a real OTLP/gRPC exporter, against a real Aperture instance running the real conformance harness, with the house attributes intact — every other capability (the lint, the env-var precedence, the deadline-exceeded flush) is theoretical. This story is the one that proves the value proposition.

### Who

- **Application developer at `acme-observability`**: wants to instrument their first Rust service for Kaleidoscope and needs to confirm the SDK works end to end.
- **Kaleidoscope CI**: runs the slice-01 example + integration test on every commit affecting `crates/spark/**` to defend the integration contract.
- **Future Aegis component author** (Phase 2): the `tenant.id` Resource attribute path established here is what Aegis will replace per-request.

### Solution

A single `spark::init(config) -> Result<SparkGuard, SparkError>` entry point that, on `Ok`:

1. Composes an `opentelemetry_sdk::Resource` containing every house attribute the `SparkConfig` carried (`service.name` and `tenant.id` at the walking-skeleton minimum).
2. Constructs an `opentelemetry-otlp` exporter targeting the resolved endpoint over OTLP/gRPC (the v0 default).
3. Sets the OTel global tracer / logger / meter providers wired to the configured Resource and exporter.
4. Returns a `SparkGuard` whose `Drop` impl flushes pending exports synchronously with the configured deadline.

The integration test verifies the full chain by spawning a real Aperture (via `aperture::spawn`) with a `RecordingSink` plugged in, pointing Spark at the bound port via `SparkConfig::with_endpoint`, recording one span via the standard OTel API, and asserting the recording sink received the expected payload.

### Domain Examples

#### 1: A first-time integration at `acme-observability`

A developer at `acme-observability` (the same fictional organisation that pilots Aperture and the harness) adds Spark to their `payments-api` service. Their `main.rs` becomes:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = spark::init(
        spark::SparkConfig::for_service("payments-api")
            .require_tenant_id()
            .with_tenant_id("acme-prod"),
    )?;
    // ... business logic emitting spans via opentelemetry::global::tracer ...
    Ok(())
}
```

Their first deployment to staging runs against a `localhost:4317` Aperture; the operator's stderr shows `event=sink_accepted signal=traces resource.service.name="payments-api" resource.tenant.id="acme-prod"` per batch. The developer confirms instrumentation in 10 minutes.

#### 2: A walking-skeleton CI run in Kaleidoscope's own pipeline

Kaleidoscope CI runs `cargo test -p spark slice_01_walking_skeleton`. The test:

1. Calls `aperture::spawn(Config::for_test())` with a `RecordingSink` and an ephemeral loopback port.
2. Calls `spark::init(SparkConfig::for_service("payments-api").require_tenant_id().with_tenant_id("acme-prod").with_endpoint(format!("http://{}", aperture_handle.grpc_addr())))`.
3. Records one span via `opentelemetry::global::tracer("ci-runner").in_span("walking-skeleton", |_| {})`.
4. Drops the `SparkGuard` (which forces the flush).
5. Calls `aperture_handle.shutdown().await`.
6. Asserts the `RecordingSink` saw exactly one `ExportTraceServiceRequest` with the expected `service.name` and `tenant.id` on its `Resource`.

The whole test runs in under a second. The CI run is green.

#### 3: A Sieve-shaped future-component author validating the Resource contract

A future contributor implementing Sieve (Phase 1) writes a unit test that constructs a Spark-emitted `ExportTraceServiceRequest` (using the same OTel SDK + Spark setup as the walking skeleton) and asserts that their Sieve sampling logic can read `tenant.id` from the Resource directly. The contract holds because Spark's house-attribute injection lives on the Resource, not on per-span attributes (D4); Sieve's per-tenant sampling decisions can read the Resource without traversing every span's attributes.

### UAT Scenarios (BDD)

#### Scenario: spark::init constructs the OTel SDK with all four house attributes on the Resource

```
Given a SparkConfig with service.name="payments-api", tenant.id="acme-prod", feature_flag={"checkout-v2":"on"}, experiment.id="exp-2026-Q2-pricing"
When the application calls spark::init(config)
Then the returned SparkGuard is Ok
And the OTel global tracer provider's Resource includes service.name="payments-api"
And the Resource includes tenant.id="acme-prod"
And the Resource includes feature_flag.checkout-v2="on"
And the Resource includes experiment.id="exp-2026-Q2-pricing"
```

#### Scenario: A traces export carries all four house attributes on the Resource

```
Given an Aperture instance running locally with a RecordingSink
And spark::init has succeeded with service.name="payments-api", tenant.id="acme-prod", feature_flag={"checkout-v2":"on"}, experiment.id="exp-2026-Q2-pricing"
When the application records one span via opentelemetry::global::tracer("checkout-service").in_span("checkout.complete", ...)
And the SparkGuard is dropped
Then the RecordingSink received exactly one ExportTraceServiceRequest
And the request's first ResourceSpans.resource.attributes contains service.name="payments-api"
And the same Resource contains tenant.id="acme-prod"
And the same Resource contains feature_flag.checkout-v2="on"
And the same Resource contains experiment.id="exp-2026-Q2-pricing"
```

#### Scenario: spark::init writes its own diagnostic to the tracing facade, not the OTel pipeline

```
Given the application has subscribed to its tracing facade and to a RecordingSink behind Aperture
When spark::init succeeds
Then exactly one tracing event with target="spark" and message containing "spark::init succeeded" is captured by the application's subscriber
And no ExportTraceServiceRequest reaches the RecordingSink as a result of the init itself
```

#### Scenario: SparkConfig is plain data with no I/O

```
Given a SparkConfig built from a sequence of builder calls
When the test observes stderr, stdout, and the application's tracing facade
Then no output has been written to any of those three channels
And no OTLP export has reached any backend
```

### Acceptance Criteria

- [ ] `spark::init(config)` returns `Ok(SparkGuard)` for the canonical `SparkConfig` (service.name + tenant.id set, valid endpoint, single-init).
- [ ] The OTel global tracer provider's `Resource` includes `service.name` exactly as set on the `SparkConfig`.
- [ ] The Resource includes `tenant.id` exactly as set on the `SparkConfig` when `with_tenant_id` was called.
- [ ] An `ExportTraceServiceRequest` emitted via `opentelemetry::global::tracer(...).in_span(...)` reaches Aperture's listener and Aperture's `RecordingSink` records the request.
- [ ] The recorded request's `ResourceSpans.resource.attributes` contains both `service.name` and `tenant.id` with the configured values.
- [ ] A single `tracing` INFO event with `target="spark"` and message containing `"spark::init succeeded"` is captured by a subscriber the application configured.
- [ ] No `ExportTraceServiceRequest`, `ExportLogsServiceRequest`, or `ExportMetricsServiceRequest` reaches the `RecordingSink` carrying `service.name="spark"` or any Spark-internal identifier (the `no_telemetry_on_telemetry` invariant defends this).
- [ ] The Slice-01 demo command sequence in `slices/slice-01-walking-skeleton.md` runs end-to-end in CI without manual intervention; this is the structural test that defends KPI 1.

### Outcome KPIs

- **Who**: Rust developers integrating Spark into their service for the first time.
- **Does what**: round-trip a real `ExportTraceServiceRequest` end-to-end (from `spark::init` through the OTel SDK and the OTLP/gRPC exporter to a real Aperture instance and into the `RecordingSink`).
- **By how much**: 100% of the documented Slice-01 demo command sequence completes without manual intervention.
- **Measured by**: Slice-01 demo command in `slices/slice-01-walking-skeleton.md` is executable; integration test in CI asserts `RecordingSink` capture and tracing-event capture.
- **Baseline**: greenfield (zero today; the v0 launch establishes the practice).

### Technical Notes

- DESIGN-wave decision (Morgan): which `opentelemetry-otlp` minor version to pin against; the exact internal module split (`config.rs`, `error.rs`, `guard.rs`, `init.rs`, `lib.rs` is one option); the `tracing` macro target string (`spark` vs `kaleidoscope::spark`).
- The `RecordingSink` Spark's tests drive against is `aperture::testing::RecordingSink` (already in the Aperture v0.1.0 public surface; see `crates/aperture/src/testing.rs`). Spark depends on `aperture` as a `[dev-dependencies]` only — never a runtime dependency. This is what makes Spark Apache-2.0-compatible: the AGPL `aperture` crate is not in the production dependency tree.
- `aperture` as a dev-dep is the only way Spark can run an integration test against a real Aperture without bringing the AGPL crate into application code. DESIGN ADR locks this dependency posture.

### Dependencies

- `crates/aperture/v0.1.0` shipped (it is, per the recent commits b96eb7d and earlier on main). Spark's integration tests drive against `aperture::spawn` + `aperture::testing::RecordingSink`.
- `crates/otlp-conformance-harness/v0.1.0` shipped (transitive: it is what Aperture uses to validate Spark-emitted bytes).

---

## US-SP-02 — Refuse missing required attributes at init time, never silently emit broken telemetry

### Elevator Pitch

- **Before**: An OTel SDK setup that omits `service.name` produces telemetry that downstream backends struggle with (Loki uses `service.name` as a label primary key; Tempo as a default trace search facet; Mimir as a metric labelset filter). The same telemetry will be rejected by Aperture's harness validation eventually, but only at the wire — by which point the application has been running and emitting data the operator cannot see in their queries. The failure is silent at process startup and loud only at the gateway.
- **After**: The application calls `spark::init(SparkConfig::for_service("").require_tenant_id())`. Spark's lint pass returns `Err(SparkError::MissingRequiredAttribute { name: "service.name" })` immediately. The application's error handler logs the failure and exits non-zero (or chooses to continue without telemetry, or panics — Spark surfaces the precise diagnostic and lets the application decide). Critically: no `opentelemetry-otlp` exporter has been constructed, no global tracer provider has been set, no telemetry has reached the wire.
- **Decision enabled**: The developer reading the error decides exactly which configuration field to fix. The error variant names the field (`name: "service.name"` or `name: "tenant.id"`); the error variant name (`MissingRequiredAttribute`) maps directly to a class of misconfiguration.

### Problem

A library that lets you start with a misconfiguration and emits broken telemetry until somebody triages it at the wire is hostile to debugging. The cheapest possible defence is a synchronous, in-process check at `init` time, with a precise error variant per failure class.

### Who

- **Application developer at `acme-observability` making a typo**: needs Spark to fail loudly at startup, not silently at the wire.
- **Operator at `acme-observability` reviewing a deployment**: needs the failure mode to be observable in the application's startup logs (where the application's `?` operator and its `tracing` subscriber surface the error), not in Aperture's reject events thirty minutes later.
- **Future Codex component**: will plug into Spark's lint pass to extend it with full semconv validation; the v0 inline lint is the seam.

### Solution

`spark::init` validates the `SparkConfig` synchronously, in-process, with no I/O, before constructing any OTel SDK type:

1. If `config.service_name` is empty (after trimming whitespace? no — empty-string check only at v0; Codex handles trimming and case in Phase 0+), return `Err(SparkError::MissingRequiredAttribute { name: "service.name" })`.
2. If `config.tenant_id_required` is true and `config.tenant_id` is `None` or empty, return `Err(SparkError::MissingRequiredAttribute { name: "tenant.id" })`.
3. If `config.endpoint` (or `OTEL_EXPORTER_OTLP_ENDPOINT` if not set on the config) cannot be parsed as a URI, return `Err(SparkError::InvalidEndpoint { endpoint, reason })`.

No OTel SDK type is constructed until all three checks pass.

### Domain Examples

#### 1: A typo on tenant.id at `acme-observability`

A developer at `acme-observability` building a multi-tenant gateway service writes:

```rust
let _guard = spark::init(
    spark::SparkConfig::for_service("gateway-api")
        .require_tenant_id()
        // ... forgot to call .with_tenant_id() ...
)?;
```

`spark::init` returns `Err(SparkError::MissingRequiredAttribute { name: "tenant.id" })`. The `?` operator propagates the error to `main`'s return type; the application exits non-zero with the error printed via `Debug` (or whatever the application's error handler does). The developer reads the error, adds `.with_tenant_id("acme-prod")`, and the next run succeeds.

#### 2: An empty-string tenant.id from a misconfigured environment variable read

A second developer reads the tenant ID from a config file and forgets the file is empty. They write:

```rust
let tenant = std::fs::read_to_string("/etc/myapp/tenant").unwrap_or_default();
let _guard = spark::init(
    spark::SparkConfig::for_service("billing-api")
        .require_tenant_id()
        .with_tenant_id(&tenant),  // empty string!
)?;
```

`spark::init` treats empty-string identically to missing: `Err(SparkError::MissingRequiredAttribute { name: "tenant.id" })`. The developer learns that "the file existed but was empty" produces the same diagnostic as "the file was not read", which is the right reduction — both bugs land on the same fix path.

#### 3: An invalid endpoint URI from a sloppy config-file value

A developer copying configuration from a v1 example writes:

```rust
let _guard = spark::init(
    spark::SparkConfig::for_service("payments-api")
        .with_endpoint("htp://aperture.acme.internal:4317"),  // typo: htp
)?;
```

`spark::init` returns `Err(SparkError::InvalidEndpoint { endpoint: "htp://aperture.acme.internal:4317", reason: "scheme \"htp\" is not http or https" })`. The developer reads the reason, fixes the scheme, and the next run succeeds.

### UAT Scenarios (BDD)

#### Scenario: spark::init refuses missing required tenant.id with a precise error

```
Given a SparkConfig built with for_service("payments-api").require_tenant_id() but no with_tenant_id call
When the application calls spark::init(config)
Then the call returns Err(SparkError::MissingRequiredAttribute { name: "tenant.id" })
And no OTLP exporter was constructed
And no telemetry has reached any backend
```

#### Scenario: spark::init refuses empty-string tenant.id with the same error as missing

```
Given a SparkConfig with require_tenant_id().with_tenant_id("")
When the application calls spark::init(config)
Then the call returns Err(SparkError::MissingRequiredAttribute { name: "tenant.id" })
```

#### Scenario: spark::init refuses an invalid endpoint with a precise diagnostic

```
Given a SparkConfig with with_endpoint("htp://typo:4317")
When the application calls spark::init(config)
Then the call returns Err(SparkError::InvalidEndpoint { ... })
And the reason field names the parse failure
And no OTLP exporter was constructed
```

#### Scenario: spark::init accepts a SparkConfig without require_tenant_id() and no with_tenant_id

```
Given a SparkConfig built with for_service("payments-api") only
When the application calls spark::init(config)
Then the call returns Ok(SparkGuard)
And the OTel global tracer provider's Resource includes service.name="payments-api"
And the Resource does NOT contain tenant.id
```

#### Scenario: spark::init refuses a second call in the same process

```
Given spark::init has already returned Ok in this process
When the application calls spark::init(config) a second time
Then the call returns Err(SparkError::GlobalAlreadyInitialised)
```

### Acceptance Criteria

- [ ] `spark::init` returns `Err(SparkError::MissingRequiredAttribute { name: "tenant.id" })` when `require_tenant_id()` was called and no `with_tenant_id` was called (or `with_tenant_id("")` was called).
- [ ] `spark::init` returns `Err(SparkError::MissingRequiredAttribute { name: "service.name" })` if the `SparkConfig` was somehow constructed with an empty `service.name` (defence-in-depth — the constructor takes a non-`Option` argument so this is unreachable from the standard builder pattern).
- [ ] `spark::init` returns `Err(SparkError::InvalidEndpoint { endpoint, reason })` when the resolved endpoint cannot be parsed as a URI; the `reason` field names the parse failure.
- [ ] `spark::init` returns `Err(SparkError::GlobalAlreadyInitialised)` on the second call in the same process.
- [ ] On any `Err` return, no OTLP exporter is constructed, no global provider is set, no telemetry reaches any backend.
- [ ] `SparkError` is `pub`, derives `Debug`, implements `Display` (single-line message naming the variant), implements `std::error::Error`, and is `#[non_exhaustive]`.

### Outcome KPIs

- **Who**: Rust developers who introduce a misconfiguration into a Spark-instrumented service.
- **Does what**: receive a precise, named diagnostic at `spark::init` time, before any telemetry is emitted.
- **By how much**: 100% of misconfigurations matching one of the closed `SparkError` variants are caught at `init` (target: every UAT scenario above passes; no misconfiguration produces telemetry reaching the wire).
- **Measured by**: Spark crate's CI unit-test sweep covering each `SparkError` variant.
- **Baseline**: greenfield.

### Technical Notes

- DESIGN-wave decision (Morgan): the `SparkError` derives (`thiserror` vs hand-rolled). The `#[non_exhaustive]` attribute is locked here at DISCUSS but the implementation mechanism is DESIGN's call.
- DISCUSS-locked: empty-string values for required attributes are treated identically to absence. Whitespace-only values (`"   "`) are NOT treated as absence at v0 — Codex Phase 0+ adds whitespace handling. v0 keeps the lint cheap.
- The lint pass MUST run before any `opentelemetry_sdk` type is constructed. This avoids leaking a half-constructed exporter into a process that's about to fail-fast.

### Dependencies

US-SP-01 (the walking skeleton's Ok-path is the precondition for testing the Err paths against the same code path).

---

## US-SP-03 — Inject all four house resource attributes on every emitted signal

### Elevator Pitch

- **Before**: An OTel SDK setup without explicit Resource composition emits telemetry without `tenant.id`, without `feature_flag.*`, without `experiment.id`. The downstream stack (Aperture today, Aegis Phase 2, Loom Phase 2, Codex Phase 0+) relies on these attributes for tenant-scoped queries, feature-flag attribution, and A/B-test correlation. Without them, the operator's "show me errors for tenant X with feature checkout-v2 enabled in experiment exp-2026-Q2-pricing" query is unanswerable.
- **After**: A `SparkConfig` carrying all four house attributes produces an OTel SDK whose `Resource` injects all four onto every `ExportTraceServiceRequest`, `ExportLogsServiceRequest`, and `ExportMetricsServiceRequest`. Aperture's `sink_accepted` events show all four; Aegis (Phase 2) reads `tenant.id` from the Resource; Loom (Phase 2) reads `feature_flag.checkout-v2` and `experiment.id` from the Resource. The runtime entry point is `SparkConfig::with_feature_flags([("checkout-v2", "on")]).with_experiment_id("exp-2026-Q2-pricing")`; the assertion is the integration test capturing all four on the wire.
- **Decision enabled**: The developer planning multi-tenant or feature-flag-aware instrumentation decides Spark covers the Phase-2 contract today. They do not need to plumb tenant-derivation logic through their own code; setting the attributes once at `init` is enough.

### Problem

The roadmap C.2 names all three Kaleidoscope-specific house attributes (`tenant.id`, `feature_flag.*`, `experiment.id`). Implementing only `service.name` and `tenant.id` at v0 would force every Phase-2 component (Aegis, Loom) to wait for a v0.1 of Spark before they could integrate. Forward-compat insurance is cheaper at v0 than retrofitting later.

### Who

- **Application developer at `acme-observability` running A/B tests**: needs to attribute spans/logs/metrics to the experiment they were emitted under.
- **Application developer using feature flags**: needs to correlate telemetry with the flag state at emission time (e.g. checkout-v2 was on for this user when this error occurred).
- **Future Aegis component**: reads `tenant.id` from the Resource for per-tenant authorisation.
- **Future Loom component**: reads `feature_flag.*` and `experiment.id` for dashboards-as-code that filter by feature/experiment.

### Solution

`SparkConfig` carries optional values for `feature_flags: HashMap<String, String>` and `experiment_id: Option<String>`. `spark::init` composes the OTel `Resource` with one attribute per non-empty value:

- `service.name` (always; from `SparkConfig::for_service`).
- `tenant.id` (when `with_tenant_id` was called and the value is non-empty).
- `feature_flag.{key}` (one attribute per `(key, value)` pair in `feature_flags`).
- `experiment.id` (when `with_experiment_id` was called and the value is non-empty).

Empty-value entries are skipped, not emitted as empty-string attributes.

### Domain Examples

#### 1: An A/B test at `acme-observability`

The `acme-observability` checkout team runs `exp-2026-Q2-pricing` experiment with two cohorts. Each instance of their `payments-api` service is started with the experiment ID:

```rust
let _guard = spark::init(
    spark::SparkConfig::for_service("payments-api")
        .require_tenant_id()
        .with_tenant_id("acme-prod")
        .with_experiment_id("exp-2026-Q2-pricing"),
)?;
```

Every span / log / metric emitted by this process carries `experiment.id="exp-2026-Q2-pricing"` on its Resource. The data team's dashboard query `service.name="payments-api" AND experiment.id="exp-2026-Q2-pricing"` partitions cohort B's traffic correctly.

#### 2: Feature-flag correlation at `acme-observability`

The same checkout team rolls out `checkout-v2` to 5% of users via a feature-flag service. The team configures the application to start with whatever flag state the deployment was bound to:

```rust
let flag_state: HashMap<&str, &str> = read_flags_from_deployment_env();
let _guard = spark::init(
    spark::SparkConfig::for_service("payments-api")
        .require_tenant_id()
        .with_tenant_id("acme-prod")
        .with_feature_flags(flag_state),
)?;
```

When `checkout-v2` is on for this deployment, every emitted signal carries `feature_flag.checkout-v2="on"`. When an alert fires for this deployment, the operator immediately sees the flag state without cross-referencing a separate feature-flag log.

#### 3: A single-tenant developer who wants none of it

A developer at a single-tenant company (no tenancy, no experiments, no feature flags) writes:

```rust
let _guard = spark::init(
    spark::SparkConfig::for_service("simple-api"),
)?;
```

`spark::init` succeeds. The Resource carries only `service.name`. The opt-in builder methods (`require_tenant_id`, `with_feature_flags`, `with_experiment_id`) were never called, so those attributes do not appear. This is the minimum-viable Spark integration, and it works.

### UAT Scenarios (BDD)

#### Scenario: A traces export carries all four house attributes when all are configured

```
Given an Aperture instance running locally with a RecordingSink
And spark::init has succeeded with service.name="payments-api", tenant.id="acme-prod", feature_flag={"checkout-v2":"on"}, experiment.id="exp-2026-Q2-pricing"
When the application records one span via opentelemetry::global::tracer("checkout-service").in_span("checkout.complete", ...)
And the SparkGuard is dropped
Then the RecordingSink received exactly one ExportTraceServiceRequest
And the request's first ResourceSpans.resource.attributes contains service.name="payments-api"
And the same Resource contains tenant.id="acme-prod"
And the same Resource contains feature_flag.checkout-v2="on"
And the same Resource contains experiment.id="exp-2026-Q2-pricing"
```

#### Scenario: A SparkConfig without optional attributes produces a minimal Resource

```
Given a SparkConfig built with for_service("simple-api") only
When the application calls spark::init(config)
And records one span
And the guard drops
Then the RecordingSink received the span
And the Resource contains service.name="simple-api"
And the Resource does NOT contain tenant.id
And the Resource does NOT contain any feature_flag.* attribute
And the Resource does NOT contain experiment.id
```

#### Scenario: feature_flag attributes are namespace-prefixed with feature_flag.

```
Given a SparkConfig with .with_feature_flags([("checkout-v2", "on"), ("dark-mode", "off")])
When the application calls spark::init(config)
And records one span
And the guard drops
Then the recorded span's Resource contains attribute "feature_flag.checkout-v2" with value "on"
And the Resource contains attribute "feature_flag.dark-mode" with value "off"
And neither attribute appears WITHOUT the feature_flag. prefix
```

#### Scenario: Empty-string optional attributes are skipped, not emitted

```
Given a SparkConfig with .with_experiment_id("")
When the application calls spark::init(config)
And records one span
And the guard drops
Then the recorded span's Resource does NOT contain attribute experiment.id
```

### Acceptance Criteria

- [ ] `SparkConfig::with_feature_flags(I)` accepts any `IntoIterator<Item = (impl Into<String>, impl Into<String>)>`.
- [ ] `SparkConfig::with_experiment_id(impl Into<String>)` sets the experiment ID.
- [ ] `spark::init` composes an OTel `Resource` with one attribute per non-empty house-attribute value; empty-string values are skipped.
- [ ] Every emitted signal (`ExportTraceServiceRequest`, `ExportLogsServiceRequest`, `ExportMetricsServiceRequest`) carries every set house attribute on its `Resource` (the `house_attribute_completeness` CI invariant).
- [ ] Each `feature_flag` entry produces an attribute named `feature_flag.{key}` (with the `feature_flag.` prefix); the prefix is locked here.
- [ ] A `SparkConfig` with no optional house attributes produces a Resource containing only `service.name`.

### Outcome KPIs

- **Who**: developers building tenant-aware, feature-flag-aware, or experiment-aware Rust services on Kaleidoscope.
- **Does what**: emit telemetry that downstream Aegis / Loom / Codex consumers can attribute correctly.
- **By how much**: 100% of canonical-config emissions carry all four house attributes on the wire, asserted by the `house_attribute_completeness` CI invariant.
- **Measured by**: integration test in CI; per-Resource attribute presence assertion on every recorded export.
- **Baseline**: greenfield.

### Technical Notes

- DESIGN-wave decision (Morgan): the exact builder method signatures (`HashMap` vs slice vs iterator). The DISCUSS contract specifies the *shape* (key/value pairs for feature flags, single string for experiment ID); the *types* are DESIGN's call.
- The `feature_flag.` prefix at v0 is singular, matching the Kaleidoscope architecture document and roadmap C.2 verbiage. If the OTel semconv repository stabilises on a different prefix at the harness's pinned spec version, that mismatch is a Codex-Phase-0+ concern, not a Spark-v0 break (per `wave-decisions.md > Risks`).

### Dependencies

US-SP-01 (the walking skeleton's Resource composition is the substrate this story extends).

---

## US-SP-04 — Honour the OTel-canonical environment variables and SparkConfig precedence

### Elevator Pitch

- **Before**: An operator deploying a Spark-instrumented service to staging has hard-coded the OTLP endpoint inside the application binary. To redirect to a different Aperture (e.g. for a regional rollout), they have to rebuild the binary. This is the exact friction the OTel SDK's environment-variable contract is meant to remove.
- **After**: The operator sets `OTEL_EXPORTER_OTLP_ENDPOINT=http://aperture.eu-west.acme.internal:4317` in the Kubernetes Deployment manifest. The application's `SparkConfig` does NOT call `with_endpoint`. Spark resolves the endpoint from the env var and targets the EU-west Aperture without a rebuild. If the application later ALSO calls `SparkConfig::with_endpoint("http://override.acme.internal:4317")`, the explicit `with_endpoint` value wins (precedence: builder > env > default).
- **Decision enabled**: The operator decides Spark fits their multi-region deployment story without rebuilds. The developer decides whether their application overrides the operator's env-var or honours it (a per-application decision the precedence rule resolves cleanly).

### Problem

A telemetry SDK that ignores the OTel-canonical env vars makes itself non-portable. Every existing OTel-instrumented Rust application uses `OTEL_EXPORTER_OTLP_ENDPOINT` and friends; Spark must not break that contract.

### Who

- **Operator deploying a Spark-instrumented service across multiple regions**: needs env-var-driven endpoint redirection.
- **Application developer**: occasionally needs to override an operator-set env var (e.g. for a debug session pointing at a local Aperture).
- **OTel SDK ecosystem**: expects every Rust OTel SDK to honour `OTEL_EXPORTER_OTLP_*` env vars.

### Solution

`spark::init` resolves the endpoint, the protocol, and other operator-tunable values via this precedence chain:

1. `SparkConfig` builder method values (highest — what the application explicitly set).
2. `OTEL_*` environment variables (`OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_EXPORTER_OTLP_PROTOCOL`, `OTEL_SERVICE_NAME`, etc.).
3. Spark defaults (`http://localhost:4317`, gRPC, 5 s flush timeout).

Spark itself does not parse the env vars by hand; it lets `opentelemetry-otlp`'s upstream resolver handle them, then surfaces the resolved value via the resolved-config tracing event so the developer sees what was chosen.

### Domain Examples

#### 1: Multi-region deployment at `acme-observability`

`acme-observability` runs Aperture in three regions. Their `payments-api` Deployment manifests differ only in the `OTEL_EXPORTER_OTLP_ENDPOINT` env var: `http://aperture.us-east.internal:4317`, `http://aperture.eu-west.internal:4317`, `http://aperture.ap-south.internal:4317`. The application binary is identical across regions; the operator's deployment-config layer carries the per-region difference. Spark resolves the endpoint at startup; the resolved-config tracing event names the chosen endpoint so the operator sees it.

#### 2: A developer overriding the env var for a debug session

A developer at `acme-observability` is debugging a tenant-specific issue locally. They start their service with `OTEL_EXPORTER_OTLP_ENDPOINT=http://aperture.us-east.internal:4317` (inherited from their workstation's `.envrc`) but want to point at a local Aperture they spawned for the debug session. They temporarily edit `main.rs` to add `.with_endpoint("http://localhost:4317")`. The explicit builder value wins; the env var is ignored. They debug; they revert the edit; the env var is honoured again.

#### 3: An application without any endpoint configuration runs against localhost

A developer trying Spark for the first time on their workstation runs:

```rust
let _guard = spark::init(spark::SparkConfig::for_service("hello-spark"))?;
```

No env var, no `with_endpoint`. Spark targets the default `http://localhost:4317`. The developer has Aperture running locally on port 4317 (per Aperture's own walking-skeleton demo); telemetry flows. The "out of the box" path works.

### UAT Scenarios (BDD)

#### Scenario: SparkConfig::with_endpoint takes precedence over OTEL_EXPORTER_OTLP_ENDPOINT

```
Given OTEL_EXPORTER_OTLP_ENDPOINT="http://env-endpoint:4317" is set in the environment
And a SparkConfig built with .with_endpoint("http://config-endpoint:4317")
When the application calls spark::init(config)
Then the OTel exporter targets http://config-endpoint:4317
And the resolved-config tracing event names http://config-endpoint:4317
```

#### Scenario: OTEL_EXPORTER_OTLP_ENDPOINT is honoured when SparkConfig::with_endpoint is not called

```
Given OTEL_EXPORTER_OTLP_ENDPOINT="http://env-endpoint:4317" is set
And a SparkConfig built without with_endpoint
When the application calls spark::init(config)
Then the OTel exporter targets http://env-endpoint:4317
```

#### Scenario: Spark defaults to http://localhost:4317 when neither config nor env var is set

```
Given no OTEL_EXPORTER_OTLP_* environment variables are set
And a SparkConfig built with for_service("hello-spark") only
When the application calls spark::init(config)
Then the OTel exporter targets http://localhost:4317
And the resolved-config tracing event names http://localhost:4317
```

#### Scenario: Resolved configuration is observable on the tracing facade

```
Given a SparkConfig with service.name="payments-api" and with_endpoint("http://aperture:4317")
When the application calls spark::init(config)
Then a tracing event with target="spark", level=INFO, message containing "spark::init succeeded" is captured
And the event's structured fields include service.name="payments-api"
And the event's structured fields include endpoint="http://aperture:4317"
And the event's structured fields include protocol="grpc"
```

### Acceptance Criteria

- [ ] Endpoint resolution precedence: `SparkConfig::with_endpoint` > `OTEL_EXPORTER_OTLP_ENDPOINT` > `http://localhost:4317`.
- [ ] Protocol resolution precedence: (DESIGN-locked builder method, if any) > `OTEL_EXPORTER_OTLP_PROTOCOL` > `grpc`.
- [ ] When `SparkConfig::with_endpoint` is set, the env var is ignored regardless of its value.
- [ ] When `SparkConfig::with_endpoint` is not set and the env var is set, the env-var value is used.
- [ ] When neither is set, the default `http://localhost:4317` is used.
- [ ] The resolved configuration is logged via `tracing::info!` at `target="spark"` with structured fields naming `service.name`, `endpoint`, `protocol`, `flush_timeout_ms`.
- [ ] No `SPARK_*` env var is read by Spark v0 (Spark does not introduce new env-var contracts; it only honours OTel-canonical names).

### Outcome KPIs

- **Who**: operators redirecting Spark-instrumented service traffic between Aperture instances.
- **Does what**: change the OTLP target via env var without rebuilding the application binary.
- **By how much**: 100% of supported `OTEL_*` env vars are honoured (target: every UAT scenario in this story passes).
- **Measured by**: Spark crate's CI scenario sweep covering each precedence path.
- **Baseline**: greenfield.

### Technical Notes

- DESIGN-wave decision (Morgan): which `OTEL_*` env vars Spark v0 explicitly tests for. The minimum is `OTEL_EXPORTER_OTLP_ENDPOINT`; `OTEL_EXPORTER_OTLP_PROTOCOL` and `OTEL_SERVICE_NAME` are nice-to-haves whose handling can ride on what `opentelemetry-otlp` supports upstream.
- Spark does not re-parse the env vars itself — it delegates to `opentelemetry-otlp`'s built-in env-var resolver. The job is to wire `SparkConfig`'s explicit values onto the upstream builder so they take precedence; nothing more.

### Dependencies

US-SP-01 (the walking skeleton with `with_endpoint`), US-SP-02 (the `InvalidEndpoint` error variant defends against env-var typos).

---

## US-SP-05 — Inject house attributes on logs and metrics, not just traces

### Elevator Pitch

- **Before**: After US-SP-01 and US-SP-03 land traces with full house attributes, only one of the three OTLP signals is proven. Logs and metrics — the other two stable signals in OTLP at the harness's pinned spec version — would emit without a guarantee that the same Resource composition reaches their wire shape.
- **After**: An application calling `opentelemetry::global::logger_provider().logger("checkout").emit(...)` and `opentelemetry::global::meter("checkout").u64_counter("orders").build().add(1, &[])` produces an `ExportLogsServiceRequest` and an `ExportMetricsServiceRequest` whose `Resource` carries all four house attributes — same shape, same names, same values as the traces case. After this story, all three OTLP stable signals work with full house-attribute inheritance.
- **Decision enabled**: The developer instrumenting a service that uses all three signals confirms Spark covers the full OTLP three-signal contract. The future Pulse / Lumen storage-engine authors (Phase 4 / Phase 3) see the metrics / logs Resource shape they will eventually consume.

### Problem

Symmetry. Spark's house-attribute injection must hold for traces, logs, and metrics with the same Resource composition; if logs or metrics emit a different Resource, the downstream stack's per-signal queries diverge and the operator's "show me everything for tenant X" workflow breaks.

### Who

- **Application developer using all three OTLP signals**: needs the same Resource on every signal.
- **Future Lumen storage-engine author** (Phase 3): consumes logs with the Resource shape established here.
- **Future Pulse storage-engine author** (Phase 4): consumes metrics with the same shape.
- **Operator running a unified-query workflow**: needs the same `tenant.id` filter to work across logs, traces, and metrics.

### Solution

`spark::init` configures the OTel SDK's `LoggerProvider` and `MeterProvider` with the same `Resource` it configures the `TracerProvider` with. This is one OTel SDK call shape, repeated three times — there is no signal-specific Resource composition.

The integration test verifies this by emitting one signal of each type via the standard OTel API and asserting all three Resource shapes match.

### Domain Examples

#### 1: A full three-signal application at `acme-observability`

`acme-observability`'s `payments-api` emits spans (per checkout transaction), logs (per error and warn), and metrics (counter for transactions completed, histogram for latency). All three reach Aperture; all three carry `service.name="payments-api"`, `tenant.id="acme-prod"`, `feature_flag.checkout-v2="on"`, `experiment.id="exp-2026-Q2-pricing"` on their Resource. The operator's unified query `tenant.id="acme-prod" AND service.name="payments-api"` returns spans, logs, and metrics in one filter.

#### 2: A logs-heavy debugging session

A developer triaging an incident at `acme-observability` adds verbose logging via `tracing` (which converts to OTel logs through `tracing-opentelemetry`). The Resource on every log record carries `service.name`, `tenant.id`, and the experiment ID. The operator's log-aggregator query filters to the affected tenant and experiment cohort cleanly.

#### 3: A metrics-only canary deployment

A canary deployment at `acme-observability` emits only metrics for its first hour (per their canary policy). Aperture's `RecordingSink` (in test) and the operator's downstream Pulse (Phase 4) see `ExportMetricsServiceRequest` payloads carrying the canary-tagged Resource. The canary's metrics partition cleanly from the baseline.

### UAT Scenarios (BDD)

#### Scenario: A logs export carries the same four house attributes on the Resource

```
Given an Aperture instance running locally with a RecordingSink
And spark::init has succeeded with the canonical configuration (service.name + tenant.id + feature_flag + experiment.id)
When the application emits one log record via opentelemetry::global::logger_provider().logger("checkout-service")
And the SparkGuard is dropped
Then the RecordingSink received an ExportLogsServiceRequest
And the request's first ResourceLogs.resource.attributes contains service.name with the configured value
And the same Resource contains tenant.id with the configured value
And the same Resource contains feature_flag.checkout-v2="on"
And the same Resource contains experiment.id with the configured value
```

#### Scenario: A metrics export carries the same four house attributes on the Resource

```
Given an Aperture instance running locally with a RecordingSink
And spark::init has succeeded with the canonical configuration
When the application increments one counter via opentelemetry::global::meter("checkout-service").u64_counter("orders.processed")
And the SparkGuard is dropped
Then the RecordingSink received an ExportMetricsServiceRequest
And the request's first ResourceMetrics.resource.attributes contains all four house attributes
```

#### Scenario: All three signals share the same Resource shape

```
Given a SparkConfig with the canonical four-attribute configuration
And an Aperture instance running locally with a RecordingSink
When the application emits one span, one log record, and one metric data point
And the SparkGuard is dropped
Then the RecordingSink has received one ExportTraceServiceRequest, one ExportLogsServiceRequest, and one ExportMetricsServiceRequest
And the four house attributes appear identically on the Resource of all three requests
And no signal carries an additional attribute the others lack
```

### Acceptance Criteria

- [ ] `spark::init` configures `LoggerProvider` and `MeterProvider` (in addition to `TracerProvider`) with the same `Resource`.
- [ ] An emitted log record reaches Aperture as an `ExportLogsServiceRequest` whose Resource carries every set house attribute.
- [ ] An emitted metric data point reaches Aperture as an `ExportMetricsServiceRequest` whose Resource carries every set house attribute.
- [ ] Across the three signal types, the Resource attribute set is identical (same names, same values).
- [ ] The single-init invariant (US-SP-02) holds across all three providers — a second `spark::init` call returns `GlobalAlreadyInitialised` regardless of which signal type the second call would have configured first.

### Outcome KPIs

- **Who**: developers using all three OTLP signal types in a Spark-instrumented service.
- **Does what**: emit logs, traces, and metrics with consistent Resource attribution.
- **By how much**: 100% of canonical-config emissions across all three signal types carry the same four house attributes (the `house_attribute_completeness` CI invariant extended to all three signals).
- **Measured by**: integration test in CI; per-signal Resource-shape assertion.
- **Baseline**: greenfield.

### Technical Notes

- DESIGN-wave decision (Morgan): the exact OTel SDK builder calls (`SdkLoggerProviderBuilder`, `SdkMeterProviderBuilder`, etc., from `opentelemetry_sdk`). The DISCUSS contract specifies that all three providers share the Resource; the implementation mechanism is DESIGN's call.
- Counter accumulation: a metric data point is emitted at flush time, not at counter-increment time. The integration test must increment a counter, drop the guard, and only then assert the `ExportMetricsServiceRequest` reached the sink. This is OTel SDK behaviour, not Spark behaviour, but the test design must respect it.

### Dependencies

US-SP-01 (the traces walking skeleton), US-SP-03 (the four-attribute Resource composition for traces).

---

## US-SP-06 — Flush pending exports synchronously on guard drop, with bounded deadline

### Elevator Pitch

- **Before**: After US-SP-05, all three signal types emit correctly while the application is running. But the OTel SDK batches exports — without an explicit shutdown call, in-flight spans / log records / metric data points in the batch processor's buffer are dropped on process exit. A short-running CLI tool (or any application that exits quickly) loses its last batch silently.
- **After**: Storing the `SparkGuard` returned from `spark::init` in `main` ensures that the guard's `Drop` impl runs at scope exit (or at panic, or at explicit `drop(guard)`). The Drop calls `force_flush` synchronously on each provider with the configured deadline (default 5 s). On clean flush: tracing INFO `spark: shutdown complete drained=N` (where `N` is the SDK-exposed drained count if available; v0 with `opentelemetry_sdk =0.27` reports `drained=unknown` because the SDK does not expose the counter). On deadline: tracing WARN `spark: flush deadline exceeded dropped=M` (same convention; v0 reports `dropped=unknown`). Either way, the application's exit is bounded by `flush_timeout_ms` and the dropped-or-drained outcome is observable.
- **Decision enabled**: The developer of a short-running tool decides Spark fits their use case — batched exports are flushed before exit, no silent data loss. The operator running the application under k8s decides the default 5 s flush timeout fits their pod-termination grace period (or tunes it).

### Problem

The most operationally load-bearing slice. A library that drops in-flight exports on every process exit is unfit for any production workload that runs to completion. The flush must be (a) synchronous from the application's seat (so `main` returning means the flush is done), (b) bounded by a deadline (so the application's exit is bounded), (c) observable on both the clean and deadline paths.

### Who

- **Application developer of a short-running CLI tool**: needs in-flight exports to be flushed before the tool exits.
- **Operator running a Spark-instrumented service under k8s**: needs the application's exit to fit within the pod's termination grace period.
- **Application developer reviewing telemetry coverage**: needs the dropped-on-deadline case to be observable so they know to investigate.

### Solution

`SparkGuard` is a value type returned from `spark::init`. Its `Drop` impl:

1. Writes `tracing::info!(target: "spark", "shutdown initiated flush_timeout_ms={}", flush_timeout_ms)`.
2. Calls `force_flush` on the configured `TracerProvider`, `LoggerProvider`, `MeterProvider` synchronously with the `flush_timeout` deadline.
3. On clean flush: writes `tracing::info!(target: "spark", "shutdown complete drained={}", N)` where `N` is the SDK-exposed drained count if available, or the literal `unknown` if the SDK does not expose it. At v0 with `opentelemetry_sdk =0.27` the SDK does not expose this counter, so `N` is `unknown`.
4. On deadline: writes `tracing::warn!(target: "spark", "flush deadline exceeded dropped={} flush_timeout_ms={}", M, flush_timeout_ms)`. Same convention; at v0 `M` is `unknown`.

The `Drop` impl does NOT panic. It does NOT call `process::exit`. It returns control to the application's normal exit path after the events are written.

### Domain Examples

#### 1: A clean flush in a short-running CLI tool

A developer writes a one-shot CLI tool at `acme-observability` that processes a batch of orders and emits one span per order. The tool's `main` returns after all orders are processed; the `_guard` drops; `force_flush` runs; all 47 spans flush within the default 5 s; `tracing` INFO `spark: shutdown complete drained=unknown` is captured by the application's subscriber (v0 with `opentelemetry_sdk =0.27` does not expose the count). The CLI exits cleanly. Aperture's stderr shows the 47 spans accepted, which is the operator's authoritative count.

#### 2: A deadline-exceeded incident at `acme-observability`

`acme-observability`'s `payments-api` is restarted during a downstream Loki incident; Aperture's `ForwardingSink` is backed up. The application receives SIGTERM; `main` is interrupted by the signal handler (which may or may not propagate via the runtime, but ultimately `Drop` runs as `main`'s scope exits). `force_flush` waits up to 5 s; spans are still in-flight when the deadline expires; `tracing` WARN `spark: flush deadline exceeded dropped=unknown flush_timeout_ms=5000` is captured (v0 with `opentelemetry_sdk =0.27` does not expose the count). The operator's log aggregator surfaces the warn line; the operator knows to investigate Loki.

#### 3: A configured short-deadline session for tests

A test author at `acme-observability` writes an integration test that wants to assert deadline-exceeded behaviour quickly. They configure `SparkConfig::for_service("test").with_flush_timeout(Duration::from_millis(500))`. The test fixture's downstream is configured to delay every accept by 10 s. The guard drop fires; the deadline expires at 500 ms; the WARN event is captured; the test passes.

### UAT Scenarios (BDD)

#### Scenario: SparkGuard drop flushes pending exports within the configured deadline

```
Given an Aperture instance running locally with a RecordingSink
And spark::init has succeeded
And the application has recorded 7 spans without flushing
When the SparkGuard is dropped
Then the RecordingSink eventually receives at least one ExportTraceServiceRequest with span_count summing to 7
And one tracing INFO event with target="spark" and message containing "shutdown complete drained=unknown" is captured (v0 contract; integer counts when the SDK eventually exposes them)
And the drop completes within the configured flush_timeout_ms
```

#### Scenario: SparkGuard drop emits a deadline-exceeded warning when downstream is slow

```
Given an Aperture instance configured to delay every accept by 10 seconds
And the SparkConfig was built with .with_flush_timeout(Duration::from_millis(500))
And the application has recorded 3 spans
When the SparkGuard is dropped
Then one tracing WARN event with target="spark" and message containing "flush deadline exceeded" is captured
And the WARN event names the dropped count
And the drop completes within ~500 ms (no indefinite block)
```

#### Scenario: drop(guard) called explicitly is equivalent to scope-exit drop

```
Given a SparkGuard returned from spark::init
When the application calls drop(guard) before main returns
Then the flush behaviour is identical to letting the guard drop at end of scope
And one tracing INFO event with target="spark" and message containing "shutdown complete" is captured
```

#### Scenario: SparkGuard drop does not panic on a downed downstream

```
Given an Aperture instance that has been forcibly killed (no listener)
And the application has recorded 3 spans
When the SparkGuard is dropped
Then the drop does NOT panic
And one tracing event with target="spark" describing the drop outcome is captured
And the drop completes within the configured flush_timeout_ms
```

### Acceptance Criteria

- [ ] `SparkGuard::Drop` calls `force_flush` synchronously on the configured `TracerProvider`, `LoggerProvider`, and `MeterProvider`.
- [ ] The flush is bounded by `flush_timeout_ms` (default 5000); no `Drop` blocks indefinitely.
- [ ] On clean flush: a single `tracing::info!(target: "spark")` event with message containing `"shutdown complete drained=N"` is emitted, where `N` is the SDK-exposed drained count if available; v0 with `opentelemetry_sdk =0.27` reports `drained=unknown` because the SDK does not expose the counter (DESIGN ADR-0014 §2).
- [ ] On deadline: a single `tracing::warn!(target: "spark")` event with message containing `"flush deadline exceeded dropped=M"` and the configured `flush_timeout_ms` is emitted, with the same `=N`/`=unknown` convention as above.
- [ ] `Drop` does not panic, does not call `process::exit`, does not return early without writing the appropriate event.
- [ ] Calling `drop(guard)` explicitly produces the same observable outcome as letting the guard drop at scope exit.
- [ ] A second drop on the same guard is a no-op (the guard's internal state is consumed on first drop; idiomatic Rust).

### Outcome KPIs

- **Who**: developers of Spark-instrumented Rust services (short-running tools, k8s pods, anything with a process exit).
- **Does what**: experience zero silent data loss on application exit; deadline-exceeded events are loud, never silent.
- **By how much**: 100% of guard drops produce exactly one observable `tracing` event (INFO on clean flush, WARN on deadline); 0% of drops are silent.
- **Measured by**: integration scenario asserts both the clean and deadline paths produce observable tracing events; a third scenario asserts the down-downstream case does not panic.
- **Baseline**: greenfield.

### Technical Notes

- DESIGN-wave decision (Morgan): the exact `force_flush` invocation pattern (per-provider sequential vs concurrent), how the `flush_timeout` is divided across providers, the drained/dropped count derivation (best-effort from the OTel SDK's internal counters; if the SDK does not expose them, "best-effort known counts" with a documented caveat is acceptable at v0).
- The DESIGN ADR documents the panic-during-Drop posture: Spark's `Drop` does NOT catch panics. If `force_flush` itself panics (an OTel SDK bug), the application's panic handler runs as it would for any other panic-during-Drop.
- The flush timeout and the deadline-exceeded WARN content are part of the `spark_log_event_vocabulary` registry entry. Renames are version-bump-able.

### Dependencies

US-SP-05 (the three-signal Resource composition is what `force_flush` flushes).

---

## Out-of-scope (forward-compat infrastructure, no story)

### Aegis-driven per-request `tenant.id` derivation

Per `wave-decisions.md`'s Q2 rationale: at v0, `tenant.id` is set once at process startup via `SparkConfig::with_tenant_id`. Per-request tenancy derivation (e.g. extracting the tenant from an incoming HTTP request's auth header) is Aegis's domain in Phase 2. Aegis will introduce a `SparkConfig::with_aegis(aegis_handle)` method (additive, non-breaking); the Phase-2 integration is what makes per-request `tenant.id` work without breaking Spark v0's contract.

Not a user story because no consumer can demonstrate value from it at v0 — its value lands at Phase 2. It is captured as a System Constraint (item 1, "Library, not service") and as a forward-compat note in `wave-decisions.md > Risks`.

### Codex-driven semantic-conventions validation

Per `wave-decisions.md > D9`: Spark v0 does the inline `MissingRequiredAttribute` lint. Full semconv validation (the resource-attribute lint that enforces OpenTelemetry Semantic Conventions on every emitted attribute, named in roadmap Phase 0 deliverables) is Codex's job in Phase 0+. Codex will plug into Spark's lint pass via a future `SparkConfig::with_codex(codex_handle)` method (additive, non-breaking).

Not a user story because Codex does not exist yet. Captured in `wave-decisions.md > Out-of-scope` and System Constraint 11.

### Auto-instrumentation

Per `wave-decisions.md > D8`: Spark v0 wraps the OTel SDK; the application calls Spark's `init` and then uses the standard OTel API for emission. Auto-instrumentation of HTTP servers, database clients, etc. is a v0.2 (or v1) feature. Not a user story because the brief explicitly defers it.

---

## Changed Assumptions

### 2026-05-06 — drained / dropped counts on shutdown / flush-deadline events

**Original assumption** (DISCUSS, Luna, 2026-05-06 a.m.) — US-SP-06 acceptance criteria stated:

> On clean flush: a single `tracing::info!(target: "spark")` event with message containing `"shutdown complete drained=N"` is emitted.
> On deadline: a single `tracing::warn!(target: "spark")` event with message containing `"flush deadline exceeded dropped=M"` and the configured `flush_timeout_ms` is emitted.

The illustrative `drained=7` and `dropped=3` literals in the journey mockup, the BDD scenarios, and the slice-06 demo command implied `N` and `M` are integer record counts.

**New assumption** (DESIGN, Morgan, 2026-05-06 p.m. via `back-propagation.md`, accepted by Bea) — the same events are emitted on the same conditions, with the same `flush_timeout_ms` field, but `N` and `M` are reported as the literal `unknown` at v0. The OpenTelemetry Rust SDK at the family-pinned version `=0.27` (DESIGN ADR-0013) does not expose drained or dropped record counts publicly: `SdkTracerProvider::force_flush_with_timeout` returns `OTelSdkResult` with no count; `BatchSpanProcessor`'s internal counters are private; the same applies to `BatchLogProcessor` and `PeriodicReader`.

**Rationale** — the v0 user value is the bounded flush plus the observable outcome event, not the integer count. Path A (this update, accept `=unknown`) preserves the contract intent (event emitted, deadline bounded, outcome observable) while acknowledging the SDK's actual API surface. Path B (Spark wraps each provider with a Spark-side counter) was rejected by DESIGN as throwaway code that duplicates state the SDK already tracks internally and that a future SDK release will likely expose. See `docs/feature/spark/design/back-propagation.md` for Morgan's full argument and `docs/product/architecture/adr-0014-spark-flush-timeout-mechanism.md` §2 for the locked event shape.

**Forward path** — when the OTel Rust SDK exposes drained / dropped counts, Spark switches the literal from `unknown` to the integer without breaking the v0 vocabulary contract: the prefix `drained=` / `dropped=` is preserved, only the value type changes. Codex Phase 0+ tracks this for the SDK upgrade window.
