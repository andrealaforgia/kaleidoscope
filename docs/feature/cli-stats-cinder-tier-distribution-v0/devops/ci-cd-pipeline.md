# CI/CD Pipeline - `cli-stats-cinder-tier-distribution-v0`

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19
- **Workflow file**: `.github/workflows/ci.yml` (existing; ZERO
  edits required)
- **Contract source**: ADR-0005 (five-gate CI contract)
- **Branching**: Trunk-Based Development

## Posture

Inherits the ADR-0005 five-gate contract UNCHANGED. **ZERO
workflow edits**. Fourth consecutive wave to realise the payoff
of the per-package Gate 5 job `gate-5-mutants-kaleidoscope-cli`
(added by commit 2baa05c): its `--in-diff` path filter on
`crates/kaleidoscope-cli/**` (ci.yml:1006) automatically picks
up this feature's diff on `src/lib.rs` (new `stats_with_tiers()`,
DD1) and `src/main.rs` (one-line `run_stats` repoint from
`stats` to `stats_with_tiers`, DD1). Gate 1 auto-discovers the
new acceptance test via its `[[test]]` block in
`crates/kaleidoscope-cli/Cargo.toml`.

## Per-gate mapping to outcome KPIs

| Gate | Tool | Owns (for this feature) | KPI(s) enforced |
|------|------|--------------------------|-----------------|
| Gate 4 - `cargo deny check` | `cargo-deny` | Zero new external deps. DD5 RCA + DESIGN handoff confirm: all used types (`Tier`, `ItemId`, `FileBackedTieringStore`, `TieringStore`, `NoopRecorder` alias `CinderRecorder`) already in `kaleidoscope-cli`'s use list. The `cinder`, `lumen`, `aegis`, `self-observe`, `pulse` workspace deps cover `stats_with_tiers()`'s needs. | none directly |
| Gate 1 - `cargo test --workspace --all-targets --locked` | `cargo test` | New `tests/stats_cinder_tier_distribution.rs` (OK1 + OK2 + OK3 + OK4 across populated-multi-tier, hot-only, tenant-isolation, orphan-cinder, backwards-compat scenarios) + UNMODIFIED `tests/stats_subcommand.rs` (predecessor oracle for OK4; DISCUSS D10) + existing `tests/observe_otlp_flag.rs` + `tests/observe_otlp_cinder_wiring.rs` + `tests/observe_otlp_read_flag.rs` + `tests/ingest_and_read_roundtrip.rs` (cross-feature non-regression). | **OK1**, **OK2**, **OK3**, **OK4** |
| Gate 2 - `cargo public-api` | `cargo-public-api` | kaleidoscope-cli is a binary; not graduated. New public `stats_with_tiers()` fn is additive (parallel to `ingest`, `read`, `stats`). | none |
| Gate 3 - `cargo semver-checks` | `cargo-semver-checks` | Same as Gate 2. Additive fn; `publish = false` keeps in-tree. | none |
| Gate 5 - `cargo mutants` (existing `gate-5-mutants-kaleidoscope-cli`, A1 INHERITED) | `cargo-mutants` | Mutation of new `stats_with_tiers()` in `src/lib.rs` AND the `run_stats` repoint in `src/main.rs` via `--in-diff` cascade (`origin/main` -> `HEAD~1` -> full). 100% kill rate required. Key mutation classes: `if count > 0` guard, `match tier` key map, array-literal order, `list_by_tier(..).len()` arithmetic. | Test-quality probe supplementing OK1/OK2/OK3/OK4. The locked `tests/stats_subcommand.rs` is the strongest oracle for `if count > 0` guard mutants. |

## The workflow change - none

There is no YAML snippet to copy. The DELIVER commit touches
**only**:

- `crates/kaleidoscope-cli/src/lib.rs` (new `pub fn
  stats_with_tiers(tenant, data_dir, writer) -> Result<usize,
  Error>` per DD1; reuses `lumen_base`, `cinder_base`, the
  quiescent `LumenToPulseRecorder`, the `CinderRecorder` alias,
  `FileBackedLogStore::open`, `FileBackedTieringStore::open`, the
  inherited Lumen-side body, and existing `Error::LumenOpen` /
  `Error::LumenQuery` / `Error::CinderOpen` / `Error::Io`
  variants per DD5)
- `crates/kaleidoscope-cli/src/main.rs` (one-line change:
  `run_stats` now calls `stats_with_tiers(..)` instead of
  `stats(..)`; the original `stats` arm at the dispatcher,
  `parse_positional` helper, and `print_usage` are unchanged)
- `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs` (new)
- `crates/kaleidoscope-cli/Cargo.toml` (one `[[test]]` block)

No in-tree caller updates required for the library side: the
existing `stats()` continues to exist and continues to be called
by the locked `tests/stats_subcommand.rs`.

**`.github/workflows/ci.yml` is byte-untouched.** The job's
`--in-diff` filter scales naturally as additional features
touch the crate (fourth consecutive proof).

## Gates and hooks NOT modified

- Gate 4: zero new external deps; DESIGN handoff confirms all
  types already imported.
- Gate 1: auto-discovers via `[[test]]` (A2).
- Gates 2/3: binary crate; not graduated.
- Existing Gate 5 jobs (harness, aperture, spark, sieve, codex,
  self-observe, kaleidoscope-cli): independent, unchanged. The
  kaleidoscope-cli job runs on commits touching
  `crates/kaleidoscope-cli/**` and short-circuits otherwise.
- Prism Gates 6-11: out of scope (Rust-only commit).
- `scripts/hooks/pre-commit`: no edit (test auto-discovered).
- `scripts/hooks/pre-push`: no edit (kaleidoscope-cli is binary;
  no per-pkg loop entry).

Trunk-Based Development: every push to `main` triggers the full
pipeline including the existing Gate 5 job for kaleidoscope-cli.
Per memory `project_kaleidoscope_pure_trunk_based`: CI is
feedback, not a gate.

## Summary

| Question | Answer |
|----------|--------|
| Is the existing 5-gate workflow sufficient? | Yes, with ZERO modifications. |
| Which gate enforces each KPI? | Gate 1 -> OK1/OK2/OK3/OK4. Existing Gate 5 -> supplemental probe. |
| New workflow files | NONE |
| Modifications to existing workflow | NONE (prior wave's `gate-5-mutants-kaleidoscope-cli` inherits via `--in-diff`) |
| Modifications to hooks | NONE |
| New CI dependencies | NONE |
| Files touched by DELIVER commit | `crates/kaleidoscope-cli/src/{lib,main}.rs` + `tests/stats_cinder_tier_distribution.rs` (new) + `Cargo.toml` (one `[[test]]`) |
