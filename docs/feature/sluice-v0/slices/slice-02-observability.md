# Slice 02 — Depth observability (US-SL-02)

## Goal

`Queue::depth(tenant)` and `Queue::total_depth()` plus a
`MetricsRecorder` trait that captures depth changes for downstream
gauge emission.

## IN scope

- O(1) `depth(tenant)` and `total_depth()`
- `MetricsRecorder` trait with `record_enqueue` / `record_dequeue` /
  `record_ack` / `record_nack` callbacks
- `NoopRecorder` and `CapturingRecorder` (the latter for tests)
- Acceptance test for KPI 2 (O(1) at sizes 10 / 100 / 1 000 / 10 000)

## OUT scope

- OTLP-binding recorder (v1 binary wrapper)
- Per-tenant alerting (Beacon rules; operator authors)

## Learning hypothesis

Disproves "the MetricsRecorder trait can capture all the events
downstream gauges need without coupling Sluice to a specific OTLP
SDK". Low risk; the trait is closed-set.
