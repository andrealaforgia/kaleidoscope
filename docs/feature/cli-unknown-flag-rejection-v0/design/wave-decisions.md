# DESIGN Decisions — cli-unknown-flag-rejection-v0

## Origin

This feature re-anchors EDD verifier defect K11 ("kaleidoscope-cli rejects
an unknown flag"), whose anchor was dropped in revert e3a8cad, and closes
the one genuine code gap behind it: silent acceptance of unknown flags
**inside** subcommands. DISCUSS was delivered by Luna (commit 0043386).
Morgan read `crates/kaleidoscope-cli/src/main.rs`, `Cargo.toml`, and the
existing acceptance suite directly to ground every decision below.

## Key Decisions

### DD1 — Fix shape: one shared helper, not N per-subcommand checks

The hand-rolled parser lives entirely in `crates/kaleidoscope-cli/src/main.rs`.
There is no pre-existing shared argv validator. Three scan helpers consume
flags positionally without rejecting anything: `parse_observe_otlp`
(main.rs:217), `parse_flag_iso` (main.rs:274), and the positional getters
`parse_positional` (main.rs:442) plus the inline `args.get(N)` calls. None
rejects a `-`-prefixed token it does not recognise; the scan loops simply
skip it. That is the silent accept (the single defect class).

Decision: add ONE shared helper

```text
fn reject_unknown_flags(args: &[String], known: &[&str]) -> Result<(), Box<dyn std::error::Error>>
```

that scans `args[2..]` (the post-subcommand tail), and for every token
beginning with `-` that is not in `known` and is not a value consumed by a
known flag, returns a usage error. Each subcommand wrapper calls it once
with its own `known` set. Rationale: the ONLY thing that varies per
subcommand is the known-flag set; the detection logic is identical. One
helper plus eight one-line call sites is smaller and less mutable than
duplicating the same loop eight times, and gives the verifier a single
behavioural seam. The grounding flag (helper vs per-subcommand) is resolved
in favour of the helper because the source shows the variation is data
(the flag set), not control flow.

Known-flag set per subcommand (read from main.rs wrappers):

| Subcommand | Positionals (argv index) | Known flags |
|---|---|---|
| ingest | tenant(2) data_dir(3) | `--observe-otlp` |
| read | tenant(2) data_dir(3) | `--observe-otlp` `--since` `--until` |
| stats | tenant(2) data_dir(3) | `--since` `--until` |
| migrate | tenant(2) data_dir(3) item_id(4) to_tier(5) | `--observe-otlp` |
| place | tenant(2) data_dir(3) item_id(4) tier(5) | `--observe-otlp` |
| evaluate-policy | data_dir(2) hot(3) warm(4) | `--observe-otlp` |
| get-tier | tenant(2) data_dir(3) item_id(4) | (none) |
| list-items | tenant(2) data_dir(3) tier(4) | (none) |

`--observe-otlp`, `--since`, `--until` each take a value; that value is
itself an argv token but is consumed by the known flag, so the helper must
skip the token following a value-taking known flag (see DD-rule below and
application-architecture.md "Positional vs flag rule").

### DD2 — Exit code: pinned to 2 for all unknown-flag and unknown-token rejection

The top-level unknown-subcommand arm (main.rs:70-74) already returns
`ExitCode::from(2)`. Pin subcommand-level unknown-flag rejection to the
SAME code 2, for consistency: an operator's eye and a script's `$?` see one
"you got the command line wrong" code regardless of where the bad token
sits. This requires the wrapper to surface the unknown-flag error as exit 2
rather than the generic `ExitCode::FAILURE` (1) that `main`'s `Err(e)` arm
currently produces. The verifier asserts exit 2 on the US-02 path, so the
helper's error must be routed to a 2 exit (see application-architecture.md
"Changes Per File").

### DD3 — stderr usage shape: name the token, use "unknown flag", emit usage

The top-level path emits `kaleidoscope-cli: unknown subcommand "<x>"`
followed by the full usage block. For subcommand-level rejection of a
`-`-prefixed token, pin the wording to **"unknown flag"** (not
"subcommand"), naming the verbatim token, then the usage block, matching
the top-level shape (prefix `kaleidoscope-cli: `, the offending token
quoted, usage on stderr). Pinned observable substring the verifier asserts:

```text
kaleidoscope-cli: unknown flag "--bogus"
```

For the top-level `--bogus` case the existing message already says
`unknown subcommand "--bogus"`; that string is unchanged (US-01 re-anchor),
and the verifier asserts only that stderr names the unknown token `--bogus`
and exit is 2, which the existing arm already satisfies. The new wording
"unknown flag" applies to the subcommand-internal case (US-02), which is
the new code.

### DD4 — No ADR

Confirmed NO (DISCUSS D6). This restores and extends an existing CLI
contract behind a reverted anchor; it introduces no new architectural
decision, no new dependency, no new component. ADR immutability is honoured
(no ADR added or modified).

### DD-rule — Positional vs flag (the load-bearing rule)

A subcommand token is classified as the helper scans `args[2..]`:

1. A token equal to a known flag for that subcommand is a FLAG. If it is a
   value-taking flag (`--observe-otlp` / `--since` / `--until`), the NEXT
   token is its value and is consumed (skipped, never re-classified).
2. A token that begins with `-` (i.e. `-` or `--` prefix) and is neither a
   known flag nor the consumed value of one is an UNKNOWN FLAG -> reject,
   exit 2, usage error naming the token.
3. Any other token is a POSITIONAL and is left untouched. The existing
   positional parsers (`parse_positional`, `args.get(N)`) keep owning
   positional extraction unchanged.

Edge cases, pinned from source (no speculative support added):

- `--` separator: NOT supported today, NOT added (no positional value here
  legitimately starts with `-`). A bare `--` token would itself be rejected
  as an unknown flag. This is acceptable and documented; adding `--`
  handling is out of scope.
- Bare `-`: rejected as an unknown flag (begins with `-`, not known). No
  subcommand reads from stdin via a `-` positional (ingest reads stdin
  unconditionally), so nothing legitimate is broken.
- Negative-value positionals: NONE exist. All positionals are tenant ids,
  filesystem paths, item ids, tier literals (`hot`/`warm`/`cold`), or
  non-negative integer seconds (`evaluate-policy` parses `u64`). No
  subcommand accepts a value that legitimately begins with `-`, so rule 2
  has no false-positive surface.

## Architecture Summary

- Pattern: hand-rolled CLI argument dispatch in a single binary entry
  point (`main.rs`). Honoured as-is per DISCUSS D5; no clap migration.
- Paradigm: Rust idiomatic (data + free functions), per project CLAUDE.md.
  The fix is one free function plus eight call sites.
- Shape: the binary is a driving adapter over the `kaleidoscope_cli`
  library. This feature touches ONLY the driving adapter (argv validation),
  not the library core. No library function signature changes.
- Key change: a single `reject_unknown_flags(args, known)` validation seam
  invoked by each subcommand wrapper before it does any I/O, so rejection
  happens fail-fast (before any Lumen/Cinder store is opened), matching the
  fail-before-store-open invariant the existing `read_time_range.rs` OK4
  tests already assert.

## Reuse Analysis

| Existing Component | File | Overlap | Decision | Justification |
|---|---|---|---|---|
| Top-level unknown-subcommand arm | main.rs:70-74 | Already exits 2 + usage for a bad first token | REUSE | US-01/US-03 are correct today; replicate its exit-2 + usage shape for the subcommand case, add acceptance tests only |
| `parse_observe_otlp` scan loop | main.rs:217 | Iterates `args[2..]`, recognises `--observe-otlp`, skips the rest | EXTEND (informs) | The new helper reuses this loop's skip-the-value-token logic for value-taking flags; the scanner itself is unchanged |
| `parse_flag_iso` scan loop | main.rs:274 | Iterates `args[2..]`, recognises `--since`/`--until` | EXTEND (informs) | Same: the helper learns which flags take a following value from these scanners' behaviour |
| Subcommand wrappers (`run_read`, `run_stats`, `run_migrate`, `run_place`, `run_get_tier`, `run_list_items`, `run_evaluate_policy`, `run_ingest`) | main.rs | Each parses its own argv; none rejects unknown tokens | EXTEND | Add one `reject_unknown_flags(args, KNOWN)?` call per wrapper before the existing parse; ~1 line each, no rewrite |
| `write_usage` | main.rs:92 | Emits the usage block to a sink | REUSE | The unknown-flag error path emits the same usage block; no new usage text |
| Subprocess acceptance harness | tests/read_time_range.rs (bin(), Command, Stdio), tests/cli_binary_smoke.rs | Spawns `CARGO_BIN_EXE_kaleidoscope-cli`, asserts exit/stderr/stdout | EXTEND (pattern) | New K11 acceptance tests follow this exact harness shape (inline helpers per DISCUSS D7); no shared harness extraction |
| K11 acceptance tests | (none — lost in revert e3a8cad) | n/a | CREATE NEW | Justification: nothing to restore (DISCUSS grep found zero surviving artefacts); these fresh subprocess tests ARE the re-anchor K11 re-verifies against |

No unjustified CREATE NEW: the only new artefact is the acceptance test
suite, which is the feature's whole reason to exist (the fresh anchor).

## Technology Stack

- Language: Rust (existing). No new crate, no new dependency.
- NO clap migration. Rationale: a clap migration would be a large parser
  rewrite touching every subcommand's option model and every error message,
  re-deriving exit codes and usage text, and would risk the US-04
  regression surface across the whole CLI. The targeted fix is one helper
  function plus eight one-line call sites: smaller, lower-risk, and it
  honours the deliberate documented "clap does not earn it" choice
  (main.rs:17-21, DISCUSS D5). DESIGN explicitly rejects clap for this
  feature.
- Test framework: Rust built-in `#[test]` with `std::process::Command`
  subprocess harness, already in use (`read_time_range.rs`,
  `cli_binary_smoke.rs`). No new test dependency.

## Constraints Established

- The fix is strictly additive: every currently-accepted known flag and
  subcommand keeps working byte-for-byte (US-04 regression guard). The
  helper only rejects tokens no scanner consumes.
- Rejection is fail-fast: `reject_unknown_flags` runs before any store is
  opened, so no Lumen/Cinder filesystem side effect occurs on a rejected
  invocation (consistent with the existing OK4 fail-before-store-open
  probe).
- Exit code 2 and stderr substring `unknown flag "<token>"` are observable
  contracts the verifier asserts; pinned in DD2 / DD3.
- British English; no em dashes in body prose; no crate bumped to 1.0.0.

## DEVOPS Handoff

- No new crate, no new dependency, no new workflow. DEVOPS is slim.
- Mutation testing: the existing `gate-5-mutants-kaleidoscope-cli` workflow
  covers the change via `--in-diff` scoping (per ADR-0005 Gate 5, 100% kill
  rate). The new helper and its call sites are in `src/main.rs`, already in
  that workflow's scope; the new subprocess acceptance tests provide the
  observable kills for body-deletion and condition-flip mutants on the
  helper. No mutation-config change required.
- No external integration; no contract tests required (this is a local
  argv parser, no network or third-party boundary).
- CI is feedback, not a gate (project is pure trunk-based).

## Upstream Changes

- None. No DISCUSS assumption is changed. The grounding's "fix shape (helper
  or per-subcommand)" flag is resolved here (DD1: helper) without altering
  any user story, acceptance criterion, or KPI.
