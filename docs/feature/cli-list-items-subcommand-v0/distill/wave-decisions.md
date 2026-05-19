# Wave Decisions — `cli-list-items-subcommand-v0` / DISTILL

Author: `@nw-acceptance-designer` (Quinn), DISTILL wave, 2026-05-19.
Mode: PROPOSE. Paradigm: Rust idiomatic.

This wave authors the RED acceptance harness for the new
`list-items` subcommand: ONE new test file
(`crates/kaleidoscope-cli/tests/list_items_subcommand.rs`) with
four `#[test]` functions covering OK1, OK2, OK3, and the
D-Sort determinism invariant. No production source changes;
the file will not compile because `kaleidoscope_cli::list_items`
does not yet exist — that compile failure IS the RED gate.

## DWD-01 — Four `#[test]` functions, mapped to OK1 / OK2 / OK3 / D-Sort

**Decision.** The five UAT scenarios in `discuss/user-stories.md`
collapse to four `#[test]` functions:

1. `list_items_hot_for_acme_emits_five_sorted_lines_and_excludes_globex`
   — OK1 (correctness on a five-item Hot tier placed in non-lex
   insertion order) PLUS OK2 (cross-tenant Warm decoys for
   `globex` must not surface in `acme`'s Hot listing).
2. `list_items_for_empty_tier_emits_no_lines` — OK1 N=0 case.
3. `list_items_subcommand_invalid_tier_exits_nonzero_with_stderr_naming_value`
   — OK3 (subprocess, `LUKEWARM`, exit non-zero, stderr substring,
   empty stdout, no Cinder side-effect).
4. `list_items_emits_lines_in_lexicographic_order_regardless_of_insertion_order`
   — D-Sort (three items {z-item, a-item, m-item} placed in
   non-lex order; stdout in lex order a, m, z).

**Rationale.** Tenant isolation is naturally expressed as a
decoy on the OK1 happy path, so test #1 carries OK1+OK2.
The UAT "determinism via two successive invocations" scenario
and the dedicated z/a/m-sort scenario both pin the SAME
boundary sort step — test #4 with its non-lex insertion order
is the stronger and more compact realisation, so we ship it
and drop the two-invocations duplicate.

## DWD-02 — Library-direct for #1/#2/#4, subprocess for #3

**Decision.** Tests #1, #2, #4 call
`kaleidoscope_cli::list_items(&acme, &data, "<tier>", &mut buf)`
directly with a `Vec<u8>` writer; test #3 spawns the binary at
`env!("CARGO_BIN_EXE_kaleidoscope-cli")` with
`["list-items", "acme", <data_dir>, "LUKEWARM"]` and asserts
non-zero exit, stderr substring, empty stdout.

**Rationale.** Library-direct exercises the parse + open +
list_by_tier + sort + writeln composition without subprocess
overhead, with exact-bytes-on-stdout assertability. Subprocess
is the only path that exercises the binary boundary (dispatcher
arm, exit-code propagation, `kaleidoscope-cli: {e}` prefix);
OK3 is DEFINED at that boundary. Same split locked by
`migrate_subcommand.rs` DWD-04 in the predecessor wave.

## DWD-03 — Substring (not byte-exact) assertion on OK3 stderr

**Decision.** Test #3 asserts `stderr.contains("LUKEWARM")`,
not byte-exact equality against the full Display string.

**Rationale.** DISCUSS D-StderrWording locked the SUBSTRING
invariant; DESIGN DD5 chose to reuse `Error::InvalidTier`'s
Display verbatim. The substring assertion accommodates both
choices and survives any future wording refactor that
preserves the offending-value-in-line property. Byte-exact
would lock the wording inadvertently.

## DWD-04 — Helpers duplicated inline (TENTH inline duplication)

**Decision.** `tenant`, `temp_root`, `cleanup`, `cinder_base`,
`place_item`, `list_by_tier` (read-side oracle), and `bin`
helpers are duplicated inline at the top of the new test file.
No `tests/common/mod.rs` extraction.

**Rationale.** DEVOPS A2 / DISCUSS D-NewTestFile inherit the
posture of the nine predecessor `tests/*.rs` files. Rule of
three nonuply discharged; cross-file extraction is overdue,
explicitly punted to the next test-touching feature per the
DEVOPS forward-compatibility note. Landing it here would
conflate acceptance authoring with cross-file refactor risk.

## DWD-05 — RED gate proven at compile time

**Decision.** The file imports
`use kaleidoscope_cli::list_items;`. That symbol does not
exist at HEAD; the file will FAIL TO COMPILE. This compile
failure IS the RED gate.

**Rationale.** For a new symbol that does not yet exist on
the library API, compile-failure is the strongest RED shape —
it is impossible for the test to spuriously pass under any
code path. DELIVER closes RED → GREEN by landing
`pub fn list_items(...)` in `lib.rs` (DESIGN DD1) plus the
`Some("list-items")` dispatch arm and `run_list_items` helper
in `main.rs`. No `cargo` execution in this wave per the task
brief hard constraint — DISTILL authors the RED harness;
DELIVER verifies AND closes it.

## Handoff

Next wave: DELIVER (`nw-software-crafter`). Deliverables:
`tests/list_items_subcommand.rs` (NEW, ~484 lines, 4
`#[test]`) + this file. DELIVER closes RED → GREEN by landing
`list_items(...)` in `lib.rs` (DD1), promoting `parse_tier`
to `pub(crate)` (DD4), adding `run_list_items` + dispatch arm
+ `print_usage` paragraph in `main.rs`, plus the `[[test]]`
block in `Cargo.toml` — in ONE atomic commit per ADR-0005
(DEVOPS A2).
