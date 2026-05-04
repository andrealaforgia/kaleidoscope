# Definition of Ready Validation — `aperture` v0

> **Wave**: DISCUSS — Phase 3 (validation gate before peer review and DESIGN handoff).
> **Author**: Luna (`nw-product-owner`).
> **Date**: 2026-05-04.
> **Companion documents**: `user-stories.md`, `outcome-kpis.md`, `wave-decisions.md`.

The 9-item DoR checklist (LeanUX methodology) is a hard gate. Each of US-AP-01 through US-AP-09 must pass all 9 items before this DISCUSS wave can hand off to DESIGN.

The 9 items: 1) problem statement clear in domain language | 2) user/persona with specific characteristics | 3) 3+ domain examples with real data | 4) UAT scenarios in Given/When/Then (3-7 scenarios) | 5) AC derived from UAT | 6) right-sized (1-3 days, 3-7 scenarios) | 7) technical notes identifying constraints | 8) dependencies resolved or tracked | 9) outcome KPIs defined with measurable targets.

---

## US-AP-01 — Bind both OTLP listeners at startup

| # | DoR Item | Status | Evidence / Issue |
|---|---|---|---|
| 1 | Problem statement clear, domain language | PASS | "A multi-listener service has multiple ways to fail at startup… Without per-listener stderr lines and a readiness probe that flips only after BOTH listeners are bound, an operator's first signal of trouble is whatever their SDK or load balancer reports." Names the operator role, the failure mode, the missing signal. |
| 2 | User/persona identified | PASS | "Operator deploying Aperture (third-party engineer running Kaleidoscope)", "OpenTelemetry SDK client", "Kaleidoscope CI". Three concrete personas, each with a specific question. |
| 3 | 3+ domain examples with real data | PASS | (1) `acme-observability` k8s clean startup, (2) port collision with `hostNetwork: true`, (3) TLS knob set true on v0. Real org name, real port numbers (4317/4318), real config keys. |
| 4 | UAT scenarios (3-7) | PASS | 3 scenarios: clean bind, port collision, TLS knob warn. Within range. |
| 5 | AC derived from UAT | PASS | 7 AC bullets, each tied directly to a UAT line. |
| 6 | Right-sized | PASS | 3 UAT scenarios; complexity is one Tokio listener bind plus stderr emission; demonstrable in a single session. |
| 7 | Technical notes | PASS | Names slice-01/slice-02 split for `/readyz`; names config keys for bind addresses; flags DESIGN decisions (Tokio Server library, gRPC vs HTTP runtime sharing). |
| 8 | Dependencies tracked | PASS | "None at the story level. Slice 01 depends on the harness crate's locked public API (Phase 0 deliverable, already shipped)." Concrete, verified. |
| 9 | Outcome KPIs defined | PASS | KPI: 100% of pilot operators (3 in 30 days) report `/readyz` as their readiness gate. Measured by interview. Greenfield baseline named explicitly. |

### DoR Status: PASSED

---

## US-AP-02 — Serve health, readiness, and HTTP/protobuf on `:4318`

| # | DoR Item | Status | Evidence / Issue |
|---|---|---|---|
| 1 | Problem statement clear | PASS | Three concerns multiplexed on one port; until the HTTP listener exists, neither probe nor HTTP exporter has a target. Domain-language. |
| 2 | User/persona identified | PASS | k8s operator, OTel SDK author choosing transport, OTel SDK client (Python HTTP exporter). Concrete. |
| 3 | 3+ domain examples | PASS | (1) k8s rolling update with readiness probe, (2) Python OTLPLogExporter to /v1/logs, (3) JSON misconfiguration. Real protocol details. |
| 4 | UAT scenarios | PASS | 5 scenarios: liveness, readiness state, valid HTTP logs, wrong content type, unknown path. Within range. |
| 5 | AC derived from UAT | PASS | 7 AC bullets, each tied to a UAT line. |
| 6 | Right-sized | PASS | 5 UAT scenarios; one new listener, three new endpoints (path-prefix dispatch). Demonstrable in a session. |
| 7 | Technical notes | PASS | Names DESIGN decision on routing mechanism (axum vs hyper handlers); flags multiplexed-vs-admin-port question. |
| 8 | Dependencies tracked | PASS | "US-AP-01" — in the same DISCUSS wave, ordered first. |
| 9 | Outcome KPIs defined | PASS | KPI 2: HTTP-protobuf success ratio ≥ 99.9% under non-overload. Measured by stderr-event ratio. Greenfield baseline. |

### DoR Status: PASSED

---

## US-AP-03 — Accept a valid logs export and acknowledge the sink

| # | DoR Item | Status | Evidence / Issue |
|---|---|---|---|
| 1 | Problem statement clear | PASS | "The whole point of Aperture is to accept OTLP. Until one valid export round-trips, every other capability is theoretical." Names the validating round-trip explicitly. |
| 2 | User/persona identified | PASS | OTel SDK client (the SDK process), operator greppping stderr, future Sieve component. |
| 3 | 3+ domain examples | PASS | (1) Python Django app with `OTEL_EXPORTER_OTLP_ENDPOINT`, (2) Rust integration test, (3) high-volume batch. Real config strings, real SDK version. |
| 4 | UAT scenarios | PASS | 3 scenarios: gRPC happy path, HTTP happy path, multi-record batch count. Within range. |
| 5 | AC derived from UAT | PASS | 4 AC bullets, each tied to UAT or to a CI invariant. |
| 6 | Right-sized | PASS | The first end-to-end story; complexity is in lighting up the harness boundary, not in any single piece. Demonstrable as the Slice-01 walking-skeleton demo. |
| 7 | Technical notes | PASS | Names DESIGN decision on `OtlpSink` trait signature; explains resource-attribute extraction. |
| 8 | Dependencies tracked | PASS | "US-AP-01, US-AP-02". |
| 9 | Outcome KPIs defined | PASS | KPIs 1, 2, 4. Per-story: ratio `sink_accepted / request_received` for logs ≥ 99% under non-overload. Measured, baseline named. |

### DoR Status: PASSED

---

## US-AP-04 — Reject malformed input with the harness's named violation rule

| # | DoR Item | Status | Evidence / Issue |
|---|---|---|---|
| 1 | Problem statement clear | PASS | "A receiver that rejects malformed input but provides no diagnostic is barely better than one that accepts malformed input." Names three properties of the diagnostic (named, precise, consistent). |
| 2 | User/persona identified | PASS | OTel SDK client with serialiser bug; operator triaging plumbing; third-party engineer building a custom emitter. |
| 3 | 3+ domain examples | PASS | (1) empty body from misconfigured client, (2) truncated body at byte 50, (3) traces misrouted to /v1/logs at acme-observability. Real byte offsets, real rules. |
| 4 | UAT scenarios | PASS | 3 scenarios: gRPC EmptyInput, HTTP SignalMismatch, HTTP ProtobufDecode (truncated). Within range. |
| 5 | AC derived from UAT | PASS | 5 AC bullets; each ties response status, header / body content, and stderr behaviour to a UAT line. |
| 6 | Right-sized | PASS | 3 UAT; reject paths come for free with the harness call from US-AP-03; complexity is in the response-shape mapping. Demonstrable. |
| 7 | Technical notes | PASS | Names DESIGN unit-test enforcement that Aperture does not reformat the harness Display output. |
| 8 | Dependencies tracked | PASS | "US-AP-03". |
| 9 | Outcome KPIs defined | PASS | KPI 4 with 100% of rejections carrying a non-empty `rule=...` substring. Measured by integration test sweep. |

### DoR Status: PASSED

---

## US-AP-05 — Accept a valid traces export

| # | DoR Item | Status | Evidence / Issue |
|---|---|---|---|
| 1 | Problem statement clear | PASS | "Symmetry. The validate-and-route module must handle traces with the same shape as logs; if the abstraction is wrong, the harness boundary leaks asymmetry into every later signal." |
| 2 | User/persona identified | PASS | OTel SDK client emitting spans; operator triaging trace volume; future Ray component author. |
| 3 | 3+ domain examples | PASS | (1) Python Django auto-instrumentation with span_count=4, (2) custom Rust SDK with span_count=1, (3) misrouted metrics body. |
| 4 | UAT scenarios | PASS | 3 scenarios: gRPC accept, HTTP accept, logs-vs-traces SignalMismatch. Within range. |
| 5 | AC derived from UAT | PASS | 4 AC bullets, each tied to UAT. |
| 6 | Right-sized | PASS | 3 UAT; reuses Slice-02 wiring; new variant on `SinkRecord`; new harness call site. Single-session demoable. |
| 7 | Technical notes | PASS | Names DESIGN decision on span-counting helper. |
| 8 | Dependencies tracked | PASS | "US-AP-04". |
| 9 | Outcome KPIs defined | PASS | KPI 4 per-signal ratio ≥ 99% under non-overload. |

### DoR Status: PASSED

---

## US-AP-06 — Accept a valid metrics export

| # | DoR Item | Status | Evidence / Issue |
|---|---|---|---|
| 1 | Problem statement clear | PASS | "Metrics is the harness's stress test. If the validate-and-route abstraction holds for metrics — including across the five point types — it holds for everything in OTLP scope." Concrete domain rationale. |
| 2 | User/persona identified | PASS | OTel SDK client emitting metrics; operator estimating metrics volume; future Pulse component author. |
| 3 | 3+ domain examples | PASS | (1) Prometheus scrape adapter with data_point_count=147/min, (2) histogram-heavy workload counted as one-per-histogram, (3) misrouted traces body. |
| 4 | UAT scenarios | PASS | 3 scenarios: gRPC accept, HTTP accept, SinkRecord exhaustiveness. Within range. |
| 5 | AC derived from UAT | PASS | 4 AC bullets, each tied to UAT. |
| 6 | Right-sized | PASS | 3 UAT; reuses Slice-03 wiring; new variant + new call site. Single-session demoable. |
| 7 | Technical notes | PASS | Names data-point counting convention; flags histogram-as-one decision. |
| 8 | Dependencies tracked | PASS | "US-AP-05". |
| 9 | Outcome KPIs defined | PASS | KPI 4 per-signal ratio ≥ 99% under non-overload. |

### DoR Status: PASSED

---

## US-AP-07 — Refuse beyond the per-transport concurrency cap, never silently drop

| # | DoR Item | Status | Evidence / Issue |
|---|---|---|---|
| 1 | Problem statement clear | PASS | "Load behaviour is the riskiest unvalidated assumption in the integration plane. Andrea's locked Q4: cap, refuse, never block, never drop silently." Names the three anti-patterns (queue, block, silent drop) and their owners. |
| 2 | User/persona identified | PASS | OTel SDK client (with retry policy); operator running Aperture; Kaleidoscope CI. |
| 3 | 3+ domain examples | PASS | (1) traffic spike from incident-induced volume at acme-observability, (2) misbehaving client with 100 concurrent streams, (3) load-test scenario at cap=4 with 100 clients. |
| 4 | UAT scenarios | PASS | 3 scenarios: gRPC RESOURCE_EXHAUSTED, HTTP 503 + Retry-After, independent-per-transport. Within range. (Plus the cross-cutting `@property` UAT in `journey-aperture.feature`.) |
| 5 | AC derived from UAT | PASS | 6 AC bullets, including the property invariant defended by the `@property` scenario. |
| 6 | Right-sized | PASS | 3 UAT; one new gate (semaphore) per transport; new event type. Demoable. |
| 7 | Technical notes | PASS | Names DESIGN decisions on semaphore mechanism and permit lifetime. |
| 8 | Dependencies tracked | PASS | "US-AP-06". |
| 9 | Outcome KPIs defined | PASS | KPIs 5 and 6: zero silent drops in 1-hour load test at 2x cap; refusal-rate equals exceeded-cap-rate. |

### DoR Status: PASSED

---

## US-AP-08 — Forward accepted records to a downstream OTel-compatible backend

| # | DoR Item | Status | Evidence / Issue |
|---|---|---|---|
| 1 | Problem statement clear | PASS | "A receiver that accepts but does not durably forward is a benchmark, not a production component. ForwardingSink is what makes Aperture meet the Phase-1 roadmap promise." Concrete rationale. |
| 2 | User/persona identified | PASS | Operator running in production; OTel SDK client (transparent to them); future Sieve component author. |
| 3 | 3+ domain examples | PASS | (1) ForwardingSink to co-located OTel Collector at acme-observability, (2) downstream Loki incident with retry behaviour, (3) misconfigured endpoint with DNS failure. |
| 4 | UAT scenarios | PASS | 3 scenarios: healthy downstream, downstream 5xx, connection refused. Within range. |
| 5 | AC derived from UAT | PASS | 5 AC bullets, including the CI invariant `no_telemetry_on_telemetry`. |
| 6 | Right-sized | PASS | 3 UAT; first outbound network from Aperture; new sink impl. Single-session demoable (with a downstream OTel Collector running). |
| 7 | Technical notes | PASS | Names DESIGN decisions on outbound HTTP client, no-retries-from-Aperture rationale, default 5 s timeout. |
| 8 | Dependencies tracked | PASS | "US-AP-07". |
| 9 | Outcome KPIs defined | PASS | KPI 7: downstream-acceptance ratio ≥ 99% under healthy-downstream conditions. Measured by integration test. |

### DoR Status: PASSED

---

## US-AP-09 — Drain in-flight requests on SIGTERM, never silently drop

| # | DoR Item | Status | Evidence / Issue |
|---|---|---|---|
| 1 | Problem statement clear | PASS | "The most operationally load-bearing slice. A service that drops in-flight requests on every restart is unfit for any production deployment." Names the three drain phases (readiness flip, listener close, in-flight drain) and the observability requirement. |
| 2 | User/persona identified | PASS | k8s operator with rolling-deploy strategy; OTel SDK client whose export was in flight; operator triaging missed deadlines. |
| 3 | 3+ domain examples | PASS | (1) k8s rolling deploy under 1 s drain, (2) drain-deadline-exceeded during downstream Loki incident, (3) SIGINT during CI run. |
| 4 | UAT scenarios | PASS | 3 scenarios: clean drain, deadline exceeded, SIGINT/SIGTERM equivalence. Within range. |
| 5 | AC derived from UAT | PASS | 7 AC bullets, each tied to a UAT line. |
| 6 | Right-sized | PASS | 3 UAT; cross-cutting integration of readiness state machine, listener lifecycle, and semaphore counts. Single-session demoable. |
| 7 | Technical notes | PASS | Names coupling to US-AP-07's semaphore design; flags the optional "wait one readiness-probe period" hardening for DESIGN. |
| 8 | Dependencies tracked | PASS | "US-AP-08, US-AP-02 (`/readyz` state machine)". |
| 9 | Outcome KPIs defined | PASS | KPI 8: in 1000-restart load test, zero requests lost without an observable stderr line. Reconciliation invariant on counts. |

### DoR Status: PASSED

---

## Summary

| Story | DoR Status |
|---|---|
| US-AP-01 — Bind both OTLP listeners | PASSED |
| US-AP-02 — Serve health, readiness, HTTP/protobuf | PASSED |
| US-AP-03 — Accept valid logs end-to-end | PASSED |
| US-AP-04 — Reject malformed with named rule | PASSED |
| US-AP-05 — Accept valid traces end-to-end | PASSED |
| US-AP-06 — Accept valid metrics end-to-end | PASSED |
| US-AP-07 — Refuse beyond concurrency cap | PASSED |
| US-AP-08 — Forward to downstream backend | PASSED |
| US-AP-09 — Drain in-flight on SIGTERM | PASSED |

**Overall DoR: 9 of 9 stories PASSED. No remediation required.**

The infrastructure-only Slice 07 (TLS / SPIFFE schema knob) does not produce a user story — it is captured as a System Constraint (item 7 in `user-stories.md > System Constraints`) and as the only `@infrastructure` slice (justified at slice level in `slice-07-tls-schema-knob.md`). The Elevator-Pitch test (PO review Dimension 0) is satisfied at slice-set level: every other slice in the v0 plan carries at least one user-facing story; Slice 07 ships alongside Slices 06 and 08, both of which are user-facing.

---

## Post-DoR sanity checks

Beyond the 9-item DoR, three additional checks specific to Aperture's wave handoff:

| Check | Status | Note |
|---|---|---|
| Every elevator-pitch "After" line references a real network entry point | PASS | gRPC :4317, HTTP :4318, /v1/{logs,traces,metrics}, /healthz, /readyz, SIGTERM. No internal-API entry points. |
| Every elevator-pitch "After" line names concrete observable output | PASS | gRPC status codes, HTTP response bodies, stderr JSON lines (with field examples). No "tests pass" or "data persisted" hand-waves. |
| Every story traces to at least one outcome KPI | PASS | Per-story → KPI mapping in `outcome-kpis.md > Per-story → KPI mapping`. No orphan stories. |

DISCUSS wave is ready for peer review and DESIGN handoff.
