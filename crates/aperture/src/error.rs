//! Top-level error type — `ApertureError`, with `thiserror`-derived
//! `Display`/`Error` and exit-code mapping in `main()`.
//!
//! See `docs/feature/aperture/design/component-design.md >
//! error::ApertureError` for the variants and exit-code table. At
//! DISTILL the public type is the simpler `ApertureError(pub String)`
//! in `lib.rs`; DELIVER replaces it with the rich enum here and
//! re-exports it through the crate root.

// SCAFFOLD: true
// Status: DISTILL placeholder. DELIVER lands the
// `#[non_exhaustive] enum ApertureError { ConfigInvalid, ConfigUnreadable,
// ListenerBindFailed, SinkProbeFailed, DrainDeadlineExceeded, Internal }`
// and the per-variant exit-code mapping per the design contract.
