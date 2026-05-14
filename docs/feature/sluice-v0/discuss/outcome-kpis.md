# Sluice v0 — outcome KPIs

Two outcome KPIs grounded in the user stories. Convention follows
the Aperture / Sieve / Codex / Prism / Beacon / Loom / Aegis pattern.

---

## KPI 1 — Enqueue + dequeue latency

**Target**: p95 ≤ 50 µs per `enqueue` and per `dequeue` call on
the in-memory adapter.

**Why**: Sluice sits on Sieve's hot path. Per-batch overhead
above microseconds adds up across an OTLP ingest stream and
becomes the bottleneck.

**How measured**: Acceptance test
`tests/slice_01_walking_skeleton.rs` runs 10 000 enqueue + 10 000
dequeue invocations on a single tenant, asserts p95 wall-clock
≤ 50 µs.

**Slice anchor**: US-SL-01.

---

## KPI 2 — Depth lookup is O(1)

**Target**: `depth(tenant)` and `total_depth()` complete in
O(1) regardless of queue size.

**Why**: depth-as-gauge is a Prometheus scrape target; calling it
hundreds of times per minute must not block the hot path.

**How measured**: Acceptance test
`tests/slice_02_observability.rs` measures the wall-clock of
`depth` at queue sizes 10, 100, 1 000, 10 000, asserts the
ratio between the smallest and largest sample stays within 3×
(i.e. no obvious linear scan).

**Slice anchor**: US-SL-02.

---

## Cross-KPI guardrails

| Guardrail | Threshold | Rationale |
|---|---|---|
| Public API stability | locked by `cargo public-api` | Sluice will be consumed by Sieve and by future storage engines; breakage propagates. |
| No telemetry-on-telemetry | 0 third-party endpoints | Per architecture doc §A.2. |
| AGPL licence-header coverage | 100% of `.rs` files | Same posture as every prior feature. |
| Mutation testing | per-feature 100% kill rate | Per ADR-0005 Gate 5. |
