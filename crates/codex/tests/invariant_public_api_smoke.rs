//! Invariant — public-API smoke
//!
//! Compile-time assertion that Codex's public surface is exactly the
//! five types ADR-0022 locks: `SchemaCatalogue`, `BlessedAttribute`,
//! `LintReport`, `LintViolation`, `ViolationKind`. Mirrors Sieve's
//! `invariant_public_api_smoke.rs` shape (ADR-0018 precedent).
//!
//! Compile-time checks: each type is referenced via `use codex::{...}`;
//! if any of the five is renamed or removed, this binary fails to
//! compile, breaking Gate 1 cleanly. The runtime body is a no-op
//! marker test so the binary has at least one `#[test]` Cargo expects.
//!
//! Other surface-stability invariants live in `cargo public-api`
//! Gate 2 and `cargo semver-checks` Gate 3. This binary is the
//! cheapest in-process companion that catches accidental renames
//! during DELIVER.

use codex::{BlessedAttribute, LintReport, LintViolation, SchemaCatalogue, ViolationKind};

fn _public_surface_compiles(
    _: SchemaCatalogue,
    _: BlessedAttribute,
    _: LintReport,
    _: LintViolation,
    _: ViolationKind,
) {
}

#[test]
fn the_public_surface_compiles_against_five_types() {
    // The use statements above plus `_public_surface_compiles` are
    // the actual invariant. This test body is a marker so Cargo
    // produces a `[[test]]` binary with one runnable test.
    let _catalogue = SchemaCatalogue::new();
}
