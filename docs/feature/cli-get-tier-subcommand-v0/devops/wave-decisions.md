# DEVOPS — cli-get-tier-subcommand-v0

Author: orchestrator-direct (agent quota exhausted)
Date: 2026-05-19

## A1 — Gate 5 mutation job: INHERIT

The existing `gate-5-mutants-kaleidoscope-cli` job at
`.github/workflows/ci.yml` covers via `--in-diff` filter on
`crates/kaleidoscope-cli/**`. Twelfth consecutive zero-workflow-
edit wave on kaleidoscope-cli.

## A2 — Gate 1 auto-discovery

The new `[[test]]` block in `Cargo.toml` is auto-discovered by
`cargo test --workspace`. No workflow edit.

## A3 — Zero new external dependencies

All Cinder types (`FileBackedTieringStore`, `TieringStore`,
`ItemId`, `Tier`, `NoopRecorder`, `MigrateError`) are already in
the use list (used by `migrate`, `list_items`, and
`stats_with_tiers`).

## A4 — No new toolchain pin

No `rust-toolchain.toml` change.

## KPI-to-gate traceability

| KPI | Gate | Verification |
|-----|------|--------------|
| OK1 success (stdout `tier=<x>`) | Gate 1 | Library-direct test seeds an item and asserts byte-exact stdout |
| OK2 unknown-item fail-fast | Gate 1 | Subprocess test asserts non-zero exit, stderr substring, no store mutation |
| OK3 tenant isolation | Gate 1 | Library-direct test places under tenant A, queries tenant B, expects UnknownItem |
| Mutation kill-rate | Gate 5 | existing `--in-diff` job, 100% target |

## Forward-compat notes

- Test harness rule-of-three extraction is now N=10 inline
  duplications across the kaleidoscope-cli test suite. Next
  test-touching feature should propose extraction to
  `tests/common/mod.rs`. Deferred this wave.
- A potential future `get-entry` subcommand would expose the
  full `TierEntry` (tier + placed_at + migrated_at). Its DESIGN
  would either ship as a richer variant of `get-tier` or as a
  parallel subcommand. Deferred.
