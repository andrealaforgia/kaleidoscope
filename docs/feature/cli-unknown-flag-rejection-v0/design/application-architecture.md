# Application Architecture — cli-unknown-flag-rejection-v0

Scope: the `kaleidoscope-cli` binary driving adapter (argv validation
only). No library core change, no new dependency, no ADR. This document
pins the exact fix the DELIVER crafter implements (RED -> GREEN) and the
observable CLI contract the EDD verifier re-anchors K11 against.

## Current parsing

All argument handling is hand-rolled in
`crates/kaleidoscope-cli/src/main.rs`. `main` (main.rs:55-84) collects
`std::env::args()` and matches on `args[1]` (the subcommand verb). The
top-level path is already correct:

- `main.rs:66-69` — `--help` / `-h` / no args -> print usage, exit 0.
- `main.rs:70-74` — `Some(other)` arm: any other first token (including
  `--bogus`) is treated as an unknown subcommand, prints
  `kaleidoscope-cli: unknown subcommand "<x>"` plus the full usage block,
  exits 2. This already covers US-01 (`--bogus`) and US-03 (`stat`).

The gap is inside the subcommands. Each `run_*` wrapper parses its own
argv with scan loops that recognise a fixed known-flag set and skip every
other token:

- `parse_observe_otlp` (main.rs:217-231) iterates `args.iter().skip(2)`,
  returns the value after `--observe-otlp`, and silently ignores any token
  that is not `--observe-otlp`. This is the loop that swallows `--bogus`.
- `parse_flag_iso` (main.rs:274-291), called by `parse_time_range`
  (main.rs:265-272), iterates `args.iter().skip(2)` for `--since` /
  `--until`, ignoring everything else.
- `parse_positional` (main.rs:442-448) and the inline `args.get(N)` calls
  (e.g. main.rs:333-334, 360-363, 392, 408-409, 437) read positionals by
  fixed index and never look at unrecognised trailing tokens.

Concrete silent-accept path for `read acme /tmp/x --bogus`: `run_read`
(main.rs:233) -> `run_read_with` (main.rs:242) calls `parse_positional`
(consumes `acme`, `/tmp/x`), `parse_observe_otlp` (sees `--bogus`, not
`--observe-otlp`, skips it), `parse_time_range` (sees `--bogus`, not
`--since`/`--until`, skips it), then runs `read(...)` and exits 0. `--bogus`
is never observed by anything. Same shape in all eight wrappers.

## The fix

Add ONE shared helper to `main.rs`, called by each subcommand wrapper
before it does any parsing or I/O:

```text
fn reject_unknown_flags(
    args: &[String],
    known: &[&str],
) -> Result<(), Box<dyn std::error::Error>>
```

Behaviour (no implementation prescribed beyond this contract; the crafter
owns the body):

- Scan `args[2..]` (the post-subcommand tail) left to right.
- When the current token equals a known value-taking flag
  (`--observe-otlp` / `--since` / `--until`), skip it AND the next token
  (its value). The helper is told which of `known` take a value, or it
  treats the three value-taking flags as a fixed set; either is acceptable
  so long as the value token is not re-classified.
- When the current token equals a known no-value flag, skip it.
- When the current token begins with `-` and is not a known flag (nor a
  consumed value), return an error whose `Display` is
  `unknown flag "<token>"`, naming the verbatim token.
- Otherwise the token is a positional; ignore it (the existing positional
  parsers still own extraction).

Each wrapper gains one line, e.g. in `run_read_with`:

```text
reject_unknown_flags(args, &["--observe-otlp", "--since", "--until"])?;
```

placed before the existing `parse_positional` call. The error must be
routed to exit code 2 (see "Changes Per File").

This is additive: the helper only returns `Err` for a token that no
existing scanner consumes, so every currently-valid invocation passes
through unchanged (US-04).

## Positional vs flag rule

A token in `args[2..]` is classified exactly once:

1. Equals a known flag for this subcommand -> FLAG. If value-taking
   (`--observe-otlp` / `--since` / `--until`), the following token is its
   VALUE and is consumed (never re-classified).
2. Begins with `-` (covers `-` and `--` prefixes) and is neither a known
   flag nor a consumed value -> UNKNOWN FLAG -> reject (exit 2, usage
   error naming the token).
3. Anything else -> POSITIONAL -> untouched; existing parsers own it.

Pinned edges (verified against source; nothing speculative added):

- `--` separator: not supported today, not added. A bare `--` would be
  rejected by rule 2. No legitimate positional starts with `-`, so no
  separator is needed. Out of scope.
- Bare `-`: rejected by rule 2. No subcommand uses `-` as a positional
  (ingest reads stdin unconditionally), so nothing valid breaks.
- Negative / dash-leading positional values: NONE exist. Positionals are
  tenant ids, paths, item ids, tier literals (`hot`/`warm`/`cold`), and
  non-negative `u64` seconds. Rule 2 therefore has zero false-positive
  surface against valid input.

## Changes Per File

| File | Change |
|---|---|
| `crates/kaleidoscope-cli/src/main.rs` | Add `reject_unknown_flags(args, known)` free function. Add one call per subcommand wrapper (`run_ingest`, `run_read_with`, `run_stats_with`, `run_migrate_with`, `run_place_with`, `run_evaluate_policy_with`, `run_get_tier_with`, `run_list_items_with`) with that subcommand's known-flag set (table in wave-decisions.md DD1). Route the unknown-flag error to exit code 2: the wrapper signals an unknown-flag error distinctly from a generic failure so `main` returns `ExitCode::from(2)` rather than `ExitCode::FAILURE` for this case. Add inline `#[cfg(test)]` unit tests over `reject_unknown_flags` for `cargo mutants` coverage of the new seam. |
| `crates/kaleidoscope-cli/tests/` (new acceptance test file, e.g. `unknown_flag_rejection.rs`, registered as a `[[test]]` in `Cargo.toml`) | New subprocess acceptance tests for US-01..US-04 (see "Verification"). These ARE the fresh K11 anchor. Inline harness helpers per DISCUSS D7 (no shared `tests/common` extraction). |
| `crates/kaleidoscope-cli/Cargo.toml` | Add one `[[test]] name/path` entry for the new acceptance file. No dependency change. |

No library (`src/lib.rs`) change. No function signature change. No new
crate.

## CLI contract

Observable behaviour the verifier asserts (input -> exit code + stderr):

| Input | Exit | stderr | Status |
|---|---|---|---|
| `kaleidoscope-cli --bogus` | 2 | `kaleidoscope-cli: unknown subcommand "--bogus"` + usage block (names `--bogus`) | Already correct (US-01 re-anchor) |
| `kaleidoscope-cli bogus-subcommand` | 2 | `kaleidoscope-cli: unknown subcommand "bogus-subcommand"` + usage block | Already correct (US-03 re-anchor) |
| `kaleidoscope-cli --help` | 0 | full usage block | Already correct (US-01 boundary) |
| `kaleidoscope-cli` (no args) | 0 | full usage block | Already correct (US-03 boundary) |
| `kaleidoscope-cli read acme <data_dir> --bogus` | 2 | `kaleidoscope-cli: unknown flag "--bogus"` + usage block; stdout empty; no store opened | FIX (US-02 happy path) |
| `kaleidoscope-cli stats acme <data_dir> --sicne 2026-01-01T00:00:00Z` | 2 | `kaleidoscope-cli: unknown flag "--sicne"` + usage block | FIX (US-02 mistyped known flag) |
| `kaleidoscope-cli read acme <data_dir> --observe-otlp /tmp/m.ndjson --bogus` | 2 | `kaleidoscope-cli: unknown flag "--bogus"` (valid `--observe-otlp` does not mask the adjacent invalid token) | FIX (US-02 adjacency) |
| `kaleidoscope-cli read acme <data_dir>` (one record seeded) | 0 | `read ok: records=1`; stdout has the record | Unchanged (US-04) |
| `kaleidoscope-cli read acme <data_dir> --observe-otlp <path>` | 0 | `read ok: records=...`; stdout records; metric line appended | Unchanged (US-04) |
| `kaleidoscope-cli stats acme <data_dir> --since <iso> --until <iso>` | 0 | windowed summary on stdout | Unchanged (US-04) |
| `kaleidoscope-cli list-items acme <data_dir> hot` | 0 | byte-equivalent to pre-fix | Unchanged (US-04) |

The `kaleidoscope-cli: ` prefix is added by `main`'s `Err(e)` Display path
(main.rs:79-82); the helper's error `Display` contributes the
`unknown flag "<token>"` fragment. The verifier asserts the substring
`unknown flag "<token>"` and exit 2 for the US-02 rows, and the existing
`unknown subcommand` substring plus exit 2 for the US-01/US-03 rows.

## Verification

New subprocess acceptance tests in `crates/kaleidoscope-cli/tests/`,
spawning the real binary via `env!("CARGO_BIN_EXE_kaleidoscope-cli")` with
`std::process::Command` and `Stdio` (the exact harness shape already used
in `read_time_range.rs` tests #5/#6 and `cli_binary_smoke.rs`). These tests
ARE the fresh, non-reverted anchor K11 re-verifies against. RED first
(binary does not yet reject subcommand-internal unknown flags), GREEN after
the crafter adds `reject_unknown_flags`.

- US-01: spawn `--bogus`; assert exit 2 and stderr names `--bogus`. Spawn
  `--help`; assert exit 0 and usage block present.
- US-02 (the code gap): seed one record for `acme`; spawn
  `read acme <data_dir> --bogus`; assert exit 2, stderr contains
  `unknown flag "--bogus"`, stdout empty, and the Lumen store was NOT
  opened (filesystem-absence probe, mirroring the OK4 fail-before-store
  invariant in `read_time_range.rs`). Spawn
  `stats acme <data_dir> --sicne <iso>`; assert exit 2 and stderr names
  `--sicne`. Spawn `read acme <data_dir> --observe-otlp <path> --bogus`;
  assert exit 2 and stderr names `--bogus` (adjacency: a valid flag does
  not mask the invalid one).
- US-03: spawn `stat acme /data`; assert exit 2 and stderr names `stat`.
  Spawn no args; assert exit 0 and usage block.
- US-04 (regression): spawn `read acme <data_dir> --observe-otlp <path>`
  with one seeded record; assert exit 0, stdout has the record, metric
  line appended. Spawn `stats acme <data_dir> --since <iso> --until <iso>`;
  assert exit 0 and summary. Spawn `list-items acme <data_dir> hot`; assert
  exit 0 and byte-equivalent output. These prove the fix is additive.

Inline harness helpers (`bin()`, `temp_root`, `cleanup`, seed) are
duplicated per DISCUSS D7; no shared `tests/common` extraction in this
feature.
