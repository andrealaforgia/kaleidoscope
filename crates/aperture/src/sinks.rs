//! Driven adapters — concrete `OtlpSink` implementations.
//!
//! See `docs/feature/aperture/design/component-design.md > Sinks` for
//! the design contract. At DISTILL this module is empty; the
//! integration tests front the application core with
//! `aperture::testing::RecordingSink` (a test double) and slice-local
//! `BarrierSink` / `SlowSink` test fixtures inside the cap and drain
//! slice tests respectively.

// SCAFFOLD: true
// Status: DISTILL placeholder. DELIVER lands two concrete sinks here:
//   - `StubSink`     — implements OtlpSink + Probe; logs to stderr
//   - `ForwardingSink` — implements OtlpSink + Probe; reqwest -> downstream
// Both are wired through `compose::run` based on `Config::sink.kind`.
