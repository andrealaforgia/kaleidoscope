# Slice Mapping — `spark` v0

> **Wave**: DESIGN.
> **Author**: Morgan (`nw-solution-architect`).
> **Date**: 2026-05-06.
> **Companion documents**: `wave-decisions.md`, `c4-component.md`,
> `../slices/slice-01..06.md`,
> `../../product/architecture/adr-0011..0016.md`.

This file maps each of the six DISCUSS-locked slices to:

- The user story it implements.
- The ADRs that lock the technology / posture for the slice.
- The internal `spark` modules the slice exercises.
- The public-API touchpoints (the `cargo public-api`-watched surface).
- The CI invariants the slice's tests defend.
- The KPI the slice moves.

---

## Slice 01 — Walking skeleton

| Aspect | Detail |
|---|---|
| User story | US-SP-01 (initialise Spark and round-trip a span end-to-end) |
| ADRs | ADR-0011 (public API + crate layout), ADR-0013 (dep pins), ADR-0014 §clean-flush path, ADR-0016 (`SparkGuard` opaque + `#[must_use]`) |
| Modules exercised | `lib.rs` (front door), `config.rs` (`SparkConfig::for_service` + `with_tenant_id` + `with_endpoint`), `init.rs` (full happy-path orchestration), `guard.rs` (Drop on clean flush), `observability.rs` (init-succeeded + shutdown-initiated + shutdown-complete events) |
| Public-API touchpoints | `pub fn init`, `pub struct SparkConfig` + the four builder methods used (`for_service`, `require_tenant_id`, `with_tenant_id`, `with_endpoint`), `pub struct SparkGuard`, `pub enum SparkError` (Ok path; the variants are not exercised here) |
| External integration | Real Aperture spawned via `aperture::spawn(Config::for_test())`; `aperture::testing::RecordingSink`. Wire: OTLP/gRPC at `:4317`, framing per `opentelemetry-proto =0.27.0`. **Contract testing recommendation for handoff to platform-architect**: wire bytes are validated by Aperture's harness on every test run; the Slice-01 test IS the consumer-driven contract test. |
| CI invariants defended | `house_attribute_completeness` (partial — service.name + tenant.id only at this slice), `no_telemetry_on_telemetry` (no INFO event with target=`spark` reaches the `RecordingSink`) |
| KPI | KPI 1 (walking-skeleton tripwire, binary milestone) |
| Mutation-test surface (Gate 5) | `init.rs::init` (the orchestration), `guard.rs::Drop::drop` (the clean-flush path), `config.rs::for_service` |

### Implementation pointers

- The `aperture::spawn(Config::for_test())` test fixture is already in
  Aperture's public surface (`crates/aperture/src/testing.rs`).
- The `spark::init` happy path:
  1. Lint passes (both required attrs set).
  2. AtomicBool CAS succeeds (first call in this test process).
  3. Resource composed with `service.name="payments-api"` and
     `tenant.id="acme-prod"`.
  4. OTLP/gRPC exporter built targeting `aperture_handle.grpc_addr()`.
  5. Three providers constructed and set globally.
  6. INFO event "spark::init succeeded" emitted at `target="spark"`.
  7. `Ok(SparkGuard)` returned.
- The integration test file: `crates/spark/tests/slice_01_walking_skeleton.rs`
  declared as a separate `[[test]]` per ADR-0015 §2.

---

## Slice 02 — Init error paths

| Aspect | Detail |
|---|---|
| User story | US-SP-02 (refuse missing required attrs at init time) |
| ADRs | ADR-0012 (`SparkError` variants + `#[non_exhaustive]`), ADR-0015 (single-init invariant; `GlobalAlreadyInitialised` mechanism) |
| Modules exercised | `config.rs` (`for_service` constructor's empty-string check; `require_tenant_id` flag; `with_endpoint` parse helper), `init.rs` (the lint pass; the AtomicBool CAS), `error.rs` (all four variants constructed), `observability.rs` (NO event emitted on Err paths; absence is the test contract) |
| Public-API touchpoints | `SparkError::MissingRequiredAttribute { name }`, `SparkError::InvalidEndpoint { endpoint, reason }`, `SparkError::GlobalAlreadyInitialised`, `SparkError::ExporterInitFailed { reason, source }`, `SparkConfig::require_tenant_id`, `SparkConfig::with_endpoint` |
| External integration | Real Aperture (for the `GlobalAlreadyInitialised` test where the first init succeeds before the second); for the other variants, no Aperture is needed because the lint runs before any wire interaction. |
| CI invariants defended | `single_init_call` (the dedicated `invariant_single_init.rs` test binary), no-side-effects-on-Err (no INFO event "spark::init succeeded" + no `RecordingSink` capture on any Err path) |
| KPI | KPI 2 (fail-loud-at-init coverage; one test case per variant) |
| Mutation-test surface (Gate 5) | `init.rs::lint` (the validation pass), `error.rs` (the variant constructors and `Display` impl), `init.rs` AtomicBool roll-back logic |

### Implementation pointers

- Per ADR-0015 §2, `GlobalAlreadyInitialised` lives in its own
  `[[test]]` declaration (`tests/invariant_single_init.rs`) so the
  process gets a pristine OTel global state.
- The other three variants (`MissingRequiredAttribute`,
  `InvalidEndpoint`, the test-only `ExporterInitFailed`) live together
  in `tests/slice_02_init_error_paths.rs`. They do not depend on the
  global state; multiple `#[test]` functions can share the binary.
- The `#[non_exhaustive]` posture is asserted indirectly: `cargo
  semver-checks` (Gate 3) refuses any commit that removes a variant.
- The `Display` substring contract: each test asserts
  `error.to_string().contains("...")` for the variant-specific
  literal.
- The `ExporterInitFailed` variant is reached at v0 via test
  scaffolding only (a deliberately-malformed configuration that
  triggers an upstream OTel error). The variant exists in the public
  surface so a future runtime-failure scenario lands cleanly.

---

## Slice 03 — Feature flags + experiment.id

| Aspect | Detail |
|---|---|
| User story | US-SP-03 (inject all four house resource attributes) |
| ADRs | ADR-0011 (`with_feature_flags` builder signature), ADR-0013 §2 (semconv version verification — `feature_flag.*` resolution) |
| Modules exercised | `config.rs` (`with_feature_flags` and `with_experiment_id` builder methods), `init.rs` (Resource composition extended with feature_flag.{key} attributes and experiment.id), `guard.rs` (unchanged from Slice 01) |
| Public-API touchpoints | `SparkConfig::with_feature_flags<I, K, V>` (the generic builder), `SparkConfig::with_experiment_id` |
| External integration | Real Aperture + `RecordingSink`. Asserts the `Resource.attributes` carries `feature_flag.checkout-v2`, `feature_flag.dark-mode`, `experiment.id` exactly. |
| CI invariants defended | `house_attribute_completeness` (full four attributes for traces) |
| KPI | KPI 3 (house-attribute completeness on traces) |
| Mutation-test surface (Gate 5) | `init.rs` Resource composition (the loop iterating feature_flags entries; the empty-value-skip rule), `config.rs::with_feature_flags` generic instantiation |

### Implementation pointers

- The `with_feature_flags<I, K, V>` signature where
  `I: IntoIterator<Item = (K, V)>`, `K: Into<String>`, `V: Into<String>`
  is the most flexible shape (covers HashMap, BTreeMap, Vec, array
  literals).
- The empty-value-skip rule: `if value.is_empty() { skip } else { add
  attribute }`. Asserted by Slice 03's "Empty-string optional
  attributes are skipped" UAT.
- The `feature_flag.` prefix is hardcoded in `init.rs` (or
  centralised in `observability.rs::FEATURE_FLAG_PREFIX`); this is
  the single source of truth the substring assertions match.
- ADR-0013 §2 verified that `feature_flag.*` does NOT collide with
  OTel semconv 0.27 (which uses `feature_flag.key`/`feature_flag.variant`
  as span-event attributes, not resource attributes). Forward-compat
  alias mode is deferred to v0.1+ via a future
  `with_semconv_compatibility(true)` builder method.

---

## Slice 04 — Env-var precedence

| Aspect | Detail |
|---|---|
| User story | US-SP-04 (honour OTel-canonical env vars + SparkConfig precedence) |
| ADRs | ADR-0011 §"SparkConfig API shape" (`with_endpoint` builder), ADR-0013 §1 (delegation to `opentelemetry-otlp`'s upstream resolver where possible) |
| Modules exercised | `config.rs` (the resolution-chain helper: `SparkConfig::with_endpoint` > `OTEL_EXPORTER_OTLP_ENDPOINT` > default `http://localhost:4317`), `init.rs` (calls the helper at lint time and passes the resolved value to the OTLP exporter builder), `observability.rs` (the resolved-config INFO event with structured fields `service.name`, `endpoint`, `protocol`, `flush_timeout_ms`) |
| Public-API touchpoints | `SparkConfig::with_endpoint`, the resolved-config tracing event vocabulary (target=`spark`, level=INFO, message contains "spark::init succeeded", structured fields `service.name`/`endpoint`/`protocol`/`flush_timeout_ms`) |
| External integration | Real Aperture spawned on the resolved port for each case; `serial_test` ensures the env-var-mutating tests do not race. |
| CI invariants defended | The resolved-config tracing event is observable on all four cases (builder-only, env-only, builder-overrides-env, default fallback) |
| KPI | KPI 4 (env-var contract honoured) |
| Mutation-test surface (Gate 5) | `config.rs::resolve_endpoint` (the precedence chain), `observability.rs::emit_init_succeeded` (the structured-field set) |

### Implementation pointers

- The resolution chain in `config.rs::resolve_endpoint`:
  ```rust
  // illustrative; software-crafter writes the actual code
  fn resolve_endpoint(&self) -> String {
      if let Some(explicit) = &self.endpoint {
          return explicit.clone();
      }
      if let Ok(env_value) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
          if !env_value.is_empty() {
              return env_value;
          }
      }
      "http://localhost:4317".to_string()
  }
  ```
- `serial_test` (dev-dep per ADR-0011): every `#[test]` function in
  `tests/slice_04_env_var_precedence.rs` carries `#[serial]` so the
  process-global env-var mutations do not race.
- The structured-field set on the resolved-config INFO event is
  fixed: `service.name`, `endpoint`, `protocol` (always `grpc` at
  v0), `flush_timeout_ms`. Adding fields is non-breaking; renaming is
  version-bump.
- Per ADR-0013 §1, `opentelemetry-otlp 0.27` itself reads
  `OTEL_EXPORTER_OTLP_ENDPOINT` via its own resolver. Spark reads it
  too (for the resolved-config event); the duplication is
  deliberate — Spark needs the value for its own tracing event before
  it hands the value to the upstream builder.

---

## Slice 05 — Logs and metrics symmetry

| Aspect | Detail |
|---|---|
| User story | US-SP-05 (inject house attrs on logs and metrics, not just traces) |
| ADRs | ADR-0011 (the three providers all share Resource), ADR-0013 §1 (`opentelemetry_sdk` features `trace`+`logs`+`metrics`) |
| Modules exercised | `init.rs` (constructs LoggerProvider and MeterProvider in addition to TracerProvider, all three sharing the same Resource), `guard.rs` (Drop already flushes all three; this slice asserts the symmetry) |
| Public-API touchpoints | (none new; the public surface is unchanged from Slice 03) |
| External integration | Real Aperture + `RecordingSink`. Asserts one `ExportTraceServiceRequest`, one `ExportLogsServiceRequest`, one `ExportMetricsServiceRequest` each carry the same four-attribute Resource. **Contract testing recommendation for handoff to platform-architect**: the wire bytes for all three signal types are validated by Aperture's harness; the Slice-05 test IS the consumer-driven contract test for the three signal types. |
| CI invariants defended | `house_attribute_completeness` (extended from traces-only to all three signals; same invariant, broader scope) |
| KPI | KPI 5 (house-attribute completeness across all three signals) |
| Mutation-test surface (Gate 5) | `init.rs` provider construction (mutations on which provider gets which Resource clone), `guard.rs` Drop's per-provider flush iteration |

### Implementation pointers

- The three providers can share an `Arc<Resource>` (one allocation,
  three references) or each clone the Resource (three allocations,
  identical contents). DESIGN does not lock this; the behavioural
  contract (identical attribute set on the wire) is the same. The
  crafter's choice can be informed by `cargo bloat` or similar.
- Counter-accumulation timing: a metric `add(1, &[])` does NOT
  produce a wire export immediately. The integration test must
  increment a counter, drop the guard, and only THEN assert the
  `ExportMetricsServiceRequest` reached the sink. Slice 05 §"Counter
  accumulation" makes this explicit.
- The single-init invariant (ADR-0015) holds across all three
  providers — a second `spark::init` returns
  `GlobalAlreadyInitialised` regardless of which signal type would
  have been configured first. The AtomicBool fires before any
  provider construction.

---

## Slice 06 — Bounded flush deadline

| Aspect | Detail |
|---|---|
| User story | US-SP-06 (flush pending exports synchronously on guard drop, with bounded deadline) |
| ADRs | ADR-0014 (sequential flush + shared remaining-time budget; best-effort drained/dropped counts; panic-safety in Drop), ADR-0016 (Drop-only contract; idempotent second drop) |
| Modules exercised | `config.rs` (`with_flush_timeout` builder method), `guard.rs` (the full bounded-flush logic: deadline arithmetic, per-provider sequential flush, INFO/WARN event selection), `observability.rs` (`emit_shutdown_complete` and `emit_flush_deadline_exceeded` helpers) |
| Public-API touchpoints | `SparkConfig::with_flush_timeout(Duration)`, the closed shutdown vocabulary on tracing (`spark: shutdown initiated`, `spark: shutdown complete`, `spark: flush deadline exceeded`) |
| External integration | Three integration scenarios: clean (real Aperture, healthy), deadline-exceeded (real Aperture configured to delay every accept by 10s; Slice 06 Case B), down-downstream (Aperture forcibly killed; Slice 06 Case C). |
| CI invariants defended | Bounded-flush guarantee (the drop completes within `flush_timeout_ms` for all paths; asserted by wall-clock measurement with tolerance), no-panic-on-Drop |
| KPI | KPI 6 (bounded-flush guarantee; observable INFO/WARN events) |
| Mutation-test surface (Gate 5) | `guard.rs::Drop::drop` (the deadline arithmetic, the per-provider iteration, the INFO/WARN event selection, the `Option::take` idempotency), `observability.rs::emit_*` helpers (the structured-field set) |

### Implementation pointers

- Per ADR-0014 §1, the Drop logic uses `Instant::now() + flush_timeout`
  as the deadline; each provider sees `remaining =
  deadline.saturating_duration_since(Instant::now())`.
- Per ADR-0014 §2, the v0 events read `drained=unknown` and
  `dropped=unknown` because the OTel SDK at 0.27 does not expose the
  counts. The journey-spark visual mockups' `drained=N` /
  `dropped=M` are illustrative; the literal value at v0 is `unknown`.
  A future SDK release that exposes the counts can switch to the
  integer without breaking the vocabulary contract.
- Per ADR-0014 §3, Drop does NOT use `std::panic::catch_unwind`. If
  the OTel SDK panics inside `force_flush`, the panic propagates as
  panic-during-drop. The Slice 06 Case C (down-downstream) test
  asserts the OTel exporter's network-failure path does NOT panic
  (the upstream OTel SDK handles network errors as `Result::Err`,
  not panic).
- Per ADR-0014 §4, second drop is a no-op via `Option::take`. The
  doc-test for `SparkGuard` shows the canonical pattern; the unit
  test exercises the explicit `drop(guard)` followed by scope-exit.

---

## Cross-slice CI invariants

These invariants are not slice-specific; they are defended by
dedicated `[[test]]` declarations in `Cargo.toml` (per ADR-0011):

| Invariant | Test binary | Defends |
|---|---|---|
| `single_init_call` | `tests/invariant_single_init.rs` | ADR-0015 §3; second call returns `GlobalAlreadyInitialised` |
| `no_telemetry_on_telemetry` | `tests/invariant_no_telemetry_on_telemetry.rs` | D5; no `ExportTraceServiceRequest` reaches the `RecordingSink` carrying `service.name="spark"` or any Spark-internal identifier; exactly one INFO event at target=`spark` is captured |

These run on every commit affecting `crates/spark/**` per Gate 1
of ADR-0011.

---

## External-integration contract test annotation (handoff to platform-architect)

Spark's external integrations:

- **Aperture (OTLP gateway)** — Spark sends OTLP/gRPC payloads to
  Aperture's `:4317` listener. The wire contract is
  `opentelemetry-proto =0.27.0` co-resolved across both crates per
  ADR-0013 §1.
- **OpenTelemetry Rust SDK family** — Spark depends on the upstream
  `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp`,
  `opentelemetry-semantic-conventions` crates pinned at `=0.27`. The
  upstream crates are not "external services" but are external
  *dependencies* whose interface contract Spark relies on.

**Contract testing recommendation for handoff to platform-architect**
(per nw-architecture-patterns "External Integration Detection"):

> External Integrations Requiring Contract Tests:
>
> - **Aperture (OTLP gateway, OTLP/gRPC)**: Spark's integration tests
>   IS the consumer-driven contract test. The Slice 01, 03, 05 tests
>   spawn a real Aperture, send wire bytes, assert acceptance via
>   `RecordingSink`. The harness Aperture runs validates the wire
>   bytes structurally. **Tool**: existing `cargo test` infrastructure;
>   no Pact / consumer-driven contract framework required because
>   Aperture's harness IS the contract validator at the wire level.
>   The platform-architect adds the Slice tests to the CI pipeline
>   (Gate 1 of ADR-0011).
>
> - **OpenTelemetry Rust SDK family**: pinned at `=0.27` per ADR-0013
>   §1. A future upstream minor bump (`0.28`) will require:
>   1. A new ADR superseding ADR-0013 with the new pin.
>   2. Re-running the Spark integration tests against the new family
>      (Gate 1) to confirm wire compatibility with Aperture's
>      `opentelemetry-proto =0.27.0` (or the new harness pin if
>      that has been bumped too).
>   3. A `cargo deny check` (Gate 4) re-run to confirm no new
>      transitive licence concerns.
>
> The wire-level test against Aperture IS the integration contract
> test. There is no separate Pact-style consumer-driven contract.

---

## Summary table — slice -> story -> ADR -> KPI

| Slice | Story | ADRs locking | KPI moved | CI invariants |
|---|---|---|---|---|
| 01 | US-SP-01 | 0011, 0013, 0014, 0016 | KPI 1 | partial house-attribute, no-telemetry-on-telemetry |
| 02 | US-SP-02 | 0012, 0015 | KPI 2 | single-init |
| 03 | US-SP-03 | 0011, 0013 §2 | KPI 3 | full house-attribute on traces |
| 04 | US-SP-04 | 0011, 0013 §1 | KPI 4 | resolved-config tracing event observability |
| 05 | US-SP-05 | 0011, 0013 §1 | KPI 5 | full house-attribute across all three signals |
| 06 | US-SP-06 | 0014, 0016 | KPI 6 | bounded-flush, no-panic-on-Drop |
