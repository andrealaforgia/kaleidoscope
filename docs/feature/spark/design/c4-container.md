# C4 Level 2 — Container Diagram for `spark` v0

> **Wave**: DESIGN.
> **Author**: Morgan (`nw-solution-architect`).
> **Date**: 2026-05-06.
> **Companion documents**: `c4-context.md`, `c4-component.md`,
> `wave-decisions.md`.

This view zooms inside the application-process boundary from
`c4-context.md`. The "containers" here are the deployable / linkable
units the developer and operator interact with: the application
binary, the Rust crates linked into it, the configuration channels,
and the wire transport.

For a library like Spark, "container" reads as "linkable Rust crate
in the dependency tree" — that is the natural unit at this zoom
level.

---

## Diagram

```mermaid
C4Container
  title Container Diagram — Spark v0 inside the application process

  Person(developer, "Rust developer", "Writes spark::init in main")
  Person(operator, "Operator", "Sets OTEL_* env vars at deploy time")

  System_Boundary(app_process, "Application process") {
    Container(app_main, "Application binary", "Rust", "The compiled service: payments-api, gateway-api, etc. Calls spark::init in main and uses the standard OTel API for emission.")
    Container(spark_crate, "spark crate", "Rust library, Apache-2.0", "Lints required attributes; composes the OTel Resource with house attributes; constructs the OTLP/gRPC exporter; sets the OTel global providers; returns SparkGuard with bounded synchronous Drop.")
    Container(otel_api, "opentelemetry crate", "Rust library, Apache-2.0", "The OTel API surface: TracerProvider, LoggerProvider, MeterProvider trait, global setters/getters, KeyValue. Consumed by both the application and Spark.")
    Container(otel_sdk, "opentelemetry_sdk crate", "Rust library, Apache-2.0", "Resource, batch processors, the actual provider implementations. Spark wires it; the application emits through it via the global API.")
    Container(otel_otlp, "opentelemetry-otlp crate", "Rust library, Apache-2.0", "OTLP exporter (gRPC default via tonic). Reads OTEL_EXPORTER_OTLP_ENDPOINT and OTEL_EXPORTER_OTLP_PROTOCOL upstream-side. Spark configures it with the resolved values.")
    Container(tonic_lib, "tonic", "Rust library, MIT", "gRPC transport layer: HTTP/2, protobuf framing, connection pool. Carries OTLP wire bytes to Aperture.")
    Container(tracing_lib, "tracing crate", "Rust library, MIT", "Diagnostic facade for application AND for spark itself. spark emits at target=\"spark\"; the application emits at its own targets.")
    ContainerDb(env_channel, "Process environment", "POSIX env", "OTEL_EXPORTER_OTLP_ENDPOINT, OTEL_EXPORTER_OTLP_PROTOCOL, OTEL_SERVICE_NAME, etc. Read by opentelemetry-otlp at exporter-build time.")
  }

  System_Ext(aperture, "Aperture (OTLP gateway)", "AGPL-3.0-or-later, separate process")
  System_Ext(tracing_subscriber, "Application's tracing subscriber", "tracing-subscriber, slog, or none")

  Rel(developer, app_main, "Writes Rust source that links spark", "cargo")
  Rel(operator, env_channel, "Sets OTEL_* env vars at deploy time", "Kubernetes Deployment, systemd unit, etc.")

  Rel(app_main, spark_crate, "Calls spark::init(SparkConfig) at startup", "Rust function call")
  Rel(app_main, otel_api, "Calls opentelemetry::global::tracer/logger/meter for emission", "Rust API")
  Rel(spark_crate, otel_api, "Sets the global TracerProvider, LoggerProvider, MeterProvider", "Rust API")
  Rel(spark_crate, otel_sdk, "Constructs Resource and SdkTracerProvider/SdkLoggerProvider/SdkMeterProvider", "Rust API")
  Rel(spark_crate, otel_otlp, "Builds OTLP exporter with the resolved endpoint", "Rust API")
  Rel(spark_crate, env_channel, "Reads OTEL_EXPORTER_OTLP_ENDPOINT for the resolution chain (Slice 04)", "std::env::var")
  Rel(spark_crate, tracing_lib, "Emits target=\"spark\" diagnostic events: init succeeded, shutdown initiated, shutdown complete, flush deadline exceeded", "tracing macros")

  Rel(otel_otlp, env_channel, "Resolves OTLP env vars upstream-side when Spark has not set them explicitly", "std::env::var")
  Rel(otel_sdk, otel_api, "Implements the OTel API traits", "Rust trait impl")
  Rel(otel_otlp, tonic_lib, "Sends OTLP/gRPC requests via tonic Channel", "Rust API")
  Rel(tonic_lib, aperture, "ExportTraceServiceRequest, ExportLogsServiceRequest, ExportMetricsServiceRequest", "OTLP/gRPC over HTTP/2 to :4317")

  Rel(tracing_lib, tracing_subscriber, "Forwards events to the application's configured subscriber", "tracing event")
  Rel(operator, tracing_subscriber, "Reads INFO/WARN events", "logs/stderr aggregation")
```

---

## Reading the diagram

### Spark's role at L2

Spark is one of seven library containers inside the application
process. Its job, expressed at this zoom level:

1. **Read** from `SparkConfig` (provided by `app_main`) and the
   process environment.
2. **Compose** a `Resource` (via `opentelemetry_sdk`) carrying the
   four house attributes.
3. **Construct** the OTLP exporter (via `opentelemetry-otlp`) targeting
   the resolved endpoint over `tonic`.
4. **Set** the global providers (via `opentelemetry`) so the application
   can emit through the standard API.
5. **Return** a `SparkGuard` that, on Drop, flushes the providers
   synchronously with the configured deadline (per ADR-0014).
6. **Emit** diagnostic events at `target="spark"` to the application's
   tracing subscriber (per D5).

### The wire

The wire arrow `tonic -> Aperture` is the load-bearing integration
edge. OTLP/gRPC at `opentelemetry-proto =0.27.0` co-resolved with
`opentelemetry-otlp =0.27` co-resolved with `tonic 0.12.x`.

The wire is the contract Aperture's harness validates. ADR-0013 §1
locks the family pin to keep the wire bytes decodable.

### Configuration channels (two, with precedence)

The diagram shows two configuration channels into Spark:

1. **`SparkConfig`** — the explicit application-side configuration
   (highest precedence per D6).
2. **Process environment** — the operator-side configuration (`OTEL_*`
   env vars; second precedence).

Default values (`http://localhost:4317`, gRPC, 5 s flush) are inside
Spark's own code (third precedence).

The resolution chain runs at `spark::init` time; the resolved value is
emitted to `tracing` (the resolved-config event) and then passed to
`opentelemetry-otlp`'s exporter builder.

### The dual-purpose `tracing` crate

`tracing` appears in two relationships:

1. **Spark -> tracing** — Spark's own diagnostic events
   (`spark::init succeeded`, `spark: shutdown initiated`, etc.).
2. **tracing -> tracing_subscriber** — the application's subscriber
   forwards events to whatever the application configured.

The application also emits at its own `tracing` targets (not shown
explicitly to keep the diagram readable; the `tracing` crate is
shared by both Spark and the application).

### What stays out of Spark's runtime tree

Aperture (the gateway) is **outside** the application-process boundary.
The dev-dependency edge from Spark's tests to Aperture is invisible
at this zoom level (it does not run in production). The `[dev-dependencies]`
posture (ADR-0013 §3) is what keeps Apache-2.0 Spark from contaminating
proprietary application code with Aperture's AGPL.

---

## Container licences (recap)

| Container | Licence | Notes |
|---|---|---|
| `spark` | Apache-2.0 | The crate this DESIGN delivers. |
| `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp` | Apache-2.0 | Substrate. |
| `tonic` | MIT | Transitive via `opentelemetry-otlp`. |
| `tracing` | MIT | Diagnostic facade. |
| Application binary | application's own | Embeds spark; not part of Spark v0's licensing concern. |
| Aperture | AGPL-3.0-or-later | Separate process; only consumed via dev-dep for integration tests. |

The runtime closure of Spark is Apache-2.0 / MIT — no copyleft. Per
ADR-0013 §3, `cargo deny check` enforces this on every commit.

---

## Sequence (illustrative; not part of the C4 contract)

The flow during `main`'s lifetime, expressed in sequence:

```mermaid
sequenceDiagram
    actor App as Application main()
    participant Spark as spark crate
    participant Env as process environment
    participant Sdk as opentelemetry_sdk
    participant Otlp as opentelemetry-otlp
    participant Tracing as tracing crate
    participant Tonic as tonic
    participant Aperture as Aperture process

    App->>Spark: spark::init(SparkConfig::for_service("payments-api")...)
    Spark->>Spark: lint config (sync, no I/O)
    Spark->>Env: read OTEL_EXPORTER_OTLP_ENDPOINT (if config did not set with_endpoint)
    Spark->>Sdk: build Resource with four house attrs
    Spark->>Otlp: build exporter with resolved endpoint over grpc-tonic
    Spark->>Sdk: build SdkTracerProvider, SdkLoggerProvider, SdkMeterProvider
    Spark->>Sdk: opentelemetry::global::set_*_provider
    Spark->>Tracing: emit INFO target="spark" "spark::init succeeded"
    Spark-->>App: Ok(SparkGuard)
    Note over App: ... business logic ...
    App->>Sdk: opentelemetry::global::tracer("...").in_span("op", |_| {})
    Sdk->>Otlp: queue ExportTraceServiceRequest in batch processor
    Otlp->>Tonic: send via tonic Channel
    Tonic->>Aperture: HTTP/2 OTLP request
    Note over App: ... main() returns ...
    App->>Spark: drop(SparkGuard)
    Spark->>Tracing: emit INFO "spark: shutdown initiated flush_timeout_ms=5000"
    Spark->>Sdk: tracer_provider.force_flush_with_timeout(remaining)
    Spark->>Sdk: logger_provider.force_flush_with_timeout(remaining)
    Spark->>Sdk: meter_provider.force_flush_with_timeout(remaining)
    Sdk->>Otlp: drain batches, send via tonic
    Otlp->>Tonic: send remaining requests
    Tonic->>Aperture: HTTP/2 OTLP requests
    Spark->>Tracing: emit INFO "spark: shutdown complete drained=unknown" OR WARN "flush deadline exceeded"
    Spark-->>App: drop returns; main exits
```

The sequence is illustrative; it is not the C4 Level 2 contract. The
contract is the container diagram and its labelled edges.

---

## What this diagram does not show

- The internal modules of `spark` (`config.rs`, `error.rs`,
  `guard.rs`, `init.rs`, `observability.rs`). That is L3.
- Aperture's internal containers (transport, sinks, harness). Those
  are Aperture's own L2 diagram.
- The dev-dep edge from `crates/spark/tests/` to `aperture`. The
  edge is real for the test build but invisible in the production
  container view.
