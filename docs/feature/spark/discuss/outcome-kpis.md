# Outcome KPIs — `spark` v0

> **Wave**: DISCUSS — Phase 3.
> **Author**: Luna (`nw-product-owner`).
> **Date**: 2026-05-06.
> **Companion documents**: `user-stories.md`, `story-map.md`, `wave-decisions.md`.

---

## Feature: spark

### Objective

Stand up Kaleidoscope's first Rust SDK in the integration plane that any application can use to emit OTLP-conformant telemetry with Kaleidoscope's house resource attributes attached, that catches the most common misconfiguration class at `init` time rather than at the wire, that honours the OTel-canonical environment-variable contract so operators can redirect traffic without rebuilding the binary, and that flushes pending exports with a bounded deadline on guard drop so no application exit drops in-flight data silently.

The KPIs below are framed around **what consumers can measure** — `Result::Ok` ratios at `init`, Resource-attribute presence on emitted signals, exit-time flush behaviour — not internal-state metrics. Spark has no `/metrics` endpoint of its own at v0 (it cannot — emitting telemetry about itself would violate the no-telemetry-on-telemetry commitment). Every KPI's data source is an integration test or the operator's own application-tracing aggregation, not a Prometheus scrape.

---

## North Star

**A canonical-config Spark integration round-trips one span / log / metric end-to-end through OTel→OTLP→Aperture with all configured house attributes intact on the wire, and never emits Spark-internal diagnostics through the application's OTel pipeline.**

The single sentence that captures both Spark's value (one-call OTel SDK setup with house attributes) and Spark's posture (loud at init, silent on the OTel pipeline). Defended by KPI 1 (the round-trip) and KPI 5 (the no-telemetry-on-telemetry guardrail).

---

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | Rust developers integrating Spark into their service for the first time | round-trip a real `ExportTraceServiceRequest` end-to-end (Slice 01 walking skeleton works) | 100% of the documented Slice-01 demo command sequence completes without manual intervention | greenfield (zero today) | Slice-01 demo command in `slices/slice-01-walking-skeleton.md` is executable; integration test in CI asserts `RecordingSink` capture and the configured house attributes on the recorded `Resource` | Leading (secondary) |
| 2 | Rust developers who introduce a misconfiguration into a Spark-instrumented service | receive a precise, named diagnostic at `spark::init` time, before any telemetry is emitted | 100% of misconfigurations matching one of the closed `SparkError` variants are caught at `init` (target: every UAT scenario in US-SP-02 passes) | greenfield | Spark crate's CI unit-test sweep covering each `SparkError` variant | Leading (primary) |
| 3 | Rust developers building tenant-aware, feature-flag-aware, or experiment-aware services | emit traces whose `Resource` carries every set house attribute | 100% of canonical-config trace exports carry all four house attributes on the wire | greenfield | integration test asserts per-Resource attribute presence on every recorded `ExportTraceServiceRequest` | Leading (primary) |
| 4 | operators redirecting Spark-instrumented service traffic between Aperture instances | change the OTLP target via env var without rebuilding the application binary | 100% of supported `OTEL_*` env vars are honoured (every UAT scenario in US-SP-04 passes); `SparkConfig::with_endpoint` always overrides env when set | greenfield | Spark crate's CI scenario sweep covering each precedence path | Leading (primary) |
| 5 | Rust developers using all three OTLP signal types in a Spark-instrumented service | emit logs, traces, and metrics with consistent Resource attribution | 100% of canonical-config emissions across all three signal types carry the same four house attributes (the `house_attribute_completeness` CI invariant extended to all three signals) | greenfield | integration test in CI; per-signal Resource-shape assertion | Leading (primary) |
| 6 | developers of Spark-instrumented Rust services (short-running tools, k8s pods, anything with a process exit) | experience zero silent data loss on application exit; deadline-exceeded events are loud, never silent | 100% of guard drops produce exactly one observable `tracing` event (INFO on clean flush, WARN on deadline); 0% of drops are silent | greenfield | integration scenario asserts both the clean and deadline paths produce observable tracing events; a third scenario asserts the down-downstream case does not panic | Leading (primary) |

KPI 1 is the walking-skeleton tripwire. KPI 2 is the **fail-loud-at-init** invariant. KPIs 3 + 5 are the **house-attribute-completeness** invariants — the heart of why Spark is more than a thin OTel SDK wrapper. KPI 4 is the **OTel-ecosystem-conformance** invariant. KPI 6 is the **bounded-flush** invariant — the operational guardrail that makes Spark fit for production.

---

## Per-story → KPI mapping

| Story | Primary KPI | Note |
|---|---|---|
| US-SP-01 — Initialise Spark and round-trip a span end-to-end | KPI 1, KPI 3 (partially — traces only) | Walking skeleton. Binds the OTel SDK + OTLP exporter integration. |
| US-SP-02 — Refuse missing required attrs at init | KPI 2 | The fail-loud-at-init invariant. |
| US-SP-03 — Inject all four house resource attributes on every emitted signal | KPI 3 | Completes the four-attribute Resource composition for traces. |
| US-SP-04 — Honour the OTel-canonical env vars and SparkConfig precedence | KPI 4 | Endpoint-resolution chain. |
| US-SP-05 — Inject house attributes on logs and metrics, not just traces | KPI 5 | Symmetry across signals; completes 3-of-3 OTLP coverage. |
| US-SP-06 — Flush pending exports synchronously on guard drop, with bounded deadline | KPI 6 | The bounded-flush guardrail. |

---

## Metric Hierarchy

- **North Star**: canonical-config integration round-trips end-to-end with all configured house attributes intact, zero telemetry-on-telemetry.
- **Leading Indicators (primary)**: KPIs 2, 3, 4, 5, 6. Each defends a specific contract (lint, traces house-attrs, env-var, all-signals house-attrs, bounded-flush).
- **Leading Indicators (secondary)**: KPI 1. Walking-skeleton binary milestone; converts to a per-developer adoption ratio after Spark has been published for a quarter or so.
- **Guardrail Metrics**: must NOT degrade.
  - Spark's outbound network footprint = the OTel SDK's exporter only. Verified by CI invariant `no_telemetry_on_telemetry`. **Hard guardrail.**
  - The single-init invariant: a second `spark::init` returns `GlobalAlreadyInitialised`. Verified by CI invariant `single_init_call`. **Hard guardrail.**
  - `#![forbid(unsafe_code)]` at crate root. Verified by `cargo deny check` at workspace level. **Hard guardrail.**
  - `cargo mutants` 100% kill rate per ADR-0005 of the harness. Verified by CI invariant `mutation_kill_rate_100_percent`. **Hard guardrail.**
  - Spark's `tracing` event vocabulary = the closed set named in `shared-artifacts-registry.md > spark_log_event_vocabulary`. Renames are version-bump-able, additions are non-breaking.

---

## Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|---|---|---|---|---|
| 1 | Slice-01 demo command | CI integration test runs the demo end-to-end | every commit affecting `crates/spark/**` | DEVOPS (CI infrastructure); Spark (test fixtures) |
| 2 | Spark crate's CI unit tests | each `SparkError` variant has at least one unit test | every commit | DEVOPS |
| 3 | integration test in CI | per-Resource attribute-presence assertion on every recorded `ExportTraceServiceRequest` | every commit | DEVOPS |
| 4 | integration test in CI | scenario sweep covering builder-only, env-only, builder-overrides-env, default cases | every commit | DEVOPS |
| 5 | integration test in CI | per-signal Resource-shape assertion across traces, logs, metrics | every commit | DEVOPS |
| 6 | integration test in CI | clean-flush, deadline-exceeded, down-downstream scenarios | every commit | DEVOPS |

> All six KPIs are CI-defended at v0. There are no production-deployment KPIs because Spark v0 has no production deployments yet — Spark is a library that ships when downstream applications adopt it. After Spark v0 is published and adopted by at least one pilot service in Phase 1, the KPIs convert from binary-pass-in-CI to longitudinal ratios in pilot deployments.

---

## Hypothesis

We believe that delivering an Apache-2.0 Rust SDK that wraps the upstream OTel SDK with sensible Kaleidoscope-shaped defaults, lints required attributes at `init` time, honours the OTel env-var contract, injects house attributes on the OTel `Resource`, and flushes synchronously on guard drop will achieve the result that **Rust developers integrating Spark into their service complete the Slice-01 walking-skeleton demo without manual intervention 100% of the time, never emit telemetry under a wrong service identity, and never lose in-flight data silently on application exit**.

We will know this is true when:

1. KPI 1 is binary-pass (the demo runs end-to-end in CI).
2. KPIs 2, 3, 5, 6 — the four contract-defence invariants — pass their respective CI scenarios deterministically.
3. KPI 4 — env-var precedence — holds in every CI scenario covering the four cases (builder-only, env-only, builder-overrides-env, default).
4. The `no_telemetry_on_telemetry` CI invariant catches any future change that would have Spark emit its own diagnostics through the OTel pipeline.

---

## DEVOPS handoff (KPI tracking infrastructure)

The platform-architect (`@nw-platform-architect`) needs from this file:

1. **Data collection requirements**
   - Spark has no `/metrics` endpoint at v0 (it cannot — emitting metrics about itself would violate D5). KPI tracking at v0 is exclusively CI-based.
   - The `spark_log_event_vocabulary` (closed set, locked in `shared-artifacts-registry.md`) IS the data schema. Each `tracing` event carries a documented set of fields that Spark's unit tests assert.
   - For CI-based KPIs (all six at v0), the data source is the test runner's stdout / stderr / tracing-subscriber capture; no production instrumentation needed.

2. **Dashboard / monitoring needs**
   - None at v0. Spark is a library; it has no service-level operational dashboard. Adoption metrics (downloads, unique caller crates) become relevant only after Spark publishes to crates.io, which is post-v0.
   - Once Spark publishes to crates.io (DESIGN/DEVOPS decision), a download-count dashboard becomes available via crates.io's built-in stats. This is a v0.1+ concern.

3. **Alerting thresholds (guardrails)**
   - CI invariant `no_telemetry_on_telemetry` MUST always pass. Any failure is a P0 incident — the production behaviour cannot be "Spark emits its own diagnostics through the OTel pipeline".
   - CI invariant `single_init_call` MUST always pass. Any failure is a P1 (the global tracer provider could be re-set in production, leading to undefined behaviour).
   - CI invariant `house_attribute_completeness` MUST always pass. Any failure means downstream queries lose attribution.
   - `cargo mutants` 100% kill rate MUST always pass per ADR-0005 Gate 5.

4. **Baseline measurement before release**
   - All KPIs are greenfield. No baseline collection needed before launch — Spark v0 IS the baseline. After Spark v0 is published and adopted by at least one downstream service, the KPIs become longitudinal.

---

## Smell tests passed

For each KPI:

| Check | KPI 1 | KPI 2 | KPI 3 | KPI 4 | KPI 5 | KPI 6 |
|---|---|---|---|---|---|---|
| Measurable today? | Y | Y | Y | Y | Y | Y |
| Rate not total? | binary | rate | rate | rate | rate | rate |
| Outcome not output? | Y | Y | Y | Y | Y | Y |
| Has baseline? | Y (greenfield acknowledged) | Y | Y | Y | Y | Y |
| Team can influence? | Y | Y | Y | Y | Y | Y |
| Has guardrails? | guardrails listed in Metric Hierarchy section apply | | | | | |

KPI 1 is binary (it's the walking-skeleton tripwire); the framework allows binary milestones for the first releases of greenfield features per Maurya's Empathy-stage OMTM. The other five are ratios (100% of UAT scenarios pass; 100% of misconfigurations are caught; etc.).

---

## Anti-pattern audit

Cross-checked against the framework's anti-patterns:

| Anti-pattern | Verdict |
|---|---|
| Output-based KPIs ("Launch X", "Build Y") | None. Every KPI names a behaviour change (developer / operator can DO something measurable), not a delivery. |
| Too many KPIs (>5 per Objective) | 6 KPIs. At the upper bound but justified — the four contract-defence invariants (KPIs 2, 3, 5, 6) are independent of each other (lint, traces house-attrs, three-signal coverage, bounded flush). Compressing them would lose precision; documenting them separately makes review and CI design simpler. |
| Vague KPIs (no numeric target) | Every KPI has a numeric target (100% of X passes) or an explicit binary milestone. |
| Sandbagging (consistently scoring 1.0) | All six are deliberately at 100% because they are CI-defended invariants — anything less than 100% means a regression. The targets cannot ease. |
| Backlog retrofit (KPIs match backlog 1:1) | The story-to-KPI mapping is one-to-mostly-one for the six v0 stories, which is unusual. The justification is that this is a small SDK with a tight contract surface; each story defends a specific KPI directly. After v0.1+ stories land (Codex integration, auto-instrumentation), the mapping becomes many-to-many. |
