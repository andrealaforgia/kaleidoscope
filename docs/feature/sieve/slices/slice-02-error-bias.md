# Slice 02 — Error-Bias Detection

## Outcome added

Any span in a trace carrying `status.code == ERROR` makes the entire
trace error-bearing, and error-bearing traces are retained at 100%
regardless of the configured non-error rate. This is the rule that
makes Sieve operationally useful: operators trust that nothing they
need for incident analysis is silently dropped on the sampling floor.

## What it lights up across the journey

- The error-bias predicate is a first-class function in the crate,
  testable in isolation
- The retention rule "error-bearing → keep, regardless of rate" is
  encoded once and reused by every later slice
- The test harness now exercises a richer fixture vocabulary
  (multi-span traces, mixed status codes) that subsequent slices
  inherit

## Demo command

```bash
cargo test -p sieve --test slice_02_error_bias
```

Returns GREEN. Asserts: a trace with one ERROR span among many OK
spans is kept at rate `0.0`; a trace with all OK spans is dropped at
rate `0.0`; the predicate `is_error_bearing(&trace)` returns the
expected boolean for both.

## Acceptance summary

- A free function `is_error_bearing(spans: &[Span]) -> bool` exists
  and returns true if any span has `status.code == ERROR`
- `HeadSampler::sample` consults the predicate first; if true,
  `Decision::Keep` is returned without consulting the rate
- The slice-01 walking-skeleton test still passes unchanged
- A new integration test exercises three fixtures: all-OK trace at
  rate `0.0` (drop), one-ERROR trace at rate `0.0` (keep),
  all-ERROR trace at rate `0.0` (keep)
- Mutation testing on the modified files passes at 100% kill rate

## Complexity drivers

- How `Span` represents `status.code`. Mirror the OTLP shape exactly
  (an enum or `i32` matching the proto values) so Aperture
  integration is a no-op later.
- The fixture builder needs a way to set status code per span; keep
  the helper additive rather than reshaping slice-01's helper.

## Out of scope

- Rate-based sampling of non-error traces (slice 03)
- Determinism of the sampling decision (slice 04)
- Logs and metrics (slice 05)
- Observability of the keep-because-error decision in tracing
  events (slice 06)
- HTTP-status or other framework-specific error signals (rejected
  in wave-decisions.md Q3)
