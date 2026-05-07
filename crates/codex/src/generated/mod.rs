//! Generated artefacts.
//!
//! Per ADR-0023 §1 the regenerator is an `xtask` Rust binary that reads
//! the upstream `opentelemetry-semantic-conventions = "=0.27"` crate
//! and emits `semconv_0_27.rs` — a `pub(crate) const SEMCONV_0_27:
//! &[BlessedAttribute] = &[...]` slice, sorted alphabetically by
//! attribute name so PR diff on regeneration is minimal.
//!
//! ## DELIVER state — Slice 02 landed
//!
//! `semconv_0_27` carries the full upstream OTel semconv 0.27
//! resource-class corpus (132 entries). The catalogue constructor in
//! `catalogue.rs` concatenates this slice with the hand-maintained
//! house-attributes slice; the two slices stay separate so a
//! regeneration that misbehaves cannot accidentally clobber the
//! house attributes (per ADR-0023 §3).
//!
//! To regenerate after an upstream pin bump, run:
//!
//! ```sh
//! cargo run --package regenerate-codex-corpus --bin regenerate-codex-corpus
//! ```

pub(crate) mod semconv_0_27;
