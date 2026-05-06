# Slice 04 — `trace_id`-Keyed Determinism

## Outcome added

The same `trace_id` always yields the same sampling decision across
calls. Spans of one trace partitioned across multiple OTLP batches —
which happens routinely in real deployments — keep or drop together.
Without this, slice 03's probability mechanism would shred trace
coherence whenever a trace straddles a batch boundary.

## What it lights up across the journey

- The probability source from slice 03 is replaced (or wrapped) by
  a `trace_id`-keyed deterministic mapping
- The trait shape for the sampler holds across this change — slices
  01–03 tests still pass without reshape
- A new test pattern (split a trace across two calls, assert both
  return the same decision) is established that any future
  cross-batch feature will reuse

## Demo command

```bash
cargo test -p sieve --test slice_04_trace_id_determinism
```

Returns GREEN. Asserts: for a fixture trace with `trace_id = T`, two
separate calls to the sampler with disjoint subsets of its spans
both return the same `Decision`; over 10000 distinct `trace_id`
values at rate `0.5`, the kept fraction is still within statistical
bands; the slice-03 distribution test still passes.

## Acceptance summary

- The sampling decision is a deterministic function of `trace_id`
  and the configured rate (and the error-bias rule, which short-
  circuits before the deterministic step)
- Two calls to `HeadSampler::sample` with different span subsets of
  the same `trace_id` always produce the same `Decision`
- The deterministic mapping over `trace_id` space produces a
  distribution close to uniform — the slice-03 statistical test is
  re-run with this new mechanism and still passes
- Slices 01, 02, 03 tests all still pass unchanged
- Mutation testing on modified files passes at 100% kill rate

## Complexity drivers

- The hash function over `trace_id`. A 64-bit hash (e.g. xxHash or
  the standard `Hasher` trait) mapped into `[0.0, 1.0)` is the
  canonical OTel approach. Pin the choice in code comments because
  this is observable behaviour — operators will notice if it
  changes.
- Empty or all-zero `trace_id` handling. OTLP guarantees a
  16-byte `trace_id`, but defensive code should not crash on a
  malformed fixture.
- Whether the rate participates in the hash or just gates the
  comparison. Standard pattern: hash the `trace_id` to a float in
  `[0.0, 1.0)`, keep iff that float < rate. Clear, deterministic,
  rate-change-friendly.

## Out of scope

- Logs and metrics (slice 05)
- Surfacing the kept/dropped decision in tracing events (slice 06)
- Persisting decisions across process restarts (out of v0; head
  sampling is by definition per-call)
