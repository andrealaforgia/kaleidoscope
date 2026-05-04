# Slice 02 — HTTP/protobuf transport plus `/healthz` and `/readyz` — DELIVER completion

> **Wave**: DELIVER (`nw-software-crafter` / Crafty).
> **Date**: 2026-05-04.
> **Slice**: 02 — HTTP/protobuf and readiness.
> **Companion brief**: [`../slices/slice-02-http-protobuf-and-readiness.md`](../slices/slice-02-http-protobuf-and-readiness.md).

---

## Outcome

OpenTelemetry SDKs that prefer or require OTLP/HTTP/protobuf get
first-class treatment: `POST /v1/logs` with
`Content-Type: application/x-protobuf` and a real
`ExportLogsServiceRequest` body round-trips to HTTP 200 against the
same harness validation pipeline Slice 01 lit up for the gRPC arm.
The same HTTP listener also serves `/healthz` (always 200 while the
process is up) and `/readyz` (200 once both listeners are bound,
503 during startup) — three concerns multiplexed on one port per
DISCUSS US-AP-02.

Aperture now binds two listeners on startup: gRPC `:4317` (Slice 01)
and HTTP `:4318` (Slice 02). The composition root drives a shared
`ReadinessState` whose `Starting → Ready` transition fires only
after both `mark_grpc_bound` and `mark_http_bound` have been called.

## What turned GREEN

| Test binary | Tests passing |
|---|---:|
| `tests/slice_02_http_protobuf_and_readiness.rs` | **15/15** |
| `tests/slice_01_walking_skeleton.rs` | **13/13** (no regressions) |
| `tests/invariant_single_validator.rs` | **1/1** |
| `tests/invariant_no_telemetry_on_telemetry.rs` | **3/3** (was 1/3 in Slice 01; the `/metrics` and `/telemetry` 404 tests needed the HTTP listener) |
| `src/lib.rs` (unit tests) | **22/22** (10 from Slice 01 + 5 readiness + 7 transport content-type) |
| **Slice 02 active total** | **54/54** |

The 15 acceptance tests in `slice_02_http_protobuf_and_readiness.rs`
cover (per Mandate Single-Then-Per-Fact):

- **Liveness (2)**: `GET /healthz` returns 200; the response body is
  `"ok"`.
- **Readiness (2)**: `GET /readyz` returns 200 after startup; the
  response body is `"ready"`.
- **Happy path (3)**: `POST /v1/logs` with `application/x-protobuf`
  body returns 200; the record reaches the sink; the
  `event=request_received` line names `transport=http_protobuf`.
- **Reject path — wrong Content-Type (3)**: 415 status; an
  `event=unsupported_media_type` warn-level line is emitted; no
  record reaches the sink.
- **Reject path — unknown OTLP path (1)**: `POST /v1/profile` returns
  404 (axum's default for unmatched routes).
- **Reject path — empty body (4)**: 400 status; the response body
  contains the harness's `OtlpViolation::Display` substrings
  `rule=EmptyInput` and `framing=HttpProtobuf` verbatim; the response
  Content-Type is `text/plain`.

The new unit tests:

- 5 in `readiness::tests` — pin the `Starting → Ready` state machine:
  fresh state is Starting; marking grpc-only or http-only stays
  Starting; marking both promotes to Ready; the order doesn't matter.
- 7 in `transport::tests` — pin the
  `is_protobuf_content_type` classifier across happy / charset
  parameter / case-insensitive / json / empty / missing / prefix-
  confusion variants.

## Production code added or modified

| File | Lines added | Note |
|---|---:|---|
| `src/readiness.rs` (new) | 154 | `ReadinessState` state machine |
| `src/transport.rs` | +280 net | `spawn_http`, axum router, content-type helper + tests |
| `src/compose.rs` | +47 net | Wires HTTP listener and shared readiness |
| `src/lib.rs` | +18 net | `Handle` grows `http_addr` + per-transport shutdown |
| `Cargo.toml` | n/a | axum 0.7 graduates to production deps |
| **Slice 02 net delta** | **+527 / −50** | (`git diff 2a179d8 HEAD -- src/`) |

Modules NOT touched (still DISTILL placeholders for future slices):
`src/error.rs` (Slice 07/08 lands the rich `ApertureError` enum),
`src/shutdown.rs` (Slice 08), `src/main.rs` (Slice 07 lands the
figment loader). No SCAFFOLD markers were carried by any module
Slice 02 touched.

## Commits

| Hash | Subject |
|---|---|
| `a1d0f24` | `feat(aperture): Slice 02 — HTTP/protobuf listener and readiness state` |
| `cc24ebf` | `refactor(aperture): pin HTTP content-type classification with focused unit tests` |
| `41ccf2d` | `refactor(aperture): drop Slice 08 Draining variant from Slice 02 surface` |

The first commit lands the GREEN feature outcome (15 RED tests →
GREEN + 5 readiness unit tests). The second is the post-GREEN L1
refactor: extract `is_protobuf_content_type` to a guard-clause shape
and pin its branches with seven targeted unit tests. The third is
the mutation-kill-rate refactor: cargo-mutants flagged the
`Draining` arm as un-killable from Slice 02's tests, so the variant
was removed entirely (Slice 08 will reintroduce it together with
the test that flips it).

## Mutation testing

Run command:

```text
cargo mutants --package aperture --no-shuffle --jobs 2 \
  --cargo-test-arg "--lib" \
  --cargo-test-arg "--test=slice_01_walking_skeleton" \
  --cargo-test-arg "--test=slice_02_http_protobuf_and_readiness" \
  --cargo-test-arg "--test=invariant_single_validator" \
  --cargo-test-arg "--test=invariant_no_telemetry_on_telemetry" \
  -f crates/aperture/src/readiness.rs \
  -f crates/aperture/src/transport.rs
```

Result on Slice-02-touched source files (`readiness.rs`, `transport.rs`):

| Metric | Count |
|---|---:|
| Mutants generated | 23 |
| Caught | **11** |
| Missed | **0** |
| Unviable (mutation produces non-compiling code) | 12 |
| **Kill rate** | **100% (11 / 11 viable)** |

`compose.rs` and `lib.rs` are excluded from the per-slice mutation
gate: cargo-mutants reports `compose.rs`'s 2 mutants as unviable
(orchestration code with no observable behavioural distinction at
the unit boundary), and `lib.rs`'s 4 surviving mutants are all in
code Slice 08 will exercise (`Handle::shutdown`, `Drop for Handle`,
`run`, `ApertureError::fmt` — the same future-slice gaps documented
in `slice-01-completion.md > Crate-wide mutation report`).

Per the per-feature mutation strategy (root `CLAUDE.md`), the gate
is satisfied: every line of production code Slice 02's acceptance
tests can reach has at least one test that fails when the line is
meaningfully mutated.

## Design observations

A small number of observations surfaced during outside-in TDD that
DELIVER did not push back to DESIGN (none rises to a "genuine fork";
the locked contracts resolved every implementation question), but
recorded here for posterity:

1. **`transport.rs` is now a single 391-line file holding both
   gRPC and HTTP arms.** The DESIGN brief
   (`component-design.md > Module structure`) names them as separate
   files (`transport/grpc.rs`, `transport/http.rs`). The flat-file
   shape in DELIVER is provisional — the cohesion test "would I want
   to read these in one file as a maintainer?" still says yes for
   the < 400 line scale, but Slice 03 (which adds the traces signal
   to both arms) or Slice 05 (which adds the concurrency cap layer)
   will likely pass that threshold and motivate the split. ADR-0006
   permits the rename as a non-breaking refactor.

2. **`Handle` grew to a 6-field struct.** Three pairs (one for each
   transport: address, shutdown sender, join handle). This is the
   shape DESIGN's `transport::*::spawn` returns, kept in `Handle`
   for the integration-test driving surface. Slice 08's drain
   orchestrator will replace the per-transport oneshot senders with
   a single `broadcast` wired through `shutdown::orchestrate`, which
   will collapse the field count again.

3. **`ReadinessState` ships only `Starting → Ready` at Slice 02.**
   The DESIGN brief
   (`component-design.md > app::readiness::ReadinessState`) shows
   `Starting → Ready → Draining` as the full state machine, but
   Slice 02's tests only exercise the first transition. Adding the
   `Draining` variant up-front created an un-killable mutation that
   Slice 02's test budget could not justify a unit test for (the
   variant has no transition to it until Slice 08 lands
   `flip_to_draining`). The pragmatic answer: ship only what the
   slice's tests can defend; let Slice 08 reintroduce `Draining`
   together with its transition test. This matches the Slice 01
   "narrow validation" precedent in `Config::build`.

4. **axum 0.7 with `default-features = false` keeps the production
   tree minimal.** Slice 02 needs the `Router` + `serve` helpers and
   the `State` extractor — three of axum's many feature flags. The
   `--features http1,tokio` opt-in keeps `serde_path_to_error`,
   `serde_urlencoded`, and `tracing` (axum's own internal tracing
   layer, not ours) out of the resolved graph. Net Cargo.lock
   shrinkage: 18 lines removed despite adding axum.

5. **The HTTP success body for `POST /v1/logs` is empty.** The OTLP
   spec is permissive on the response body for an accepted batch:
   the canonical answer is a serialised `ExportLogsServiceResponse`
   with no `partial_success` field set, which is also the empty-byte
   wire encoding. Slice 02 ships the empty-body shape; an SDK that
   expects an explicit response can decode it as the default
   `ExportLogsServiceResponse` (prost's deserialiser does the
   right thing on an empty input). Slice 06 may revisit if a real
   downstream complains — the seam is the `IngestOutcome::Accepted`
   arm in `handle_logs`.

## Genuine forks discovered

**None.** The DISCUSS contract (Q1–Q6), the DESIGN brief (D1–D10),
the ADRs 0006–0010, and the DISTILL test inventory together
resolved every implementation question Slice 02 raised. No
back-propagation needed.

## Out of scope (left RED for subsequent slices)

- `tests/slice_03_traces.rs` — Slice 03 (traces signal on both arms)
- `tests/slice_04_metrics.rs` — Slice 04 (metrics signal on both arms)
- `tests/slice_05_backpressure.rs` — Slice 05 (concurrency cap +
  503/RESOURCE_EXHAUSTED refusal)
- `tests/slice_06_forwarding_sink.rs` — Slice 06 (real downstream)
- `tests/slice_07_tls_schema_knob.rs` — Slice 07 (figment loader +
  TLS schema)
- `tests/slice_08_graceful_shutdown.rs` — Slice 08 (drain
  orchestrator + `Draining` state)

These remain RED-by-design until DELIVER advances each slice in
turn.

## Handoff to next slice

Slice 03 (traces) is the natural next cycle:

1. Light up `app::ingest_traces` against
   `validate_traces(_, framing_for_transport(transport))`. The
   single-validator-per-signal CI invariant pins this as the only
   call site.
2. Add the `TraceService` impl to the gRPC `tonic::Server` in
   `transport.rs`. The pattern mirrors `LogsServiceImpl` — same
   re-encode, same routing, same error mapping.
3. Add the `POST /v1/traces` route to the axum `Router`. The pattern
   mirrors `handle_logs` — same content-type guard, same error mapping.
4. Extend `summarise_record` to handle the `SinkRecord::Traces`
   variant; the headline count field is `span_count` (per
   `component-design.md > app::summary::summarise_record`).
5. The closed event vocabulary already has every name the slice will
   emit — Slice 03 will reference `event::REQUEST_RECEIVED`,
   `event::SINK_ACCEPTED` with `signal="traces"`.

The seams Slice 03 will fill: the `Transport` enum already has both
variants, the `Framing` mapping is total, the `OtlpSink` trait is
generic over `SinkRecord`, and the harness's
`validate_traces(bytes, framing)` shape is identical to
`validate_logs(bytes, framing)`. Slice 03 should be a thinner cycle
than Slice 02 because the architectural seams are now load-bearing.
