# Back-propagation note 2 — DESIGN to DISCUSS for ADR-0017

- **Wave**: DESIGN (post-DISTILL escalation)
- **Author**: Bea (orchestrator), drafting on behalf of Morgan (`nw-solution-architect`) who stalled mid-write before producing this note
- **Date**: 2026-05-06
- **Recipient**: Bea (self) and Luna (`nw-product-owner`) for the
  mechanical DISCUSS edits

ADR-0017 locks Option A3: Spark adopts `opentelemetry-appender-tracing
=0.28` as the v0 logs-emission seam. Applications emit logs via
`tracing::*!` macros instead of through a non-existent
`opentelemetry::global::logger_provider()` API.

The semantic intent of US-SP-05 is unchanged. Only the emission API
literal changes.

## Mechanical DISCUSS edits

### 1. `docs/feature/spark/discuss/user-stories.md` US-SP-05

In the **Solution** section, replace any reference to
`opentelemetry::global::logger_provider().logger("svc")` with the
`tracing` macro emission path. Suggested phrasing:

> The application emits log records via the standard `tracing::*!`
> macros (`tracing::info!`, `tracing::warn!`, etc.). Spark's `init`
> wires `opentelemetry-appender-tracing` as a `tracing_subscriber`
> layer over the configured `LoggerProvider`; the application's
> `tracing` events flow through that bridge and out as OTel
> `LogRecord`s. Spark's own internal `tracing` events (target:
> "spark") are filtered out so they do not feed back into the OTel
> pipeline.

In the **UAT scenarios** for US-SP-05, replace the literal
`When the application emits one log record via opentelemetry::global::logger_provider().logger("checkout-service")`
with:

> `When the application emits one log record via tracing::info!(target: "checkout-service", ...)`

Acceptance criteria (`Then` clauses) stay identical: a `LogRecord`
arrives at Aperture's RecordingSink with the four house attributes
on its Resource.

In the **Domain Examples** section, update any code snippet that
shows `opentelemetry::global::logger_provider()` to use `tracing::info!`
instead.

Append a new entry to the file's `## Changed Assumptions` section at
the bottom:

```markdown
### 2026-05-06 — logs emission API at v0 (Path A3 from DISTILL back-propagation)

**Original assumption** (DISCUSS, Luna, 2026-05-06 a.m.) — US-SP-05
referenced `opentelemetry::global::logger_provider()` as the
application-side emission API for log records, mirroring the
symmetric three-signal API for traces and metrics.

**New assumption** (DESIGN ADR-0017, Bea on Morgan's behalf,
2026-05-06 p.m. via `back-propagation-2.md`, accepted by Andrea) —
applications emit logs via the `tracing` ecosystem
(`tracing::info!`, etc.); Spark's `init` wires
`opentelemetry-appender-tracing =0.27` as a `tracing_subscriber`
layer over the configured `LoggerProvider`. The OpenTelemetry Rust
SDK at the family-pinned `=0.27` does not expose
`opentelemetry::global::logger_provider()` or
`set_logger_provider()`. Spark's public surface stays at four items
(ADR-0011 holds).

**Rationale** — the `tracing` crate is pervasive in the Rust
ecosystem in 2026; almost any service-grade Rust application already
uses it. Adopting the appender means Spark consumers do not need to
learn a new logs-emission API; they keep using what they already
use. Public surface stays minimal. The semantic intent of US-SP-05
(logs flowing through Spark's configured `LoggerProvider` reach
Aperture with the four house attributes intact) is preserved; only
the literal emission API changes.

**Forward path** — when the OTel Rust SDK adds the global
logger-provider getter, Spark may offer that as an additional
emission path without removing the appender bridge. The bridge is
non-breaking. See ADR-0017 for the full alternatives analysis.
```

### 2. `docs/feature/spark/discuss/journey-spark.yaml`

In **step 4 command**, replace the snippet that calls
`opentelemetry::global::logger_provider()` with the `tracing` macro
form. Suggested replacement:

```yaml
command: |
  // For traces and metrics, applications use the standard OTel global
  // accessors:
  let tracer = opentelemetry::global::tracer_provider().tracer("my-component");
  // For logs, applications use the `tracing` ecosystem; Spark's init
  // configures the appender bridge over the OTel LoggerProvider, so
  // tracing events are forwarded as OTel LogRecords:
  tracing::info!(target: "my-component", order_id = "ord-42", "order processed");
```

In **step 4 gherkin** for the logs scenario, change:

```
When the application emits one log record via opentelemetry::global::logger_provider().logger("checkout-service")
```

to:

```
When the application emits one log record via tracing::info!(target: "checkout-service", ...)
```

### 3. `docs/feature/spark/discuss/journey-spark.feature`

The same Gherkin scenario(s) referencing
`opentelemetry::global::logger_provider()` — replace verbatim with
the `tracing::info!` form.

### 4. `docs/feature/spark/slices/slice-05-logs-and-metrics.md`

In **Demo command**, replace the example log-emission snippet with
the `tracing::info!` form.

In **Acceptance summary**, the assertion ("a LogRecord reaches
Aperture's RecordingSink with the four house attributes on its
Resource") is unchanged. The path the application uses to produce
the LogRecord changes: from `opentelemetry::global::logger_provider()`
to `tracing::*!`.

In **Out of scope for this slice** or a new section if cleaner,
note that the application is responsible for its own
`tracing-subscriber` setup; Spark's appender layer attaches to the
subscriber Spark's `init` configures, but the application can
compose additional layers above or below the bridge per their own
needs.

## What does NOT change

- The four house attributes (`service.name`, `tenant.id`,
  `feature_flag.*`, `experiment.id`) on the Resource.
- The three-signal symmetry contract (logs + traces + metrics carry
  identical Resource).
- Slice 05's BDD scenario function names (Scholar preserved them
  verbatim in `tests/slice_05_logs_and_metrics.rs` for exactly this
  reason — the un-ignore step is mechanical).
- Spark's public surface (four items, ADR-0011 lock holds).
- The Slice 05 KPI 5 target (100% of canonical-config emissions
  carry all four house attributes on Resource across all three
  signals).

## What DELIVER (Crafty) does after DISCUSS edits land

1. Adds `opentelemetry-appender-tracing = "=0.27"` to
   `crates/spark/Cargo.toml` `[dependencies]` (the appender's minor
   aligns with the core's; the original ADR-0017 claim of `=0.28`
   was a misreading caught by Crafty at Slice 05 DELIVER).
2. Wires `OpenTelemetryTracingBridge::new(&logger_provider)` into
   `spark::init`, attached as a `tracing_subscriber` layer with a
   filter that excludes `target: "spark"`.
3. Un-ignores the three Slice 05 log-emission tests; rewrites the
   `When` clauses to use `tracing::info!` (or equivalents); leaves
   the `Then` assertions identical.
4. Extends `tests/invariant_no_telemetry_on_telemetry.rs` to assert
   `tracing::info!(target: "spark", "marker")` does NOT produce a
   LogRecord at Aperture's RecordingSink.
5. DEVOPS updates the licence-audit table in
   `docs/feature/spark/design/technology-choices.md` to add the new
   dep row (Apache-2.0).

The slice-by-slice DELIVER plan is unchanged otherwise.
