# Journey Visual — Spark v0 (the Kaleidoscope Rust SDK)

> **Wave**: DISCUSS — Phase 2 (Journey Visualisation).
> **Author**: Luna (`nw-product-owner`).
> **Date**: 2026-05-06.
> **Companion documents**: `journey-spark.yaml`, `journey-spark.feature`, `shared-artifacts-registry.md`.

---

## What this journey actually is

Spark is **not a CLI**, and Spark is **not a service**. Spark is a Rust **library** consumed via Cargo at `crates/spark/`. It is a thin Apache-2.0 wrapper around the upstream `opentelemetry` Rust SDK + `opentelemetry-otlp` exporter, adding three things the OTel SDK alone does not give you out of the box:

1. **Kaleidoscope's house resource attributes** on every emitted signal (`service.name`, optional `tenant.id`, optional `feature_flag.*`, optional `experiment.id`).
2. **Sensible defaults** so an application can call `spark::init(SparkConfig::for_service("payments-api"))` and ship telemetry to `localhost:4317` (the OTLP/gRPC default that pairs with Aperture's listener) without further configuration.
3. **A required-attribute lint at startup** that catches the most common misconfiguration class — empty or missing identifiers — at `init` time, by returning `Err(SparkError::MissingRequiredAttribute { name })`, rather than letting the application emit semantically-broken telemetry that Aperture would later reject on the wire.

The "user" in this journey is therefore a **Rust developer** writing source code that links against the Spark crate, plus the resulting application process at runtime. The journey is the integration handshake between the developer's source code, Spark's `init` call, the OTel SDK that Spark configures, and the Aperture instance the configured exporter targets.

The four personas the journey serves:

| Persona | Touch point | Confidence question |
|---|---|---|
| **Application developer at `acme-observability`** | adds `spark = "0.1"` to `Cargo.toml`, calls `spark::init(...)` in `main`, uses standard OTel API calls in business logic | "Did `init` succeed? Are my spans reaching Aperture? Are the house attributes attached?" |
| **Operator deploying a Spark-instrumented service** | runs the application, may set `OTEL_EXPORTER_OTLP_ENDPOINT` in the environment | "Can I redirect Spark's traffic to my Aperture without rebuilding the application?" |
| **Future Aegis component** (Phase 2) | will compose with Spark to derive `tenant.id` per request | "Is the contract Spark draws (`tenant.id` on Resource, opt-in required) the contract I want to inherit?" |
| **Future Codex component** (Phase 0+) | will validate Spark's resource attributes against published semantic conventions | "Are the attribute names Spark uses the same names the OTel semconv repository publishes at the harness's pinned spec version?" |

---

## Backbone — the five activities

```
+------------+    +----------+    +----------+    +-----------+    +-----------+
| CONFIGURE  | -> |   LINT   | -> | INITIALISE-> |  EMIT     | -> | SHUTDOWN  |
| SparkConfig|    | required |    | SDK      |    | telemetry|    | / FLUSH   |
| builder    |    | attrs    |    |          |    |           |    |           |
+------------+    +----------+    +----------+    +-----------+    +-----------+
   for_service     service.name    Resource +     OTel API:        SparkGuard
   .require_       (always)        opentelemetry- tracer.in_span,  ::Drop
    tenant_id      tenant.id       otlp exporter  log, counter     synchronous
   .with_*         (opt-in)        + global set   over OTLP/gRPC   force_flush
                                                  to Aperture      with deadline
```

Five activities, left to right, each owned by a discrete concern. The walking skeleton (Slice 01) lights up exactly one path through this backbone: a single span over OTLP/gRPC to a local Aperture, with `service.name` and `tenant.id` on the Resource. Subsequent slices extend each station horizontally.

---

## Confidence arc, station by station

The arc maps each activity to the developer's question and the design lever that answers it.

| Station | Developer's question | Design lever (v0) |
|---|---|---|
| **Configure** | "Will the builder accept the values I want, including the four house attributes?" | `SparkConfig::for_service` constructor (forces `service.name` at the type level), then chained builder methods for `require_tenant_id`, `with_tenant_id`, `with_feature_flags`, `with_experiment_id`, `with_endpoint`, `with_flush_timeout`. No I/O until `init`. |
| **Lint** | "Will Spark catch my typos before they reach the wire?" | `spark::init` returns `Err(SparkError::MissingRequiredAttribute { name })` on missing or empty `service.name`, or on missing/empty `tenant.id` when `require_tenant_id()` was called. `Err(SparkError::InvalidEndpoint { ... })` on URI parse failure. `Err(SparkError::GlobalAlreadyInitialised)` on second-call. Errors are precise and named; the application chooses fatal-or-recoverable by handling the `Result`. |
| **Initialise SDK** | "Did Spark wire the OTel SDK with the house attributes I configured?" | `spark::init` constructs an `opentelemetry_sdk::Resource` containing all four house attributes, builds an `opentelemetry-otlp` exporter targeting the resolved endpoint, sets the global tracer/logger/meter providers, and returns a `SparkGuard`. Spark writes ONE `tracing` INFO event to the application's `tracing` facade describing the resolved configuration; nothing reaches the OTel pipeline as a Spark-internal diagnostic. |
| **Emit telemetry** | "Are my spans reaching Aperture with the house attributes attached?" | The application uses the standard OTel API (`opentelemetry::global::tracer(...)`, etc.) and every emitted signal inherits the Resource Spark composed at `init`. Aperture's `sink_accepted` events show `resource.service.name` and `resource.tenant.id` on every record. |
| **Shutdown / flush** | "If my application exits while exports are pending, do I lose data silently?" | `SparkGuard::Drop` calls `force_flush` synchronously with the configured deadline (default 5 s). On clean flush: tracing INFO `spark: shutdown complete drained=N`. On deadline: tracing WARN `spark: flush deadline exceeded dropped=N`. The drop is bounded by `flush_timeout_ms`; never indefinite. |

The arc starts at "will the API accept what I want?" (resolved at the type level by the `SparkConfig` builder) and ends at "will my data survive the application's exit?" (resolved by the bounded, observable `Drop` flush).

---

## Wire-level mockups

### Activity 1 — Configure (Rust source code, no runtime effects)

What the developer types in `Cargo.toml`:

```toml
[dependencies]
spark = "0.1"
opentelemetry = "0.27"
```

What the developer types in `src/main.rs`:

```rust
use spark::SparkConfig;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = SparkConfig::for_service("payments-api")
        .require_tenant_id()
        .with_tenant_id("acme-prod")
        .with_feature_flags([("checkout-v2", "on")])
        .with_experiment_id("exp-2026-Q2-pricing")
        .with_endpoint("http://localhost:4317");

    let _guard = spark::init(config)?;

    // ... application's normal OTel API usage ...

    Ok(())  // _guard drops here, flushing pending exports
}
```

No telemetry has been emitted yet. The `SparkConfig` is pure data; constructing it has no side effects.

### Activity 2 — Lint (`spark::init` validates the config)

The happy path: `Ok(SparkGuard)`. The unhappy paths:

```text
// Missing tenant.id when require_tenant_id() was called
Err(SparkError::MissingRequiredAttribute { name: "tenant.id" })

// Empty-string tenant.id (treated identically to missing)
Err(SparkError::MissingRequiredAttribute { name: "tenant.id" })

// Invalid endpoint URI
Err(SparkError::InvalidEndpoint {
    endpoint: "htp://typo:4317".into(),
    reason: "scheme \"htp\" is not http or https".into(),
})

// opentelemetry-otlp exporter constructor returned an error
Err(SparkError::ExporterInitFailed {
    reason: "tonic transport setup failed: <upstream error message>".into(),
})

// Second call to spark::init in the same process
Err(SparkError::GlobalAlreadyInitialised)
```

Every variant carries enough context for the application's error handler to make a decision (log + exit, or log + continue without telemetry, or panic — the developer chooses, Spark does not).

### Activity 3 — Initialise SDK (Resource composed, providers set, tracing event emitted)

What the OTel `Resource` looks like in memory after `spark::init`:

```text
Resource {
    attrs: {
        "service.name":               "payments-api",
        "tenant.id":                  "acme-prod",
        "feature_flag.checkout-v2":   "on",
        "experiment.id":              "exp-2026-Q2-pricing",
    }
}
```

What Spark writes to the application's `tracing` facade (NOT to the OTel pipeline — D5 no-telemetry-on-telemetry):

```text
INFO spark: spark::init succeeded
  service.name=payments-api
  tenant.id=acme-prod (required=true)
  endpoint=http://localhost:4317 (resolved from SparkConfig::with_endpoint)
  protocol=grpc
  flush_timeout_ms=5000
```

After this point, `opentelemetry::global::tracer("any-component-name")` returns a tracer wired to the OTLP exporter Spark configured.

### Activity 4 — Emit telemetry (standard OTel API, house attrs ride along)

What the application code looks like (no Spark types involved beyond `init`):

```rust
let tracer = opentelemetry::global::tracer("checkout-service");
tracer.in_span("checkout.complete", |_cx| {
    // ... business logic ...
});
```

What reaches Aperture on the wire (the same `ExportTraceServiceRequest` shape the harness `validate_traces` defends):

```text
ExportTraceServiceRequest {
    resource_spans: [
        ResourceSpans {
            resource: Resource {
                attrs: [
                    ("service.name",              "payments-api"),
                    ("tenant.id",                 "acme-prod"),
                    ("feature_flag.checkout-v2", "on"),
                    ("experiment.id",             "exp-2026-Q2-pricing"),
                ],
            },
            scope_spans: [
                ScopeSpans {
                    scope: InstrumentationScope { name: "checkout-service", ... },
                    spans: [Span { name: "checkout.complete", ... }],
                }
            ],
        }
    ]
}
```

What Aperture's `RecordingSink` (in Spark's integration tests) writes to stderr per accepted batch:

```text
{"event":"sink_accepted","sink":"recording","signal":"traces","span_count":1,
 "resource.service.name":"payments-api","resource.tenant.id":"acme-prod"}
```

`resource.service.name` AND `resource.tenant.id` both visible on the same `sink_accepted` line — this is the structural proof that Spark's house-attribute injection reached the wire.

### Activity 5 — Shutdown / flush (`SparkGuard::Drop`)

What Spark writes to `tracing` during a clean drop:

```text
INFO spark: shutdown initiated flush_timeout_ms=5000
INFO spark: shutdown complete drained=7
```

What Aperture sees during the flush — the OTel SDK pushes any pending batches:

```text
{"event":"request_received","transport":"grpc","signal":"traces","bytes":612}
{"event":"sink_accepted","sink":"recording","signal":"traces","span_count":7}
```

What Spark writes on a deadline-exceeded drop (downstream is slow):

```text
INFO spark: shutdown initiated flush_timeout_ms=500
WARN spark: flush deadline exceeded dropped=3 flush_timeout_ms=500
```

Both events are observable; the drop is bounded; the application's exit is bounded. **Never indefinite.**

---

## Walking-skeleton coherence check

The skeleton (Slice 01) covers all five activities of the backbone:

| Activity | Slice 01 coverage |
|---|---|
| Configure | `SparkConfig::for_service("payments-api").with_tenant_id("acme-prod").with_endpoint("http://127.0.0.1:<ephemeral>")`. |
| Lint | Spark's required-attribute lint pass — both `service.name` and `tenant.id` present, lint returns Ok. (Lint failure paths arrive in Slice 02.) |
| Initialise SDK | Real `opentelemetry-otlp` exporter constructed; OTel global providers set. |
| Emit telemetry | One span recorded via the standard OTel API. **Real wire**, not a stub. |
| Shutdown / flush | `SparkGuard` dropped at end of test; flush completes synchronously within the configured timeout. (Deadline-exceeded path arrives in Slice 06.) |

Activities 1–5 all light up at slice 01; only the **alternative paths** within each activity (lint failures, opt-in attribute combinations, env-var precedence, deadline-exceeded flush) are deferred to subsequent slices. This is intentional — the skeleton is the thinnest end-to-end thing that demonstrates the value proposition (a real OTLP/gRPC export with house attributes reaches a real Aperture instance), not a feature-complete first cut.

---

## Material honesty: Spark is a library, not a CLI

The skill notes warn against treating non-CLI mediums as CLI. Spark is a Rust **library**. There is no command, no subcommand, no `spark --help`, no terminal output by default. The "interface" is the public Rust API at `crates/spark/src/lib.rs`. The "feedback channel" is the `tracing` facade the application has already configured (Spark uses `tracing::info!` / `tracing::warn!` macros; if the application has not subscribed to `tracing`, these events are dropped — same as any other library).

The TUI mockups above are wire-level traces, not screen captures. The library's UX is its **API ergonomics**, evaluated against:

- **Norman's affordances**: `SparkConfig::for_service(name)` affords "this is how you set the service name"; the type-system constraint (`name: impl Into<String>`) signifies "anything that can become a String works". The compile-time error on missing argument is the constraint that prevents the most common misconfiguration class.
- **Norman's mapping**: each builder method maps to exactly one resource attribute or one configuration knob. No method does two things; no attribute is set by two different methods.
- **Norman's feedback**: every `init` call returns a `Result` immediately. No async, no callback, no future to poll. The error variants name the problem precisely.
- **Hick's Law**: `SparkConfig` has six builder methods at v0 (`require_tenant_id`, `with_tenant_id`, `with_feature_flags`, `with_experiment_id`, `with_endpoint`, `with_flush_timeout`). Below the cognitive-load threshold of 7±2.
- **Recognition over recall**: every builder method name describes its effect. The developer does not have to remember which method sets `tenant.id` — `with_tenant_id` is the obvious name.

Spark's emotional design is the **absence of surprise**: a developer reading the API for the first time should be able to predict what each method does without reading the docs. The brief surface, the OTel-canonical defaults, and the precise error variants are how Spark earns that.

---

## Cross-references

- The structured journey schema is in `journey-spark.yaml` (machine-readable; Gherkin embedded per step).
- The Gherkin scenarios extracted are in `journey-spark.feature` (running list for the DISTILL author).
- The user stories carrying the full LeanUX template (Elevator Pitch, Problem, Who, Solution, Domain Examples, UAT, AC, KPIs, Technical Notes, Dependencies) are in `user-stories.md`.
- The slice files (one per `slices/slice-NN-*.md`) carry the demo command and acceptance summary per thin slice.
