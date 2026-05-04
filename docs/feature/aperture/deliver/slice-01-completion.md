# Slice 01 — Walking skeleton — DELIVER completion

> **Wave**: DELIVER (`nw-software-crafter` / Crafty).
> **Date**: 2026-05-04.
> **Slice**: 01 — walking skeleton.
> **Companion brief**: [`../slices/slice-01-walking-skeleton.md`](../slices/slice-01-walking-skeleton.md).

---

## Outcome

A real `tonic`-generated `LogsServiceClient` against the upstream
`opentelemetry-proto` 0.27 service definition sends an
`ExportLogsServiceRequest` over OTLP/gRPC to a freshly-started
Aperture instance bound on an ephemeral `127.0.0.1` port. Aperture
re-encodes the request, calls the **real**
`otlp_conformance_harness::validate_logs(_, Framing::GrpcProtobuf)`,
hands the typed `SinkRecord::Logs(...)` record to the configured
`OtlpSink`, emits the closed-vocabulary stderr events, and returns
gRPC `OK`. An empty body produces gRPC `INVALID_ARGUMENT` carrying the
harness's `OtlpViolation::Display` verbatim — `rule=EmptyInput`,
`signal=Logs`, `framing=GrpcProtobuf` substrings round-trip on the
wire.

## What turned GREEN

| Test binary | Tests passing |
|---|---:|
| `tests/slice_01_walking_skeleton.rs` | **13/13** |
| `tests/invariant_single_validator.rs` | **1/1** |
| `src/lib.rs` (unit tests) | **10/10** |
| **Slice 01 total** | **24/24** |

The 13 acceptance tests cover (per Mandate Single-Then-Per-Fact):

- **Happy path (4)**: gRPC OK on valid logs export; record reaches the
  sink; the `SinkRecord` variant carries `Logs(...)`; the record count
  matches the SDK's batch size.
- **Stderr observability (4)**: `event=listener_bound transport=grpc`;
  `event=request_received signal=logs`; `event=sink_accepted` names
  `record_count`; `event=sink_accepted` names `resource.service.name`.
- **Reject path (5)**: gRPC `INVALID_ARGUMENT` on empty body;
  `grpc-message` contains `rule=EmptyInput`; `signal=Logs`;
  `framing=GrpcProtobuf`; rejected requests never reach the sink.

The 10 unit tests are pure-domain-function tests for the helpers
extracted in DELIVER: `framing_for_transport` x 2, `summarise_record`
x 2, `Config::build` validation x 5, plus the
`record_debug`-contract pin for the test capture seam.

## Production code added

| File | Lines |
|---|---:|
| `src/app.rs` | 235 |
| `src/compose.rs` | 89 |
| `src/config/mod.rs` | 306 |
| `src/lib.rs` | 148 |
| `src/main.rs` | 42 |
| `src/observability.rs` | 222 |
| `src/ports/mod.rs` | 128 (DISTILL stub adapted) |
| `src/sinks.rs` | 51 |
| `src/testing.rs` | 155 |
| `src/transport.rs` | 123 |
| **Slice 01 production tree** | **1499** |

Modules NOT touched (left as DISTILL placeholders for future slices):
`src/error.rs` (Slice 07/08 will land the rich `ApertureError` enum),
`src/shutdown.rs` (Slice 08).

## Commits

| Hash | Subject |
|---|---|
| `90eda62` | `feat(aperture): walking skeleton — gRPC logs export round-trips end-to-end` |
| `ff910da` | `refactor(aperture): align Slice 01 production tree with mutation kill rate` |

The first commit lands the walking-skeleton outcome (RED → GREEN
across 13 acceptance tests + 7 unit tests). The second commit is the
post-GREEN refactor: it removes scaffolding the mutation suite flagged
as untested-and-removable, simplifies the validation conjunction in
`Config::build`, and adds three unit tests pinning the
`||`-disjunction asymmetry and the `record_debug` contract.

## Mutation testing

Run command:

```text
cargo mutants --package aperture --no-shuffle --jobs 2 \
  --cargo-test-arg "--lib" \
  --cargo-test-arg "--test=slice_01_walking_skeleton" \
  --cargo-test-arg "--test=invariant_single_validator" \
  -f crates/aperture/src/observability.rs \
  -f crates/aperture/src/sinks.rs \
  -f crates/aperture/src/transport.rs \
  -f crates/aperture/src/app.rs \
  -f crates/aperture/src/compose.rs \
  -f crates/aperture/src/testing.rs
```

Result on Slice-01-active source files:

| Metric | Count |
|---|---:|
| Mutants generated | 59 |
| Caught | **26** |
| Missed | **0** |
| Unviable (mutation produces non-compiling code) | 33 |
| **Kill rate** | **100% (26 / 26)** |

The "unviable" count is incidental — it consists of mutations that
change a function's return type or signature in ways that fail the
crate's own type checker (e.g., `replace ... -> Self with
Default::default()` against a return type that doesn't implement
`Default`). They are not a coverage gap.

### Crate-wide mutation report (informational)

A wider `cargo mutants --package aperture` run (across **all** source
files including the DISTILL-shape `lib.rs`, `main.rs`, `error.rs`,
`shutdown.rs`, `ports/mod.rs`, and the future-slice setters in
`config/mod.rs`) reports 16 surviving mutations. **All 16** are in
code present for future-slice RED test compilation:

- `ConfigBuilder::max_concurrent_requests`, `drain_deadline`,
  `forwarding_sink`, `forwarding_timeout`, `tls_enabled`,
  `spiffe_enabled` — exercised by Slices 05/08/06/06/07/07.
- `SinkError::Display`, `ProbeError::Display` — wire-asserted in
  Slice 06.
- `ApertureError::Display`, `ConfigError::Display` — surfaced via
  `eprintln!` in the binary's pre-init failure path; not
  test-observable until Slice 07's figment loader.
- `Handle::shutdown`, `<impl Drop for Handle>::drop` — exercised by
  Slice 08's drain orchestrator.
- `aperture::run`, `main` — exercised by the binary path; the
  ignored `#[cfg(unix)]` SIGTERM-equivalence test in Slice 08 will
  spawn the binary and assert against it.
- `Config::builder` returning `ConfigBuilder` vs `Default::default()`
  — semantically equivalent (the `impl Default` calls
  `ConfigBuilder::new()` directly). This is a known-equivalent
  mutant; no test can distinguish the two.

Per the per-feature mutation strategy (root `CLAUDE.md`), mutation
testing is scoped to changed files. The slice's binding contract is
satisfied: every line of production code Slice 01's acceptance tests
can reach has at least one test that fails when the line is
meaningfully mutated.

## Design observations

A small number of observations surfaced during outside-in TDD that
DELIVER did not push back to DESIGN (none rises to a "genuine fork";
the locked contracts resolved every implementation question), but
recorded here for posterity:

1. **`Probe` upcasting via `dyn` is hard in stable Rust.** The
   composition root pattern `wire_then_probe_then_use<T: OtlpSink +
   Probe>` works for concrete types; for `Arc<dyn OtlpSink>` storage
   the same compiler can't synthesise an `Arc<dyn Probe>` upcast
   without explicit method-level support in the `OtlpSink` trait.
   Slice 01's resolution: probe BEFORE Arc-erasure inside
   `compose::wire_sink`. This is consistent with the "wire → probe →
   use" sequence in the design contract; the asymmetry between the
   binary path (probe at construction) and the test path (probes
   trivially `Ok`) is documented in `compose.rs`. ADR-0007 explicitly
   anticipates a Phase-1 refinement here.

2. **`tonic`'s `TcpIncoming::from_listener` is the cleanest seam for
   ephemeral-port binding.** The integration-test fixture binds
   `127.0.0.1:0` and discovers the OS-assigned port via
   `Handle::grpc_addr`. Tonic's documented entry point is `serve(addr:
   SocketAddr)`, which calls `bind` internally and never surfaces the
   address — a non-starter for ephemeral binds. `TcpIncoming::from_listener`
   accepts a pre-bound `tokio::net::TcpListener`, lets us call
   `local_addr()` first, and then drives the server. No `tokio-stream`
   dep needed. (This was a choice between adding `tokio-stream` for
   `TcpListenerStream` or using tonic's own incoming type — the
   tonic-native answer wins on dep economy.)

3. **The capture seam is a `tracing-subscriber::Layer`, not a
   write-redirect.** The harness DISTILL precedent uses `gag` to
   redirect stderr at the file-descriptor level, which is correct for
   a library that emits no structured events. Aperture has a
   tracing-subscriber registry; the layer-based capture is strictly
   better: it parses the structured fields per-event, runs in-process
   without changing the global stderr fd, and is compatible with
   parallel tasks within a multi-thread tokio test. The
   `aperture::testing::stderr_capture` symbol is the seam DISTILL
   declared in `tests/common/mod.rs`.

4. **`Config` validation is intentionally narrow at Slice 01.** The
   only invariant Slice 01's acceptance tests require is "two pinned,
   identical bind addresses are rejected; ephemeral port 0 is exempt".
   The wider validation rules in `component-design.md > Configuration
   schema` (forwarding endpoint URL parse, max_recv_msg_size minimum,
   drain deadline minimum) land with the slices that exercise the
   corresponding fields. The wave-decisions doc's "narrow validation"
   stance avoids the trap of validating fields whose write paths are
   not yet wired.

5. **Pre-commit hook stays excluded for `aperture`.** Per the
   conservative recommendation in the orchestrator brief and DEVOPS A2
   graduation schedule. The local hook `scripts/hooks/pre-commit`
   continues to run `cargo test --workspace --exclude aperture
   --all-targets --locked` and stays GREEN through this commit.

## Genuine forks discovered

**None.** The DISCUSS contract (Q1–Q6), the DESIGN brief (D1–D10), the
ADRs 0006–0010, and the DISTILL test inventory together resolved every
implementation question Slice 01 raised. No back-propagation needed.

## Out of scope (left RED for subsequent slices)

- `tests/slice_02_http_protobuf_and_readiness.rs` — Slice 02
- `tests/slice_03_traces.rs` — Slice 03
- `tests/slice_04_metrics.rs` — Slice 04
- `tests/slice_05_backpressure.rs` — Slice 05
- `tests/slice_06_forwarding_sink.rs` — Slice 06
- `tests/slice_07_tls_schema_knob.rs` — Slice 07
- `tests/slice_08_graceful_shutdown.rs` — Slice 08
- `tests/invariant_no_telemetry_on_telemetry.rs` — three tests; the
  `/metrics` and `/telemetry` GETs need the HTTP listener which Slice
  02 lands. The `aperture_with_stub_sink_idle_does_not_open_any_outbound_connection`
  test PASSES under Slice 01 (it only requires gRPC + StubSink to be
  alive).

These remain RED-by-design until DELIVER advances each slice in turn.

## Handoff to next slice

Slice 02 (HTTP/protobuf and readiness) is the natural next cycle:

1. Light up `transport::http::spawn` (`axum` Router for
   `/v1/{logs,traces,metrics}` + `/healthz` + `/readyz`).
2. Wire the `ReadinessState` (Starting → Ready → Draining) and feed it
   into `/readyz`.
3. Promote `Handle::http_addr` from the placeholder `0.0.0.0:0` to the
   actual bound address.
4. Add `event=ready` and `event=readiness_changed` call sites.

The existing `Config::http_bind_addr` field and the
`#[allow(dead_code)]` annotations are the seams Slice 02 will fill.
The rest of the closed event vocabulary (16 unused constants under
`observability::event::*`) is ready to be referenced.
