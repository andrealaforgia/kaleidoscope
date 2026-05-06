# C4 Level 3 — Component Diagram for `spark` v0

> **Wave**: DESIGN.
> **Author**: Morgan (`nw-solution-architect`).
> **Date**: 2026-05-06.
> **Companion documents**: `c4-context.md`, `c4-container.md`,
> `wave-decisions.md`, `../../product/architecture/adr-0011-spark-public-api-and-crate-layout.md`.

This view zooms inside the `spark` crate from `c4-container.md`. The
"components" here are the internal Rust modules ADR-0011 §"Internal
layout" defines. C4 Level 3 is justified for Spark because the crate
has 5+ named internal modules (the agent's principle 9 threshold) and
because the cross-module relationships during `spark::init` and
`SparkGuard::drop` are load-bearing for crafter and acceptance-designer
implementation work.

---

## Diagram

```mermaid
C4Component
  title Component Diagram — spark crate internal modules

  Container_Boundary(spark_boundary, "spark crate") {
    Component(lib_rs, "lib.rs", "Rust module, public surface", "Re-exports SparkConfig, SparkError, SparkGuard from internal modules. Exposes pub fn init. Crate-root #![forbid(unsafe_code)].")
    Component(config_rs, "config.rs", "Rust module, pub struct SparkConfig", "The SparkConfig builder. Constructor for_service; builder methods require_tenant_id, with_tenant_id, with_feature_flags, with_experiment_id, with_endpoint, with_flush_timeout. Endpoint resolution chain helpers.")
    Component(error_rs, "error.rs", "Rust module, pub enum SparkError", "The four-variant error enum. thiserror derive in Cargo.toml; explicit Display/Error impls. From<url::ParseError> conversion in init.rs's lambda, not in this module.")
    Component(guard_rs, "guard.rs", "Rust module, pub struct SparkGuard", "Opaque RAII guard. Drop impl runs sequential force_flush across three providers with shared remaining-time budget. Emits target=\"spark\" tracing events on shutdown initiated / shutdown complete / flush deadline exceeded.")
    Component(init_rs, "init.rs", "Rust module, pub(crate) impl", "The lint pass; OTel SDK pipeline construction; global-provider set; SparkGuard return. Owns the AtomicBool single-init flag (ADR-0015) and the roll-back-on-failure transactional logic.")
    Component(observability_rs, "observability.rs", "Rust module, pub(crate) helpers", "Centralised tracing macro invocations for the target=\"spark\" event vocabulary: spark::init succeeded, spark: shutdown initiated, spark: shutdown complete, spark: flush deadline exceeded, spark: exporter initialisation failed.")
  }

  Container_Ext(otel_api, "opentelemetry crate", "Rust library")
  Container_Ext(otel_sdk, "opentelemetry_sdk crate", "Rust library")
  Container_Ext(otel_otlp, "opentelemetry-otlp crate", "Rust library")
  Container_Ext(otel_semconv, "opentelemetry-semantic-conventions crate", "Rust library")
  Container_Ext(thiserror_lib, "thiserror crate", "Rust library")
  Container_Ext(url_lib, "url crate", "Rust library")
  Container_Ext(tracing_lib, "tracing crate", "Rust library")
  Container_Ext(env, "Process environment", "POSIX env")

  Rel(lib_rs, init_rs, "Delegates pub fn init to pub(crate) init::init", "Rust call")
  Rel(lib_rs, config_rs, "Re-exports SparkConfig", "pub use")
  Rel(lib_rs, error_rs, "Re-exports SparkError", "pub use")
  Rel(lib_rs, guard_rs, "Re-exports SparkGuard", "pub use")

  Rel(init_rs, config_rs, "Reads SparkConfig fields and the resolution chain", "Rust call")
  Rel(init_rs, error_rs, "Constructs SparkError variants on failure paths", "Rust call")
  Rel(init_rs, guard_rs, "Constructs SparkGuard on Ok", "Rust call")
  Rel(init_rs, observability_rs, "Emits 'spark::init succeeded' INFO event", "Rust call")
  Rel(init_rs, env, "Reads OTEL_EXPORTER_OTLP_ENDPOINT for the resolution chain", "std::env::var")
  Rel(init_rs, otel_sdk, "Builds Resource and the three SDK providers", "Rust API")
  Rel(init_rs, otel_otlp, "Builds the OTLP exporter with the resolved endpoint", "Rust API")
  Rel(init_rs, otel_api, "Sets the three global providers via opentelemetry::global::set_*", "Rust API")
  Rel(init_rs, otel_semconv, "Uses SERVICE_NAME constant for the OTel semconv attribute key", "Rust use")

  Rel(config_rs, url_lib, "Parses with_endpoint values to detect InvalidEndpoint at init time (called from init.rs)", "Rust call")

  Rel(error_rs, thiserror_lib, "Derives Error trait on SparkError variants (reserve; v0 uses explicit impl)", "derive")

  Rel(guard_rs, otel_sdk, "Calls force_flush_with_timeout on each provider in Drop", "Rust API")
  Rel(guard_rs, observability_rs, "Emits shutdown initiated / shutdown complete / flush deadline exceeded events", "Rust call")

  Rel(observability_rs, tracing_lib, "Calls tracing::info!/warn! at target=\"spark\"", "tracing macro")
```

---

## Reading the diagram

### Module responsibilities

| Module | Responsibility |
|---|---|
| `lib.rs` | The crate's documentation-only front door. Re-exports the public surface (`SparkConfig`, `SparkError`, `SparkGuard`). Declares `pub fn init` as a one-line delegate to `init::init`. Crate-root attributes (`#![forbid(unsafe_code)]`). |
| `config.rs` | The `SparkConfig` struct + builder methods. Pure data, no I/O. The endpoint-resolution helper used by `init.rs` lives here (separated from `init.rs` because it is the same precedence chain used in tests; living in `config.rs` lets the test scaffolding exercise it without going through full init). |
| `error.rs` | The `SparkError` enum + explicit `Display`/`Error` impls. No business logic; the variants are constructed by `init.rs` and `guard.rs` at the failure sites. |
| `guard.rs` | The `SparkGuard` struct + `Drop` impl. The drop logic does the sequential per-provider flush with the shared remaining-time budget (ADR-0014). Emits the shutdown vocabulary via `observability.rs`. |
| `init.rs` | The orchestrator. Lint -> AtomicBool flag -> resource construction -> provider construction -> exporter construction -> global-set -> guard return. Owns the rollback logic for failed init (ADR-0015). |
| `observability.rs` | The closed `target="spark"` vocabulary. One file means renames are one-file edits and `cargo public-api` cannot accidentally promote the helper functions to `pub`. |

### `init.rs` is the integration hub

The diagram shows `init.rs` as the busiest node — it integrates the
config (read), the SDK and OTLP crates (build), the global API (set),
the env channel (resolution), and the observability vocabulary (emit).
This concentration is deliberate: the entire init flow is one
function's worth of code, and keeping it in one module makes the
sequence reviewable end-to-end.

### `guard.rs` is the second integration hub

`guard.rs` integrates the SDK (drop-time flush calls) and the
observability vocabulary (shutdown events). It does NOT touch the
config, the error types (after construction), or the env channel.
The Drop's sole job is "flush three providers within the budget;
emit one tracing event describing the outcome".

### `observability.rs` is the vocabulary boundary

Every `tracing::info!` and `tracing::warn!` invocation with
`target="spark"` flows through `observability.rs`. Centralising
matters for two reasons:

1. **Test substring assertions** (Slice 02: "the error message
   contains 'tenant.id'", Slice 06: "shutdown complete drained=N")
   need a single source of truth for the literals.
2. **Future renames** are one-file edits.

`observability.rs` exports `pub(crate) fn` helpers consumed by
`init.rs` and `guard.rs`. It is not on the public surface; the
`target="spark"` vocabulary is observable but the helper API is
internal.

### What this diagram does NOT show

- The drop sequence's per-provider arithmetic — that is in ADR-0014.
- The resolved-config tracing event's exact field set — that is in
  Slice 04's UAT.
- The crafter's choice between `match` and `if let` for the error
  construction — that is implementation detail.

---

## Cross-module flow during `spark::init`

The flow during a successful init, by component:

1. **`lib.rs::init`** (the one-line delegate) calls `init::init(config)`.
2. **`init.rs`** runs the lint pass:
   - Reads `config.rs::SparkConfig`'s fields.
   - On failure, returns `error.rs::SparkError::MissingRequiredAttribute`.
3. **`init.rs`** runs the endpoint-resolution chain (delegates to
   `config.rs`'s helper that returns the resolved endpoint string).
   - Reads `env` for `OTEL_EXPORTER_OTLP_ENDPOINT` if not set on
     `SparkConfig`.
   - Parses the endpoint via `url`.
   - On failure, returns `error.rs::SparkError::InvalidEndpoint`.
4. **`init.rs`** atomically sets the `SPARK_INITIALISED` flag.
   - On collision, returns `error.rs::SparkError::GlobalAlreadyInitialised`.
5. **`init.rs`** constructs the `Resource` via `otel_sdk` using
   `otel_semconv::SERVICE_NAME` as the canonical attribute key.
6. **`init.rs`** constructs the OTLP exporter via `otel_otlp` with
   `grpc-tonic` features.
7. **`init.rs`** constructs the three providers via `otel_sdk` and
   sets them globally via `otel_api::global::set_*_provider`.
8. **`init.rs`** calls `observability.rs::emit_init_succeeded`, which
   calls `tracing::info!(target: "spark", ...)`.
9. **`init.rs`** constructs `guard.rs::SparkGuard` and returns it.

Failure at any step rolls back the AtomicBool flag (if it was set)
and returns the appropriate error variant.

## Cross-module flow during `SparkGuard::drop`

1. **`guard.rs::Drop::drop`** takes ownership of `Inner`.
2. Calls `observability.rs::emit_shutdown_initiated`.
3. Computes the deadline = `Instant::now() + flush_timeout`.
4. For each provider in `[tracer, logger, meter]`:
   - Computes `remaining = deadline - now()`.
   - Calls `force_flush_with_timeout(remaining)` on the provider.
   - Accumulates outcome (clean / deadline / err).
5. Calls either `observability.rs::emit_shutdown_complete` (clean)
   or `observability.rs::emit_flush_deadline_exceeded` (any deadline
   or err).

The Drop never panics, never calls `process::exit`, and always emits
exactly one observability event after the shutdown_initiated event.

---

## Why L3 is justified for Spark

Per the agent's principle 9: "Component (L3) only for complex
subsystems". Spark has 5+ internal modules and the cross-module
relationships during init and drop are load-bearing for the crafter's
implementation work. Without L3, the relationships in
`init.rs <-> config.rs <-> error.rs <-> guard.rs <-> observability.rs`
would have to be inferred from the ADRs alone; the L3 diagram makes
them visible at a glance.

The alternative (no L3, only L1+L2) was considered. Rejected because:

- Slice 06's bounded-flush logic is the most subtle code in Spark; a
  picture of which module owns which step of the flush is more useful
  than prose describing it across several ADRs.
- The acceptance-designer (Atlas, next wave) needs to understand
  which module emits which tracing event so the integration tests can
  assert against the right targets.
- The crafter (Crafty, DELIVER wave) needs to know the module
  ownership boundaries before writing the `init.rs` orchestration
  function.
