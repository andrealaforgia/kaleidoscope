# Definition of Ready Validation — `spark` v0

> 9-item hard gate per `nw-leanux-methodology` (DoR items 1–8) and `nw-outcome-kpi-framework` (item 9). Every user story must pass every item with evidence before handoff to DESIGN. Item 0 (Elevator Pitch presence and quality) is owned by the reviewer (Dimension 0); it is shown here for completeness because the precedent (the harness, Aperture) has the Elevator Pitch as a mandatory part of every story.

---

## US-SP-01 — Initialise Spark and round-trip a span end-to-end

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 0 | Elevator Pitch present (Before / After / Decision enabled) | PASS | Section "Elevator Pitch" in `user-stories.md` US-SP-01. After-line names a runtime entry point (`spark::init(SparkConfig::for_service("payments-api"))`) and a Cargo-test entry point (`cargo test -p spark slice_01_walking_skeleton`). Decision-enabled line names the integration confidence the developer gains. |
| 1 | Problem statement clear, in domain language | PASS | "Spark's value proposition is 'one function call replaces a page of OTel SDK setup'. Until one valid export round-trips successfully — through a real OTLP/gRPC exporter, against a real Aperture instance running the real conformance harness, with the house attributes intact — every other capability... is theoretical." Names the domain (Rust SDK setup) and the pain (manual OTel SDK wiring). |
| 2 | User/persona identified with specific characteristics | PASS | Three personas named with role and context: `acme-observability` developer instrumenting their first Rust service; Kaleidoscope CI; future Aegis component author. |
| 3 | At least 3 domain examples with real data | PASS | Three examples: developer at `acme-observability` adding Spark to `payments-api`; Kaleidoscope CI's `cargo test -p spark slice_01_walking_skeleton`; future Sieve contributor reading `tenant.id` from the Resource. Each names real (project-grounded) actors and concrete data (`payments-api`, `acme-prod`, the OTel SDK 0.27 version). |
| 4 | UAT scenarios in Given/When/Then (3-7) | PASS | 4 scenarios: spark::init constructs SDK with all four house attrs; traces export carries all four attrs; init writes diagnostic to tracing not OTel pipeline; SparkConfig is plain data with no I/O. Each is in Given/When/Then form. |
| 5 | AC derived from UAT | PASS | 8 acceptance criteria, each tied to a scenario or to the Solution section. |
| 6 | Right-sized | PASS | Wall-clock: 2-3 days. 4 UAT scenarios, single demonstrable behaviour (the walking-skeleton round-trip). |
| 7 | Technical notes identify constraints | PASS | Names DESIGN-wave decisions Morgan must make: `opentelemetry-otlp` minor version pin, internal module split, `tracing` macro target string. Names the `aperture` dev-dep posture (Apache-2.0 compatibility). Plus the System Constraints section at the top of `user-stories.md`. |
| 8 | Dependencies tracked | PASS | "`crates/aperture/v0.1.0` shipped; `crates/otlp-conformance-harness/v0.1.0` shipped." Both verified by recent commits on main (b96eb7d and earlier). |
| 9 | Outcome KPIs defined with measurable targets | PASS | KPI 1 (and partially KPI 3 traces-only): 100% of the documented Slice-01 demo command sequence completes without manual intervention, measured by the CI integration test. |

**DoR Status: PASSED**

---

## US-SP-02 — Refuse missing required attributes at init time, never silently emit broken telemetry

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 0 | Elevator Pitch | PASS | Before/After/Decision-enabled, with concrete entry points (function call returning specific error variants). |
| 1 | Problem statement clear | PASS | Names the pain: a library that lets a misconfiguration through into emitted telemetry is hostile to debugging. |
| 2 | User/persona | PASS | Three personas: `acme-observability` developer making a typo; `acme-observability` operator reviewing a deployment; future Codex component plugging into Spark's lint pass. |
| 3 | 3+ domain examples with real data | PASS | Three examples grounded in real components and concrete misconfigurations: tenant.id typo on a multi-tenant gateway; empty-string tenant.id from a misconfigured environment file read; invalid endpoint URI from a sloppy v1-example copy-paste. Each names real actors and concrete error variants. |
| 4 | UAT scenarios | PASS | 5 scenarios: missing tenant.id rejected; empty-string tenant.id rejected (same error); invalid endpoint rejected; SparkConfig without require_tenant_id() succeeds (negative-case proof); second init call rejected. Within the 3-7 ceiling. |
| 5 | AC derived from UAT | PASS | 6 ACs covering each error variant, the no-side-effects-on-Err guarantee, and the SparkError trait/derive posture. |
| 6 | Right-sized | PASS | Wall-clock: 1-2 days. 5 UAT scenarios, single demonstrable behaviour (the lint pass). |
| 7 | Technical notes | PASS | Names DESIGN-wave decisions: thiserror-vs-handrolled, `#[non_exhaustive]` mechanism, the explicit DISCUSS-locked posture (empty-string = absence; whitespace-only = absence is deferred to Codex). |
| 8 | Dependencies tracked | PASS | "US-SP-01 (the walking skeleton's Ok-path is the precondition for testing the Err paths against the same code path)." |
| 9 | Outcome KPIs | PASS | KPI 2: 100% of misconfigurations matching one of the closed `SparkError` variants are caught at `init`. |

**DoR Status: PASSED**

---

## US-SP-03 — Inject all four house resource attributes on every emitted signal

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 0 | Elevator Pitch | PASS | Before/After/Decision-enabled with the runtime call (`SparkConfig::with_feature_flags(...).with_experiment_id(...)`) and the wire-level assertion. |
| 1 | Problem statement | PASS | Names the pain: a Spark v0 that omits `feature_flag.*` and `experiment.id` would force every Phase-2 component (Aegis, Loom) to wait for a v0.1 of Spark before they could integrate. |
| 2 | User/persona | PASS | Four personas: A/B-testing developer; feature-flag-using developer; future Aegis; future Loom. |
| 3 | 3+ domain examples | PASS | Three examples grounded in real-world scenarios: `exp-2026-Q2-pricing` A/B test at `acme-observability`; checkout-v2 feature-flag rollout; single-tenant developer using the minimum-viable integration. |
| 4 | UAT scenarios | PASS | 4 scenarios: all four house attrs on traces; minimum-viable Resource (only service.name); feature_flag namespace-prefixed correctly; empty-string optional attrs are skipped. Within the 3-7 ceiling. |
| 5 | AC derived from UAT | PASS | 6 ACs covering builder-method shape, Resource composition, namespace prefix, and the empty-value skip rule. |
| 6 | Right-sized | PASS | Wall-clock: 2 days. 4 UAT scenarios, single demonstrable behaviour (the four-attribute Resource composition). |
| 7 | Technical notes | PASS | Names DESIGN-wave decisions: builder method signatures (HashMap vs slice vs iterator). Documents the `feature_flag.` (singular) prefix decision and the Codex-Phase-0+ migration path if OTel semconv stabilises differently. |
| 8 | Dependencies tracked | PASS | "US-SP-01 (the walking skeleton's Resource composition is the substrate this story extends)." |
| 9 | Outcome KPIs | PASS | KPI 3: 100% of canonical-config emissions carry all four house attributes on the wire. |

**DoR Status: PASSED**

---

## US-SP-04 — Honour the OTel-canonical environment variables and SparkConfig precedence

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 0 | Elevator Pitch | PASS | Before/After/Decision-enabled. Names operator's deployment story explicitly. |
| 1 | Problem statement | PASS | Names the pain: a telemetry SDK that ignores OTel-canonical env vars makes itself non-portable. |
| 2 | User/persona | PASS | Three personas: multi-region operator; developer overriding env var for debug; OTel SDK ecosystem (the upstream contract). |
| 3 | 3+ domain examples | PASS | Three examples grounded in real-world scenarios: multi-region deployment at `acme-observability`; debug-session env-var override; default-localhost-4317 first-time integration. |
| 4 | UAT scenarios | PASS | 4 scenarios: builder overrides env; env honoured when builder absent; default localhost; resolved-config tracing event. Within the 3-7 ceiling. |
| 5 | AC derived from UAT | PASS | 7 ACs covering precedence chain, individual cases, the resolved-config event, and the "no SPARK_ env vars" rule. |
| 6 | Right-sized | PASS | Wall-clock: 1-2 days. 4 UAT scenarios, single demonstrable behaviour (the precedence chain). |
| 7 | Technical notes | PASS | Names DESIGN-wave decision: which `OTEL_*` env vars to explicitly test for. Notes that Spark delegates env-var parsing to `opentelemetry-otlp`'s upstream resolver. |
| 8 | Dependencies tracked | PASS | "US-SP-01 (the walking skeleton with `with_endpoint`), US-SP-02 (the `InvalidEndpoint` error variant defends against env-var typos)." |
| 9 | Outcome KPIs | PASS | KPI 4: 100% of supported `OTEL_*` env vars are honoured. |

**DoR Status: PASSED**

---

## US-SP-05 — Inject house attributes on logs and metrics, not just traces

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 0 | Elevator Pitch | PASS | Before/After/Decision-enabled with the runtime entry points (the standard OTel `logger_provider` and `meter` calls) and the wire-level assertion. |
| 1 | Problem statement | PASS | Names the pain: symmetry across signals. If logs/metrics emit a different Resource, the unified-query workflow breaks. |
| 2 | User/persona | PASS | Four personas: full-three-signal application developer; future Lumen author; future Pulse author; unified-query operator. |
| 3 | 3+ domain examples | PASS | Three examples grounded in real-world scenarios: full three-signal at `acme-observability`; logs-heavy debugging; metrics-only canary deployment. Each names concrete signal volumes and operator workflows. |
| 4 | UAT scenarios | PASS | 3 scenarios: logs export carries four attrs; metrics export carries four attrs; all three signals share the same Resource shape. Within the 3-7 ceiling. |
| 5 | AC derived from UAT | PASS | 5 ACs covering provider-symmetry (`LoggerProvider` and `MeterProvider`), Resource attribute presence per signal, cross-signal consistency, and single-init across all providers. |
| 6 | Right-sized | PASS | Wall-clock: 2 days. 3 UAT scenarios, single demonstrable behaviour (signal-type symmetry). |
| 7 | Technical notes | PASS | Names DESIGN-wave decision: exact OTel SDK builder calls. Notes the metrics-emission test pattern (counter increments are flushed at drop, not at increment). |
| 8 | Dependencies tracked | PASS | "US-SP-01 (the traces walking skeleton), US-SP-03 (the four-attribute Resource composition for traces)." |
| 9 | Outcome KPIs | PASS | KPI 5: 100% of canonical-config emissions across all three signal types carry the same four house attributes. |

**DoR Status: PASSED**

---

## US-SP-06 — Flush pending exports synchronously on guard drop, with bounded deadline

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 0 | Elevator Pitch | PASS | Before/After/Decision-enabled. Names the runtime mechanism (RAII `Drop`) and the bounded-flush guarantee. |
| 1 | Problem statement | PASS | Names the pain: a library that drops in-flight exports on every clean exit is unfit for any short-running tool or k8s pod. |
| 2 | User/persona | PASS | Three personas: short-running CLI developer; k8s operator; coverage reviewer. |
| 3 | 3+ domain examples | PASS | Three examples grounded in real-world scenarios: clean-flush in a one-shot CLI tool; deadline-exceeded during a downstream Loki incident at `acme-observability`; configured short-deadline test session. |
| 4 | UAT scenarios | PASS | 4 scenarios: clean drop within deadline; deadline-exceeded WARN; explicit drop equivalence; down-downstream no-panic. Within the 3-7 ceiling. |
| 5 | AC derived from UAT | PASS | 7 ACs covering provider-flush, bounded deadline, INFO/WARN events, no-panic posture, drop-twice no-op, explicit-drop equivalence. |
| 6 | Right-sized | PASS | Wall-clock: 2 days. 4 UAT scenarios, single demonstrable behaviour (the bounded flush). |
| 7 | Technical notes | PASS | Names DESIGN-wave decisions: per-provider flush pattern (sequential vs concurrent), how `flush_timeout` is divided across providers, drained/dropped count derivation. Notes panic-during-Drop posture. |
| 8 | Dependencies tracked | PASS | "US-SP-05 (the three-signal Resource composition is what `force_flush` flushes)." |
| 9 | Outcome KPIs | PASS | KPI 6: 100% of guard drops produce exactly one observable `tracing` event; 0% silent. |

**DoR Status: PASSED**

---

## Summary

All six user stories pass the 9-item Definition of Ready hard gate. Every item has explicit evidence above. No story is blocked.

| Story | DoR Status |
|---|---|
| US-SP-01 — Walking skeleton | PASSED |
| US-SP-02 — Init error paths | PASSED |
| US-SP-03 — Four house attributes on traces | PASSED |
| US-SP-04 — OTel env-var precedence | PASSED |
| US-SP-05 — Logs and metrics symmetry | PASSED |
| US-SP-06 — Bounded flush on guard drop | PASSED |

Ready for peer review by `@nw-product-owner-reviewer` (Sentinel). Iteration budget: 2 per the skill.
