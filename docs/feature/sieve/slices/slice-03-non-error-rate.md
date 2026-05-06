# Slice 03 — Non-Error Sampling Probability

## Outcome added

The configured non-error rate is honoured: at `0.0` all non-error
traces drop; at `1.0` all non-error traces keep; at intermediate
rates approximately the right fraction is kept across a fixture
stream of N traces, asserted within statistical bands.

This is the slice that makes Sieve a sampler in the operational
sense — a knob the operator can turn to control downstream volume.

## What it lights up across the journey

- The crate now has a probability mechanism (a deterministic source
  in tests, real randomness in production)
- The boundary cases (`0.0` and `1.0`) are pinned, so future
  refactors can't silently break them
- A statistical test pattern is established that slice 04 will
  reuse to prove `trace_id` determinism doesn't skew the distribution

## Demo command

```bash
cargo test -p sieve --test slice_03_non_error_rate
```

Returns GREEN. Three assertions: at rate `0.0` over 1000 non-error
traces, 0 are kept; at rate `1.0` over 1000 non-error traces, 1000
are kept; at rate `0.5` over 10000 non-error traces, the kept count
is within ±3% of 5000.

## Acceptance summary

- `HeadSampler::new` accepts a rate `f64` in `[0.0, 1.0]`; values
  outside the range are rejected at construction (return `Result`
  or panic with a clear message — pick one and document it)
- For non-error traces, the sampler keeps with probability equal to
  the rate; the boundary cases `0.0` and `1.0` are exact
- The probability source is injectable for testing (a trait or
  function pointer) so the statistical test can use a seeded RNG
- Slice 02's error-bias test still passes; error-bearing traces
  are kept at every rate including `0.0`
- Mutation testing on modified files passes at 100% kill rate

## Complexity drivers

- The choice of probability source. A `rand` crate dependency is
  the obvious move, but the test path needs determinism. Pick a
  small abstraction (`trait RandomSource` or `Fn() -> f64`) so
  production uses `rand` and tests use a seeded source.
- The statistical band on the `0.5` test: pick a sample size and
  tolerance that makes the test reliably non-flaky (the brief says
  "within X% bands"; 10000 traces at ±3% is one defensible pick).
- Boundary semantics: rate `0.0` must be exactly zero kept (no
  rounding hazards), rate `1.0` must be exactly all kept.

## Out of scope

- Reading the rate from env var (slice 06 wires the env-var path)
- `trace_id`-keyed determinism across batches (slice 04 — this
  slice may use any source, not necessarily `trace_id`)
- Logs and metrics (slice 05)
- Per-service or per-tenant rates (rejected in wave-decisions.md
  Q5)
