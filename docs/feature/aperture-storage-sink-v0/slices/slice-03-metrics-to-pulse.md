# Slice 03: Metrics persist to pulse end to end

## Story
US-03 (metrics to pulse and survive a restart).

## Elevator Pitch
With the storage sink configured, send OTLP gauge and sum metrics over gRPC
`:4317`, query pulse, and the points are there with value, unit and attributes
intact, surviving a restart.

## Entry point and observable output
- **Entry point**: OTLP metrics over the gateway (gRPC `:4317` / HTTP `:4318`).
- **Observable output**: `event=sink_accepted sink=storage signal=metrics` on
  accept; pulse `query(tenant, metric_name, range)` returning the points with
  value and kind — before and after a restart.

## Carpaccio taste tests
- **End-to-end?** Yes: send metric -> persist -> query -> restart -> query.
- **Shippable alone?** Yes: a working metrics storage pipeline; pulse gains a
  consumer; completes the three OTLP-stable signals aperture receives.
- **Thin?** Yes: one signal; scaffold reused; gauge + sum only (bounded by pulse v0).
- **User-visible value?** Yes: operator decision "metrics pipeline is
  production-ready; the platform runs end to end".

## Scope
`ExportMetricsServiceRequest` -> `pulse::Metric` + `MetricPoint`s (name,
description, unit, kind gauge/sum, points with time/value/attributes, resource
attributes) + `MetricStore::ingest` + restart re-query. Reuses Slice 01
config/probe/tenant rule.

## Outcome KPI
KPI-3 (metrics round-trip fidelity + durability); KPI-4; KPI-5.

## Out of scope
Logs, traces, profiles. Histogram / exponential histogram / summary point types
(pulse v0 supports gauge + sum only).
