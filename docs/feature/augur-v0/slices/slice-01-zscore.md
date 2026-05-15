# Slice 01 — `AnomalyObserver<f64>` + ZScoreObserver (US-AU-01)

## Goal

Ship the trait the v1 BOCPD detector will implement, plus
one z-score detector using Welford's algorithm.

## IN scope

- `AnomalyObserver<T>` trait (generic in `T`)
- `ZScoreObserver` implementing
  `AnomalyObserver<f64>`
- Welford's online mean / variance algorithm
- `Anomaly<T>` event type
- Warm-up + reset
- KPI 1

## OUT scope

- BOCPD (v1)
- Multi-variate detection (v1)
- Persistence (v1)
- Beacon integration (v1)

## Learning hypothesis

Disproves "Welford's algorithm with a configurable
threshold is sensitive enough at v0 to flag step changes
without dominating CPU". If KPI 1 fails, the linear-scan
v0 design is wrong and v1 work gets re-prioritised.

## Effort

≤1 day.
