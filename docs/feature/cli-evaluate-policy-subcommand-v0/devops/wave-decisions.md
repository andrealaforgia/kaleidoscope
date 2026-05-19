# DEVOPS — cli-evaluate-policy-subcommand-v0

Author: orchestrator-direct (agent quota exhausted)
Date: 2026-05-19

## A1 — Gate 5 mutation job: INHERIT

`gate-5-mutants-kaleidoscope-cli` covers via `--in-diff` filter
on `crates/kaleidoscope-cli/**`. Thirteenth consecutive zero-
workflow-edit wave on kaleidoscope-cli.

## A2 — Gate 1 auto-discovery
New `[[test]]` block auto-discovered.

## A3 — Zero new external dependencies
All Cinder types already in use list. `std::time::Duration` is
std-only.

## A4 — No new toolchain pin

## KPI-to-gate

| KPI | Gate | Verification |
|-----|------|--------------|
| OK1 success | Gate 1 | Library-direct test seeds aged items, asserts return count + stdout |
| OK2 invalid-secs fail-fast | Gate 1 | Subprocess test with non-numeric arg, asserts non-zero exit + stderr substring |
| OK3 idempotent | Gate 1 | Library-direct test: call twice with same args, second returns 0 |
| OK4 --observe-otlp emission | Gate 1 | Library-direct test counts cinder.migrate.count lines in sink |
| Mutation kill-rate | Gate 5 | existing --in-diff job, 100% target |

## Forward-compat
Test harness extraction overdue (eleventh inline duplication).
Deferred.

A potential `cli-evaluate-policy-dry-run-v0` would extend the
function with a `dry_run: bool` flag, requiring Cinder API
extension. Out of scope here.
