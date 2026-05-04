//! Composition root — `wire_then_probe_then_use`, `build_app`, the
//! sequenced startup that binds listeners after the sink probe.
//!
//! See `docs/feature/aperture/design/component-design.md > Composition
//! root (compose.rs)` for the design contract. At DISTILL this module
//! is empty; the integration tests reach the composition root only
//! indirectly via `aperture::spawn` (declared in `lib.rs`).

// SCAFFOLD: true
// Status: DISTILL placeholder. DELIVER lands `run` and the
// `wire_then_probe_then_use<T: OtlpSink + Probe>` generic here, with
// the binary `main()` calling `compose::run(config).await`.
