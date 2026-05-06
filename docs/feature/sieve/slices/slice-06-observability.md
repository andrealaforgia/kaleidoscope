# Slice 06 — Observability of Sampling Decisions

## Outcome added

A tracing-event vocabulary surfaces every sampling decision: "sieve:
trace kept (error-bearing)", "sieve: trace dropped (rate=0.1)",
"sieve: trace kept (rate=0.1, hash=0.07)". The operator can correlate
volume drops in their dashboard with the configured rate and
diagnose unexpected sampling behaviour without rebuilding Sieve.

This slice also wires the env var `SIEVE_NON_ERROR_TRACE_RATE`
(default `0.1`) so the operator's chosen rate is real and the
tracing events can show it.

## What it lights up across the journey

- The operator-facing observability story for v0 is complete —
  Sieve is no longer a black box
- The env-var wiring path exists, so v1 can extend with additional
  knobs (per-tenant rate, scrub rules) on the same pattern
- A self-observability vocabulary is established. Future slices
  (tail sampling, scrubbing) will emit on the same `target =
  "sieve"` so dashboards aggregate cleanly

## Demo command

```bash
SIEVE_NON_ERROR_TRACE_RATE=0.5 \
  cargo test -p sieve --test slice_06_observability -- --nocapture
```

Returns GREEN. The captured tracing output contains a "trace kept"
event for an error-bearing fixture with reason "error-bearing", a
"trace dropped" event for a non-error fixture with the rate value
visible, and a "trace kept" event for a non-error fixture (when the
hash falls below `0.5`) with both the rate and the hash value
visible.

## Acceptance summary

- `SIEVE_NON_ERROR_TRACE_RATE` is read at sampler construction; an
  unset var produces the default `0.1`; an out-of-range or
  unparseable value is rejected with a clear error
- Every sampling decision emits a `tracing` event at `INFO` (or
  `DEBUG` — pick one and document) with a structured field set:
  `decision`, `reason` (one of `error_bearing`, `rate_kept`,
  `rate_dropped`), and where applicable the configured `rate` and
  computed `hash`
- An integration test captures tracing output (via
  `tracing_subscriber`'s test layer or equivalent) and asserts the
  expected events fire for the three fixture cases
- All previous slices' tests still pass with the new event-emitting
  code path
- Mutation testing on modified files passes at 100% kill rate

## Complexity drivers

- The event vocabulary is a public surface. Once operators write
  dashboards against it, changing field names is a breaking change.
  Spend time on naming now.
- Choosing `INFO` vs `DEBUG`. INFO is loud at production rates;
  DEBUG hides decisions when operators want them. Reasonable
  default: DEBUG for per-trace events, INFO for aggregate counts
  (which are out of scope for v0 but worth signposting).
- Env var parsing edge cases: empty string, whitespace, "1.0e0",
  negative values. Lock the parser shape with explicit tests.

## Out of scope

- Aggregate sample-rate metrics (a counter of kept/dropped per
  minute). Useful for v1; the `tracing` events are enough for v0
  diagnostics.
- Dynamic rate changes at runtime (v0 reads at startup)
- Per-tenant or per-service event tagging (no tenant catalogue at
  v0, per wave-decisions.md Q5)
