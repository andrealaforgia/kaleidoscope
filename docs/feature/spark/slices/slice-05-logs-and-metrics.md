# Slice 05 — Logs and metrics symmetry

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: US-SP-05.
> **Companion slice files**: depends on Slices 01 and 03.

## Outcome added

`spark::init` configures the OTel SDK's `LoggerProvider` and `MeterProvider` (in addition to the `TracerProvider`) with the same Resource. After this slice, all three OTLP signal types — traces, logs, metrics — emit with the four-attribute Resource intact. The `house_attribute_completeness` CI invariant extends from traces-only (slice 03) to all three signals.

## What it lights up (across the five backbone activities)

| Activity | Slice 05 coverage |
|---|---|
| Configure | Same as Slice 03 — the canonical four-attribute SparkConfig. |
| Lint | Reused. |
| Initialise SDK | Three OTel SDK providers configured with the same Resource: `TracerProvider`, `LoggerProvider`, `MeterProvider`. All three export to the same OTLP endpoint with the same exporter shape. |
| Emit telemetry | Standard OTel API for all three signal types: `tracer.in_span(...)`, `logger.emit(...)`, `meter.u64_counter(...).add(...)`. Each emission inherits the same Resource. |
| Shutdown / flush | The flush-on-Drop logic flushes all three providers (the v0 implementation calls `force_flush` on each in sequence). Bounded by `flush_timeout_ms` collectively. |

## Demo command

```bash
# Run the logs-and-metrics integration test.
cargo test -p spark --test slice_05_logs_and_metrics

# Expected: the test passes.
# Expected: the test internally:
#   1. Spawns a real Aperture with a RecordingSink.
#   2. Calls spark::init(SparkConfig::for_service("payments-api")
#         .require_tenant_id().with_tenant_id("acme-prod")
#         .with_feature_flags([("checkout-v2", "on")])
#         .with_experiment_id("exp-2026-Q2-pricing")
#         .with_endpoint(...)).
#   3. Records one span via opentelemetry::global::tracer("svc").in_span("op", |_| {}).
#   4. Emits one log record via opentelemetry::global::logger_provider().logger("svc").emit(...).
#   5. Increments one counter via opentelemetry::global::meter("svc").u64_counter("ctr").build().add(1, &[]).
#   6. Drops the SparkGuard.
#   7. Asserts the RecordingSink received one ExportTraceServiceRequest, one ExportLogsServiceRequest, and one ExportMetricsServiceRequest.
#   8. Asserts each request's Resource.attributes contains the same four entries (service.name, tenant.id, feature_flag.checkout-v2, experiment.id) with identical values.
```

## Acceptance summary

- `LoggerProvider` and `MeterProvider` are configured with the same Resource as `TracerProvider`.
- An emitted log record reaches Aperture as an `ExportLogsServiceRequest` whose Resource carries every set house attribute.
- An emitted metric data point reaches Aperture as an `ExportMetricsServiceRequest` whose Resource carries every set house attribute.
- Across the three signal types, the Resource attribute set is identical (same names, same values).
- The single-init invariant (US-SP-02) holds across all three providers — a second `spark::init` returns `GlobalAlreadyInitialised` regardless of which signal type would have been configured first.
- The clean-flush path on guard drop flushes all three providers (full deadline-exceeded behaviour lands in Slice 06).

## Complexity drivers

- Three OTel SDK providers must share a Resource. The DESIGN-wave decision is whether they share an `Arc<Resource>` or whether each is constructed with its own clone. The behavioural contract (identical attribute set) is the same either way.
- Counter accumulation is asynchronous in the OTel SDK: `add(1, &[])` does not produce a wire export immediately. The integration test must explicitly trigger a flush (via the `SparkGuard::Drop`) to make the metric emission observable.
- The `house_attribute_completeness` CI invariant grows from "every recorded ExportTraceServiceRequest carries the four attributes" (Slice 03) to "every recorded export of any signal carries the four attributes" (Slice 05). The CI invariant test is updated accordingly.

## Known unknowns

- Whether `opentelemetry-otlp` 0.27 supports a single exporter handle wired to all three providers, or whether v0 needs three separate exporter constructions (one per signal type), is a DESIGN-wave decision (Morgan). The behavioural contract (the Resource flows through to the wire) is the same either way; the wire-byte count and connection-handling shape may differ.
- Whether `tracing-opentelemetry` (the bridge between the application's `tracing` facade and OTel logs) is in Spark v0's recommended-pattern documentation, is a DESIGN-wave question. DISCUSS-locked: Spark v0 wires the OTel SDK; the application chooses how to bridge (via `opentelemetry::global::logger_provider()` directly, or via `tracing-opentelemetry`).

## Out of scope for this slice

- Bounded flush + deadline-exceeded behaviour (Slice 06).
- HTTP/protobuf transport (Spark v0 default is gRPC; per-transport per-signal coverage is post-v0).
- OTLP Profiles signal (not stable in OTel spec at the harness's pinned version per harness W4).
