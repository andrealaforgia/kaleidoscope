# Augur v0 — DISCUSS wave decisions

## Key decisions

- **[D1] Hand-rolled numerical methods at v0**. No
  `numpy`, no `scikit-learn`, no `sentence-transformers`.
  v0 uses Welford's algorithm for online mean/variance and
  a simple `HashMap<event, count>` frequency baseline. The
  Phase 9 roadmap accepts the ML stack at v1; the v0
  dependency graph stays tiny (only `aegis`).

- **[D2] Generic `AnomalyObserver<T>` trait**. The
  observation type is a generic parameter (`f64` for
  numerics, `String` for categorical). v0 ships two
  concrete observers — one per `T`. v1 may add a
  multi-variate observer (`Vec<f64>`) or a structural
  observer (`Span`) behind the same generic trait.

- **[D3] Per-tenant baseline, one observer per
  (tenant, signal)**. The trait is small enough that the
  operator creates one observer per `(tenant, metric_name)`
  or `(tenant, log_signal)`. Cross-tenant baseline sharing
  is explicitly out of scope.

- **[D4] Streaming online algorithm**. `observe` updates
  state and returns the anomaly verdict in one call. No
  batch reanalysis, no rolling-window scan at v0.

- **[D5] z-score detector at slice 01**. The simplest
  detector that exercises the trait: configurable
  threshold, configurable warm-up. The KPI test injects a
  3-sigma step change and asserts it is flagged.

- **[D6] Rare-event detector at slice 02**. The simplest
  categorical detector: frequency-of-total relative to a
  rarity threshold; first-crossing emission only (no
  re-emission on every observation of a known-rare event).

- **[D7] No Beacon integration at v0**. The detectors
  return structured `Anomaly` events; the operator binary
  at v1 wires them into Beacon's incident channel.

- **[D8] No LLM summarisation at v0**. The roadmap
  explicitly defers Qwen/Mistral with guardrails to v1.
  Augur is a detection layer at v0; summarisation is a
  separate concern.

- **[D9] No persistence**. Observer state lives in
  memory. Restart loses baselines; v1 adds persistence.

- **[D10] `MetricsRecorder` seam carries forward
  verbatim**. `record_observation` + `record_anomaly`
  events.

- **[D11] AGPL-3.0-or-later**.

- **[D12] Two carpaccio slices in one implementation
  commit** per established precedent.

## Slicing

- **Slice 01 — z-score** (US-AU-01).
  `AnomalyObserver<f64>` trait + `ZScoreObserver` +
  Welford's algorithm + warm-up + reset + KPI 1.
- **Slice 02 — rare events** (US-AU-02).
  `AnomalyObserver<String>` (same trait, different `T`)
  + `RareEventObserver` + frequency baseline + KPI 2.

## Constraints established

- v1 statistical models implement the same
  `AnomalyObserver<T>` trait. The trait shape is the
  permanent contract; v0 ships hand-rolled detectors,
  v1 ships proper statistical ones.
- Augur depends on `aegis` (for `TenantId`) only. No ML
  libraries, no async runtime, no network.

## DESIGN handoff

DESIGN collapses into the implementation commit.
