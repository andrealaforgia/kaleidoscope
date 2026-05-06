# Sieve v0 — story map

## Backbone

The single user activity Sieve v0 supports is **trace volume
control with error retention**. The backbone steps Riley (SRE)
walks through:

1. **Configure** — set `SIEVE_NON_ERROR_TRACE_RATE` to a value in
   `[0.0, 1.0]` (default `0.1`).
2. **Wire** — Aperture's pipeline includes Sieve before the sink;
   traces, logs, metrics flow through.
3. **Observe** — Sieve emits per-decision DEBUG events and a
   periodic INFO summary; Riley sees the outcomes.
4. **Verify** — error-bearing traces are always kept; non-error
   traces are kept at the configured rate; logs and metrics pass
   through unfiltered.

## Walking skeleton

The thinnest end-to-end Sieve v0 installation: a Rust integration
test that builds a `HeadSampler::new(0.0)`, runs one error-bearing
trace through it (kept), and one non-error trace through it
(dropped). The test asserts the two `Decision` values directly. No
real Aperture binary, no live OTLP wire — but the typed contract is
exercised end-to-end and the trait shape is locked for subsequent
slices.

## Slices

Six elephant-carpaccio slices, each ≤1 day, each with a named
learning hypothesis, each demoable as a `cargo test` invocation that
returns GREEN.

| Slice | Story | Demo command | Learning hypothesis |
|---|---|---|---|
| 01 walking-skeleton | US-SI-01 | `cargo test -p sieve --test slice_01_walking_skeleton` | The `Sampler` trait is the right shape and `Decision::Keep`/`Decision::Drop` is the right return. If this slice's contract feels awkward to extend in slice 02, the trait shape is wrong. |
| 02 error-bias | US-SI-02 | `cargo test -p sieve --test slice_02_error_bias` | Error-bias retention is the right load-bearing rule; if some operator team needs a different "error" definition we discover here. |
| 03 non-error rate | US-SI-03 | `cargo test -p sieve --test slice_03_non_error_rate` | The `xxh3_64`-based rate decision is statistically honoured to within ±3% on a 10000-trace fixture; if the band is wider, the hash mapping is wrong. |
| 04 trace-id determinism | US-SI-04 | `cargo test -p sieve --test slice_04_trace_id_determinism` | The decision is deterministic per `trace_id` across calls; if a re-query flips the decision, batching breaks trace coherence. |
| 05 logs and metrics passthrough | US-SI-05 | `cargo test -p sieve --test slice_05_logs_metrics_passthrough` | Logs and metrics are unaffected by Sieve at v0; if the pipeline shape forces them through the sampler, slice 05 fails and v0 scope is wrong. |
| 06 observability | US-SI-06 | `cargo test -p sieve --test slice_06_observability` | DEBUG-per-decision plus INFO-summary is the right verbosity; if operators report log flooding or invisibility, Q8 needs revisiting. |

## Carpaccio taste tests

Each slice has been checked against the elephant-carpaccio
discipline:

1. **End-to-end value** — every slice closes with a Rust integration
   test that exercises the public surface and returns GREEN. The
   walking skeleton is the smallest unit of value; subsequent slices
   add capability.
2. **≤1 day ship** — each slice is bounded by one Rust crate change
   plus its integration test; mutation testing on the diff brings
   the kill-rate to 100%. Sister-crate precedents (harness, Aperture,
   Spark) shipped each slice in a single dispatch.
3. **Named learning hypothesis** — every slice has a what-could-go-wrong
   hypothesis (column above); the slice fails fast if the hypothesis
   doesn't hold.
4. **Production-shape data** — fixture traces are real
   `opentelemetry_proto::tonic::trace::v1::Span` objects with
   realistic field values (service.name, status.code, status.message,
   spans linked into traces). No synthetic strings masquerading as
   contract.
5. **Dogfood moment** — slice 01 ships a working sampler; slice 06
   ships an operator-readable summary. The library is dogfoodable
   inside its own integration tests from day one.
6. **IN/OUT scope** — each slice brief at `slices/slice-NN-*.md`
   has explicit IN scope (the contract added) and OUT scope (the
   capability the slice does not address).

No slice "ships 4+ new components". No two slices are
"identical-except-for-scale". No slice runs only on synthetic data.
The carpaccio discipline holds.

## Prioritisation

Execution order is the order listed (01 → 06). Rationale per the
nWave story-mapping discipline:

- **Learning leverage first**. Slice 01 is the trait shape; if it is
  wrong the cost of reshaping cascades across slices 02–06.
- **Dependency chain**. Slice 02 depends on slice 01's `HeadSampler`
  shape. Slice 03 depends on slice 02's `is_error_bearing` shape (the
  rate decision applies only when the trace is not error-bearing).
  Slice 04 confirms a property slice 03 already produces. Slice 05
  is independent of 02-04 and could move earlier, but moving it
  before 02 means slice 02 has to revisit the pipeline shape; safer
  to land 02 first. Slice 06 depends on 02-04 (the events report
  outcomes those slices produce).
- **Dogfood cadence**. After slice 01 the trait exists; after slice
  03 the volume-control story is real; after slice 06 the operator
  has visibility. Each closes with a demoable artefact.

## Slice briefs

Per-slice IN/OUT scope, demo command, complexity drivers in:

- `../slices/slice-01-walking-skeleton.md`
- `../slices/slice-02-error-bias.md`
- `../slices/slice-03-non-error-rate.md`
- `../slices/slice-04-trace-id-determinism.md`
- `../slices/slice-05-logs-metrics-passthrough.md`
- `../slices/slice-06-observability.md`
