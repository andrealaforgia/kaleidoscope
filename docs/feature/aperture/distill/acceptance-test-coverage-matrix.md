# Acceptance Test Coverage Matrix — `aperture` v0 (DISTILL)

> **Wave**: DISTILL.
> **Author**: Quinn (Scholar).
> **Date**: 2026-05-04.
> **Companion documents**: `wave-decisions.md`, `gherkin-scenarios.feature`,
> `walking-skeleton.md`.

This matrix is the traceability artefact connecting:

- DISCUSS user stories (`discuss/user-stories.md` US-AP-01..US-AP-09)
- DISCUSS slices (`slices/slice-01..slice-08`)
- DISCUSS outcome KPIs (`discuss/outcome-kpis.md` KPI 1..8)
- DISCUSS shared artefacts (`discuss/shared-artifacts-registry.md`)
- DISTILL Gherkin scenarios (`gherkin-scenarios.feature`)
- DISTILL Rust acceptance tests (`crates/aperture/tests/slice_*.rs`)

Every story has at least one acceptance test. Every KPI has at least
one acceptance test exercising the behaviour the KPI measures. Every
driving and driven adapter has at least one real-protocol /
real-I/O scenario.

---

## Story → Slice → KPI → Test mapping

| Story | Slice(s) | Primary KPI(s) | Test file(s) | Test count (approx) |
|---|---|---|---|---|
| US-AP-01 — Bind both OTLP listeners at startup | 01 (gRPC), 02 (HTTP) | KPI 1, KPI 3 | `slice_01_walking_skeleton.rs`, `slice_02_http_protobuf_and_readiness.rs`, `slice_07_tls_schema_knob.rs` | 5 |
| US-AP-02 — HTTP + healthz/readyz | 02 | KPI 2, KPI 3 | `slice_02_http_protobuf_and_readiness.rs` | 8 |
| US-AP-03 — Accept a valid logs export | 01 (gRPC), 02 (HTTP) | KPI 1, KPI 2, KPI 4 | `slice_01_walking_skeleton.rs`, `slice_02_http_protobuf_and_readiness.rs`, `invariant_single_validator.rs` | 9 |
| US-AP-04 — Reject malformed input | 01 (gRPC), 02 (HTTP) | KPI 4 | `slice_01_walking_skeleton.rs`, `slice_02_http_protobuf_and_readiness.rs` | 9 |
| US-AP-05 — Accept a valid traces export | 03 | KPI 4 | `slice_03_traces.rs` | 10 |
| US-AP-06 — Accept a valid metrics export | 04 | KPI 4 | `slice_04_metrics.rs` | 9 |
| US-AP-07 — Refuse beyond per-transport concurrency cap | 05 | KPI 5, KPI 6 | `slice_05_backpressure.rs` | 10 |
| US-AP-08 — Forward accepted records to a downstream OTel backend | 06 | KPI 7 | `slice_06_forwarding_sink.rs` | 11 |
| US-AP-09 — Drain in-flight requests on SIGTERM | 08 | KPI 8 | `slice_08_graceful_shutdown.rs` | 5 (+1 ignored) |

Slice 07 (TLS / SPIFFE schema knob) has no companion story (it is
the only `@infrastructure` slice in v0); 7 tests defend the
forward-compat contract.

---

## KPI → defending tests (full)

| KPI | Numeric target | Defending tests |
|---|---|---|
| KPI 1 — first integration round-trip | binary; 100% of Slice-01 demo runs end-to-end | `slice_01_walking_skeleton.rs::customer_exports_one_log_record_and_receives_grpc_ok` (and 12 more in same file) |
| KPI 2 — gRPC OK / HTTP 200 ratio under non-overload | ≥ 99% | `slice_01_*::customer_exports_one_log_record_and_receives_grpc_ok` (gRPC); `slice_02_*::customer_posts_valid_logs_body_and_receives_status_200` (HTTP); `slice_03_*::customer_exports_one_span_over_grpc_and_receives_grpc_ok`; `slice_04_*::customer_exports_metrics_over_grpc_and_receives_grpc_ok` |
| KPI 3 — readiness three-state machine | structural pass on every commit | `slice_02_*::operator_probes_readyz_after_startup_and_receives_status_200` (starting → ready); `slice_08_*::shutdown_flips_readyz_to_503_draining_within_100ms` (ready → draining) |
| KPI 4 — per-signal acknowledgement ratio | ≥ 99% per signal | `slice_01_*` (logs), `slice_03_*` (traces), `slice_04_*` (metrics) — each with both accept and reject scenarios |
| KPI 5 — concurrency saturation events | zero residual in 1-h load test at 2x cap | `slice_05_*::grpc_concurrency_cap_hit_emits_warn_stderr_event`; `slice_05_*::http_concurrency_cap_hit_event_names_http_protobuf_transport`; the `@property` test below |
| KPI 6 — refusal-not-drop invariant | 100% deterministic refusal | `slice_05_*::every_excess_request_under_overload_receives_a_deterministic_refusal_or_acceptance` (the `@property`-tagged one) |
| KPI 7 — downstream-acceptance ratio | ≥ 99% under healthy downstream | `slice_06_*::customer_exports_one_log_record_and_downstream_receives_protobuf_post`; `slice_06_*::forwarding_sink_accepted_event_includes_downstream_endpoint`; failure scenarios for the symmetric (5xx, refused, timeout) shape |
| KPI 8 — graceful-restart drop ratio | zero silent drops | `slice_08_*::in_flight_request_completes_when_drain_finishes_within_deadline`; `slice_08_*::clean_drain_emits_in_flight_drained_stderr_event`; `slice_08_*::drain_deadline_exceeded_emits_warn_stderr_event_with_dropped_count` |

All 8 KPIs have at least one acceptance test. The `@kpi` tag in
`gherkin-scenarios.feature` marks the observability-shaped scenarios
that DEVOPS will invoke from CI.

---

## Slice → integration test details

| Slice | Test file | Active tests | Ignored | Walking-skeleton scenarios | Focused scenarios | Error-path tests |
|---|---|---|---|---|---|---|
| 01 — Walking skeleton (gRPC + logs) | `slice_01_walking_skeleton.rs` | 13 | 0 | 4 (customer_exports_*) | 4 (stderr observability) | 5 (empty body reject) |
| 02 — HTTP + healthz/readyz | `slice_02_http_protobuf_and_readiness.rs` | 15 | 0 | 4 (operator probes, customer posts valid) | 5 (HTTP accept observability) | 6 (415, 404, empty body) |
| 03 — Traces | `slice_03_traces.rs` | 10 | 0 | 4 (gRPC + HTTP accept + record_carries_traces_variant) | 1 (span_count) | 5 (signal-mismatch reject paths) |
| 04 — Metrics | `slice_04_metrics.rs` | 9 | 0 | 4 (gRPC + HTTP accept + record_carries_metrics_variant) | 1 (data_point_count) | 4 (signal-mismatch reject paths) |
| 05 — Backpressure | `slice_05_backpressure.rs` | 10 | 0 | 1 (saturated_grpc_does_not_block_http) | 0 (the slice IS overload behaviour) | 8 (cap-exceeded paths) + 1 `@property` |
| 06 — Forwarding sink | `slice_06_forwarding_sink.rs` | 11 | 0 | 3 (probe success + downstream-receives + accepted_event_includes_downstream_endpoint) | 2 (fall-back probe + latency_ms field) | 6 (probe-lies, 503-on-POST, refused, timeout, sink_failed, unreachable) |
| 07 — TLS schema | `slice_07_tls_schema_knob.rs` | 7 | 0 | 1 (defaults parse) | 3 (warn-line, exactly-one, plaintext-still-binds) | 3 (spiffe warn, defaults-no-warn, unknown-key reject) |
| 08 — Graceful shutdown | `slice_08_graceful_shutdown.rs` | 5 | 1 | 2 (readyz-flip, in-flight-completes) | 2 (in_flight_drained, signal field) | 1 (deadline_exceeded) |
| **Slice subtotal** | | **80** | **1** | **23** | **18** | **38 (≈ 47.5%)** |
| inv — single validator | `invariant_single_validator.rs` | 1 | 0 | n/a | 1 (corroboration) | 0 |
| inv — no telemetry | `invariant_no_telemetry_on_telemetry.rs` | 3 | 0 | n/a | 3 (corroboration) | 0 |
| **Total** | | **84** | **1** | **23** | **22** | **38 (≈ 45%)** |

Error-path ratio (38/84 ≈ 45%) is comfortably above the 40% mandate
threshold.

---

## Driving-adapter coverage

Per RCA P1, every entry point must be exercised by at least one
real-protocol scenario.

| Entry point | Real-protocol invocation | At least one `@driving_adapter` scenario? |
|---|---|---|
| gRPC listener on `:4317` (ephemeral) | Real `tonic::transport::Channel` -> `LogsServiceClient` / `TraceServiceClient` / `MetricsServiceClient` | YES — `slice_01`, `slice_03`, `slice_04`, `slice_05`, `slice_06`, `slice_08` |
| HTTP listener on `:4318` (ephemeral) | Real `reqwest::Client::post` to `/v1/{logs,traces,metrics}` with `Content-Type: application/x-protobuf` | YES — `slice_02`, `slice_03`, `slice_04`, `slice_05`, `slice_06`, `slice_07` |
| `/healthz` HTTP endpoint | Real `reqwest::Client::get` | YES — `slice_02::operator_probes_healthz_and_receives_status_200`, `slice_02::operator_probes_healthz_and_response_body_is_ok` |
| `/readyz` HTTP endpoint | Real `reqwest::Client::get` | YES — `slice_02::operator_probes_readyz_after_startup_and_receives_status_200`, `slice_07::tls_enabled_true_listeners_still_bind_and_readyz_returns_ok`, `slice_08::shutdown_flips_readyz_to_503_draining_within_100ms` |

Every entry point has at least one real-protocol scenario. There is
no CLI subcommand surface in v0; the binary's startup path is
exercised indirectly through `aperture::spawn` in the integration
tests, plus the `#[ignore]`-flagged process-fork test in Slice 08
that DELIVER may pick up.

---

## Driven-adapter coverage (Mandate 6)

Per Mandate 6, every driven adapter must have at least one
`@real-io @adapter-integration` scenario.

| Adapter | `@real-io @adapter-integration` scenario | Notes |
|---|---|---|
| **StubSink** | YES — Slice 01's `customer_exports_one_log_record_and_record_reaches_sink` (DELIVER lands `StubSink`; the test substitutes `RecordingSink` at the trait seam — this IS the canonical hexagonal pattern under Strategy C; the production `StubSink` stderr line is asserted by Slice 01's `sink_accepted` assertions) | DISTILL substitution at the trait seam, not at the I/O boundary; `StubSink`'s only I/O is stderr, exercised at runtime by all `sink_accepted` assertions |
| **ForwardingSink** | YES — Slice 06's full file (probe + accept + downstream + failure modes) against an in-process `wiremock` server on loopback | Real HTTP traffic over loopback TCP between Aperture and `wiremock`; matches Strategy C "in-process axum stub on loopback" exactly |

Substrate exemption (per `architecture/brief.md` substrate stratum):
- `otlp-conformance-harness` is library substrate (Apache-Foundation
  stewarded). Aperture imports it directly; there is no port boundary
  around it. Not listed as a driven adapter.
- `opentelemetry-proto` is the same. Not listed.

---

## Shared artefact → defending test mapping

Per `discuss/shared-artifacts-registry.md`, every shared artefact has
a CI-enforced contract; this matrix shows which acceptance tests
defend each one at the user-observable surface.

| Artefact | Defending acceptance test(s) |
|---|---|
| `harness_function_logs` (`validate_logs`) | `slice_01_*` accept and reject; `slice_02_*` HTTP accept and reject; `invariant_single_validator.rs` |
| `harness_function_traces` | `slice_03_*` accept and reject |
| `harness_function_metrics` | `slice_04_*` accept and reject |
| `framing_enum` | The framing variant appears verbatim in reject messages (`framing=GrpcProtobuf`, `framing=HttpProtobuf`); asserted in `slice_01_*::customer_sends_empty_body_and_grpc_message_names_grpc_protobuf_framing` and `slice_02_*::customer_posts_empty_body_and_response_body_names_http_protobuf_framing` |
| `violation_display` | The harness's `OtlpViolation::Display` output is asserted verbatim in every reject test in `slice_01..04` (rule, signal, framing substrings) |
| `grpc_port` | Tests bind to `127.0.0.1:0` and discover the port via `Handle::grpc_addr()`; the default `0.0.0.0:4317` is documented in `Config::builder` defaults but not asserted in tests (defaults are runtime constants; tests use ephemeral ports for parallelism) |
| `http_port` | Same — ephemeral ports; default documented |
| `sink_trait` | Every `slice_*.rs` test imports `aperture::ports::OtlpSink` and uses it as the integration seam |
| `sink_record_enum` | `slice_01..04` each assert `matches!(record, SinkRecord::{Logs,Traces,Metrics}(_))` |
| `aperture_version` | Not directly asserted at v0; DELIVER may add a `--version` smoke test |
| `max_recv_msg_size` | Not directly asserted at v0; DISCUSS leaves the body-too-large path as a DELIVER concern |
| `max_concurrent_requests` | `slice_05_*::start_with_cap` configures it; cap-hit assertions defend the contract |
| `drain_deadline_ms` | `slice_08_*` configures it via `Config::builder().drain_deadline(...)`; deadline-exceeded assertion defends the contract |
| `downstream_endpoint` | `slice_06_*` configures it via `Config::builder().forwarding_sink(...)`; sink-accepted-with-downstream assertion defends |
| `readyz_state_machine` | `slice_02_*` (starting → ready); `slice_08_*` (ready → draining); `slice_07_*::tls_enabled_true_listeners_still_bind_and_readyz_returns_ok` |
| `tls_config_schema` | Slice 07's full file |
| `log_event_vocabulary` | Every stderr-line assertion uses an event name from the closed set; `slice_*.rs::expect_stderr_event` calls the helper with literal strings |
| `request_received_event_schema` | `slice_01_*::customer_exports_one_log_record_and_request_received_line_is_emitted` and `slice_02_*::customer_posts_valid_logs_body_and_sink_accepted_line_names_http_protobuf_transport` |
| `otlp_spec_version` | Not directly asserted at v0; informational only |

CI invariants:
- `no_telemetry_on_telemetry` — corroborated by `tests/invariant_no_telemetry_on_telemetry.rs`; load-bearing defence is DEVOPS-owned net-ns fixture.
- `single_validator_per_signal` — corroborated by `tests/invariant_single_validator.rs`; load-bearing defence is DEVOPS-owned `xtask` AST walk.

---

## Walking-skeleton + focused-scenario count

Per Mandate CM-C, per-feature recommended ratio is 2-3 walking
skeletons + 15-20 focused scenarios. At the per-slice level:

| Slice | Walking-skeleton scenarios | Focused scenarios |
|---|---|---|
| 01 | 4 | 9 |
| 02 | 4 | 11 |
| 03 | 4 | 6 |
| 04 | 4 | 5 |
| 05 | 1 | 9 |
| 06 | 3 | 8 |
| 07 | 1 | 6 |
| 08 | 2 | 3 |
| **Suite** | **23** | **57** |

Suite-wide the ratio is roughly 1:2.5 (skeleton : focused), well
within the spirit of the recommendation (2-3 per feature scaled to
8 slices = ~16-24 skeletons; we have 23).

Each per-slice "walking skeleton" is the user-observable success
path for that slice; the "focused" scenarios cover error paths,
stderr observability, and boundary conditions. The Slice-01 walking
skeleton (the project-level walking skeleton) is
`customer_exports_one_log_record_and_receives_grpc_ok` and the four
related `customer_exports_*` tests; together they prove the
end-to-end value proposition.

---

## Mandate compliance summary

| Mandate | Compliance | Evidence |
|---|---|---|
| **CM-A** Hexagonal | YES | Test imports limited to `aperture::{config, ports, testing, Handle, spawn}`; zero `pub(crate)` symbol imports; zero `aperture::{transport, app, sinks, shutdown, compose, error}::*` imports |
| **CM-B** Business language | YES | Test function names use domain terms ("customer", "operator", "sink", "downstream"); technical jargon limited to wire-level identities operators read |
| **CM-C** Walking skeleton + focused mix | YES | 23 skeletons + 57 focused; per-slice 1-4 skeletons; project-level walking skeleton is Slice 01 |
| **CM-D** Pure-function extraction | DELIVER's responsibility | DESIGN names the pure helpers (`framing_for_transport`, `summarise_record`, `responses::*`); DISTILL writes integration tests; DELIVER lands unit tests under `src/**` |

| Strategy C compliance | Compliance | Evidence |
|---|---|---|
| Real loopback transports | YES | Every test binds `127.0.0.1:0`; real `tonic` Server, real `axum` Server; real `tonic` and `reqwest` clients |
| No InMemory transport doubles | YES | Zero in-memory test transports; `RecordingSink` is at the sink seam (the trait), not the transport seam |
| No costly external deps | YES | `wiremock` is in-process; no Docker; no Testcontainers |
| `@real-io @driving_adapter` tag on walking skeletons | YES | See `gherkin-scenarios.feature` |

---

## DELIVER unblock readiness

- All 84 active tests are RED at DISTILL completion (every test
  panics at runtime on the first call into a `unimplemented!()`
  symbol).
- The scaffold compiles cleanly (`cargo build --tests` green; `cargo
  test --no-run` green; `cargo test` panics in every test as
  expected).
- DELIVER's first task is Slice 01; the 13 tests in
  `slice_01_walking_skeleton.rs` are the RED-to-GREEN units to drive
  through, in the order documented in `walking-skeleton.md`.
- Subsequent slices light up in dependency order:
  Slice 02 → Slices 03+04 → Slice 05 → Slice 06 → Slice 07 → Slice 08.
- The two `invariant_*.rs` tests run alongside; they go GREEN as the
  corresponding slices land (single-validator behaviour is a
  byproduct of Slice 01; no-telemetry-on-telemetry behaviour is a
  byproduct of every slice not introducing outbound network).

Vai.
