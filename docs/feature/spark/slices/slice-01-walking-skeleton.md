# Slice 01 — Walking skeleton

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: US-SP-01.
> **Companion slice files**: none upstream — this is the walking skeleton.

## Outcome added

A real Rust application calling `spark::init(SparkConfig::for_service("payments-api").require_tenant_id().with_tenant_id("acme-prod").with_endpoint(<aperture-test-port>))` constructs the OTel SDK with the configured Resource, sends an `ExportTraceServiceRequest` carrying one span over OTLP/gRPC to a real Aperture instance running with a `RecordingSink`, and the `RecordingSink` records the request with `service.name="payments-api"` and `tenant.id="acme-prod"` on its `Resource`. The `SparkGuard` returned from `init` drops at end of test scope, flushing pending exports synchronously within the default 5 s deadline.

## What it lights up (across the five backbone activities)

| Activity | Slice 01 coverage |
|---|---|
| Configure | `SparkConfig::for_service("payments-api").require_tenant_id().with_tenant_id("acme-prod").with_endpoint(<aperture-test-port>)`. The `with_feature_flags` and `with_experiment_id` builder methods exist but are not exercised here. |
| Lint | Happy path: both required attrs set, lint returns Ok. (Lint-failure paths arrive in Slice 02.) |
| Initialise SDK | Real `opentelemetry_sdk::Resource` with `service.name` + `tenant.id`; real `opentelemetry-otlp` gRPC exporter; real `opentelemetry::global::set_tracer_provider`. **Not a stub.** |
| Emit telemetry | One span recorded via the standard OTel API (`opentelemetry::global::tracer("ci-runner").in_span("walking-skeleton", \|_\| {})`). Real wire to the spawned Aperture instance. |
| Shutdown / flush | `SparkGuard` dropped at end of test; clean-flush path completes within default 5 s. (Deadline-exceeded path arrives in Slice 06.) |

## Demo command

```bash
# Run the walking-skeleton integration test from a clean Cargo workspace.
cargo test -p spark --test slice_01_walking_skeleton

# Expected: the test passes.
# Expected: the test internally:
#   1. Calls aperture::spawn(Config::for_test()) with a RecordingSink.
#   2. Calls spark::init(SparkConfig::for_service("payments-api").require_tenant_id().with_tenant_id("acme-prod").with_endpoint(format!("http://{}", aperture_handle.grpc_addr()))).
#   3. Records one span via opentelemetry::global::tracer("ci-runner").in_span("walking-skeleton", |_| {}).
#   4. Drops the SparkGuard.
#   5. Calls aperture_handle.shutdown().await.
#   6. Asserts the RecordingSink saw exactly one ExportTraceServiceRequest.
#   7. Asserts the request's Resource.attributes contains ("service.name", "payments-api") and ("tenant.id", "acme-prod").
#
# A second demo, runnable by hand:
# Terminal 1: run a real Aperture with the StubSink configuration that prints sink_accepted to stderr.
cargo run -p aperture -- --config crates/aperture/examples/config-stub.toml

# Terminal 2: build and run the example application.
cargo run -p spark --example send_one_span_grpc

# Expected: the example prints "exported 1 span" with no error.
# Expected: Aperture's stderr (terminal 1) shows:
#   {"event":"sink_accepted","sink":"stub","signal":"traces","span_count":1,
#    "resource.service.name":"payments-api","resource.tenant.id":"acme-prod"}
```

## Acceptance summary (full UAT in user-stories.md and journey-spark.feature)

- `spark::init` returns `Ok(SparkGuard)` for the canonical Slice-01 config.
- The OTel global tracer provider is configured with a Resource containing `service.name` and `tenant.id`.
- A span recorded via `opentelemetry::global::tracer(...).in_span(...)` reaches Aperture's listener as a valid `ExportTraceServiceRequest`.
- The `RecordingSink` records the request; the `Resource.attributes` contains both house attributes with the configured values.
- A `tracing` INFO event with `target="spark"` and message containing `"spark::init succeeded"` is captured by a subscriber the test configured.
- No `ExportTraceServiceRequest` reaches the `RecordingSink` carrying `service.name="spark"` (the `no_telemetry_on_telemetry` invariant).

## Complexity drivers

- First integration of `opentelemetry-otlp` with `tonic` against a real Aperture listener. The `opentelemetry-otlp` gRPC exporter's `tonic` Channel construction is the surface that needs handling.
- First definition of the `SparkConfig` builder, `SparkError` enum (only the no-error happy path here; full variant set lands in Slice 02), and `SparkGuard` type. DESIGN-wave (Morgan) locks the exact signatures.
- First use of the application's `tracing` facade as Spark's diagnostic channel (separate from the OTel pipeline Spark configures). The `target="spark"` convention established here gets reused by every later slice.

## Known unknowns

- The exact `opentelemetry-otlp` minor version pin (compatible with `opentelemetry-proto =0.27.0`) is a DESIGN-wave decision (Morgan).
- Whether the `SparkGuard` carries enough state to support `Drop` writing the exact drained-record count, or whether v0 settles for a "best-effort known counts" caveat in the WARN/INFO event, is a DESIGN-wave decision.
- Whether `aperture::spawn` from Aperture's public `testing` module is sufficient for Spark's integration test, or whether a slimmer test fixture is needed, is a DESIGN-wave question. DISCUSS specifies that a real Aperture is the integration target; the harness mechanism is DESIGN's call.

## Out of scope for this slice

- Init error paths: `MissingRequiredAttribute`, `InvalidEndpoint`, `GlobalAlreadyInitialised` (Slice 02).
- `feature_flag.*` and `experiment.id` Resource attributes (Slice 03).
- `OTEL_EXPORTER_OTLP_ENDPOINT` and other env-var precedence (Slice 04).
- Logs and metrics signals (Slice 05).
- Deadline-exceeded flush + down-downstream no-panic (Slice 06).
