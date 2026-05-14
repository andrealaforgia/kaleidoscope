# Slice 02 — Predicate filtering (US-PU-02)

## Goal

Lift Pulse from "give me points for metric M in a time range"
to "give me points for metric M in a time range that match this
predicate", without changing the metric-name or tenant surface.

## IN scope

- `Predicate` value type with `service` + `label_eq` filters
- `MetricStore::query_with(tenant, metric_name, range,
  &Predicate)`
- Composition semantics (intersection)
- KPI 2 acceptance test

## OUT scope

- Regex / globbing label match (v1)
- PromQL operators (v1)
- Histogram bucket predicates (v1)
- Aperture retrofit (v1)

## Learning hypothesis

Disproves "a linear scan with cheap predicate matching is fast
enough at 10k points to ship as the v0 query ceiling". If KPI 2
fails on the linear scan, we either tighten the ceiling or
split out a substrate-first slice.

## Effort

≤1 day. Same shape as Lumen slice 02.
