# Slice 04 — Metrics signal end-to-end

> **Wave**: DISCUSS — Phase 2.5.
> **Companion stories**: US-AP-06.
> **Depends on**: Slice 03.

## Outcome added

OpenTelemetry SDKs that emit metrics complete the OTLP three-signal contract. A real `ExportMetricsServiceRequest` over either transport round-trips to gRPC OK / HTTP 200; the StubSink writes a structured stderr line naming `signal=metrics` and the data-point count.

After this slice, Aperture v0 is a complete OTLP receiver in the sense that every stable OTLP signal type is supported on every supported transport.

## What it lights up

| Activity | Slice 04 coverage |
|---|---|
| Bind listeners | (Reuse.) |
| Receive payload | gRPC `ExportMetricsServiceRequest` and `POST /v1/metrics`. |
| Validate via harness | New call site: `otlp_conformance_harness::validate_metrics(bytes, framing)`. |
| Hand off to sink | `StubSink::accept(SinkRecord::Metrics(record))`. The `SinkRecord` enum is now complete. |
| Observe self | New stderr field: `data_point_count`. |
| Shut down gracefully | (Reuse.) |

## Demo command

```bash
# Terminal 1: Aperture as before.
cargo run -p aperture -- --config examples/config-stub.toml

# Terminal 2: gRPC metrics from a real SDK
cargo run --example send_one_gauge_grpc

# Terminal 2: HTTP metrics
curl -fsS \
  -H 'Content-Type: application/x-protobuf' \
  --data-binary @examples/fixtures/metrics-minimal.bin \
  http://localhost:4318/v1/metrics

# Expected stderr: event=sink_accepted sink=stub signal=metrics data_point_count=2
```

## Acceptance summary

- Real `ExportMetricsServiceRequest` on gRPC -> gRPC OK + stderr `sink_accepted` with `signal=metrics`.
- Real `ExportMetricsServiceRequest` on HTTP -> HTTP 200 + stderr `sink_accepted` with `signal=metrics`.
- `POST /v1/metrics` with traces bytes -> HTTP 400 with `rule=WireType::SignalMismatch`, `observed=Traces`, `asserted=Metrics`.
- `validate_metrics` is invoked exactly once per metrics request.
- The `SinkRecord` enum now has exactly three variants (`Logs`, `Traces`, `Metrics`); a Cargo unit test asserts variant exhaustiveness.

## Complexity drivers

- Counting data points requires walking `ResourceMetrics` -> `ScopeMetrics` -> `Metric` -> `data` (a oneOf of point types: gauge, sum, histogram, exponential histogram, summary). Document the counting convention. Histograms count one data point per bucket-set, not per bucket.
- Metrics is the most complex of the three OTLP signals; if Slice 04 lands cleanly the harness boundary has been proven for the full signal set.

## Known unknowns

- Whether the stderr field is `data_point_count` or `metric_count` (one metric can carry many points). DISCUSS picks `data_point_count` because that is the unit the operator's downstream sees.

## Out of scope

- Concurrency cap (Slice 05).
- ForwardingSink (Slice 06).
