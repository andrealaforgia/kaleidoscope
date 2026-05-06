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
- Every sampling decision emits a `tracing` event at `DEBUG` (per
  wave-decisions Q8) with a structured field set: `decision`,
  `reason` (one of `error_bearing`, `rate_kept`, `rate_dropped`), and
  where applicable the configured `rate` and computed `hash`
- A tokio timer task ticks every 60 seconds (parameterisable down for
  test runs) and emits a single INFO event with `target="sieve"` and
  fields `kept`, `dropped`, `error_bearing`, `rate` summarising the
  decisions accumulated during the window. Per Q8 + KPI 5 this is the
  default-verbosity visibility for operators
- An integration test captures tracing output (via
  `tracing_subscriber`'s test layer or equivalent) and asserts the
  per-trace DEBUG events fire for the three fixture cases AND the
  periodic INFO summary fires once per window with the right field set
- All previous slices' tests still pass with the new event-emitting
  code path
- Mutation testing on modified files passes at 100% kill rate

## Complexity drivers

- The event vocabulary is a public surface. Once operators write
  dashboards against it, changing field names is a breaking change.
  Spend time on naming now.
- Choosing `INFO` vs `DEBUG`. Locked at DISCUSS Q8: DEBUG for
  per-trace events, INFO for the 60-second aggregate summary.
- The summary aggregator: a `Mutex<Counters>` updated by each
  decision; the timer task reads, snapshots, resets, emits the INFO
  event. DESIGN picks the exact synchronisation primitive (Mutex,
  RwLock, atomic counters); DISCUSS locks the contract.
- Env var parsing edge cases: empty string, whitespace, "1.0e0",
  negative values. Lock the parser shape with explicit tests.

## Out of scope

- Dynamic rate changes at runtime (v0 reads at startup)
- Per-tenant or per-service event tagging (no tenant catalogue at
  v0, per wave-decisions.md Q5)
- Exporting the aggregate counters as OTel metrics. The INFO summary
  is the v0 visibility surface; turning the counters into OTel
  metrics is post-v0 (when Sieve becomes self-instrumenting and
  emits its own telemetry through Spark).
