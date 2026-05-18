# CI/CD Pipeline - `cli-read-observe-otlp-v0`

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19
- **Workflow file**: `.github/workflows/ci.yml` (existing; ZERO
  edits required by this feature)
- **Contract source**: ADR-0005 (five-gate CI contract)
- **Branching**: Trunk-Based Development (project default)

## Posture

Inherits the ADR-0005 five-gate contract UNCHANGED. **ZERO
workflow edits**. This is the case where the prior wave's
investment in the per-package Gate 5 job
`gate-5-mutants-kaleidoscope-cli` (added by commit 2baa05c)
pays off in full: the job's `--in-diff` path filter on
`crates/kaleidoscope-cli/**` automatically picks up this
feature's diff on `src/lib.rs` (the `read()` signature
extension + match body) and `src/main.rs` (the `run_read`
dispatcher gains a `parse_observe_otlp` call) on the merge
commit. Gate 1 auto-discovers the new acceptance test via
its `[[test]]` block. No other gate is affected.

## Per-gate mapping to outcome KPIs

| Gate | Tool | Owns (for this feature) | KPI(s) enforced |
|------|------|--------------------------|-----------------|
| Gate 4 - `cargo deny check` | `cargo-deny` | Dependency policy. The wiring edit adds ZERO new external deps. The `self-observe` import line at `crates/kaleidoscope-cli/src/lib.rs:65` already names `LumenToOtlpJsonWriter` (used by `ingest`); the new `read()` body reuses that import unchanged. | none directly (transitive: regression in deny.toml blocks merge) |
| Gate 1 - `cargo test --workspace --all-targets --locked` | `cargo test` | Acceptance tests: `tests/observe_otlp_read_flag.rs` (new, OK1 + OK2 + OK3) + `tests/observe_otlp_flag.rs` (existing, cross-feature non-regression for the `ingest` side) + `tests/observe_otlp_cinder_wiring.rs` (existing, cross-feature non-regression for the Cinder wiring side). | **OK1**, **OK2**, **OK3** |
| Gate 2 - `cargo public-api` | `cargo-public-api` | `kaleidoscope-cli` is a binary; not graduated to Gate 2. The `read()` signature gains a fourth `Option<&Path>` parameter (DD3), but the crate is not on the public-api graduation list. | none |
| Gate 3 - `cargo semver-checks` | `cargo-semver-checks` | Same as Gate 2. The signature extension IS a breaking change at the source level, but `publish = false` (Cargo.toml:9) keeps it in-tree only; in-tree callers (the binary + the existing `ingest_and_read_roundtrip.rs` test) are updated atomically. | none |
| Gate 5 - `cargo mutants` (existing per-package job `gate-5-mutants-kaleidoscope-cli`, A1: INHERITED, no edit) | `cargo-mutants` | Mutation of the wiring surface in `crates/kaleidoscope-cli/src/lib.rs` + `src/main.rs` via the existing job's `--in-diff` cascade (`origin/main` -> `HEAD~1` -> full). 100% kill rate per ADR-0005 Gate 5 + CLAUDE.md. | Test-quality probe supplementing OK1/OK3. |

## The workflow change - none

There is no YAML snippet for Crafty to copy. The DELIVER commit
touches **only**:

- `crates/kaleidoscope-cli/src/lib.rs` (the `read()` body +
  signature extension per DD3)
- `crates/kaleidoscope-cli/src/main.rs` (the `run_read`
  dispatcher gains a `parse_observe_otlp` call; the
  `print_usage` text gains one mention of `--observe-otlp` on
  the `read` line)
- `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs`
  (new)
- `crates/kaleidoscope-cli/Cargo.toml` (one `[[test]]` block)
- Optionally one in-tree caller update if
  `tests/ingest_and_read_roundtrip.rs` calls `read()`
  positionally (it does; the new `None` final argument is
  appended).

**`.github/workflows/ci.yml` is byte-untouched.** This is the
explicit case the prior wave anticipated: with
`gate-5-mutants-kaleidoscope-cli` already in place and
`--in-diff`-scoped to `crates/kaleidoscope-cli/**`, any future
wiring feature within the crate inherits Gate 5 coverage at
zero workflow-edit cost. The amortisation horizon is now
realised on the second wiring feature.

## Gates and hooks NOT modified

- Gate 4 (`cargo deny`): zero new external deps.
- Gate 1 (`cargo test --workspace`): auto-discovers new test
  via `[[test]]` block (A2).
- Gates 2/3: kaleidoscope-cli is a binary; not graduated.
- Existing Gate 5 jobs (harness, aperture, spark, sieve,
  codex, self-observe, kaleidoscope-cli): all independent, all
  unchanged. The kaleidoscope-cli job runs in parallel with
  the others on commits that touch `crates/kaleidoscope-cli/**`
  and short-circuits to a zero-second exit otherwise.
- Prism Gates 6-11: out of scope (Rust-only commit).
- `scripts/hooks/pre-commit`: no edit (test auto-discovered).
- `scripts/hooks/pre-push`: no edit (kaleidoscope-cli is
  binary; no per-pkg loop entry).

Trunk-Based Development: workflow already encodes TBD; every
push to `main` triggers the full pipeline including the
existing Gate 5 job for kaleidoscope-cli. Per memory
`project_kaleidoscope_pure_trunk_based`: CI is feedback, not
a gate.

## Summary

| Question | Answer |
|----------|--------|
| Is the existing 5-gate workflow sufficient? | Yes, with ZERO modifications. |
| Which gate enforces each KPI? | Gate 1 -> OK1/OK2/OK3. Existing Gate 5 job -> supplemental test-quality probe. |
| New workflow files | NONE |
| Modifications to existing workflow | NONE (the prior wave's `gate-5-mutants-kaleidoscope-cli` job inherits this feature via `--in-diff`) |
| Modifications to hooks | NONE |
| New CI dependencies | NONE |
| Files touched by DELIVER commit | `crates/kaleidoscope-cli/src/lib.rs` + `crates/kaleidoscope-cli/src/main.rs` + `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` (new) + `crates/kaleidoscope-cli/Cargo.toml` (one `[[test]]` block) + optional caller update in `tests/ingest_and_read_roundtrip.rs` |
