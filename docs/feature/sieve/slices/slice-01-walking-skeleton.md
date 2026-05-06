# Slice 01 — Walking Skeleton

## Outcome added

A Rust-API call into the Sieve sampler returns a deterministic
keep/drop decision for a trace. The sampler is wired into a test
harness that mimics Aperture's pipeline shape: spans go in, a
sampling decision comes out, kept spans are forwarded, dropped spans
are not.

This is the thinnest end-to-end proof that Sieve exists as a
component: a `Sampler` trait, one concrete implementation, and a
single test that exercises both the keep and drop paths.

## What it lights up across the journey

- The `crates/sieve` crate exists with AGPL-3.0-or-later licence
- The `Sampler` trait is defined and stable enough for slices 02–06
  to extend without reshaping
- Aperture's pipeline has a hook point where a `Sampler` can be
  inserted (companion fixture from Aperture's harness)
- The test harness shape Sieve will use for every subsequent slice
  is established: build a fixture trace, feed it through the
  sampler, assert on the decision

## Demo command

```bash
cargo test -p sieve --test slice_01_walking_skeleton
```

Returns GREEN. Two assertions: one error-bearing trace is kept; one
non-error trace is dropped at configured rate `0.0`.

## Acceptance summary

- `crates/sieve/Cargo.toml` declares the crate, AGPL-3.0-or-later,
  workspace member
- A `Sampler` trait exists with a method that takes a slice of
  spans (a trace) and returns a `Decision::Keep` or `Decision::Drop`
- A concrete `HeadSampler` implements the trait with a fixed
  non-error rate passed at construction
- The walking-skeleton integration test builds two traces (one with
  a span carrying `status.code == ERROR`, one without), runs both
  through `HeadSampler::new(0.0)`, and asserts the error-bearing
  trace is kept and the non-error trace is dropped
- `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and
  mutation testing on the modified files all pass (per ADR-0005)

## Complexity drivers

- The shape of `Sampler::sample` — does it take `&[Span]`, an
  iterator, or borrow Aperture's batch type? Resolve by picking the
  smallest shape that the test harness can construct.
- Where the rate lives: at construction (`HeadSampler::new(rate)`)
  rather than at call site, so subsequent slices don't have to
  reshape the trait.
- The fixture span builder: a small helper in the test module so
  every later slice can construct traces ergonomically.

## Out of scope

- Reading the rate from `SIEVE_NON_ERROR_TRACE_RATE` (slice 03 / 06)
- Statistical assertions across many traces (slice 03)
- `trace_id`-keyed determinism across batches (slice 04)
- Logs and metrics handling (slice 05)
- Tracing-event observability (slice 06)
- Wiring Sieve into the live Aperture binary (post-v0)
