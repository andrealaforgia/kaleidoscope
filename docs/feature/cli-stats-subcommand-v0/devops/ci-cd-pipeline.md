# CI/CD Pipeline - `cli-stats-subcommand-v0`

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19
- **Workflow file**: `.github/workflows/ci.yml` (existing; ZERO
  edits required)
- **Contract source**: ADR-0005 (five-gate CI contract)
- **Branching**: Trunk-Based Development

## Posture

Inherits the ADR-0005 five-gate contract UNCHANGED. **ZERO
workflow edits**. Third consecutive wave to realise the payoff
of the per-package Gate 5 job `gate-5-mutants-kaleidoscope-cli`
(added by commit 2baa05c): its `--in-diff` path filter on
`crates/kaleidoscope-cli/**` (ci.yml:1006) automatically picks
up this feature's diff on `src/lib.rs` (new `stats()` + private
ISO 8601 formatter, DD1) and `src/main.rs` (new `run_stats` +
extended `print_usage`, DD-handoff). Gate 1 auto-discovers the
new acceptance test via its `[[test]]` block in
`crates/kaleidoscope-cli/Cargo.toml`.

## Per-gate mapping to outcome KPIs

| Gate | Tool | Owns (for this feature) | KPI(s) enforced |
|------|------|--------------------------|-----------------|
| Gate 4 - `cargo deny check` | `cargo-deny` | Zero new external deps. DD1 rejects adding any datetime crate; the formatter uses `std::fmt::Write` + `u64` arithmetic. The existing `aegis`/`lumen`/`self-observe`/`pulse` workspace deps cover `stats()`'s needs. | none directly |
| Gate 1 - `cargo test --workspace --all-targets --locked` | `cargo test` | New `tests/stats_subcommand.rs` (OK1 + OK2 + OK3 across populated, tenant-isolation, single-record, empty-tenant scenarios) + existing `tests/observe_otlp_flag.rs` + `tests/observe_otlp_cinder_wiring.rs` + `tests/observe_otlp_read_flag.rs` + `tests/ingest_and_read_roundtrip.rs` (cross-feature non-regression). | **OK1**, **OK2**, **OK3** |
| Gate 2 - `cargo public-api` | `cargo-public-api` | kaleidoscope-cli is a binary; not graduated. New public `stats()` fn is additive. | none |
| Gate 3 - `cargo semver-checks` | `cargo-semver-checks` | Same as Gate 2. Additive fn; `publish = false` keeps in-tree. | none |
| Gate 5 - `cargo mutants` (existing `gate-5-mutants-kaleidoscope-cli`, A1: INHERITED) | `cargo-mutants` | Mutation of new `stats()` + private ISO 8601 formatter in `src/lib.rs` AND new `run_stats` in `src/main.rs` via `--in-diff` cascade (`origin/main` -> `HEAD~1` -> full). 100% kill rate. | Test-quality probe supplementing OK1/OK2/OK3. |

## The workflow change - none

There is no YAML snippet to copy. The DELIVER commit touches
**only**:

- `crates/kaleidoscope-cli/src/lib.rs` (new `pub fn stats(...)
  -> Result<usize, Error>` per DD2 + private ISO 8601
  formatter per DD1; reuses existing `lumen_base`, the
  quiescent `LumenToPulseRecorder`, and the existing
  `Error::LumenOpen`/`Error::LumenQuery`/`Error::Io` variants
  per DD4)
- `crates/kaleidoscope-cli/src/main.rs` (new `Some("stats") =>
  run_stats(&args)` arm + `run_stats` helper that calls
  `parse_positional` and forwards `io::stdout().lock()` into
  `stats()` + extended `print_usage` block)
- `crates/kaleidoscope-cli/tests/stats_subcommand.rs` (new)
- `crates/kaleidoscope-cli/Cargo.toml` (one `[[test]]` block)

No in-tree caller updates required: `stats()` is a new function
with no pre-existing callers.

**`.github/workflows/ci.yml` is byte-untouched.** The job's
`--in-diff` filter scales naturally as additional features
touch the crate.

## Gates and hooks NOT modified

- Gate 4: zero new external deps; DD1 avoids the datetime-crate
  question entirely.
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
| Which gate enforces each KPI? | Gate 1 -> OK1/OK2/OK3. Existing Gate 5 -> supplemental probe. |
| New workflow files | NONE |
| Modifications to existing workflow | NONE (prior wave's `gate-5-mutants-kaleidoscope-cli` inherits via `--in-diff`) |
| Modifications to hooks | NONE |
| New CI dependencies | NONE |
| Files touched by DELIVER commit | `crates/kaleidoscope-cli/src/{lib,main}.rs` + `tests/stats_subcommand.rs` (new) + `Cargo.toml` (one `[[test]]`) |
