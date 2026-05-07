//! # Codex — Kaleidoscope's schema authority
//!
//! Codex is the pinned-OTel-semconv-plus-house-attributes catalogue
//! that Spark consults at `init` time to lint a composed Resource for
//! typos and unrecognised attribute names. The catalogue is a static
//! corpus (the v0.27 OpenTelemetry semantic conventions resource set
//! plus three Kaleidoscope-house attributes); the lint surface is one
//! free-function method on the catalogue type.
//!
//! ## Public surface (locked at five types per ADR-0022 §1)
//!
//! - [`SchemaCatalogue`] — the catalogue type. `new()` returns an
//!   owned-by-the-caller value; `validate(&[(&str, &str)])` is the
//!   single behavioural method.
//! - [`BlessedAttribute`] — one entry in the catalogue. Two variants
//!   (`Exact`, `Prefix`) cover the v0 match shapes; `#[non_exhaustive]`
//!   so future match kinds (regex, glob, version-pattern) land
//!   additively without breaking matchers.
//! - [`LintReport`] — the value `validate(...)` returns on `Err`. Carries
//!   one or more [`LintViolation`] entries.
//! - [`LintViolation`] — a single offending attribute: name, kind, and
//!   optional `nearest_blessed_match` (populated by Slice 05 fuzzy
//!   matching).
//! - [`ViolationKind`] — `#[non_exhaustive]`. v0 variants: `Unknown`. v1+
//!   may add `Deprecated`, `Misnamed`, etc.
//!
//! ## Architectural posture
//!
//! - **Library, not service.** Codex v0 has no network surface. Spark
//!   takes Codex as a runtime dep and calls into it by direct API call
//!   (per ADR-0025).
//! - **Zero runtime deps beyond `std`** (per ADR-0024 §3). The upstream
//!   `opentelemetry-semantic-conventions = "=0.27"` crate is consumed
//!   only by the xtask regenerator (per ADR-0023), not at runtime.
//! - **`forbid(unsafe_code)`** at the crate root. Mirrors Spark's
//!   posture (ADR-0011) and Sieve's (ADR-0018).
//! - **AGPL-3.0-or-later.** Symmetric with Aperture and Sieve. The
//!   AGPL on Codex applies to Codex's source; downstream consumers of
//!   Spark (closed-source applications) consume Spark, not Codex, so
//!   the AGPL does not propagate virally across the SDK boundary
//!   (per ADR-0025 §1 / `LICENSING.md`).
//!
//! ## DISTILL state
//!
//! The public surface is real (types and constructors compile and are
//! visible to acceptance tests). The behavioural methods
//! ([`SchemaCatalogue::validate`], [`LintReport`]'s `Display` impl, the
//! internal Levenshtein helper) panic with `unimplemented!()` until the
//! corresponding DELIVER slice lands. The acceptance tests under
//! `tests/slice_*.rs` are the canonical RED state — every slice
//! becomes GREEN one panic at a time.

#![forbid(unsafe_code)]

pub mod catalogue;
pub(crate) mod fuzzy;
pub(crate) mod generated;
pub mod lint;

pub use crate::catalogue::{BlessedAttribute, SchemaCatalogue};
pub use crate::lint::{LintReport, LintViolation, ViolationKind};
