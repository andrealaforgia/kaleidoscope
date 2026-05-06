# Sieve v0 — DISTILL test mapping

Per-slice mapping of BDD scenario → test binary → `#[test]` function
name → asserted public-API touchpoint.

## Slice 01 — Walking skeleton (US-SI-01)

Binary: `crates/sieve/tests/slice_01_walking_skeleton.rs`

| BDD scenario | `#[test]` function | Public-API touchpoint |
|---|---|---|
| An error-bearing trace is always kept | `an_error_bearing_trace_is_kept_at_rate_zero` | `HeadSampler::new(0.0)`, `Sampler::sample(&TraceView<'_>) → Decision::Keep`, `__test_trace_view` |
| A non-error trace is dropped at rate 0.0 | `an_all_ok_trace_is_dropped_at_rate_zero` | `HeadSampler::new(0.0)`, `Sampler::sample → Decision::Drop`, `__test_trace_view` |
| Reflective: configured rate is observable | `head_sampler_exposes_its_configured_rate` | `HeadSampler::new(0.25)`, `HeadSampler::rate()` |

DISTILL state: tests #1 and #2 panic on `HeadSampler::sample`'s
`unimplemented!()`; test #3 passes (the `rate()` accessor is real).
DELIVER slice 01 turns the two panicking tests GREEN.

## Slice 02 — Error-bias retention (US-SI-02)

Binary: `crates/sieve/tests/slice_02_error_bias.rs`

| BDD scenario | `#[test]` function | Public-API touchpoint |
|---|---|---|
| Error trace kept at rate 0.0 | `error_bearing_trace_kept_at_rate_0_0` | `HeadSampler::new(0.0)`, `sample → Keep` |
| Error trace kept at rate 0.1 | `error_bearing_trace_kept_at_rate_0_1` | `HeadSampler::new(0.1)`, `sample → Keep` |
| Error trace kept at rate 0.5 | `error_bearing_trace_kept_at_rate_0_5` | `HeadSampler::new(0.5)`, `sample → Keep` |
| Error trace kept at rate 1.0 | `error_bearing_trace_kept_at_rate_1_0` | `HeadSampler::new(1.0)`, `sample → Keep` |
| Multi-span trace, one error span | `multi_span_trace_with_single_error_span_is_kept` | `sample → Keep` for 12-span fixture |
| All-OK trace at rate 0.0 falls to rate rule | `all_ok_trace_at_rate_zero_is_dropped_by_rate_rule` | `sample → Drop` for 5 OK spans |
| Negative rate rejected | `head_sampler_new_rejects_negative_rate` | `HeadSampler::new(-0.1) → Err(RateOutOfRange)` |
| Rate > 1.0 rejected | `head_sampler_new_rejects_rate_above_one` | `HeadSampler::new(1.5) → Err(RateOutOfRange)` |
| NaN rate rejected | `head_sampler_new_rejects_nan_rate` | `HeadSampler::new(NaN) → Err(RateOutOfRange)` |

DISTILL state: the six sampling tests panic; the three error-path
tests pass (the `new` constructor is real). DELIVER slice 02 turns
the panicking tests GREEN.

## Slice 03 — Non-error rate honoured statistically (US-SI-03)

Binary: `crates/sieve/tests/slice_03_non_error_rate.rs`

| BDD scenario | `#[test]` function | Public-API touchpoint |
|---|---|---|
| At rate 0.0 at most 2 traces are kept | `at_rate_zero_at_most_two_non_error_traces_are_kept` | 10 000 calls to `sample` at rate 0.0; kept ≤ 2 |
| At rate 1.0 at least 9998 are kept | `at_rate_one_at_least_nine_thousand_nine_hundred_ninety_eight_are_kept` | 10 000 calls at rate 1.0; kept ≥ 9998 |
| At rate 0.5 kept count in [4700, 5300] | `at_rate_half_kept_count_lies_in_three_percent_band_around_half` | 10 000 calls at rate 0.5; kept in `[4700, 5300]` |

DISTILL state: all three panic on `HeadSampler::sample`'s
`unimplemented!()`. DELIVER slice 03 turns them GREEN.

The fixture trace_ids are deterministic (sequential 64-bit seeds
splatted into 16-byte trace_ids); the `xxh3_64` distribution
guarantees the kept count lands in the band on every run. The
test is **not flaky**.

## Slice 04 — Trace coherence (US-SI-04)

Binary: `crates/sieve/tests/slice_04_trace_id_determinism.rs`

| BDD scenario | `#[test]` function | Public-API touchpoint |
|---|---|---|
| Same trace_id queried twice → equal decisions | `same_trace_id_queried_twice_yields_equal_decisions` | Two calls to `sample` with same trace_id → equal `Decision` |
| Same trace_id 100 times → variance zero | `same_trace_id_across_one_hundred_queries_never_flips_decision` | 100 calls to `sample` → all-Keep or all-Drop |
| Same trace_id under different rates → may differ | `same_trace_id_under_different_rates_may_yield_different_decisions` | Trace at rate 0.0 → Drop; same trace at rate 1.0 → Keep |

DISTILL state: all three panic. DELIVER slice 04 turns them GREEN
(the same code lands as slice 03's body; slice 04 promotes the
property to an explicit invariant).

## Slice 05 — Logs and metrics passthrough (US-SI-05)

Binary: `crates/sieve/tests/slice_05_logs_metrics_passthrough.rs`

| BDD scenario | `#[test]` function | Public-API touchpoint |
|---|---|---|
| Log record passes through unchanged | `a_log_record_passes_through_unchanged_at_rate_zero` | `SamplingSink::accept(SinkRecord::Logs(...))` forwards to inner |
| Metric data point passes through unchanged | `a_metric_data_point_passes_through_unchanged_at_rate_zero` | `SamplingSink::accept(SinkRecord::Metrics(...))` forwards to inner |
| 100 logs all pass through | `one_hundred_log_records_pass_through_at_rate_zero` | 100 logs → inner sink receives 100 logs |
| Type-level: SamplingSink<RecordingSink, HeadSampler>: OtlpSink | `sampling_sink_implements_otlp_sink_for_recording_inner` | Compile-time assertion |

DISTILL state: tests #1, #2, #3 panic on `SamplingSink::new`'s
`unimplemented!()`; test #4 passes (compile-time assertion has no
runtime body). DELIVER slice 05 turns the three panicking tests
GREEN.

## Slice 06 — Sampling decision observability (US-SI-06)

Binary: `crates/sieve/tests/slice_06_observability.rs`

| BDD scenario | `#[test]` function | Public-API touchpoint |
|---|---|---|
| Kept error trace emits DEBUG event | `a_kept_error_trace_emits_a_debug_kept_error_bearing_event` | `accept` emits DEBUG event with target="sieve" message containing "kept (error-bearing)" |
| Dropped non-error trace emits DEBUG event | `a_dropped_non_error_trace_emits_a_debug_dropped_event` | `accept` emits DEBUG event with target="sieve" message containing "dropped" |
| Periodic INFO summary | `periodic_summary_emits_info_event_with_counts_and_rate` | `__test_summary_tick_now(&sink)` emits INFO event with `kept`, `dropped`, `rate` fields |
| Default rate from unset env var | `head_sampler_from_env_defaults_to_zero_point_one_when_var_is_unset` | `HeadSampler::from_env()` → rate 0.1 |
| Non-numeric env value rejected | `head_sampler_from_env_rejects_non_numeric_value` | `HeadSampler::from_env()` with invalid env → `Err(RateUnparseable)` |
| Out-of-range env value rejected | `head_sampler_from_env_rejects_out_of_range_value` | `HeadSampler::from_env()` with rate=1.5 env → `Err(RateOutOfRange)` |

DISTILL state: tests #1, #2, #3 panic on `SamplingSink::new`'s
`unimplemented!()`; tests #4, #5, #6 pass (the `from_env`
constructor is real). DELIVER slice 06 turns the three panicking
tests GREEN.

The three env-var tests serialise via `#[serial_test::serial]`
because `SIEVE_NON_ERROR_TRACE_RATE` is process-global.

## Invariant binaries

### `tests/invariant_public_api_smoke.rs`

| `#[test]` function | Asserted public-API touchpoint |
|---|---|
| `decision_keep_and_drop_are_distinct_values` | `Decision::Keep != Decision::Drop` |
| `keep_reason_error_bearing_and_sampled_are_distinct_values` | `KeepReason::ErrorBearing != KeepReason::Sampled` |
| `head_sampler_new_accepts_zero_one_and_half` | `HeadSampler::new(0.0)`, `new(1.0)`, `new(0.5)` succeed |
| `head_sampler_new_returns_rate_out_of_range_for_two_point_zero` | `HeadSampler::new(2.0) → Err(RateOutOfRange)` |
| `trace_view_exposes_trace_id_and_spans` | `__test_trace_view(...)` returns `TraceView<'_>`; `trace_id()` and `spans()` are accessible |
| `sampling_sink_is_publicly_named_with_otlp_sink_probe_and_sampler_bounds` | Compile-time bound check |

DISTILL state: all six pass. These tests do not exercise
panicking production methods.

### `tests/invariant_sampling_sink_is_otlp_sink_and_probe.rs`

| `#[test]` function | Asserted public-API touchpoint |
|---|---|
| `sampling_sink_is_otlp_sink_and_probe_for_any_compatible_inner_and_sampler` | `SamplingSink<RecordingSink, HeadSampler>: OtlpSink + Probe` (compile-time) |
| `sampler_trait_has_locked_method_signature` | `Sampler::sample(&self, &TraceView<'_>) -> Decision` (compile-time) |

DISTILL state: both pass. Compile-time assertions; no runtime body.

## Cross-cutting traceability

| Story | Slices covered | Walking skeleton? |
|-------|----------------|-------------------|
| US-SI-01 | slice 01 | ✓ |
| US-SI-02 | slice 02 | (focused) |
| US-SI-03 | slice 03 | (focused) |
| US-SI-04 | slice 04 | (focused) |
| US-SI-05 | slice 05 | ✓ (passthrough trio is user-centric: "logs / metrics keep their fidelity") |
| US-SI-06 | slice 06 | (focused) |

Walking skeletons (3): slice 01's two `Decision::Keep`/`Drop`
assertions and slice 05's "log passes through unchanged"
constellation. Both are demo-able to Riley: "the sampler kept the
error trace; the log passed through".

Focused scenarios (33): slices 02-06 (across rate boundaries,
multi-span traces, env-var error paths, observability vocabulary)
plus the two invariant binaries.

Total: 36 `#[test]` functions across 8 binaries.

## Mandate compliance grep witnesses

### CM-A — public-surface imports only

```text
$ grep -hE '^use sieve' crates/sieve/tests/*.rs | sort -u
use sieve::SamplingSink;
use sieve::__test_summary_tick_now;
use sieve::{
use sieve::{Decision, HeadSampler, Sampler};
```

### CM-B — no technical jargon in `#[test]` names

```text
$ grep -hE '^\s*fn (test_)?[a-z]' crates/sieve/tests/*.rs | head -20
```

(Results: every name reads as a sentence in business language —
"an error-bearing trace is kept at rate zero", "same trace_id
queried twice yields equal decisions", "a metric data point passes
through unchanged at rate zero".)

### CM-C — walking-skeleton vs focused split

3 walking skeletons + 33 focused scenarios = 36 total. Ratio is in
the `nw-test-design-mandates` recommended range.

### CM-D — pure-function inventory

| Pure function | Module | Adapter boundary |
|---|---|---|
| `is_error_bearing(spans)` | `sampler` (pub(crate)) | None — pure CPU |
| `HeadSampler::sample` | `sampler` | None — pure CPU |
| `HeadSampler::rate` | `sampler` | None — accessor |
| `parse_summary_tick_ms_from_env` | `aggregator` | std::env (impure side isolated to the parser; pure logic is the integer parse + zero-rejection rule) |
| `emit_summary(kept, kept_err, dropped, rate)` | `observability` | `tracing` (impure side isolated to the macro; pure logic is the field formatting) |

Impure code is bounded to:

- `tokio::time::interval` (timer task body)
- `std::env::var` (rate / tick env-var reads)
- `tracing::debug!` / `tracing::info!` macros
- The inner sink's `accept` / `probe` (delegated)

Fixture parametrization in `tests/common/mod.rs` applies only to the
inner-sink adapter (`RecordingSink`); slice tests do not parametrize
across environments because Sieve has no environment to parametrize
across.
