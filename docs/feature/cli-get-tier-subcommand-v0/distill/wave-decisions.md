# DISTILL — cli-get-tier-subcommand-v0

Author: orchestrator-direct (agent quota exhausted)
Date: 2026-05-19

## DWD-01 — Idiom: Rust integration tests, no Gherkin

`crates/kaleidoscope-cli/tests/get_tier_subcommand.rs`. 5
`#[test]` functions with `// Given / // When / // Then` comments
where useful. Mix of library-direct and subprocess per the
established pattern.

## DWD-02 — Real-File substrate

`std::env::temp_dir()` with PID and nanos namespacing, mirroring
the rest of the test cluster. Real `FileBackedTieringStore` on
disk; no in-memory shortcut.

## DWD-03 — Scenario coverage

| Test | OK# | Idiom |
|------|-----|-------|
| `get_tier_returns_lowercase_tier_for_placed_item` | OK1 | library-direct |
| `get_tier_renders_each_tier_as_lowercase_keyword` | OK1 | library-direct (three tiers) |
| `get_tier_library_direct_unknown_item_returns_err_without_writing_stdout` | OK2 | library-direct |
| `get_tier_subcommand_unknown_item_exits_nonzero_with_stderr_naming_item` | OK2 | subprocess |
| `get_tier_for_one_tenant_does_not_see_items_placed_under_another` | OK3 | library-direct |

## DWD-04 — Library vs subprocess split

OK1 and OK3 are library contracts (return value + stdout bytes);
library-direct tests suffice. OK2 has two facets: the library
returns `Err` (test #3) AND the binary surfaces exit code +
stderr (test #4). Both facets need their own test.

## DWD-05 — Out-of-scope confirmations

- No JSON / CSV output.
- No `--observe-otlp` (no MetricsRecorder hook on `get_tier`).
- No modification of locked test files.
- No production source written (DELIVER's job).
- The RED gate: file imports `kaleidoscope_cli::get_tier` which
  does not exist yet. The compile failure is the outside-in
  starting signal.

## Harness duplication note

Tenth inline duplication. Rule-of-three extraction overdue; the
DEVOPS forward-compat note tracks this for the next test-touching
feature to address.
