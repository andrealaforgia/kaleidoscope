# Story Map — Aperture v0

> **Wave**: DISCUSS — Phase 2.5 (User Story Mapping + Elephant Carpaccio).
> **Author**: Luna (`nw-product-owner`).
> **Date**: 2026-05-04.
> **Companion documents**: `prioritization.md`, `../slices/slice-*.md`, `journey-aperture.yaml`.

---

## User: OpenTelemetry SDK clients (primary), with Sieve, third-party engineers and Kaleidoscope CI as secondary consumers

## Goal: stand up an OTLP gateway that accepts gRPC + HTTP/protobuf, validates via the conformance harness, hands off via a typed sink, and remains observable, backpressure-aware, and gracefully restartable

---

## Backbone

The map's six activities, left-to-right, taken from `journey-aperture.yaml`. Each column is one activity in the user's journey through Aperture; each row is one slice of incremental capability across all activities.

| 1. Bind listeners | 2. Receive payload | 3. Validate via harness | 4. Hand off to sink | 5. Observe self | 6. Shut down gracefully |
|---|---|---|---|---|---|
| Bind gRPC :4317 | Read gRPC body | `validate_logs(bytes, GrpcProtobuf)` | `StubSink::accept` (logs to stderr) | stderr `startup` + `ready` events | (deferred to Slice 09) |
| Bind HTTP :4318 (Slice 02) | Read HTTP/protobuf body (Slice 02) | `validate_logs(bytes, HttpProtobuf)` (Slice 02) | (StubSink reused) | `/healthz` (Slice 02) | |
| (reuse Slice 01) | (reuse Slice 02) | `validate_traces(...)` (Slice 03) | (StubSink reused) | `/readyz` startup-aware (Slice 02) | |
| (reuse Slice 01) | (reuse Slice 02) | `validate_metrics(...)` (Slice 04) | (StubSink reused) | `concurrency_cap_hit` events (Slice 05) | |
| (reuse Slice 01) | gRPC + HTTP concurrency cap (Slice 05) | (reuse Slice 04) | `ForwardingSink` (Slice 06) | (reuse Slice 02) | |
| TLS / SPIFFE schema (Slice 07) | (reuse) | (reuse) | (reuse) | (reuse) | (reuse Slice 02) |
| (reuse) | (reuse) | (reuse) | (reuse) | (reuse) | Graceful shutdown drain (Slice 08) |

Walking-skeleton row is **Slice 01**. Each subsequent row is a thin end-to-end slice that adds capability to one or two columns while keeping the rest functioning.

---

## Walking Skeleton — Slice 01

The thinnest possible end-to-end slice, with all six activities lit (some trivially):

1. **Bind listeners** — gRPC :4317 only (HTTP deferred to Slice 02).
2. **Receive payload** — gRPC ExportLogsServiceRequest only.
3. **Validate via harness** — `otlp_conformance_harness::validate_logs(bytes, Framing::GrpcProtobuf)`. **Real harness**, not a stub.
4. **Hand off to sink** — `StubSink::accept` (logs the record to stderr, returns Ok).
5. **Observe self** — stderr structured JSON for startup, ready, request_received, sink_accepted. (`/healthz` and `/readyz` lit fully in Slice 02 once the HTTP listener is there to serve them.)
6. **Shut down gracefully** — process responds to SIGTERM by exiting with the listener still draining-by-best-effort (full graceful drain lands in Slice 08).

Andrea explicitly chose this thicker walking skeleton over a smaller "hard-coded reject" version. The harness is the load-bearing dependency for Aperture's value proposition; integration risk lands at Slice 01 so subsequent slices add capability without re-litigating the harness boundary.

The acceptance proof for Slice 01 is in [`../slices/slice-01-walking-skeleton.md`](../slices/slice-01-walking-skeleton.md): a real OpenTelemetry Rust SDK 0.27 sends an `ExportLogsServiceRequest` over gRPC to `localhost:4317`; Aperture binds the listener, calls `validate_logs`, hands the typed record to `StubSink`, the sink writes one structured stderr line naming `service.name="payments-api"` and `record_count=3`, and the SDK receives gRPC OK.

---

## Release slices (one per file in `../slices/slice-NN-name.md`)

Each slice is sized to be demonstrable in a single session and to deliver one verifiable user-observable capability. Each is a thin end-to-end slice across the six activities; none is a single-column vertical.

| # | Slice | Outcome added | KPI moved |
|---|---|---|---|
| 01 | `slice-01-walking-skeleton.md` | OTel SDK round-trips a logs export over gRPC; first sink-accept event reaches stderr | KPI 1 — first integration round-trip |
| 02 | `slice-02-http-protobuf-and-readiness.md` | OTel SDK can also use HTTP/protobuf; `/healthz` + `/readyz` answer operator probes | KPI 2 — transport coverage; KPI 3 — readiness signal |
| 03 | `slice-03-traces.md` | Both transports accept ExportTraceServiceRequest end-to-end | KPI 4 — signal coverage (2 of 3) |
| 04 | `slice-04-metrics.md` | Both transports accept ExportMetricsServiceRequest end-to-end | KPI 4 — signal coverage (3 of 3) |
| 05 | `slice-05-backpressure.md` | Per-transport `max_concurrent_requests`; refusals are deterministic and observable | KPI 5 — concurrency saturation events; KPI 6 — refusal-not-drop |
| 06 | `slice-06-forwarding-sink.md` | `ForwardingSink` writes downstream OTLP and propagates success/failure | KPI 7 — downstream-acceptance success ratio |
| 07 | `slice-07-tls-schema-knob.md` | TLS / SPIFFE schema present in v0 config (defaulting off, warn on enable) | (forward-compat insurance; no behavioural KPI) |
| 08 | `slice-08-graceful-shutdown.md` | SIGTERM drains in-flight requests; deadline-exceeded is loud | KPI 8 — graceful-restart drop ratio |

Eight slices total. Slice 01 is the walking skeleton; Slices 02–08 each add one concrete user-observable capability.

---

## Priority Rationale

Order is **outcome impact first, dependency-graph second, riskiest-assumption-first as tie-breaker**. The full Value × Urgency / Effort table is in [`prioritization.md`](prioritization.md); the rationale for the ordering is here.

1. **Slice 01 (walking skeleton)** is first because Andrea chose this shape explicitly: the harness is the load-bearing dependency, and a thicker walking skeleton lands integration risk at slice 01 rather than late. Until Slice 01 is green, no other slice has a substrate to add to.

2. **Slice 02 (HTTP/protobuf + readiness)** is second because:
   - HTTP/protobuf is the second OTel-canonical transport and is a hard requirement at v0 (Q1 locked decision).
   - `/healthz` and `/readyz` need the HTTP listener; until slice 02 lands they have nowhere to live.
   - Without slice 02, the v0 transport contract is half-met. This is the single biggest outcome-impact unlock after slice 01.

3. **Slices 03 + 04 (traces + metrics)** complete the OTLP three-signal contract. Order between them is by complexity (traces is simpler than metrics, per the harness's own US-05 → US-06 rationale). Without 03 and 04, Aperture is "logs gateway" not "OTLP gateway", which fails the v0 promise.

4. **Slice 05 (backpressure)** is fourth because it is the **riskiest unvalidated assumption** for the integration plane. An Aperture that accepts traffic correctly under good conditions but melts down under load is a liability, not an asset. Slice 05's per-transport concurrency cap with deterministic refusal is the cheapest possible answer to the load question. Andrea's locked Q4 decision says explicitly: no internal queue, no block, no silent drop.

5. **Slice 06 (ForwardingSink)** is fifth because it is what makes Aperture **integrate with the operator's existing OTel-compatible backend**, which is the Phase-1 roadmap promise. Without it, Slice 01–05 produce a service that accepts traffic and logs it to stderr — useful for testing, useless in production.

6. **Slice 07 (TLS/SPIFFE schema knob)** is sixth because it is **forward-compatibility insurance, not a behaviour change**. Skipping it costs nothing at v0 but breaks the config schema in Phase 2 when Aegis ships. Doing it now is cheap; doing it later is expensive. Andrea's locked Q5 decision: schema present, default off.

7. **Slice 08 (graceful shutdown)** is last because it is the **most operationally-load-bearing** slice (a service that drops in-flight requests on every restart is unfit for production), and because it has the most subtle interactions with every other slice (readiness, sink, listener lifecycles all participate). Putting it last lets each preceding slice be demonstrable in isolation; putting it earlier would force every subsequent slice to integrate with shutdown machinery before the slice's own contract was settled.

### Dependency graph (acyclic)

```
slice-01-walking-skeleton
    |
    +--> slice-02-http-protobuf-and-readiness
    |        |
    |        +--> slice-03-traces
    |        |       |
    |        |       +--> slice-04-metrics
    |        |               |
    |        |               +--> slice-05-backpressure
    |        |                       |
    |        |                       +--> slice-06-forwarding-sink
    |        |                               |
    |        |                               +--> slice-07-tls-schema-knob
    |        |                                       |
    |        |                                       +--> slice-08-graceful-shutdown
    |        |
    |        +-- /healthz, /readyz also unblock slice-08
    |
    +-- StubSink + harness contract used by every later slice
```

Each slice depends only on slices to its left in the graph. No slice forward-references a later one.

### Six taste tests applied (Elephant Carpaccio)

| Test | Verdict | Note |
|---|---|---|
| **End-to-end** — every slice exercises Bind → Receive → Validate → Hand-off → Observe (and Shut-down where in-scope) | PASS | Each slice file's "What it lights up" section names the activities it touches. No slice is single-column. |
| **Demonstrable** — each slice can be shown working in a single session | PASS | Every slice has an explicit "Demo command" in its slice file. |
| **Independently valuable** — each slice delivers a verifiable user-observable behaviour | PASS | Each slice file lists 1–2 user-observable behaviours added; none merely "supports" a future slice. Slice 07 is the closest to "infrastructure", and it is justified as forward-compat insurance against a known Phase-2 schema break. |
| **Right-sized** — wall-clock days, not weeks | PASS | Each slice file declares its complexity drivers. None is large enough to justify further splitting; none is so trivial it could be merged with a neighbour without losing a clean demo. |
| **Vertical, not horizontal** — slices are user-outcome-shaped, not technical-layer-shaped | PASS | None of "decode layer", "transport layer", "config layer" appears as a slice. Every slice is a user-observable outcome. |
| **Riskiest assumption first (after walking skeleton)** | PASS | Slice 05 (backpressure) is positioned as the first slice that defends an unproven load assumption, immediately after the three-signal contract is complete. |

## Scope Assessment: PASS

- **Stories**: 9 user stories (US-AP-01 through US-AP-09). At the upper end of right-sized; under the >10 oversize signal.
- **Bounded contexts**: 1 (Aperture itself; consumes the harness as a substrate dependency, not as a separate context).
- **Walking skeleton integration points**: 5 (SDK→Aperture, Aperture→harness, Aperture→sink, Aperture→stderr, Aperture→SDK response).
- **Wall-clock estimate**: 1.5–2 weeks of single-session-per-slice cadence; under the >2-week oversize signal.
- **Independent user outcomes that could ship separately**: 0; this is one cohesive integration plane, the smallest ship-able shape of an OTLP gateway.

Right-sized. No split required.

---

## Story-to-slice mapping

The full story crafting lives in `user-stories.md`. Provisional mapping (revisit after Phase 4):

| Story | Slice(s) | Note |
|---|---|---|
| US-AP-01 — Bind gRPC listener | 01 | Walking skeleton's first activity. |
| US-AP-02 — Bind HTTP listener and serve `/healthz` + `/readyz` | 02 | Three concerns naturally co-locate on the HTTP port. |
| US-AP-03 — Accept a valid logs export end-to-end | 01 (gRPC) + 02 (HTTP) | Same story, both transports; HTTP arm reuses Slice-01 sink + harness wiring. |
| US-AP-04 — Reject malformed input with the harness's named violation rule | 01 (gRPC) + 02 (HTTP) | Reject paths come for free with Slice 01's harness call; HTTP variant arrives in 02. |
| US-AP-05 — Accept a valid traces export end-to-end | 03 | Symmetric with US-AP-03 for traces. |
| US-AP-06 — Accept a valid metrics export end-to-end | 04 | Symmetric for metrics. |
| US-AP-07 — Refuse beyond the per-transport concurrency cap, never drop | 05 | The riskiest-assumption slice. |
| US-AP-08 — Forward accepted records to a downstream OTel backend | 06 | The slice that makes Aperture useful in production. |
| US-AP-09 — Drain in-flight requests on SIGTERM | 08 | The operationally load-bearing slice. |

(Slice 07's TLS/SPIFFE schema knob does not produce a user-facing story — it is forward-compatibility infrastructure. It is captured as a System Constraint in `user-stories.md` and as an `@infrastructure`-tagged technical task in the slice file. **It is the only `@infrastructure` slice; it has explicit DoR justification and ships alongside Slices 06 and 08, both of which carry user-facing stories.**)

---

## Walking-skeleton coherence check

The skeleton (Slice 01) covers all six activities of the backbone:

| Activity | Slice 01 coverage |
|---|---|
| Bind listeners | Bind gRPC :4317. (HTTP listener arrives Slice 02 to support `/healthz` + `/readyz`.) |
| Receive payload | gRPC ExportLogsServiceRequest. |
| Validate via harness | Real `otlp_conformance_harness::validate_logs(bytes, Framing::GrpcProtobuf)` call. |
| Hand off to sink | Real `OtlpSink` trait dispatch to a concrete `StubSink` impl. |
| Observe self | stderr structured JSON for startup, request_received, sink_accepted. |
| Shut down gracefully | Process exits cleanly on SIGTERM (best-effort drain; full graceful drain in Slice 08). |

Activity 1 is partial (gRPC only); Activity 5 is partial (no `/healthz` / `/readyz` until HTTP exists in Slice 02); Activity 6 is partial (no full drain until Slice 08). This is intentional — the skeleton is the thinnest end-to-end thing that demonstrates the value proposition, not a feature-complete first cut. Subsequent slices fill in each activity's missing capabilities.
