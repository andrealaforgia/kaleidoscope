//! Observability — `tracing-subscriber` JSON layer, closed event-name
//! constants, panic handler, the `aperture::testing::stderr_capture`
//! seam the slice tests subscribe to.
//!
//! See `docs/feature/aperture/design/component-design.md >
//! Observability design` and ADR-0009 for the contract. At DISTILL
//! this module is empty; the test-side seam
//! `common::capture_stderr_events` is `unimplemented!()` and DELIVER
//! lands the production-side hook.

// SCAFFOLD: true
// Status: DISTILL placeholder. DELIVER lands `init_logging()`, the
// `event::*` constants for the closed v0 vocabulary (20 names), the
// `set_hook` panic handler emitting `event=internal_invariant_violation`
// and exit 70, and the `aperture::testing::stderr_capture` test hook
// per the design contract.
