# Slice 05 — Logs and Metrics Passthrough

## Outcome added

Sieve's pipeline forwards logs and metrics records unchanged. Only
traces are filtered. After this slice, Sieve is a complete v0
pipeline component: an operator can drop it in front of any OTel
backend and all three signals reach the backend, with traces
sampled and logs/metrics intact.

## What it lights up across the journey

- The crate's surface now covers all three OTLP signal types (traces,
  logs, metrics), even if only traces have non-trivial behaviour
- The shape of the passthrough path is locked, so v1 logs/metrics
  sampling can be added by replacing the passthrough body without
  reshaping the trait surface
- The integration test harness can now assert "logs in == logs
  out" and "metrics in == metrics out" — properties that any
  future log/metric sampling slice will need to preserve as
  defaults

## Demo command

```bash
cargo test -p sieve --test slice_05_logs_metrics_passthrough
```

Returns GREEN. Asserts: a logs batch in produces an identical logs
batch out; a metrics batch in produces an identical metrics batch
out; a traces batch in is sampled per slices 02–04.

## Acceptance summary

- The Sieve crate exposes whatever the v0 pipeline shape needs for
  logs and metrics (free functions, methods, or a wider trait —
  pick the smallest shape that the integration test can call)
- For logs: the input batch is returned unchanged (same record
  count, same record content)
- For metrics: the input batch is returned unchanged
- For traces: behaviour from slices 02–04 is preserved
- All previous slices' tests still pass
- Mutation testing on modified files passes at 100% kill rate

## Complexity drivers

- Whether logs/metrics passthrough is a separate function or a
  variant of a unified `process(signal)` entry point. The
  unified shape is more honest about Sieve being a pipeline
  stage; the separate-function shape is simpler for v0. Pick
  whichever the Aperture integration shape suggests.
- Avoiding accidental allocation or clone on the passthrough
  path. v0 doesn't need to be allocation-free, but a cloning
  passthrough establishes a bad performance precedent.

## Out of scope

- Log severity filtering (v1)
- Metric aggregation reduction (v1)
- Per-signal sample rates (v1)
- Dropping unsampled logs/metrics on the floor (rejected in
  wave-decisions.md Q6)
