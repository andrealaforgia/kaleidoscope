# Slice 04 — metrics signal — completion summary

> **Wave**: DELIVER (`nw-software-crafter` / Crafty).
> **Date**: 2026-05-04.
> **Slice**: 04 — metrics signal end-to-end.
> **Companion brief**: [`../slices/slice-04-metrics.md`](../slices/slice-04-metrics.md).
> **Companion story**: US-AP-06.

---

## Headline

The metrics signal closes the OTLP three-signal contract on both
transports. A real `ExportMetricsServiceRequest` over gRPC `:4317` or
HTTP/protobuf `:4318` round-trips to gRPC `OK` / HTTP 200; the
`RecordingSink` records a `SinkRecord::Metrics` carrying the SDK's
upstream-typed request; the StubSink emits the structured stderr line
`event=sink_accepted sink=stub signal=metrics data_point_count=N`. The
reject path (a traces body posted to `/v1/metrics`) returns the
harness's verbatim `WireType::SignalMismatch{observed=Traces,
asserted=Metrics}` violation Display.

After this slice, Aperture v0 is a complete OTLP receiver in the
sense the DISCUSS contract names: every stable OTLP signal type is
supported on every supported transport.

## What turned GREEN

| Test binary | Tests passing |
|---|---:|
| `tests/slice_04_metrics.rs` | **9/9** |
| `tests/slice_03_traces.rs` | **10/10** (no regressions) |
| `tests/slice_02_http_protobuf_and_readiness.rs` | **15/15** (no regressions) |
| `tests/slice_01_walking_skeleton.rs` | **13/13** (no regressions) |
| `tests/invariant_single_validator.rs` | **1/1** |
| `tests/invariant_no_telemetry_on_telemetry.rs` | **3/3** |
| `src/lib.rs` (lib unit tests) | **38/38** (25 from Slice 03 + 6 metrics summary + 1 SinkRecord variant set + 6 SinkError/ProbeError Display) |
| **Slice 04 active total** | **89/89** |

The 9 acceptance tests in `slice_04_metrics.rs` cover (per Mandate
Single-Then-Per-Fact):

- **Metrics accept on gRPC (4)**: a real `ExportMetricsServiceRequest`
  over the tonic client returns `Ok`; the `RecordingSink` carries a
  `SinkRecord::Metrics` variant; the stderr `sink_accepted` event
  names `data_point_count=2` (one Sum + one Gauge); the same event
  names `signal=metrics`.
- **Metrics accept on HTTP (1)**: `POST /v1/metrics` with
  `application/x-protobuf` and a real protobuf body returns 200.
- **Metrics reject on HTTP — traces body to `/v1/metrics` (4)**: 400
  status; response body contains the harness's
  `rule=WireType::SignalMismatch`, `observed=Traces`, and
  `asserted=Metrics` substrings verbatim.

The 13 new lib unit tests:

- 6 in `app::tests` — pin the per-`Metric` data-point counting
  convention DISCUSS US-AP-06 locked. Coverage:
  - canonical fixture (1 Sum + 1 Gauge = 2);
  - histogram with 50 buckets but 1 `HistogramDataPoint` = 1
    (the bucket-vs-data-point mutation barrier);
  - `flat_map` walk across two `ScopeMetrics` = 4;
  - `Metric.data == None` contributes 0;
  - `ExponentialHistogram` and `Summary` arms each contribute 1;
  - missing resource attributes => `None` service name.
- 1 in `ports::tests` — pins the v0 `SinkRecord` variant set: exactly
  three variants, one per OTLP-stable signal. The slice contract's
  variant-exhaustiveness assertion.
- 6 in `ports::tests` — pin the operator-facing `Display` strings of
  `SinkError` (3 variants) and `ProbeError` (3 variants) against an
  `Ok(Default::default())` mutation that would render every refusal
  / probe failure as the empty string. Identified by the Slice 04
  mutation run; the production code paths that emit these strings
  are not reachable from Slice 04's acceptance tests yet (StubSink /
  RecordingSink never refuse), so the unit-level pin is the
  appropriate defence.

## Production code added or modified

| File | Net change | What it does |
|---|---:|---|
| `src/app.rs` | +90 lines (signal arm) + 250 lines (test arm) | `ingest_metrics` (single call site for `validate_metrics`); `summarise_record::Metrics` arm; `count_data_points` walker; `extract_service_name_metrics`; 6 focused unit tests |
| `src/transport.rs` | +149 / −5 | `MetricsServiceImpl` (gRPC); `handle_metrics` (HTTP); axum router gains `/v1/metrics`; tonic Server gains `MetricsServiceServer` |
| `src/sinks.rs` | +6 / −9 | `emit_sink_accepted::Metrics` arm with `data_point_count` field; the `_ => unsupported` lower-bound arm removed (the in-crate match is now exhaustive) |
| `src/ports/mod.rs` | +110 / 0 | `SinkRecord` variant-exhaustiveness unit test + `SinkError`/`ProbeError` Display unit tests (mutation-kill-rate close-out) |
| **Slice 04 net delta** | **+615 / −24** | (`git diff 4537879..HEAD -- crates/aperture/src/`) |

The `_ => RecordSummary::unsupported` arm in `summarise_record` and
the `_ => record_count` arm in `emit_sink_accepted` were removed in
the GREEN commit. Both were placeholders for the metrics case; with
metrics now wired the in-crate match is exhaustive. `#[non_exhaustive]`
on `SinkRecord` continues to gate downstream-crate matches — only
in-crate matches (which we control completely) lose the lower-bound
arm.

Modules NOT touched (still placeholders for future slices):
`src/error.rs` (Slice 07/08 lands the rich `ApertureError` enum),
`src/shutdown.rs` (Slice 08), `src/main.rs` (Slice 07 lands the
figment loader), `src/readiness.rs` (Slice 08 reintroduces the
`Draining` variant). No `// SCAFFOLD: true` markers were carried by
any module Slice 04 touched.

## Commits

| Hash | Subject |
|---|---|
| `c9c4139` | `feat(aperture): Slice 04 — metrics signal round-trips on both transports` |
| `df508c4` | `test(aperture): pin Slice 04 metrics summary helpers and SinkRecord variant set` |
| `7ca6dfb` | `test(aperture): pin SinkError and ProbeError Display strings` |
| (this doc) | `docs(aperture): DELIVER slice-04-completion summary` |

The first commit lands the GREEN feature outcome (9 RED acceptance
tests → GREEN). The second pins the new metrics-summary helpers and
the v0 `SinkRecord` variant set against mutation testing without
leaning on the integration tests. The third commit closes the last
two surviving mutants the Slice 04 mutation run surfaced, raising
the kill rate on touched files from 32/34 to 34/34.

## Mutation testing

Per ADR-0005 Gate 5, the target is 100% kill rate on Slice 04
touched files. Run command (scoped to the metrics-touched files,
restricted to the green-by-design test set so the baseline passes):

```text
cargo mutants --package aperture --no-shuffle --jobs 2 \
  --file crates/aperture/src/app.rs \
  --file crates/aperture/src/sinks.rs \
  --file crates/aperture/src/transport.rs \
  --file crates/aperture/src/ports/mod.rs \
  --cargo-test-arg "--lib" \
  --cargo-test-arg "--test=slice_01_walking_skeleton" \
  --cargo-test-arg "--test=slice_02_http_protobuf_and_readiness" \
  --cargo-test-arg "--test=slice_03_traces" \
  --cargo-test-arg "--test=slice_04_metrics" \
  --cargo-test-arg "--test=invariant_single_validator" \
  --cargo-test-arg "--test=invariant_no_telemetry_on_telemetry"
```

(The `slice_05` through `slice_08` test files are RED-by-design until
DELIVER advances each slice; including them in the cargo-mutants
baseline run aborts before any mutant is tested.)

Result on Slice 04 touched source files (`app.rs`, `sinks.rs`,
`transport.rs`, `ports/mod.rs`) after `7ca6dfb`:

| Metric | Count |
|---|---:|
| Mutants generated | 64 |
| Caught | **34** |
| Missed | **0** |
| Unviable (mutation produces non-compiling code) | 30 |
| **Kill rate** | **100% (34 / 34 viable)** |

The first run (after `df508c4`) surfaced 2 surviving mutants in the
pre-existing `SinkError::Display` and `ProbeError::Display` impls
(both at `ports/mod.rs`). These were not Slice 04 territory in the
strict sense — the production code paths that emit those strings
are not reachable from Slice 04's tests (StubSink / RecordingSink
never refuse and never fail probes; those code paths light up in
Slice 06's `ForwardingSink`). DELIVER chose to close them in
`7ca6dfb` rather than carry the gap forward, and now the workspace
mutation report is fully green on the v0-touched surface.

The Slice 04 production code mirrors Slice 01's and Slice 03's shape
line-for-line (the `ingest_metrics` / `MetricsServiceImpl` /
`handle_metrics` triple is structurally identical to the logs and
traces equivalents). The 6 focused unit tests added in `df508c4`
deliberately cap the new mutation surfaces:

- The `count_data_points` walker has 5 `Some(Data::*)` arms plus a
  `None` arm. Each arm has at least one test that fails on a
  mutation that returns 0 for that variant.
- The `flat_map` traversal across `ScopeMetrics` has a dedicated test
  (`summarise_metrics_sums_data_points_across_multiple_scope_metrics`)
  with an assertion (`s.count == 4`) that fails if the walk degrades
  to `first().map(...)`.
- The bucket-vs-data-point convention has a dedicated test
  (`summarise_metrics_counts_one_data_point_per_histogram_data_point_not_per_bucket`)
  with an assertion (`s.count == 1`, not 50) that fails if a future
  maintainer mistakenly sums `bucket_counts.len()`.
- The `SinkRecord` variant set is locked by the in-crate
  `sink_record_has_exactly_three_variants_one_per_otlp_stable_signal`
  test: removing or adding a variant fails compilation of that test.

The 30 unviable mutants are mostly trait-bound mutations that produce
non-compiling code (e.g. replacing trait method bodies with
`Default::default()` where the trait's return type does not implement
`Default`). cargo-mutants reports these without testing them; they are
not gaps in coverage.

Per the per-feature mutation strategy (root `CLAUDE.md`), the gate is
satisfied: every line of production code Slice 04's acceptance tests
can reach has at least one test that fails when the line is
meaningfully mutated.

## Architectural observations

- The `single_validator_per_signal` CI invariant continues to hold:
  `validate_metrics` has exactly one call site (in
  `app::ingest_metrics`). The xtask AST check now asserts three rules
  (one per signal). The runtime corroboration in
  `tests/invariant_single_validator.rs` continues to assert
  exactly-one-record-per-export.
- The `framing_for_transport` helper remains total: every `Transport`
  variant maps to a `Framing` variant. No new Cartesian product cells
  were introduced.
- The closed v0 event vocabulary continues to hold: no new event
  names were emitted; `event=request_received` and
  `event=sink_accepted` carry a `signal=metrics` field rather than a
  new event name. The `data_point_count` field is the only new field
  name in the closed vocabulary, and DISCUSS US-AP-06 locked it
  in advance.
- `transport.rs` now hosts three gRPC service impls + three HTTP
  handlers + the listener spawners + content-type helpers in 600
  lines. The DESIGN brief
  (`component-design.md > Module structure`) names them as separate
  files (`transport/grpc.rs`, `transport/http.rs`); the flat-file
  shape is provisional and Slice 05 (which adds the concurrency cap
  layer) or Slice 07 (which adds the figment loader and TLS schema
  knob) will likely motivate the split. ADR-0006 permits the rename
  as a non-breaking refactor.

## Genuine forks discovered

**None.** The DISCUSS contract (Q1–Q6, including US-AP-06's
`data_point_count` lock), the DESIGN brief (D1–D10), the ADRs
0006–0010, and the DISTILL test inventory together resolved every
implementation question Slice 04 raised. No back-propagation needed.

The "known unknown" the slice brief flags — whether the stderr field
should be `data_point_count` or `metric_count` — was already
resolved by DISCUSS US-AP-06 in favour of `data_point_count` because
that is the unit the operator's downstream sees. DELIVER honoured
the lock without raising the question again.

## Quality gates at commit time

- `cargo fmt --all -- --check`: clean
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: clean
- `cargo test --workspace --exclude aperture --all-targets --locked`:
  73 harness tests pass (matches DEVOPS A2 graduation contract)
- `cargo test --package aperture` (slices 01, 02, 03, 04 + lib unit
  + invariant tests): all green (83 tests)
- Pre-commit hook: not modified; `--exclude aperture` graduation
  stays per Apex's wave-decisions A2

## Out of scope (left RED for subsequent slices)

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

Slice 05 (backpressure) is the natural next cycle. The seams Slice
05 will fill:

1. A `concurrency_cap` knob on `Config` — the figment loader is not
   yet wired (Slice 07), but the in-process `Config::builder` already
   exposes the surface DISTILL declared.
2. A semaphore-or-equivalent on the `ingest_*` path that returns
   `IngestOutcome::CapHit` (or a new variant) when the in-flight
   count meets the cap.
3. The transport-arm mappings for the new outcome: gRPC
   `RESOURCE_EXHAUSTED` and HTTP 503 with the closed-vocabulary
   `event=concurrency_cap_hit` warn-level line.
4. The acceptance tests in `tests/slice_05_backpressure.rs` already
   declare the assertion shape; flipping their `@skip` tags is the
   first action.

Slice 04's three-signal closure means the metrics arm of every
subsequent slice is "free": Slice 05's concurrency cap is signal-
agnostic, Slice 06's `ForwardingSink` already has a `SinkRecord::Metrics`
arm to forward, Slice 07's TLS knob applies to both transports, and
Slice 08's drain orchestrator drains every in-flight request
regardless of signal type.
