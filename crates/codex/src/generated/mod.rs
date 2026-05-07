//! Generated artefacts.
//!
//! Per ADR-0023 §1 the regenerator is an `xtask` Rust binary that reads
//! the upstream `opentelemetry-semantic-conventions = "=0.27"` crate
//! and emits `semconv_0_27.rs` — a `pub(crate) const SEMCONV_0_27:
//! &[BlessedAttribute] = &[...]` slice, sorted alphabetically by
//! attribute name so PR diff on regeneration is minimal.
//!
//! ## DISTILL state
//!
//! Empty module declaration. The real `semconv_0_27.rs` file lands at
//! Slice 02 DELIVER alongside the xtask binary that produces it. The
//! Slice 01 walking-skeleton test does not touch this module — its
//! seed (the two-attribute corpus `service.name` + `tenant.id`) lives
//! inline in `catalogue.rs` per ADR-0023 §3's separation between the
//! generated semconv slice and the hand-maintained house-attributes
//! slice.
