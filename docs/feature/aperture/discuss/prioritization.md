# Prioritisation — Aperture v0

> **Wave**: DISCUSS — Phase 2.5.
> **Author**: Luna.
> **Date**: 2026-05-04.
> **Companion documents**: `story-map.md`, `outcome-kpis.md`, `../slices/slice-*.md`.

---

## Release priority

The full rationale is in `story-map.md > Priority Rationale`. This table is the at-a-glance ordering, the outcome each slice targets, and the KPI each slice moves.

| Priority | Slice | Target outcome | KPI | Rationale |
|---|---|---|---|---|
| 1 | `slice-01-walking-skeleton` | First end-to-end OTel SDK round-trip lands. Real harness, real sink, real gRPC OK back to SDK. | KPI 1 — `first_integration_round_trip` | Andrea's locked walking-skeleton shape. The harness is the load-bearing dependency; integration risk lands here. |
| 2 | `slice-02-http-protobuf-and-readiness` | OTel SDKs using HTTP/protobuf are first-class; operators can probe `/healthz` and `/readyz`. | KPI 2 — transport coverage; KPI 3 — readiness signal | HTTP/protobuf is a v0 hard requirement (Q1). `/healthz` + `/readyz` need the HTTP listener. |
| 3 | `slice-03-traces` | Both transports accept ExportTraceServiceRequest. | KPI 4 — signal coverage (2 of 3) | Symmetry with logs; simpler than metrics; unblocks Slice 04. |
| 4 | `slice-04-metrics` | Both transports accept ExportMetricsServiceRequest. | KPI 4 — signal coverage (3 of 3) | Completes the OTLP three-signal contract. |
| 5 | `slice-05-backpressure` | Per-transport concurrency cap; refusals are deterministic and observable. | KPI 5 — concurrency saturation events; KPI 6 — refusal-not-drop | First slice that defends an unproven load assumption. Andrea's locked Q4 decision: cap, refuse, never block, never drop silently. |
| 6 | `slice-06-forwarding-sink` | `ForwardingSink` writes downstream OTLP and propagates success/failure. | KPI 7 — downstream-acceptance success ratio | Phase-1 roadmap promise: integrate with operator's existing OTel-compatible backend. |
| 7 | `slice-07-tls-schema-knob` | TLS / SPIFFE keys present in v0 config schema, defaulting off, warn on enable. | (no behavioural KPI; forward-compat insurance) | Andrea's locked Q5: schema present, default off. Phase-2 Aegis will not break the schema. |
| 8 | `slice-08-graceful-shutdown` | SIGTERM drains in-flight requests; deadline-exceeded is loud, never silent. | KPI 8 — graceful-restart drop ratio | Most operationally load-bearing slice; placed last because it integrates with every preceding slice. |

---

## Value × Urgency / Effort scoring

| Slice | Value (1-5) | Urgency (1-5) | Effort (1-5) | Score | Tie-breaker |
|---|---|---|---|---|---|
| 01 | 5 — unblocks every later slice | 5 — nothing else can ship without it | 4 — first integration of harness, sink, gRPC server | 6.25 | Walking skeleton — fixed first |
| 02 | 5 — completes v0 transport contract | 4 — Q1 hard requirement | 3 — second transport, two new endpoints | 6.66 | Highest-value next |
| 03 | 4 — signal coverage 2/3 | 3 — symmetric with logs, no new contract | 2 — reuses Slice 02's wiring | 6.0 | |
| 04 | 4 — signal coverage 3/3 | 3 — completes OTLP three-signal | 2 — reuses Slice 03's wiring | 6.0 | |
| 05 | 5 — defends load assumption | 4 — riskiest unvalidated assumption | 3 — semaphores, 503 / RESOURCE_EXHAUSTED, observability | 6.66 | Riskiest-assumption tie-break |
| 06 | 5 — makes Aperture production-useful | 3 — no Phase-1 milestone without it | 4 — first outbound network from Aperture, first downstream-error mapping | 3.75 | |
| 07 | 2 — schema-only, no behaviour | 4 — must land before Phase 2 Aegis | 1 — config-schema test only | 8.0 | Cheap and time-bounded |
| 08 | 4 — production-readiness gate | 3 — needs every other slice in place first | 4 — drain interacts with readiness, listeners, sinks | 3.0 | Last by integration risk |

Score = Value × Urgency / Effort. Tie-breaker order: (1) Walking Skeleton, (2) Riskiest Assumption, (3) Highest Value. Score is informational; the actual ordering is the dependency-respecting topological sort in the table at the top.

---

## Backlog suggestions (story-to-slice)

(Story IDs assigned in Phase 4; this is a forward declaration so reviewers can sanity-check the slicing.)

> **Numbering note**: Story IDs (`US-AP-NN`) are stable across the feature's lifecycle. Priority labels (`Pn`) in the table below are derived from the slice each story lands in and may shift if slice ordering changes during DESIGN. Stories never get renumbered; priority labels do.

| Story | Slice | Priority | Outcome link | Dependencies |
|---|---|---|---|---|
| US-AP-01 — Bind gRPC listener | 01 | P1 | KPI 1 | None |
| US-AP-02 — Bind HTTP listener + `/healthz` + `/readyz` | 02 | P2 | KPI 2, KPI 3 | US-AP-01 |
| US-AP-03 — Accept valid logs export end-to-end | 01 (gRPC), 02 (HTTP) | P1, P2 | KPI 1, KPI 4 | US-AP-01 (gRPC), US-AP-02 (HTTP) |
| US-AP-04 — Reject malformed with named violation | 01 (gRPC), 02 (HTTP) | P1, P2 | KPI 4 | US-AP-03 |
| US-AP-05 — Accept valid traces export | 03 | P3 | KPI 4 | US-AP-03 |
| US-AP-06 — Accept valid metrics export | 04 | P4 | KPI 4 | US-AP-05 |
| US-AP-07 — Refuse beyond concurrency cap | 05 | P5 | KPI 5, KPI 6 | US-AP-06 |
| US-AP-08 — Forward to downstream backend | 06 | P6 | KPI 7 | US-AP-07 |
| US-AP-09 — Drain in-flight on SIGTERM | 08 | P8 | KPI 8 | US-AP-08, US-AP-02 |

(Slice 07 carries no user story; it is forward-compat schema infrastructure. Tracked as a single technical task in `user-stories.md` and in [`../slices/slice-07-tls-schema-knob.md`](../slices/slice-07-tls-schema-knob.md).)

---

## Risk register (release-level)

The `wave-decisions.md` carries the load-bearing decision risks. This table carries the *delivery* risks the slicing introduces.

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| Slice 01 ships with a stub harness instead of the real one | Medium | Critical | Andrea's locked walking-skeleton shape says real harness. Demo command in `slice-01-walking-skeleton.md` exercises the real harness path. Peer review verifies. |
| Slice 02 splits the HTTP listener and the readiness endpoints into two slices | Low | Medium | Fused intentionally — three concerns multiplex on the same port; splitting makes Slice 02 not-end-to-end. Documented in slice file. |
| Slice 05's concurrency-cap default value is wrong for production | Medium | Low | Default is 1024 per transport (placeholder). DESIGN may revisit. The value is operator-tunable, so calibration is not a contract issue. |
| Slice 07 (TLS schema knob) is dropped as "infrastructure not user-value" | Low | High | Story map's `Priority Rationale` and this file both record the load-bearing reason: skipping it costs nothing now, breaks the schema in Phase 2. The slice ships even though it is `@infrastructure`. |
| Slice 08's drain-deadline default is too short for production | Medium | Medium | Default 30 s; tunable. UAT scenarios cover both successful drain and deadline-exceeded paths. |
| OpenTelemetry Rust SDK 0.27 (the v0 demo SDK) ships a breaking change before slice 01 lands | Low | Medium | Pin the SDK version in the demo / corpus capture in the same way the harness pins `opentelemetry-proto = "=0.27.0"`. |
| Sink trait shape locked at Slice 01 turns out wrong for Sieve in Phase 1 | Low | High | DESIGN-wave ADR (Morgan) finalises the trait shape; DISCUSS specifies the contract loosely (`Send + Sync`, `async accept(record) -> Result<(), SinkError>`). The trait is `#[non_exhaustive]`-friendly, additive evolution is safe. |
