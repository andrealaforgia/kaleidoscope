# Story Map — Spark v0

> **Wave**: DISCUSS — Phase 2.5 (User Story Mapping + Elephant Carpaccio).
> **Author**: Luna (`nw-product-owner`).
> **Date**: 2026-05-06.
> **Companion documents**: `../slices/slice-*.md`, `journey-spark.yaml`, `user-stories.md`.

---

## User: Rust application developer instrumenting a service for Kaleidoscope (primary)

Secondary consumers: operators redirecting Spark's traffic via env vars, future Aegis (Phase 2) for `tenant.id` per-request derivation, future Loom (Phase 2) for `feature_flag.*` and `experiment.id` consumption, future Codex (Phase 0+) for semantic-conventions validation, Kaleidoscope CI.

## Goal

Provide a thin Apache-2.0 Rust crate that wraps the upstream `opentelemetry` SDK + `opentelemetry-otlp` exporter, injects Kaleidoscope's house resource attributes (`service.name`, opt-in `tenant.id`, optional `feature_flag.*`, optional `experiment.id`) on every emitted OTLP signal, lints required attributes at startup so misconfiguration surfaces as a returned `Result::Err` rather than as malformed telemetry on the wire, ships to the operator's Aperture endpoint over OTLP/gRPC by default, and flushes pending exports synchronously when the returned guard drops.

---

## Backbone

The map's five activities, left-to-right, taken from `journey-spark.yaml`. Each column is one activity in the developer's journey through Spark; each row is one slice of incremental capability across all activities.

| 1. Configure | 2. Lint | 3. Initialise SDK | 4. Emit telemetry | 5. Shutdown / flush |
|---|---|---|---|---|
| `SparkConfig::for_service` constructor + `with_tenant_id` (Slice 01) | Required-attribute lint (service.name + tenant.id) returns Ok (Slice 01 happy path) | Resource composition + OTel SDK pipeline + global tracer (Slice 01) | One span via standard OTel API, gRPC OTLP export, recorded by RecordingSink (Slice 01) | `SparkGuard::Drop` flushes synchronously (Slice 01 best-effort) |
| (reuse Slice 01) | Required-attribute lint REJECTS missing/empty/invalid values (Slice 02) | (reuse Slice 01) | (reuse Slice 01) | (reuse Slice 01) |
| `with_feature_flags` + `with_experiment_id` (Slice 03) | (reuse Slice 02) | Resource composes all four house attributes (Slice 03) | All four house attrs ride on every emitted span's Resource (Slice 03) | (reuse Slice 01) |
| `with_endpoint` + env-var resolution (Slice 04) | `InvalidEndpoint` variant (Slice 02 covers) | Endpoint-resolution chain `SparkConfig > OTEL_*env > default` (Slice 04) | Resolved endpoint reaches the actual exporter target (Slice 04) | (reuse Slice 01) |
| (reuse) | (reuse) | LoggerProvider + MeterProvider also configured with same Resource (Slice 05) | Logs and metrics carry all four house attrs (Slice 05) | Flush flushes all three providers (Slice 05) |
| `with_flush_timeout` (Slice 06) | (reuse) | (reuse) | (reuse) | Deadline-exceeded WARN event; drop never panics (Slice 06) |

Walking-skeleton row is **Slice 01**. Each subsequent row is a thin end-to-end slice that adds capability to one or two columns while keeping the rest functioning.

---

## Walking Skeleton — Slice 01

The thinnest possible end-to-end slice, with all five activities lit (some trivially):

1. **Configure** — `SparkConfig::for_service("payments-api").require_tenant_id().with_tenant_id("acme-prod").with_endpoint(<aperture-test-port>)`.
2. **Lint** — happy path; both `service.name` and `tenant.id` set, lint returns Ok. (Lint-failure paths arrive in Slice 02.)
3. **Initialise SDK** — `opentelemetry_sdk::Resource` with `service.name` + `tenant.id`, `opentelemetry-otlp` gRPC exporter targeting the configured endpoint, `opentelemetry::global::set_tracer_provider`.
4. **Emit telemetry** — one span via `opentelemetry::global::tracer("ci-runner").in_span("walking-skeleton", |_| {})`. **Real wire**, not a stub.
5. **Shutdown / flush** — `SparkGuard` dropped at end of test; synchronous `force_flush` with default 5 s deadline; clean-flush path. (Deadline-exceeded path arrives in Slice 06.)

Bea explicitly chose this thicker walking skeleton over a "spark::init returns Ok" smoke test (per `wave-decisions.md > Slice 01`). The OTel SDK + OTLP exporter integration is the load-bearing dependency for Spark's value proposition; integration risk lands at slice 01 so subsequent slices add capability without re-litigating the SDK boundary.

The acceptance proof for Slice 01 is in [`../slices/slice-01-walking-skeleton.md`](../slices/slice-01-walking-skeleton.md): `cargo test -p spark slice_01_walking_skeleton` spawns a real Aperture instance with a `RecordingSink`, points Spark at the bound port, calls `spark::init`, records one span, drops the guard, and asserts the recording sink received an `ExportTraceServiceRequest` with `service.name="payments-api"` and `tenant.id="acme-prod"` on its `Resource`.

---

## Release slices (one per file in `../slices/slice-NN-name.md`)

Each slice is sized to be demonstrable in a single session and to deliver one verifiable user-observable capability. Each is a thin end-to-end slice across the five activities; none is a single-column vertical.

| # | Slice | Outcome added | KPI moved |
|---|---|---|---|
| 01 | `slice-01-walking-skeleton.md` | One span round-trips through OTel→OTLP→Aperture with `service.name` + `tenant.id` on the Resource | KPI 1 — first integration round-trip |
| 02 | `slice-02-init-error-paths.md` | `spark::init` rejects missing/empty required attrs and invalid endpoints with precise error variants | KPI 2 — fail-loud-at-init coverage |
| 03 | `slice-03-feature-flags-and-experiment.md` | `feature_flag.*` and `experiment.id` join the Resource on traces | KPI 3 — house-attribute completeness on traces |
| 04 | `slice-04-env-var-precedence.md` | Endpoint resolution honours `OTEL_EXPORTER_OTLP_ENDPOINT` with `SparkConfig::with_endpoint` taking precedence | KPI 4 — env-var contract honoured |
| 05 | `slice-05-logs-and-metrics.md` | Logs and metrics emit with the same four-attribute Resource | KPI 5 — house-attribute completeness across all three signals |
| 06 | `slice-06-flush-deadline.md` | `SparkGuard::Drop` is bounded by `flush_timeout_ms`; deadline-exceeded is observable | KPI 6 — bounded-flush guarantee |

Six slices total. Slice 01 is the walking skeleton; Slices 02–06 each add one concrete user-observable capability.

---

## Priority Rationale

Order is **outcome impact first, dependency-graph second, riskiest-assumption-first as tie-breaker**. The rationale for the ordering is here.

1. **Slice 01 (walking skeleton)** is first because Bea chose this shape explicitly: the OTel SDK + OTLP exporter integration is the load-bearing dependency, and a thicker walking skeleton lands integration risk at slice 01 rather than late. Until Slice 01 is green, no other slice has a substrate to add to.

2. **Slice 02 (init error paths)** is second because the **non-silent-misconfiguration** invariant is the riskiest unvalidated assumption for Spark v0. A library that lets a misconfiguration through into emitted telemetry is hostile to debugging; Spark's value proposition explicitly includes the lint. Slice 02 is the cheapest possible answer: synchronous, in-process, named error variants. Putting it second means every later slice's tests can rely on the lint-rejection paths being covered.

3. **Slice 03 (feature flags + experiment.id)** is third because the **forward-compat insurance** for Aegis (Phase 2) and Loom (Phase 2) lives here. Skipping it costs nothing at v0 but breaks the API surface in Phase 2 when `feature_flag.*` and `experiment.id` are first asked for. Bea's locked Q5 decision: all three house attributes documented and supportable at v0. Doing it now is cheap; doing it later is expensive.

4. **Slice 04 (env-var precedence)** is fourth because the **OTel ecosystem-conformance** concern lives here. Spark must honour `OTEL_*` env vars to be a non-surprising OTel SDK; without slice 04, an operator deploying Spark across regions has to rebuild the binary per region, which fails the Phase-0 promise of "drop-in OTel SDK plus house attributes".

5. **Slice 05 (logs and metrics)** is fifth because it completes the **OTLP three-signal contract**. Without 05, Spark is "traces SDK plus house attributes" not "OTLP SDK plus house attributes". The logs and metrics Resource paths must be symmetric with traces or the unified-query workflow downstream breaks.

6. **Slice 06 (flush deadline)** is last because it is the **most operationally load-bearing** slice (a library that drops in-flight exports on every clean exit is unfit for any short-running tool or any k8s pod), and because it has the most subtle interactions with every other slice (the `SparkGuard` lifetime spans the application's lifetime, so all four signal types and all configuration paths participate). Putting it last lets each preceding slice be demonstrable in isolation; putting it earlier would force every subsequent slice to reason about deadline semantics before the slice's own contract was settled.

### Dependency graph (acyclic)

```
slice-01-walking-skeleton
    |
    +--> slice-02-init-error-paths
    |         |
    |         +--> slice-03-feature-flags-and-experiment
    |         |         |
    |         |         +--> slice-04-env-var-precedence
    |         |                   |
    |         |                   +--> slice-05-logs-and-metrics
    |         |                             |
    |         |                             +--> slice-06-flush-deadline
    |         |
    |         +-- error variants used by every later slice
    |
    +-- SparkConfig builder + spark::init seam used by every later slice
```

Each slice depends only on slices to its left in the graph. No slice forward-references a later one.

### Six taste tests applied (Elephant Carpaccio)

| Test | Verdict | Note |
|---|---|---|
| **End-to-end** — every slice exercises Configure → Lint → Initialise SDK → Emit telemetry → Shutdown / flush | PASS | Each slice file's "What it lights up" section names the activities it touches. No slice is single-column. Slice 02 is closest (it focuses on Lint failures) but every Lint test still goes through Configure (the SparkConfig constructor) and confirms no Initialise / Emit / Shutdown side effects occurred. |
| **Demonstrable** — each slice can be shown working in a single session | PASS | Every slice has an explicit "Demo command" in its slice file. |
| **Independently valuable** — each slice delivers a verifiable user-observable behaviour | PASS | Each slice file lists 1–2 user-observable behaviours added; none merely "supports" a future slice. |
| **Right-sized** — wall-clock days, not weeks | PASS | Each slice file declares its complexity drivers. None is large enough to justify further splitting; none is so trivial it could be merged with a neighbour without losing a clean demo. |
| **Vertical, not horizontal** — slices are user-outcome-shaped, not technical-layer-shaped | PASS | None of "config layer", "exporter layer", "tracing-layer integration" appears as a slice. Every slice is a developer-observable outcome. |
| **Riskiest assumption first (after walking skeleton)** | PASS | Slice 02 (init error paths) defends the fail-loud-at-init invariant immediately after the integration round-trip is proven. |

## Scope Assessment: PASS — 6 stories, 1 bounded context, estimated 1.5–2 weeks

- **Stories**: 6 user stories (US-SP-01 through US-SP-06). Comfortably under the >10 oversize signal.
- **Bounded contexts**: 1 (Spark itself; consumes the OTel SDK and depends on Aperture as a `[dev-dependencies]` test target only).
- **Walking skeleton integration points**: 4 (App→Spark, Spark→OTel SDK, OTel SDK→OTLP exporter→Aperture, Aperture→RecordingSink).
- **Wall-clock estimate**: 1.5–2 weeks of single-session-per-slice cadence; under the >2-week oversize signal.
- **Independent user outcomes that could ship separately**: 0; this is one cohesive Rust SDK, the smallest ship-able shape of "OTel SDK + Kaleidoscope house attributes + lint + bounded flush".

Right-sized. No split required.

---

## Story-to-slice mapping

The full story crafting lives in `user-stories.md`. Provisional mapping (revisit after Phase 4):

| Story | Slice(s) | Note |
|---|---|---|
| US-SP-01 — Initialise Spark and round-trip a span end-to-end | 01 | Walking skeleton. Logs path is reused from slice 05. |
| US-SP-02 — Refuse missing required attributes at init time | 02 | The error-paths slice; covers `MissingRequiredAttribute`, `InvalidEndpoint`, `GlobalAlreadyInitialised`. |
| US-SP-03 — Inject all four house resource attributes on every emitted signal | 03 | Adds `feature_flag.*` and `experiment.id` to the traces path; cross-signal coverage arrives in slice 05. |
| US-SP-04 — Honour the OTel-canonical env vars and SparkConfig precedence | 04 | The endpoint-resolution slice; precedence chain is documented and tested. |
| US-SP-05 — Inject house attributes on logs and metrics, not just traces | 05 | Symmetry across signals; LoggerProvider + MeterProvider configured with the same Resource. |
| US-SP-06 — Flush pending exports synchronously on guard drop, with bounded deadline | 06 | The operationally load-bearing slice; deadline-exceeded WARN event is observable. |

---

## Walking-skeleton coherence check

The skeleton (Slice 01) covers all five activities of the backbone:

| Activity | Slice 01 coverage |
|---|---|
| Configure | `SparkConfig::for_service("payments-api").require_tenant_id().with_tenant_id("acme-prod").with_endpoint(<aperture-test-port>)`. |
| Lint | Happy path: both required attrs set, lint returns Ok. (Lint-failure paths arrive in Slice 02.) |
| Initialise SDK | Resource with `service.name` + `tenant.id`; `opentelemetry-otlp` gRPC exporter; OTel global providers set. |
| Emit telemetry | One span recorded via the standard OTel API. **Real wire**, not a stub. |
| Shutdown / flush | `SparkGuard` dropped at end of test; clean-flush path. (Deadline-exceeded path arrives in Slice 06.) |

Activities 1–5 all light up at slice 01; only the **alternative paths** within each activity (lint failures, opt-in attribute combinations, env-var precedence, deadline-exceeded flush, logs/metrics) are deferred to subsequent slices. This is intentional — the skeleton is the thinnest end-to-end thing that demonstrates the value proposition (a real OTLP/gRPC export with house attributes reaches a real Aperture instance), not a feature-complete first cut.
