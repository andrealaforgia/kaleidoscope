# Outcome KPIs — `aperture` v0

> **Wave**: DISCUSS — Phase 3.
> **Author**: Luna (`nw-product-owner`).
> **Date**: 2026-05-04.
> **Companion documents**: `user-stories.md`, `prioritization.md`, `wave-decisions.md`.

---

## Feature: aperture

### Objective

Stand up an OTLP gateway in the integration plane that any OpenTelemetry SDK can target out of the box, that validates every byte sequence through the Phase-0 conformance harness, that hands accepted records to a typed sink contract Sieve will plug into in Phase 1, and that remains observable, backpressure-aware, and gracefully restartable from day one.

The KPIs below are framed around **what consumers can measure** — gRPC status ratios, HTTP success ratios, listener uptime, observed concurrency saturation events, downstream-acceptance ratios — not internal-state metrics. Aperture has no `/metrics` endpoint at v0; every KPI's data source is the operator's stderr-log aggregation or an integration-test harness, not a Prometheus scrape.

---

## North Star

**Acceptance latency for a valid OTLP/gRPC logs export ≤ 50 ms p99 under non-overload conditions, with refusal-not-drop guarantee under overload conditions.**

The single number that captures both Aperture's value (fast OTLP acceptance) and Aperture's posture (loud, observable refusal under load). Single-instance, single-process, no horizontal scaling, downstream healthy. p99, not mean — long tail is what operators triage.

---

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | OTel SDK clients targeting Aperture for the first time | round-trip a real ExportLogsServiceRequest end-to-end (Slice 01 walking skeleton works) | 100% of the documented Slice-01 demo command sequence completes without manual intervention | greenfield (zero today) | Slice-01 demo command in `slices/slice-01-walking-skeleton.md` is executable; integration test in CI asserts gRPC OK + expected stderr line | Leading (secondary) |
| 2 | OTel SDK clients using either OTLP transport | receive successful acknowledgements for valid exports across both transports | gRPC OK / HTTP 200 ratio ≥ 99% under non-overload conditions | greenfield | stderr-event ratio: `count(sink_accepted) / count(request_received)` per transport, 5-minute rolling window | Leading (primary) |
| 3 | Aperture's `/readyz` endpoint | reflects listener-bound state correctly across the three-state machine (`starting` -> `ready` -> `draining`) | 100% of CI test runs assert `/readyz` returns 200 only when both listeners are bound AND not draining; survey-based confirmation from at least 3 pilot operators 30 days post-Phase-1 launch is a secondary check | n/a | structural: the readiness-state UAT in `journey-aperture.feature`; longitudinal: pilot operator interviews | Leading (primary structural; secondary survey) |
| 4 | OTel SDK clients across all three signal types | receive successful acknowledgements for valid exports of logs, traces, and metrics | per-signal acknowledgement ratio ≥ 99% under non-overload, all three signals lit | greenfield | stderr-event ratio per signal | Leading (primary) |
| 5 | Aperture under overload conditions | refuse excess traffic deterministically with observable refusal events | refusal-rate equals exceeded-cap-rate; zero silent drops in a 1-hour load test at 2x cap | greenfield | load test in CI: `count(request_received) − count(sink_accepted) − count(reject_*) − count(concurrency_cap_hit)` must equal 0 | Leading (primary) |
| 6 | OTel SDK clients hitting the cap | receive deterministic refusal status (gRPC RESOURCE_EXHAUSTED / HTTP 503 with Retry-After) instead of dropped connections or undefined behaviour | 100% of cap-exceeded requests carry a documented refusal status; 0% silent drops | greenfield | `@property`-tagged UAT in `journey-aperture.feature` plus the load-test scenario | Leading (primary) |
| 7 | Operators running Aperture with `sink=forwarding` | see accepted records arrive at the configured downstream OTel-compatible backend | downstream-acceptance ratio ≥ 99% under healthy-downstream conditions | greenfield | integration test asserts every Aperture `sink_accepted` produces a downstream `request_received`-equivalent on the configured Collector | Leading (primary) |
| 8 | Operators driving rolling restarts of Aperture | experience zero silent drops during graceful restarts | in a 1000-restart load test (offered load below capacity, downstream healthy), zero requests are lost without an observable stderr line | greenfield | integration scenario asserts `request_received_count = sink_accepted_count + reject_count + drain_deadline_exceeded_dropped_count` over the full test | Leading (primary) |

KPI 1 is the walking-skeleton tripwire. KPIs 2 + 4 + 7 are the production-quality acceptance metrics. KPIs 5 + 6 + 8 are the **non-silent-drop** invariants — the heart of why Aperture is fit for production.

---

## Per-story → KPI mapping

| Story | Primary KPI | Note |
|---|---|---|
| US-AP-01 — Bind both listeners | KPI 1, KPI 3 | Walking skeleton + readiness signal. |
| US-AP-02 — HTTP + healthz/readyz | KPI 2, KPI 3 | Second transport coverage; readiness state machine. |
| US-AP-03 — Accept valid logs | KPI 1, KPI 2, KPI 4 | First valid round-trip. Binds the harness boundary. |
| US-AP-04 — Reject malformed | KPI 4 | Reject-path correctness; counted in the same `request_received → reject` denominator as accept. |
| US-AP-05 — Accept valid traces | KPI 4 | Per-signal acknowledgement ratio for traces. |
| US-AP-06 — Accept valid metrics | KPI 4 | Per-signal acknowledgement ratio for metrics; completes 3-of-3 OTLP coverage. |
| US-AP-07 — Concurrency cap | KPI 5, KPI 6 | The non-silent-drop invariant under overload. |
| US-AP-08 — ForwardingSink | KPI 7 | Production-usefulness: records reach the operator's existing stack. |
| US-AP-09 — Graceful shutdown | KPI 8 | The non-silent-drop invariant during restart. |

---

## Metric Hierarchy

- **North Star**: acceptance latency p99 ≤ 50 ms under non-overload + refusal-not-drop guarantee under overload.
- **Leading Indicators (primary)**: KPIs 2, 4, 5, 6, 7, 8. These are observable from any operator's stderr aggregation in real time.
- **Leading Indicators (secondary)**: KPIs 1, 3. These are walking-skeleton and adoption indicators; they convert from binary milestones to ratio metrics once Aperture has been running for a quarter or so.
- **Guardrail Metrics**: must NOT degrade.
  - Aperture's outbound network footprint = ForwardingSink-only. Verified by CI invariant `no_telemetry_on_telemetry`. **Hard guardrail.**
  - Single validation gate: exactly one `validate_*` call per accepted request per signal. Verified by CI invariant `single_validator_per_signal`. **Hard guardrail.**
  - Aperture's `/healthz` 200 uptime = 100% while process is up. (If the process is up and `/healthz` is not 200, that is a fatal invariant violation.)
  - Aperture's stderr message vocabulary = the closed event set named in `shared-artifacts-registry.md > log_event_vocabulary`. Renames are version-bump-able, additions are non-breaking.

---

## Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|---|---|---|---|---|
| 1 | Slice-01 demo command | CI integration test runs the demo end-to-end | every commit affecting `crates/aperture/**` | DEVOPS (CI infrastructure); Aperture (test fixtures) |
| 2 | operator's stderr aggregation | `count(sink_accepted) / count(request_received)` per transport, 5-min rolling | every 5 min in production | operator (Phase-1 onward) |
| 3 | structural: CI; survey: pilot operators | structural: the readiness-state UAT in `journey-aperture.feature` runs every commit and asserts the three-state behaviour; survey: structured 30-min interview with each pilot operator 30 days post-launch, with a five-question form covering "is /readyz what you probe?", "do you parse stderr?", "did you customise drain_deadline?", "did you adjust max_concurrent_requests?", "any unexpected behaviour during rolling restarts?" | structural: every commit; survey: one-off, then quarterly | DEVOPS (CI); Andrea (survey, post-launch) |
| 4 | operator's stderr aggregation | per-signal `sink_accepted / request_received` ratio | every 5 min in production | operator |
| 5 | CI load test | dedicated test scenario at 2x cap for 1 hour; counts must reconcile to zero residual | every release | DEVOPS |
| 6 | CI load test + property UAT | property-based scenario in `journey-aperture.feature` plus the 1-hour load test | every commit (property UAT); every release (load test) | DEVOPS |
| 7 | integration test | end-to-end test with a real OTel Collector downstream; counts Aperture-accept vs Collector-receive | every commit | DEVOPS |
| 8 | CI load test | 1000-restart scenario with continuous offered load; reconciliation invariant | every release | DEVOPS |

> CI time-budget for KPI 8: at Aperture's expected startup time of ~50 ms per restart, 1000 restarts plus drain time per restart fits in roughly 5–10 minutes of wall-clock CI time. DEVOPS calibrates the trigger cadence (every release vs every-merge-to-main) against the available runner time budget; the contract here is the test, not its frequency.

---

## Hypothesis

We believe that delivering an OTLP gateway with the harness as the validation gate, an `OtlpSink` trait as the Sieve boundary, deterministic refusal-on-overload, and graceful drain on restart will achieve the result that **OpenTelemetry SDK clients targeting Aperture for the first time complete the Slice-01 walking-skeleton demo without manual intervention 100% of the time, and that operators running Aperture in production observe zero silent drops over a 30-day window**.

We will know this is true when:

1. KPI 1 is binary-pass (the demo runs end-to-end in CI).
2. KPIs 5, 6, 8 — the three non-silent-drop invariants — pass their respective CI scenarios deterministically over a release cycle.
3. KPI 7 — downstream-acceptance ratio — holds at ≥ 99% in pilot deployments for 30 days.

---

## DEVOPS handoff (KPI tracking infrastructure)

The platform-architect (`@nw-platform-architect`) needs from this file:

1. **Data collection requirements**
   - Aperture has no `/metrics` endpoint at v0. KPI tracking must consume the operator's stderr aggregation (whatever they already run — Loki, Elasticsearch, journald + `journalctl`, etc.).
   - The `log_event_vocabulary` (closed set, locked in `shared-artifacts-registry.md`) IS the data schema. Each event name carries a documented set of fields.
   - For the in-CI load tests (KPIs 5, 6, 8), the data source is Aperture's own stderr captured by the CI runner; no production instrumentation needed.

2. **Dashboard / monitoring needs**
   - Real-time dashboards for KPIs 2, 4, 5, 7. These are operational metrics; operators need them on their existing log dashboards. The dashboard *queries* are simple counts/ratios over the `log_event_vocabulary`; the dashboard *infrastructure* is whatever the operator already runs.
   - Weekly reports for KPIs 3, 6, 8. These are quality-assurance metrics; they belong in a release-cadence report, not a real-time dashboard.

3. **Alerting thresholds (guardrails)**
   - `count(sink_accepted) / count(request_received)` per transport drops below 95% sustained for 5 min → page. This catches both downstream incidents and Aperture-internal regressions.
   - Any `/healthz` non-200 response → page (fatal invariant).
   - `count(concurrency_cap_hit) > 0` over a 5-min window → ticket (not a page; saturation is informational, but it should drive a horizontal-scale decision).
   - Any new outbound network connection from Aperture beyond ForwardingSink → page (CI invariant `no_telemetry_on_telemetry` should have caught this; if it reaches production, that is a CI-gate failure, not just a production incident).

4. **Baseline measurement before release**
   - All KPIs are greenfield. No baseline collection needed before launch — Aperture v0 IS the baseline. After 30 days post-Phase-1, the KPIs become longitudinal.

---

## Smell tests passed

For each KPI:

| Check | KPI 1 | KPI 2 | KPI 3 | KPI 4 | KPI 5 | KPI 6 | KPI 7 | KPI 8 |
|---|---|---|---|---|---|---|---|---|
| Measurable today? | Y | Y | Y | Y | Y | Y | Y | Y |
| Rate not total? | binary | rate | rate | rate | rate | rate | rate | rate |
| Outcome not output? | Y | Y | Y | Y | Y | Y | Y | Y |
| Has baseline? | Y (greenfield acknowledged) | Y | Y | Y | Y | Y | Y | Y |
| Team can influence? | Y | Y | Y | Y | Y | Y | Y | Y |
| Has guardrails? | guardrails listed in Metric Hierarchy section apply | | | | | | | |

KPI 1 is binary (it's the walking-skeleton tripwire); the framework allows binary milestones for the first releases of greenfield features per Maurya's Empathy-stage OMTM. The other seven are ratios.

---

## Anti-pattern audit

Cross-checked against the framework's anti-patterns:

| Anti-pattern | Verdict |
|---|---|
| Output-based KPIs ("Launch X", "Build Y") | None. Every KPI names a behaviour change, not a delivery. |
| Too many KPIs (> 5 per Objective) | 8 KPIs. At the upper bound but justified — three are non-silent-drop invariants that are independent of each other (overload, downstream-fail, restart). Compressing them would lose precision; documenting them separately makes review and CI design simpler. |
| Vague KPIs (no numeric target) | Every KPI has a numeric target or an explicit binary milestone. |
| Sandbagging (consistently scoring 1.0) | Acceptance ratios at 99% are deliberately tight; if they hit 1.0 every quarter the targets get stricter, not the work eased. |
| Backlog retrofit (KPIs match backlog 1:1) | Two stories per KPI on average. The story-to-KPI mapping is many-to-many, not bijective. |
