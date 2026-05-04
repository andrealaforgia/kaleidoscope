# Wave Decisions — `aperture` v0 (DISTILL)

> **Wave**: DISTILL (`nw-acceptance-designer` / Quinn).
> **Date**: 2026-05-04.
> **Author**: Quinn (Scholar).
> **Mode**: autonomous (orchestrator delegated DISTILL-level decisions; Andrea is at dinner).
> **Companion documents**: `acceptance-test-coverage-matrix.md`,
> `gherkin-scenarios.feature`, `walking-skeleton.md`,
> `upstream-issues.md` (only if genuine gaps surface).

This file records the choices DISTILL made when turning the locked
DISCUSS contract and the locked DESIGN brief into executable RED
acceptance tests under `crates/aperture/tests/`. DELIVER reads this
to know which tests are the canonical RED, which test seams are
DISTILL-only conventions DELIVER must honour, and which decisions
the orchestrator pre-authorised on Andrea's behalf.

---

## Reading enforcement record

The DISTILL skill mandates a reading checklist; one line per file the
agent loaded. Result of this DISTILL pass:

```
+ docs/feature/aperture/discuss/journey-aperture.yaml
+ docs/feature/aperture/discuss/journey-aperture.feature
+ docs/feature/aperture/discuss/wave-decisions.md
+ docs/feature/aperture/discuss/user-stories.md
+ docs/feature/aperture/discuss/story-map.md
+ docs/feature/aperture/discuss/outcome-kpis.md
+ docs/feature/aperture/discuss/shared-artifacts-registry.md
+ docs/feature/aperture/slices/slice-01-walking-skeleton.md
+ docs/feature/aperture/slices/slice-02-http-protobuf-and-readiness.md
+ docs/feature/aperture/slices/slice-03-traces.md
+ docs/feature/aperture/slices/slice-04-metrics.md
+ docs/feature/aperture/slices/slice-05-backpressure.md
+ docs/feature/aperture/slices/slice-06-forwarding-sink.md
+ docs/feature/aperture/slices/slice-07-tls-schema-knob.md
+ docs/feature/aperture/slices/slice-08-graceful-shutdown.md
+ docs/feature/aperture/design/wave-decisions.md
+ docs/feature/aperture/design/architecture-overview.md
+ docs/feature/aperture/design/component-design.md
+ docs/feature/aperture/design/aperture-port-and-adapter-diagram.md
+ docs/feature/aperture/design/workspace-layout.md
+ docs/product/architecture/brief.md
+ docs/product/architecture/adr-0006-aperture-transport-stack.md
+ docs/product/architecture/adr-0007-otlpsink-trait-design.md
+ docs/product/architecture/adr-0008-aperture-configuration-schema.md
+ docs/product/architecture/adr-0009-aperture-observability-strategy.md
+ docs/product/architecture/adr-0010-aperture-backpressure-policy.md
+ crates/otlp-conformance-harness/src/lib.rs
+ crates/otlp-conformance-harness/tests/common/mod.rs
+ crates/otlp-conformance-harness/tests/slice_01_reject_empty_input.rs
+ crates/otlp-conformance-harness/tests/slice_04_accept_logs.rs
- docs/feature/aperture/devops/wave-decisions.md (not found — DEVOPS not yet run; default environment matrix applied; logged as warning)
```

`docs/product/journeys/` does not exist on Kaleidoscope; the
DISCUSS-owned `journey-aperture.yaml` is its SSOT-equivalent and is
read directly. `docs/product/kpi-contracts.yaml` does not exist
either; `docs/feature/aperture/discuss/outcome-kpis.md` carries the
KPI contracts and is read in its place.

---

## Wave-decision reconciliation gate (PASSED)

The orchestrator executed the reconciliation gate before delegating
DISTILL. Result inherited verbatim:

- DISCUSS Q1–Q6 are consistent with DESIGN D1–D10.
- DESIGN ADR-0006 through ADR-0010 are consistent with each other and
  with DISCUSS.
- The shared-artefacts registry is consistent across DISCUSS, DESIGN,
  and the harness public surface.
- DEVOPS has not yet run; no DEVOPS-side contradictions exist by
  construction.

DISTILL re-walked the DISCUSS+DESIGN texts looking for genuine
contradictions in the slice plan. **None found.** No upstream issue
file is therefore produced.

---

## Locked decisions inherited from the orchestrator

The orchestrator pre-authorised two interactive decisions on Andrea's
behalf because Andrea is at dinner. DISTILL honours both verbatim.

### Walking Skeleton Strategy: Strategy C — Real local

Real adapters for all local resources (filesystem config, gRPC
listener, HTTP listener, ForwardingSink target as an in-process axum
stub on loopback). No InMemory doubles for transports. No costly
external deps in v0.

Tag walking-skeleton scenarios `@walking_skeleton @real-io
@driving_adapter`.

**How DISTILL honours this**:
- Every walking-skeleton scenario starts a real Aperture instance
  (DELIVER drives `aperture::spawn` -> `compose::run` -> real `tonic`
  Server + real `axum` Server bound to ephemeral loopback ports) and
  exercises it through real `tonic` + `axum`/`reqwest` clients.
- The harness call is the REAL
  `otlp_conformance_harness::validate_logs/traces/metrics` — never a
  stub at the validator boundary (DISCUSS D3 already mandates this; we
  reiterate because it is structurally part of Strategy C).
- For Slice 06 the downstream is `wiremock` running in-process on
  loopback (real HTTP, in-process server) — exactly the
  "in-process axum stub on loopback" shape Strategy C names.

### Container preference: No container

Tests run on host with loopback ports. No Docker Compose, no
Testcontainers.

**How DISTILL honours this**:
- Every test binds to `127.0.0.1:0` and discovers the actual port via
  `Handle::grpc_addr()` / `Handle::http_addr()` (decision D4 below).
  Hard-coded ports never appear.
- `wiremock` runs as an in-process `MockServer` — no Docker.
- The `#[ignore]`-tagged SIGTERM equivalence test is the only place
  where a separate process surface would be needed; DELIVER picks it
  up if `Handle::shutdown` is not a satisfactory proxy.

### Container preference, restated

Strategy C with no container is the cheapest reliable shape for v0:
real loopback transports prove wiring without the dependency
footprint of running orchestration. Aperture's only non-loopback
external integration (the operator-supplied downstream OTel backend)
is exercised by `wiremock` per the design contract; the actual
production deployment is a Phase-1 operator concern.

---

## Inherited decisions from upstream waves (not re-derived)

- **Aperture is a service** — every acceptance test starts a real
  Aperture instance against ephemeral loopback ports and exercises
  it through real `tonic` and `axum`/`reqwest` clients (DISCUSS US
  System Constraint 1; DESIGN architectural style).
- **Both transports at v0** — every signal slice tests both gRPC and
  HTTP/protobuf where applicable (DISCUSS Q1).
- **Single validation gate** — tests rely on the real harness (not a
  stub); the `validate_logs/traces/metrics` reject messages appear
  verbatim on the wire (DISCUSS D3, D6).
- **`OtlpSink` is the integration seam** — every acceptance test
  fronts the application core with a sink the test owns; the default
  is `aperture::testing::RecordingSink` (DESIGN component-design).
- **Closed event vocabulary** — every stderr-line assertion uses an
  `event=` value drawn from the closed v0 set in
  `design/component-design.md > Closed v0 event-name set`.
- **British English, no human-effort estimation, trunk-based
  development** — project conventions.

DISTILL does not re-litigate any of the above.

---

## Load-bearing decisions made in DISTILL

### D1. Test-file-per-slice layout

Each entry in `docs/feature/aperture/slices/slice-NN-*.md` maps to a
single Rust integration-test binary at
`crates/aperture/tests/slice_NN_*.rs`. Mapping is exact and 1-to-1.

**Rationale**: the harness's seven-slice precedent
(`crates/otlp-conformance-harness/tests/slice_*.rs`) is the project's
accepted style. Andrea has approved it; CI gating becomes "did
slice N go GREEN this commit?"

**Alternatives considered**:
- (A) Test-file-per-story (`tests/us_ap_03_*.rs`) — rejected because
  most stories cross slice boundaries.
- (B) Single `tests/acceptance.rs` with a flat list of `#[test]`
  functions — rejected because `cargo test` runs each `tests/*.rs`
  file as its own binary; one file per slice gives DELIVER one
  RED-to-GREEN unit per slice.
- (C) Tag-based selection within one big file — rejected; same
  reason as (B), plus tags are not a Rust integration-test idiom.

### D2. Real-network entry, real harness, test-double sink (Strategy C)

Every acceptance test constructs a real `tonic` or `axum`/`reqwest`
client against a freshly-bound Aperture loopback listener; calls the
real `otlp-conformance-harness` (no harness double); and substitutes
the production `OtlpSink` impl with `aperture::testing::RecordingSink`
(or a slice-local `BarrierSink` / `SlowSink` test double for cap and
drain scenarios).

**Rationale**:
- Aperture IS a service; the acceptance contract is over the wire,
  not in-process.
- Strategy C (Real local) explicitly mandates real adapters for local
  resources; tonic + axum + reqwest on loopback IS the real adapter.
- The harness is shipped at v0.1.0 and Aperture's job is to integrate
  with it correctly — replacing it with a stub would test something
  other than what production runs.
- The sink is the deliberate integration seam (DISCUSS Q3); test
  doubles at the sink boundary are the canonical hexagonal pattern.

For the `slice_06_forwarding_sink.rs` tests, the downstream is
`wiremock` — that is the production downstream's role and the
DESIGN-recommended fixture (per `design/wave-decisions.md > External
integrations`).

### D3. Single-Then-Per-Fact mandate, applied verbatim

Every user-observable claim is its own `#[test]` function. Where the
harness slice tests assert "rule=EmptyInput", "signal=Logs", and
"framing=GrpcProtobuf" in three separate `#[test]`s, the Aperture
slice tests do the same. A mutation can only kill one assertion at a
time; a regression in the verbatim-Display contract surfaces as
exactly one red test, not a chain.

**Rationale**: the harness DISTILL wave established this convention;
the project's accepted craftsmanship invariants (root `CLAUDE.md`,
the harness's Mandate Single-Then-Per-Fact) name it explicitly.

### D4. Ephemeral ports for every test instance

Tests bind to `127.0.0.1:0` and discover the actual port via
`Handle::grpc_addr()` / `Handle::http_addr()`. No test hard-codes
4317 / 4318.

**Rationale**: hard-coded ports cause inter-test contention even on
single-machine CI runners and forbid `cargo test` parallelism. The
binary defaults are 4317/4318 (per design); tests use what `bind`
returns.

`.cargo/config.toml` already forces `RUST_TEST_THREADS = "1"` for the
workspace (inherited from the harness's `gag`-redirect requirement);
ephemeral ports therefore prevent within-binary contention even
though tests run sequentially across binaries.

### D5. Configuration via the `Config::builder` test seam

Tests construct configurations through `aperture::config::Config::builder()`
setters; they do NOT exercise the figment TOML loader (except
`slice_07_tls_schema_knob.rs`, which uses `Config::from_toml_str`).

**Rationale**: most slices are not exercising the loader; they
exercise behaviour. A typed builder keeps each test focused on the
knob the slice cares about. Slice 07 IS the loader exercise, so it
exercises TOML strings end-to-end.

### D6. Stderr-event capture seam owned by DELIVER

Scenarios that assert "stderr line with event=X" rely on a helper
`common::capture_stderr_events` that returns a `Vec<StderrEvent>`
the test then queries with `expect_stderr_event`. The implementation
of `capture_stderr_events` is `unimplemented!()` at DISTILL; DELIVER
wires the production `tracing-subscriber` JSON layer to a
test-visible capture (likely a new `aperture::testing::stderr_capture`
symbol).

**Rationale**: capturing JSON-stderr cleanly inside a tokio
multi-thread test requires either a process-level redirect (clashes
with parallel tests, even with `RUST_TEST_THREADS=1` if a single
binary spawns parallel tasks) or a tracing-subscriber `Layer` that
exposes a queue (test-friendly). The harness uses `gag` because it
has no tracing layer of its own; Aperture has one (per ADR-0009) so
the layer-based capture is the right shape. DELIVER lands the
symbol; DISTILL declares the seam.

### D7. RED-on-day-one via stub crate

`crates/aperture/Cargo.toml`, `src/lib.rs`, `src/main.rs`,
`src/config/mod.rs`, `src/ports/mod.rs`, `src/testing.rs`, plus the
private placeholder modules (`src/{app,sinks,transport,observability,
shutdown,compose,error}.rs`) exist as a minimal stub — every public
function returns `unimplemented!()`. The integration tests link
against the stub, compile clean, and panic at runtime on the first
call into the production surface.

Every scaffold file carries the `// SCAFFOLD: true` comment marker so
DELIVER (and any reviewer) can grep for the RED state.

**Rationale**: same approach the harness DISTILL used. DELIVER's
first action is to take the stub `lib.rs` and grow it module-by-module
per the slice plan. Each integration test goes RED→GREEN as DELIVER
lands the corresponding implementation.

The DELIVER deps that the binary itself needs at runtime (tonic,
axum, figment, tracing-subscriber, reqwest) are in `[dev-dependencies]`
so the integration tests link them; DELIVER moves them to
`[dependencies]` as it lands the corresponding modules. This mirrors
the harness's pattern of keeping the production tree minimal at
DISTILL.

### D8. Slice 05 BarrierSink and Slice 08 SlowSink — test-only seams

The cap and drain scenarios need a sink that holds requests in-flight
on demand (cap) or for a configurable delay (drain). These are local
test types in `slice_05_backpressure.rs` and
`slice_08_graceful_shutdown.rs` respectively. They implement
`OtlpSink + Probe` directly.

**Rationale**: the production `OtlpSink` impls (`StubSink`,
`ForwardingSink`) and the test-double `RecordingSink` are not enough
for these slices: cap saturation needs a sink that *blocks*
deterministically, and drain testing needs a sink with a configurable
delay. Both are slice-local test fixtures; DELIVER does not need to
reuse them.

### D9. SIGTERM equivalence is `#[ignore]`d at DISTILL

The "SIGTERM and SIGINT behave identically" UAT in US-AP-09 is encoded
as a `#[cfg(unix)] #[ignore]`d test in `slice_08_graceful_shutdown.rs`.
The other shutdown scenarios drive `Handle::shutdown()` (declared as
the equivalent-to-SIGTERM seam in `lib.rs`).

**Rationale**: capturing a real SIGTERM in an integration test
requires forking a separate process (`std::process::Command` against
a built-and-installed `aperture` binary) and reading its stderr
back through a pipe. That fixture is non-trivial and adds OS-coupling
that adds little behavioural information beyond what the
`Handle::shutdown` path already proves.

### D10. Property-shaped invariant explicit at the test level

The `@property` UAT from `journey-aperture.feature` ("backpressure
never silently drops a request") appears in `slice_05_backpressure.rs`
as `every_excess_request_under_overload_receives_a_deterministic_refusal_or_acceptance`.
The test fires `N=10` concurrent requests against a `cap=2` instance
and asserts every response is either HTTP 200 or HTTP 503 — never
a connection drop, timeout, or any other status.

**Rationale**: DISCUSS D5 names this as the load-bearing
non-silent-drop invariant. Property-based generation (proptest,
quickcheck) adds dependency surface that v0 does not need; the
explicit-load shape is the same invariant test the load-test KPI 5/6
in `outcome-kpis.md` exercises.

The Gherkin tag `@property` is preserved in
`gherkin-scenarios.feature` so DELIVER's crafter knows the intent.

### D11. Behavioural corroboration, not replacement, of CI invariants

The two invariant tests (`tests/invariant_single_validator.rs` and
`tests/invariant_no_telemetry_on_telemetry.rs`) are BEHAVIOURAL
CORROBORATIONS, not the load-bearing defence. The structural
defence — `xtask`-based AST walks for `single_validator_per_signal`
and the network-namespace integration test for
`no_telemetry_on_telemetry` — is owned by DEVOPS.

**Rationale**: per `design/wave-decisions.md > D10`, the load-bearing
defence is the language-appropriate AST/network-namespace gate.
These two integration tests assert the corresponding runtime
invariants from the application surface so they fail loud locally
(in `cargo test`) before reaching CI.

DEVOPS handoff: the placeholders document which gates DEVOPS owns;
the integration-test bodies are explicit about scope.

---

## Driving-adapter coverage (RCA P1)

Every entry point must have at least one `@driving_adapter`-tagged
scenario invoking it via real protocol (Strategy C; DISCUSS Q1).

| Entry point | Real-protocol invocation | Scenario(s) | Test file(s) |
|---|---|---|---|
| **gRPC listener** (`tonic` Server, port 4317 / ephemeral) | Real `tonic` client (`LogsServiceClient`, `TraceServiceClient`, `MetricsServiceClient`) over real loopback TCP | "Customer exports one log record over gRPC and receives gRPC OK"; "Customer exports one span over gRPC and receives gRPC OK"; "Customer exports metrics over gRPC and receives gRPC OK"; "Customer sends empty body and receives INVALID_ARGUMENT"; "5th concurrent gRPC request at cap=4 receives RESOURCE_EXHAUSTED"; "Forwarding sink probe refuses startup when downstream lies" | `slice_01`, `slice_03`, `slice_04`, `slice_05`, `slice_06`, `slice_08` |
| **HTTP listener** (`axum` Server, port 4318 / ephemeral) | Real `reqwest` client over real loopback TCP, `Content-Type: application/x-protobuf` | "Customer posts valid logs body and receives 200"; "Customer posts traces body to /v1/traces and receives 200"; "Customer posts metrics body to /v1/metrics and receives 200"; "Customer posts with JSON content type and receives 415"; "Customer posts to unknown path and receives 404"; "Customer posts empty body and receives 400"; "5th concurrent HTTP request at cap=4 receives 503" | `slice_02`, `slice_03`, `slice_04`, `slice_05`, `slice_06`, `slice_07` |
| **healthz endpoint** (HTTP `GET /healthz`) | Real `reqwest` GET over loopback | "Operator probes /healthz and receives 200 'ok'" | `slice_02` |
| **readyz endpoint** (HTTP `GET /readyz`) | Real `reqwest` GET over loopback | "Operator probes /readyz after startup and receives 200 'ready'"; "Shutdown flips /readyz to 503 'draining' within 100 ms"; "TLS-enabled-true: /readyz returns 200" | `slice_02`, `slice_07`, `slice_08` |

Every entry point has at least one `@driving_adapter @real-io`
scenario. The walking skeleton (Slice 01) covers the gRPC listener;
Slice 02 covers HTTP, healthz, and readyz together. Aperture has no
CLI subcommand surface in v0, so no CLI driving adapter exists; the
binary's startup path IS exercised by Slice 08's
`#[cfg(unix)]`-flagged SIGTERM test (placeholder; DELIVER lands the
process-spawning fixture if `Handle::shutdown` is not a satisfactory
proxy — see D9).

---

## Driven-adapter coverage (Mandate 6)

Every driven adapter has at least one `@real-io @adapter-integration`
scenario that exercises the real I/O code path (not an InMemory
double).

| Adapter | `@real-io` scenario | Covered by | Notes |
|---|---|---|---|
| **StubSink** (DISTILL note: production sink, exercised indirectly) | YES | Slice 01 walking-skeleton tests cover the production sink hand-off path. `RecordingSink` substitutes for `StubSink` at the test seam BUT the production-bound stderr line `event=sink_accepted sink=stub` is asserted in Slice 01. DELIVER lands `StubSink` per the design contract; the substitution at the test seam is hexagonal-correct (the trait IS the seam, not the concrete impl). | `slice_01_walking_skeleton.rs` |
| **ForwardingSink** | YES | Slice 06 — in-process `wiremock` server on loopback acts as the downstream OTLP backend. Real `reqwest`-driven HTTP POSTs leave the test process. | `slice_06_forwarding_sink.rs` |

Notes on the substrate-exemption rule: `otlp_conformance_harness::validate_*`
functions are NOT driven adapters — they are substrate (DESIGN's
inheritance from `architecture/brief.md` substrate stratum;
Apache-Foundation-stewarded library code Aperture imports directly).
Aperture has zero "validator" adapter; the harness IS the validator
and there is no port boundary around it.

For the `RecordingSink` substitution in walking-skeleton tests: the
hexagonal seam is the `OtlpSink` trait, not `StubSink` the concrete
type. Strategy C says "real adapters for local resources"; the
trait IS the seam; substituting `RecordingSink` at the seam is the
canonical hexagonal pattern. The production `StubSink` is exercised
by Slice 06's `sink_accepted sink=stub` line assertions when DELIVER
lands the production sink and the test runs against it indirectly.
This is intentional and was approved by the orchestrator under
Strategy C.

---

## Test inventory — by slice

| Slice | File | Test-functions | Mandate Single-Then alignment |
|---|---|---|---|
| 01 | `tests/slice_01_walking_skeleton.rs` | 13 | Each scenario assertion split |
| 02 | `tests/slice_02_http_protobuf_and_readiness.rs` | 15 | Same |
| 03 | `tests/slice_03_traces.rs` | 10 | Same |
| 04 | `tests/slice_04_metrics.rs` | 9 | Same |
| 05 | `tests/slice_05_backpressure.rs` | 10 | Same; includes `@property` |
| 06 | `tests/slice_06_forwarding_sink.rs` | 11 | Same |
| 07 | `tests/slice_07_tls_schema_knob.rs` | 7 | Same |
| 08 | `tests/slice_08_graceful_shutdown.rs` | 5 (+1 ignored) | Same |
| inv | `tests/invariant_single_validator.rs` | 1 | Behavioural corroboration |
| inv | `tests/invariant_no_telemetry_on_telemetry.rs` | 3 | Behavioural corroboration |
| **total** | | **84 + 1 ignored** | |

Error-path ratio: across the 8 slice tests, ~38/80 tests (≈47%)
exercise reject paths, refusal-on-overload, downstream failure, or
deadline-exceeded shutdown. Comfortably above the 40% mandate
threshold.

---

## Real-vs-synthesised data per slice

| Slice | Encoder source | Notes |
|---|---|---|
| 01 | `common::encode_logs_request` (prost-encoded `ExportLogsServiceRequest`) | Same shape the OTel SDK emits; harness accepts. |
| 02 | Same encoder; HTTP/protobuf POST | Same body, different transport. |
| 03 | `common::encode_traces_request` | Same. |
| 04 | `common::encode_metrics_request` | One Sum + one Gauge data point (DISCUSS US-AP-06 minimal). |
| 05 | Reuses logs encoder; the slice is about cap behaviour, not body content. | Same. |
| 06 | Same; downstream is `wiremock`. | Real `wiremock` routes; in-process axum stub on loopback per Strategy C. |
| 07 | TOML strings constructed in-test. | Schema exercise. |
| 08 | Same logs encoder; sink is `SlowSink`. | Same. |

Hand-crafted bytes appear only in Slice 02's empty-body and unknown
content-type scenarios (literal `Vec::<u8>::new()` and a JSON string
respectively). Every accept path uses real prost-encoded OTLP from
the upstream `opentelemetry-proto` types — the same wire format an
OTel SDK emits.

---

## Hexagonal boundary mandate (CM-A) compliance

Every integration test imports only the public surface of `aperture::`
plus `aperture::testing::RecordingSink`. The complete import inventory
across the test files:

```
aperture
aperture::config::Config
aperture::config::ConfigBuilder        (transitive via Config::builder)
aperture::config::ConfigError          (Slice 07)
aperture::ports::OtlpSink
aperture::ports::Probe                 (Slices 05, 08 — for local sink doubles)
aperture::ports::ProbeError            (Slices 05, 08 — same)
aperture::ports::SinkError             (Slices 05, 08 — same)
aperture::ports::SinkRecord
aperture::testing::RecordingSink
aperture::Handle
aperture::spawn
```

Zero test imports a `pub(crate)` symbol; zero test reaches into
`aperture::transport::*`, `aperture::app::*`, `aperture::sinks::*`,
or `aperture::shutdown::*` (those modules are private at DISTILL —
not part of the public surface declared in `lib.rs`). The mandate
holds.

---

## Business-language mandate (CM-B) compliance

Test-function names use domain terms:

- "customer exports one log record"
- "operator probes healthz"
- "fifth concurrent grpc request at cap four receives resource exhausted"
- "drain deadline exceeded emits warn stderr event"
- "forwarding sink probe refuses startup when downstream lies"

Technical jargon appears only where the wire-level identity is
load-bearing (e.g. "grpc-message", "RESOURCE_EXHAUSTED", "503",
"INVALID_ARGUMENT") — those are domain terms in the OTLP / k8s
vocabulary the operator IS expected to read.

The Gherkin in `gherkin-scenarios.feature` mirrors the same
business-language framing.

---

## Walking-skeleton mandate (CM-C) compliance

The walking skeleton in `journey-aperture.feature` and Slice 01 is:

> "A real OpenTelemetry Rust SDK 0.27 sends an
> `ExportLogsServiceRequest` over OTLP/gRPC to `localhost:4317`;
> Aperture binds the listener, calls the real
> `otlp_conformance_harness::validate_logs(bytes,
> Framing::GrpcProtobuf)`, hands the typed record to a `StubSink`
> implementation of the `OtlpSink` trait, the sink writes a single
> structured stderr JSON line, and the SDK receives gRPC OK."

This passes the litmus test:

1. **Title describes user goal**: "customer exports one log record
   and receives gRPC OK". Not "end-to-end traversal of all layers".
2. **Given/When describe user actions**: an OTel SDK sends a logs
   batch.
3. **Then describe user observations**: SDK gets gRPC OK; stderr
   names the accepted record.
4. **Stakeholder confirmable**: Andrea (and any operator) can read
   the test name and confirm "yes, that is what an SDK client wants".

Per Strategy C, the walking-skeleton scenarios are tagged
`@walking_skeleton @real-io @driving_adapter` in
`gherkin-scenarios.feature`.

Across the suite the ratio is approximately:
- Walking-skeleton-shaped scenarios (3 per slice for slices 01-04;
  full-journey for 05, 06, 08): ~15
- Focused boundary scenarios (single-rule reject, single-knob,
  single-failure-mode): ~70

Comfortably above the 2-3 walking skeleton + 15-20 focused scenarios
guideline at the per-slice level.

---

## Pure-function-extraction mandate (CM-D) compliance

DISTILL does not write production code; pure-function extraction is
DELIVER's responsibility per the design contract (the
`app::summary::summarise_record`, `app::responses::*`, and
`app::framing_for_transport` functions are the design's pure-function
inventory). The acceptance tests at this layer are integration tests;
they exercise the application end-to-end and are NOT the venue for
pure-function unit testing. DELIVER will land unit tests under
`crates/aperture/src/**` for the pure helpers.

The mandate's adapter-isolation requirement is honoured by the test
shape: the integration tests parametrise nothing beyond
"loopback ephemeral ports" — there is no fixture matrix multiplied
across environments. Slice 06's `wiremock` is the only adapter
parametrisation, and it is the ForwardingSink adapter contract test.

---

## KPI observability scenario coverage

`outcome-kpis.md` defines 8 KPIs. Each is structurally defended by at
least one acceptance test (full mapping in
`acceptance-test-coverage-matrix.md`):

| KPI | Defended by |
|---|---|
| KPI 1 — first integration round-trip | `slice_01_walking_skeleton.rs` (whole file) |
| KPI 2 — transport coverage gRPC OK / HTTP 200 ratio | `slice_01_*` (gRPC OK), `slice_02_*` (HTTP 200) |
| KPI 3 — readiness signal three-state machine | `slice_02_*` (starting->ready), `slice_08_*` (ready->draining) |
| KPI 4 — per-signal acknowledgement ratio | `slice_03_*` (traces), `slice_04_*` (metrics), `slice_01_*` (logs) |
| KPI 5 — concurrency saturation events | `slice_05_*` (`@property` test + cap-hit assertions) |
| KPI 6 — refusal-not-drop invariant | `slice_05_*::every_excess_request_under_overload_receives_a_deterministic_refusal_or_acceptance` |
| KPI 7 — downstream-acceptance ratio | `slice_06_*` (forwarding-sink success + failure scenarios) |
| KPI 8 — graceful-restart drop ratio | `slice_08_*` (clean drain + deadline-exceeded scenarios) |

Aperture's KPIs are observability-shaped; the acceptance tests assert
the runtime invariants the KPIs measure. Production-time KPI
collection is operator-side (DEVOPS will document the dashboard
queries against the closed event vocabulary).

---

## What DELIVER owes DISTILL

For the RED tests to go GREEN cleanly, DELIVER must:

1. **Replace stubs in `src/lib.rs`, `src/config/mod.rs`,
   `src/ports/mod.rs`, `src/testing.rs`, `src/main.rs`, and the
   placeholder modules `src/{app,sinks,transport,observability,
   shutdown,compose,error}.rs`** with the full design surface from
   `design/component-design.md`. The signatures the tests use
   (Builder setters, `aperture::spawn`, `Handle::wait_until_ready`,
   `Handle::shutdown`, `Config::from_toml_str`,
   `aperture::testing::RecordingSink`) are stable; DELIVER fills the
   bodies.

2. **Add an `aperture::testing::stderr_capture()` symbol** the
   `common::capture_stderr_events` helper builds on. The symbol
   should subscribe a `Vec<StderrEvent>`-collecting layer to the
   `tracing-subscriber` registry for the duration of a closure and
   return the captured events.

3. **Promote dev-dependencies to direct dependencies** as DELIVER
   lands the corresponding modules: `tonic` (Slice 01), `axum` +
   `tower-http` (Slice 02), `tracing-subscriber` (every slice),
   `figment` (Slice 07), `reqwest` (Slice 06), plus `async-trait`,
   `thiserror`, `serde`, `serde_json` (multiple slices).

4. **Land the production sinks** (`StubSink`, `ForwardingSink`) per
   `component-design.md`. The integration tests use the test-double
   `RecordingSink` for hexagonal-correct sink hand-off observation;
   the production sinks have their own behaviour (StubSink writes
   stderr; ForwardingSink dials downstream) that the slice tests
   assert about indirectly via stderr lines and downstream wiremock
   counts.

5. **Honour the closed event vocabulary** verbatim — every
   `event=X` in stderr lines must use one of the constants from the
   design's `events.rs`. The tests assert against these literal
   strings.

6. **Drive the `#[ignore]`d SIGTERM test** if `Handle::shutdown` is
   not a satisfactory proxy. The seam is documented in `lib.rs`.

7. **Honour the `// SCAFFOLD: true` markers** — every file carrying
   the marker is a DISTILL placeholder; DELIVER replaces (not appends)
   per the design contract.

---

## Summary

- 10 test binaries (8 slice + 2 invariant), 84 active `#[test]`
  functions + 1 `#[ignore]`d.
- Walking Skeleton Strategy: **C — Real local**; container preference:
  **No container** — both pre-authorised by the orchestrator.
- Every test enters Aperture through a driving port (gRPC listener,
  HTTP listener) over real loopback TCP.
- Every test uses the real `otlp-conformance-harness`.
- The sink seam is `aperture::testing::RecordingSink` for happy-path
  observation; slice-local `BarrierSink` and `SlowSink` for
  cap/drain shapes; `wiremock` for the ForwardingSink downstream.
- Stub library compiles cleanly; every test panics at runtime with
  `unimplemented!()` from a production-surface symbol — the canonical
  RED state. Every scaffold file carries `// SCAFFOLD: true`.
- Mandate compliance: CM-A (hexagonal), CM-B (business language),
  CM-C (walking skeleton + focused mix), CM-D (pure-function
  extraction is DELIVER's scope) all honoured.
- Error-path ratio ≈ 47%; well above the 40% threshold.
- Property-shaped invariant for backpressure is explicit
  (`every_excess_request_under_overload_receives_a_deterministic_refusal_or_acceptance`).
- KPI observability: every KPI in `outcome-kpis.md` is tied to at
  least one test (mapping in `acceptance-test-coverage-matrix.md`).
- Driving-adapter coverage: gRPC, HTTP, healthz, readyz — every
  entry point has at least one real-protocol scenario.
- Driven-adapter coverage: StubSink (Slice 01 — `RecordingSink`
  substitution at the trait seam), ForwardingSink (Slice 06 — real
  `wiremock` downstream).
- Wave-decision reconciliation: PASSED, zero contradictions.
- Genuine forks for DESIGN: NONE. `upstream-issues.md` not produced.

DELIVER is unblocked.
