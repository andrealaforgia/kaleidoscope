# Wave Decisions — `spark` v0 (DISCUSS)

> **Wave**: DISCUSS (`nw-product-owner` / Luna).
> **Date**: 2026-05-06.
> **Author**: Luna, single-pass on Bea's overnight delegation.
> **Companion documents**: `user-stories.md`, `journey-spark.yaml`, `journey-spark-visual.md`, `journey-spark.feature`, `story-map.md`, `outcome-kpis.md`, `shared-artifacts-registry.md`, `dor-validation.md`, `../slices/slice-*.md`.

This file is the load-bearing artefact for the DESIGN wave. Morgan (`nw-solution-architect`) reads this to know which decisions are locked at DISCUSS and not to be re-litigated in DESIGN.

---

## Inherited decisions (from architecture and roadmap, recorded for posterity)

The platform-level architecture is laid in `docs/architecture/kaleidoscope-architecture.md`; the implementation roadmap is in `docs/roadmap/kaleidoscope-implementation-roadmap.md`; the licensing posture is in `LICENSING.md`. The following are inherited and DESIGN does not re-derive them:

- **Spark is in the integration plane** (architecture View 2). It is the SDK class — code intended to be embedded in third-party applications — not a server-side platform component.
- **Phase 0 deliverable** (roadmap phase 0: Months 0–2). Per phase 0 deliverables: Spark v0 SDK wrappers add house resource attributes (`tenant.id`, `feature_flag.*`, `experiment.id`) and pin the OTLP wire contract.
- **Substrate**: `opentelemetry` Rust SDK + `opentelemetry-otlp` (Apache-2.0), `tonic` (the gRPC transport `opentelemetry-otlp` rides on at the `=0.27.0` proto version), `opentelemetry-proto =0.27.0` (per harness ADR-0003 — Spark uses the same pin so Aperture, the harness and Spark all agree on the wire types).
- **Sends to Aperture** (architecture View 2 solid arrow `Spark -- OTLP --> Aperture`). The Phase-0 corpus is the harness; the Phase-1 receiver is Aperture. Spark v0 ships before Aperture v0.1.0 was tagged on `aperture/v0.1.0` so this DISCUSS wave can specify Aperture as Spark's integration target.
- **Licence**: **Apache-2.0**. Spark is the SDK class per `LICENSING.md`. This is the same licence as the OTLP conformance harness and the opposite of Aperture (AGPL-3.0-or-later). Spark must remain embeddable in proprietary application code without copyleft contamination.
- **No telemetry from telemetry**: roadmap section A.2 forbids any Kaleidoscope component from emitting telemetry about itself through itself. Spark is the embedded library that emits the application's telemetry; therefore Spark's *own* internal diagnostics (errors at `init`, flush failures at `Drop`) MUST go to a separate channel (the returned `Result`, the application's `tracing` facade, or `stderr`) — never through the OTLP pipeline Spark itself configures.
- **British English**, **no human-effort estimation**, **trunk-based development**, **CI is feedback not gate** (project conventions).
- **Mutation testing per-feature with 100% kill rate** (ADR-0005 Gate 5 of the harness, Aperture's `gate-5-mutants-aperture` workflow inheriting the pattern).
- **Idiomatic Rust**: `#![forbid(unsafe_code)]`, data + free functions, `dyn Trait` only where polymorphism is genuinely needed, no class-style inheritance hierarchies.

DESIGN does not re-derive any of the above.

---

## Bea's locked configuration (operating-overnight, on Andrea's behalf)

Andrea is asleep; Bea trusts Luna's judgement on Spark v0. The locked configuration Bea handed in:

- **feature_type**: backend (it is a library consumed by application developers, but its operational shape is "telemetry pipeline producer")
- **walking_skeleton**: yes
- **ux_research_depth**: lightweight (no human users; consumers are Rust developers writing instrumented services)
- **jtbd_analysis**: no
- **format**: all (visual, yaml, gherkin)
- **output_directory**: `docs/feature/spark/discuss/`

Per the skill, with JTBD = no, Luna executed `*journey for spark` then `*story-map` then `*gather-requirements` with outcome KPIs, in that order, in a single pass.

---

## Open questions raised by Luna and resolved on Bea's behalf

There are five real choices Spark v0 brings up. Bea pre-stated her recommended defaults; Luna evaluated each against the brief, the four-prior-DISCUSS precedent, and the OTel ecosystem's conventions, and accepts each default. The ground truth, recorded VERBATIM:

### Q1 — OTLP transport default

**Decision**: gRPC (port 4317) at v0. HTTP/protobuf (port 4318) is reachable via configuration but defaults to gRPC.

**Why**: gRPC is the OTel-canonical default in `opentelemetry-otlp`'s own examples; Aperture binds gRPC first (slice 01) and HTTP/protobuf second (slice 02). Defaulting to gRPC makes Spark's "out of the box" demo the same shape as Aperture's own walking-skeleton demo. Spark accepts a configuration knob to switch to HTTP/protobuf, but the v0 default is gRPC. (Note: `opentelemetry-otlp` lets the application override transport via env var `OTEL_EXPORTER_OTLP_PROTOCOL`; Spark does not redefine that contract, only pre-fills sensible defaults.)

### Q2 — Required attribute set at v0

**Decision**: `service.name` is **always required** (OTel-canonical service identifier; the OTLP spec considers a record without it semantically incomplete). `tenant.id` is **opt-in required** via a `SparkConfig` builder method (`require_tenant_id()`); when this method has not been called, missing or empty `tenant.id` is not a `spark::init` error. `feature_flag.*` and `experiment.id` are **always optional** pass-throughs Spark adds to the OTel resource when set.

**Why**: `service.name` is the universal OTel resource identifier — every stable downstream depends on it; treating it as anything other than required would let every silent regression slip through. `tenant.id` is Kaleidoscope-specific and only meaningful in multi-tenant deployments (which Aegis owns at Phase 2); making it always-required at v0 would force every single-tenant adopter to set a dummy value, which is hostile. The opt-in-required posture (default off, `require_tenant_id()` flips it on) preserves both single-tenant and multi-tenant use-cases without the schema breaking when Aegis ships.

### Q3 — Failure mode on missing required attribute

**Decision**: `spark::init` returns `Err(SparkError::MissingRequiredAttribute { name })`. Never panic. Never warn-and-continue. The application's main function decides whether the missing attribute is a fatal-startup-error or a warning-and-continue (some test harnesses may want the latter).

**Why**: Panic would crash the application's own startup over a configuration mistake — that's a worse failure mode than the application choosing how to handle the error itself. Warn-and-continue would emit telemetry under a wrong identity (e.g. spans without `service.name`), and that telemetry would land in Aperture, the harness corpus would catch the missing required attribute, and Aperture would reject the export. The fail-loud-at-init posture mirrors the harness's no-telemetry-from-telemetry commitment: Spark refuses to produce telemetry it knows is malformed at config time, rather than relying on Aperture to reject it at the wire.

### Q4 — Auto-shutdown / flush

**Decision**: yes. `spark::init` returns `SparkGuard`, an opaque RAII type whose `Drop` impl flushes pending exports synchronously with a configurable timeout (default 5 s). The application stores the guard in `main` (or wherever its lifetime matches the tracer's expected lifetime).

**Why**: OTel SDK exporters are batched; without an explicit shutdown, in-flight spans/logs/metrics in the export buffer are dropped on process exit. RAII is the idiomatic Rust way to bind a resource's lifetime to a value. `atexit`-style global handlers exist in Rust (`std::panic::set_hook`, `ctrlc` crate, etc.) but they do not cover all exit paths and they make testing harder; `Drop` covers panics, normal returns, and explicit `drop(guard)` calls uniformly. The 5 s default matches the OTel SDK's recommended exporter shutdown timeout (per `OTEL_EXPORTER_OTLP_TIMEOUT`).

### Q5 — House attribute set at v0

**Decision**: All three are documented and supportable at v0:
- `tenant.id` — opt-in required via `SparkConfig::require_tenant_id()` (per Q2).
- `feature_flag.*` — optional, namespace-prefixed pass-through. Spark's API takes a `HashMap<String, String>` (or an `IntoIterator<Item=(impl Into<String>, impl Into<String>)>`) and emits each key as `feature_flag.{key}` resource attribute.
- `experiment.id` — optional, single-string pass-through.

The schema is documented in this DISCUSS (in `shared-artifacts-registry.md > house_attributes`); DESIGN locks the exact API shape (builder method names, value types, `#[non_exhaustive]` posture).

**Why**: The roadmap C.2 names all three. Implementing all three at v0 (with the right opt-in posture for `tenant.id`) is the minimum that lets Aegis (Phase 2) and Loom (Phase 2) plug in without Spark changing its public API. Deferring `feature_flag.*` or `experiment.id` would break the same forward-compat invariant Aperture's TLS schema knob protects against.

---

## Slice 01 — walking-skeleton shape (verbatim)

A Rust binary at `crates/spark/examples/send_one_span_grpc.rs` calls `spark::init(SparkConfig::for_service("payments-api").with_tenant_id("acme-prod"))`, opens a tracer via the configured global SDK, records a single span named `walking-skeleton`, and lets the returned `SparkGuard` drop at the end of `main`.

The integration test `tests/slice_01_walking_skeleton.rs` spawns a real Aperture instance in-process at an ephemeral loopback port (using `aperture::spawn` with a `RecordingSink`), points Spark at that port via `SparkConfig::with_endpoint`, runs the example, drops the guard, and asserts:
1. The recording sink received one `ExportTraceServiceRequest`.
2. The request contains exactly one span with name `walking-skeleton`.
3. The resource attributes on the request include `service.name="payments-api"` and `tenant.id="acme-prod"`.
4. `spark::init` returned `Ok(SparkGuard)` (no error path).
5. Drop completed within the configured flush timeout.

End-to-end through the **full** OTel→OTLP→Aperture→harness→sink flow. This is **Strategy C "real local"** — same posture Aperture's DISTILL wave used (real loopback ports, no InMemory transports for Slice 01). Bea explicitly chose this thicker walking skeleton over a "spark::init returns Ok" smoke test because the OTel SDK + OTLP exporter integration is the load-bearing dependency for Spark's value proposition; integration risk lands at slice 01 so subsequent slices add capability without re-litigating the SDK boundary.

---

## Decisions made in DISCUSS (additive, derived from the locked scope)

These are the requirements-level decisions Luna made by deriving them from the inherited and Bea-locked scope. They are the substrate Morgan starts from in DESIGN. None of them is a "DESIGN decision" — DESIGN locks the *technology* and *internal structure*; DISCUSS locks the *contract* and *user-observable behaviour*.

### D1 — `spark::init` is the only public entry point at v0

Spark exposes one function: `pub fn init(config: SparkConfig) -> Result<SparkGuard, SparkError>`. There is no `spark::tracer()`, no `spark::shutdown()`, no `spark::flush()`. The application uses the standard OTel API (`opentelemetry::global::tracer(...)`) to obtain the tracer that Spark configured; Spark's job ends after `init` returns. This is the same shape `opentelemetry-otlp`'s own initialisation has — Spark is a *sensible-default helper*, not a competing tracer API.

DESIGN locks the exact `SparkConfig` builder method set; DISCUSS locks the function signature and the Result shape.

### D2 — `SparkError` is a closed enum

The closed set of error variants Spark returns from `init` at v0:

```
MissingRequiredAttribute { name: String }       // service.name absent or empty; tenant.id absent when require_tenant_id() was called
InvalidEndpoint { endpoint: String, reason: String }  // OTEL_EXPORTER_OTLP_ENDPOINT or SparkConfig::with_endpoint value cannot be parsed
ExporterInitFailed { reason: String }           // opentelemetry-otlp's exporter constructor returned an error (TLS, transport setup, etc.)
GlobalAlreadyInitialised                        // a previous spark::init or opentelemetry::global::set_tracer_provider already ran
```

Renames are version-bump-able; additions are non-breaking (`#[non_exhaustive]`). DESIGN locks the implementation mechanism (`thiserror`, `pub enum SparkError`, etc.); DISCUSS locks the variant set.

### D3 — `SparkConfig` is a builder with required service-name at construction time

The required-attribute discipline starts at the type level: `SparkConfig::for_service(name: impl Into<String>) -> SparkConfig` is the only constructor. Builder methods then accept optional / opt-in-required attributes:

```
SparkConfig::for_service("payments-api")             // service.name required at construction
    .require_tenant_id()                             // makes tenant.id required-at-init
    .with_tenant_id("acme-prod")                     // sets tenant.id
    .with_feature_flags([("checkout-v2", "on")])     // optional pass-through
    .with_experiment_id("exp-2026-Q2-pricing")       // optional pass-through
    .with_endpoint("http://localhost:4317")          // optional override of OTEL_EXPORTER_OTLP_ENDPOINT
    .with_flush_timeout(Duration::from_secs(5))      // optional override of guard's Drop timeout
```

Empty string values for required attributes are treated identically to absence — `MissingRequiredAttribute`. DESIGN locks the exact method names and `#[must_use]`/`#[non_exhaustive]` posture; DISCUSS locks the builder pattern and the constructor's required-name argument.

#### Rejected alternatives (recorded for posterity so DESIGN does not re-derive)

Three alternative shapes were considered:

1. **Free-function arguments** (`spark::init(service_name, options) -> Result<...>`) — rejected because `SparkConfig` will grow more fields over the next two phases (Aegis brings TLS keys, Codex brings schema-version pinning, Sieve brings sampling hints); a builder is the standard Rust shape for avoiding argument explosion.
2. **Environment-only configuration** (read all values from `OTEL_*` env vars, no `SparkConfig`) — rejected because the application wants to set `tenant.id` programmatically (a multi-tenant gateway service derives the tenant from each request, not from process env).
3. **Trait-based plugin** (`impl SparkProvider for MyConfig`) — rejected as over-engineered for v0; `dyn Trait` indirection is not warranted when a struct + builder is sufficient. The crafter agent's "data + free functions" paradigm forbids this shape.

### D4 — House attribute injection happens in the OTel `Resource`, not on every span

Spark configures the OTel SDK's `Resource` with the four house attributes (`service.name`, `tenant.id` if set, `feature_flag.*`, `experiment.id` if set). Every span / log record / metric data point inherits these via the SDK's standard `Resource` mechanism. Spark does NOT add per-span attribute injection — that would defeat the purpose of `Resource` and would conflict with the application's own span attribution.

DESIGN locks the exact OTel SDK call site (likely `opentelemetry_sdk::Resource::new(...)`); DISCUSS locks the contract: attributes go on the Resource.

### D5 — No internal tracing, no telemetry-on-telemetry

Spark itself emits no telemetry to the application's OTel pipeline. If Spark needs to surface a diagnostic (a flush failure during Drop, an exporter that returned an error mid-process), it uses the application's `tracing` facade (`tracing::warn!`, `tracing::error!`) — NOT the OTel pipeline. This is the inverse of Aperture's no-telemetry-on-telemetry commitment: Aperture refuses to send its OWN telemetry through itself; Spark refuses to add to the application's telemetry (a customer's telemetry), even with diagnostic intent.

Verified by an integration test: with a `RecordingSink` plugged into Aperture, run a Spark application that triggers an internal error (e.g. malformed endpoint), and assert no `ExportTraceServiceRequest` reaches the sink with `service.name="spark"` or any other Spark-internal identifier.

### D6 — `OTEL_EXPORTER_OTLP_ENDPOINT` is the canonical env-var contract

Spark honours the OTel-canonical environment variables: `OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_EXPORTER_OTLP_PROTOCOL`, `OTEL_SERVICE_NAME`, etc. If the application has set these, Spark respects them. Spark does NOT introduce `SPARK_*` env vars. This is what makes Spark a *thin wrapper*: it does not redefine the OTel SDK's own configuration surface.

Configuration precedence (DISCUSS-locked, DESIGN locks the implementation):
1. `SparkConfig` builder method values (highest — what the application explicitly set).
2. `OTEL_*` environment variables.
3. Spark defaults (`http://localhost:4317`, gRPC, 5 s flush timeout).

If `SparkConfig::for_service("payments-api")` is called and `OTEL_SERVICE_NAME=cart-api` is set in the environment, the application's explicit `"payments-api"` wins. If `with_endpoint` is not called and `OTEL_EXPORTER_OTLP_ENDPOINT=http://collector.acme.internal:4317` is set, the environment value is used.

### D7 — The single-init invariant (CI invariant)

Calling `spark::init` twice in the same process returns `Err(SparkError::GlobalAlreadyInitialised)` on the second call. This mirrors `opentelemetry::global::set_tracer_provider`'s own guard. Defended by a unit test in Spark's CI.

### D8 — No auto-instrumentation at v0

Spark v0 wraps the OTel SDK; the application calls Spark's `init` and then uses the standard OTel API for span/log/metric emission (`tracer.in_span(...)`, `info!`, `metrics::counter(...)`). Spark does NOT auto-instrument HTTP servers, database clients, or any other application library. Auto-instrumentation is a v0.2 (or v1) feature the roadmap names (C.2) but does not schedule.

### D9 — No "strict resource-attribute lint that fails CI" at v0

Spark v0 does the *runtime* check (D2, `MissingRequiredAttribute`). The CI lint that the roadmap (Phase 0 deliverables) mentions — "Spark resource-attribute lint enforces OpenTelemetry Semantic Conventions" — is a separate component, and it lives in Codex (per harness DISCUSS W5: semantic-conventions checks belong in Codex, not in the substrate libraries). Spark v0 does not import or shell out to Codex; it does the minimum required-attribute check inline at `init`.

---

## Out-of-scope (intentional, with rationale)

| Item | Rationale | Whose job, when |
|---|---|---|
| Other languages (Go, Python, Java, TypeScript) | Roadmap C.2 names them but the v0 slice is Rust-only per Bea's brief; future iterations of Spark add other languages. | Future Spark releases. |
| Codex integration (auto schema-version pinning) | Codex does not yet exist; Spark v0 hardcodes the house attribute schema. | Codex (Phase 0) + Spark v0.1. |
| Auto-instrumentation | Spark v0 wraps the OTel SDK; the application calls Spark explicitly. | Spark v0.2 / v1. |
| Strict resource-attribute lint that fails CI | Spark v0 does the runtime check; the CI lint comes later (and lives in Codex per the harness's W5). | Codex (Phase 0) + Spark v0.1. |
| Sampling configuration | Sieve's domain. | Sieve, Phase 1+. |
| Custom exporters (other than OTLP) | OTLP is the only wire format Kaleidoscope supports. | n/a. |
| OTLP/HTTP/JSON encoding | Not stable in OTel spec at the harness's pinned version (v1.5.0). | Future release; harness adds `Framing::OtlpJson` first. |
| OTLP Profiles signal | Not stable in OTel spec at the harness's pinned version. | Future release. |
| Process-exit `atexit` handler outside `Drop` | RAII covers normal returns and panics; the global `atexit` shape is harder to test and harder to reason about. | n/a unless an operator demands it. |
| Spark-side retry / circuit-breaker | The OTel SDK already retries; double-retry is anti-pattern. | n/a. |
| TLS / SPIFFE mutual auth | Aperture v0 is plaintext (per Aperture Q5); Spark v0 mirrors that. The TLS schema knob has Aegis (Phase 2) as its target. | Aegis, Phase 2. |
| Multi-tenant attribute derivation per-request | The application owns per-request tenancy; `SparkConfig` is set once at process startup. Multi-tenant per-request attribution is Aegis's domain. | Aegis, Phase 2. |

---

## Risks Morgan needs to know about (DESIGN risk register input)

| Risk | Probability | Impact | Surface to Morgan |
|---|---|---|---|
| `opentelemetry-rust` API breaking change before Phase 1 (`opentelemetry-otlp` is not yet 1.0; minor versions can break) | Medium | Medium | Pin `opentelemetry-otlp` to a single minor version compatible with `opentelemetry-proto =0.27.0`. DESIGN ADR documents the pin and the migration path. |
| Spark's required-attribute lint diverges from OTel semantic conventions over time | Low | Medium | The v0 lint covers `service.name` (always) and `tenant.id` (opt-in). Both names match the OTel semconv repository's published names at the harness's pinned spec version. Drift detection is Codex's job in Phase 0+; v0's contract is the inline check matches the names locked here. |
| `tenant.id`'s opt-in posture wrong for future Aegis integration | Low | Medium | The opt-in builder method is additive — Aegis will introduce a `SparkConfig::with_aegis(aegis_handle)` method that internally sets `require_tenant_id()`. The v0 API is forward-compatible. |
| `feature_flag.*` namespace prefix misaligned with OTel semconv | Low | Low | OTel semconv 1.27 has `feature_flags.*` (singular) vs Spark's `feature_flag.*` (singular). Spark v0 uses singular `feature_flag.*` to match the Kaleidoscope architecture document and the roadmap C.2 verbiage. If OTel semconv stabilises on a different prefix at the pinned version, DESIGN flags it as a Codex-Phase-0+1 concern, not a Spark-v0 break. |
| `Drop`-flush blocks the application's exit longer than 5 s in production | Medium | Low | The default is operator-tunable via `with_flush_timeout`. UAT scenarios cover both the clean-flush and timeout-exceeded paths. |
| Spark's silent config-precedence (env vars override defaults but not explicit config) is opaque to the application author | Low | Low | DESIGN-wave decision: log the resolved configuration to `tracing` at `init` time so the application sees what Spark chose. The vocabulary of those events is locked in `shared-artifacts-registry.md > spark_log_event_vocabulary`. |
| Calling `spark::init` twice produces undefined behaviour | Low | Medium | D7 names the `GlobalAlreadyInitialised` variant. Defended by a unit test. |
| Aperture v0.1.0 changes wire shape between Spark v0 lock-time and Phase 1 ship | Low | Medium | Spark consumes only the OTel SDK + OTLP wire, not Aperture-specific APIs; the wire is locked at `opentelemetry-proto =0.27.0` and the harness corpus defends it. |
| `opentelemetry-rust` and `opentelemetry-otlp` MSRV diverge from Kaleidoscope's `rust-version = "1.88"` | Low | Low | Workspace MSRV is already at 1.88; `opentelemetry-otlp` 0.27 supports MSRV 1.75. No conflict at v0 lock-time. |

---

## Discoverability — references for Morgan

Files Morgan should read before starting DESIGN, in order:

| File | Why |
|---|---|
| `wave-decisions.md` (this file) | The locked decisions DESIGN starts from. |
| `journey-spark.yaml` | The structured contract Morgan's ADRs must defend. |
| `journey-spark-visual.md` | The wire-level picture; mockups of every entry point and `tracing` line. |
| `journey-spark.feature` | The full Gherkin scenario set. |
| `user-stories.md` | The 6 user stories with embedded acceptance criteria. |
| `outcome-kpis.md` | The 6 KPIs Morgan must keep measurable in DESIGN's choices. |
| `shared-artifacts-registry.md` | Every `${variable}`'s source of truth and consumer list. |
| `dor-validation.md` | The 9-item gate, passed for every story with evidence. |
| `slices/slice-*.md` | The 6 thin end-to-end slices, each with its own demo command and acceptance summary. |
| `crates/aperture/src/lib.rs` | Spark's integration target: the `aperture::spawn`/`Handle`/`testing::RecordingSink` surface Spark's tests will drive against. |
| `crates/aperture/Cargo.toml` | Workspace conventions and dependency posture. |
| `crates/otlp-conformance-harness/src/lib.rs` | The harness public API; informational only — Spark does NOT depend on the harness directly (the harness is the consumer-side validator; Spark is the producer). |
| `docs/feature/aperture/discuss/wave-decisions.md` | Aperture's locked DISCUSS decisions for Morgan's reuse / consistency reasoning. |
| `docs/feature/aperture/discuss/user-stories.md` | LeanUX template precedent and Elevator Pitch shape. |

---

## What Morgan owns next (DESIGN)

The DESIGN wave's job is to lock the **technology** and **internal crate structure**, not to re-litigate the contract. Concretely:

1. The exact `SparkConfig` builder method set (names, parameter types, `#[must_use]` / `#[non_exhaustive]`).
2. The exact `SparkError` variants (with `thiserror` derive vs hand-rolled `Display`).
3. The exact `SparkGuard` shape (`Drop` impl, fields, debug output).
4. The `opentelemetry-otlp` minor version pin and the rationale (an ADR mirroring harness ADR-0003 in style).
5. The internal module split of the `crates/spark/` crate (likely `config.rs`, `error.rs`, `guard.rs`, `init.rs`, `lib.rs`).
6. The `tracing` facade integration (Spark's own diagnostic events): which target, which level, which fields.
7. The `Resource`-builder mechanism: how house attributes are merged with the application's own attributes (precedence: application's explicit Resource > Spark's house attributes > OTel SDK defaults).
8. The CI workflow contract (mirrors harness ADR-0005 and Aperture's): five gates including 100% mutation kill rate per ADR-0005 of the harness.
9. ADRs for any of the above whose alternatives have material trade-offs.

Anything DESIGN decides that requires changing a DISCUSS contract (a story, an AC, a KPI, an event name) flows back via `design/upstream-changes.md` (only if needed). DISCUSS contracts are otherwise frozen.

---

## Handoff to DISTILL

Recipient: `nw-acceptance-designer`. The acceptance designer turns the BDD scenarios in `discuss/user-stories.md` and `discuss/journey-spark.yaml` into executable Cargo tests against the public surface defined above. No new requirements are introduced by DESIGN; the DESIGN-wave output crystallises *how* the v0 contract is shaped without changing *what* the contract is.

Required reading order for DISTILL is the same as DESIGN above, plus:
- `docs/feature/spark/design/wave-decisions.md` (whatever Morgan locks).
- The five (or so) DESIGN-wave ADRs Morgan produces.

The DISTILL author follows Aperture's posture: real Aperture instances at ephemeral loopback ports, `RecordingSink` to capture export traffic, no InMemory transports for the integration tests.

## Handoff to DEVOPS

Recipient: `nw-platform-architect`. Receives:
- `docs/feature/spark/discuss/outcome-kpis.md` — the six KPIs with measurement plans.
- DESIGN ADR for CI contract — the gates and exit conditions.
- The mutation-testing pattern Aperture inherited (`gate-5-mutants-aperture` in `.github/workflows/ci.yml`); the platform architect produces `gate-5-mutants-spark`.

The platform architect chooses the workflow runner specifics; the contract gates are runner-agnostic and must all pass on every commit affecting `crates/spark/**`.

---

## Missing-DIVERGE note

This DISCUSS wave executed without prior `docs/feature/spark/diverge/recommendation.md` or `job-analysis.md`. Per Bea's brief, no DIVERGE wave was run; the architecture document, the roadmap (C.2 + Phase 0 deliverables), the harness's DISCUSS posture, Aperture's DISCUSS posture, and Bea's locked configuration substituted. The brief documents this as deliberate — Spark's architectural posture is settled enough that JTBD analysis would not have surfaced new motivations.

The DESIGN wave should treat this `wave-decisions.md` plus the prior architecture / roadmap / Aperture artefacts as the upstream context.

---

## Definition-of-Ready status

All six user stories have passed the 9-item Definition of Ready hard gate. Evidence is in `dor-validation.md`. Peer review next.

## Next-step instruction (for Bea)

Invoke `nw-product-owner-reviewer` (Sentinel) against `docs/feature/spark/discuss/`. After review approval (max 2 iterations per the skill), proceed with handoff to DESIGN (`nw-solution-architect` / Morgan) and prepare the DISTILL handoff package for `acceptance-designer` (Atlas).

The chain Bea follows from here: Luna → Sentinel → Morgan → Atlas → Scholar → Sentinel → Apex → Forge → Crafty → Crafty (review).

Vai.
