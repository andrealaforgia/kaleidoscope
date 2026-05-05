# Slice 04 — OTel-canonical env-var precedence

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: US-SP-04.
> **Companion slice files**: depends on Slice 01 (and Slice 02 for `InvalidEndpoint`).

## Outcome added

Spark resolves the OTLP endpoint via the documented precedence chain: `SparkConfig::with_endpoint` (highest) > `OTEL_EXPORTER_OTLP_ENDPOINT` env var > default `http://localhost:4317`. The resolved value is logged via `tracing::info!(target: "spark")` at `init` time so the developer sees what was chosen. Spark itself does NOT introduce `SPARK_*` env vars; it delegates env-var resolution to `opentelemetry-otlp`'s upstream resolver where possible.

## What it lights up (across the five backbone activities)

| Activity | Slice 04 coverage |
|---|---|
| Configure | `SparkConfig::with_endpoint` exercised in three positions: set (overrides env), unset (env wins if set), and the default-fallback case. |
| Lint | Reused from Slices 01 and 02 — the `InvalidEndpoint` check still runs on the resolved value. |
| Initialise SDK | The OTel exporter targets the resolved endpoint; the resolved-config tracing event names which value won. |
| Emit telemetry | Reused from Slice 01 — emission paths are unchanged; only the target endpoint may differ. |
| Shutdown / flush | Reused from Slice 01. |

## Demo command

```bash
# Run the env-var-precedence integration test.
cargo test -p spark --test slice_04_env_var_precedence

# Expected: the test passes.
# Expected: the test runs four sub-cases, each in its own scope:
#   Case A (builder overrides env):
#     OTEL_EXPORTER_OTLP_ENDPOINT="http://env-aperture:4317"
#     SparkConfig::for_service("svc").with_endpoint("http://config-aperture:4317")
#     -> exporter targets http://config-aperture:4317
#     -> resolved-config tracing event names http://config-aperture:4317
#
#   Case B (env wins when builder absent):
#     OTEL_EXPORTER_OTLP_ENDPOINT="http://env-aperture:4317"
#     SparkConfig::for_service("svc")  // no with_endpoint
#     -> exporter targets http://env-aperture:4317
#
#   Case C (default fallback):
#     no env var, no with_endpoint
#     -> exporter targets http://localhost:4317
#
#   Case D (resolved-config tracing event includes structured fields):
#     SparkConfig with explicit values
#     -> tracing event has fields service.name, endpoint, protocol, flush_timeout_ms
#
# Each sub-case spawns a real Aperture on the expected port and asserts
# the export reaches that Aperture (not a different one).
#
# A second demo, by hand: set the env var, run the example.
OTEL_EXPORTER_OTLP_ENDPOINT="http://eu-west.aperture.acme.internal:4317" \
  cargo run -p spark --example send_one_span_grpc

# Expected: the example connects to eu-west.aperture.acme.internal:4317.
# Expected: the application's stderr (subscribed to tracing) shows:
#   INFO spark: spark::init succeeded
#     service.name=hello-spark
#     endpoint=http://eu-west.aperture.acme.internal:4317 (resolved from OTEL_EXPORTER_OTLP_ENDPOINT)
#     protocol=grpc
#     flush_timeout_ms=5000
```

## Acceptance summary

- `SparkConfig::with_endpoint` always wins over `OTEL_EXPORTER_OTLP_ENDPOINT`.
- `OTEL_EXPORTER_OTLP_ENDPOINT` is honoured when `SparkConfig::with_endpoint` was not called.
- The default `http://localhost:4317` is used when neither is set.
- `tracing::info!(target: "spark", "spark::init succeeded", ...)` includes structured fields naming `service.name`, `endpoint`, `protocol`, `flush_timeout_ms`.
- The resolved-config tracing event indicates which source the endpoint was resolved from (builder vs env vs default), so the developer can debug surprises.
- No `SPARK_*` env vars are read by Spark v0.

## Complexity drivers

- Coordinating env vars in tests is fiddly: `std::env::set_var` is process-global and Cargo runs tests in parallel within a process by default. The integration test must use `serial_test` or a `Mutex`-guarded fixture so env-var sets don't race across cases.
- The resolved-config tracing event is the user-facing surface for "did Spark pick the value I expected?". Its field set is part of the v0 contract; field renames are version-bump-able.

## Known unknowns

- Whether `opentelemetry-otlp`'s upstream env-var resolver supports the OTel-canonical fallback (env var > default) cleanly when Spark has explicitly set the builder, is a DESIGN-wave question. The simplest implementation is: Spark reads the env var directly, applies the precedence chain, then passes the resolved value to `opentelemetry-otlp`. Morgan locks the mechanism.
- Which other `OTEL_*` env vars Spark v0 explicitly tests for (`OTEL_EXPORTER_OTLP_PROTOCOL`, `OTEL_SERVICE_NAME`, etc.) is a DESIGN-wave decision. DISCUSS-locked: at minimum `OTEL_EXPORTER_OTLP_ENDPOINT`.

## Out of scope for this slice

- Logs and metrics (Slice 05).
- Bounded flush (Slice 06).
- HTTP/protobuf as an alternative transport (Spark v0's transport default is gRPC per `wave-decisions.md > Q1`; the application can use `OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf` to switch, but the slice that exhaustively tests both transports is post-v0).
