# DISCUSS Decisions — cli-unknown-flag-rejection-v0

## Origin

This feature rebuilds the contract behind defect K11 of the EDD black-box
verifier (severity low, provenance class). The contract
"kaleidoscope-cli rejects an unknown flag" was anchored on commit e7fbee0,
which was dropped en bloc by e3a8cad ("revert: drop overnight session —
methodology violation"). The verifier marked K11 `held`: it will not
declare the contract satisfied against a reverted anchor. The feature
exists to give K11 a fresh, non-reverted anchor it can re-verify against.

## Grounding (the real "before", observed from source)

The CLI argument parser was read directly at
`crates/kaleidoscope-cli/src/main.rs` and `Cargo.toml`. Findings:

- **Parser shape**: hand-rolled. `main` does
  `std::env::args().collect()` then `match args.get(1).map(String::as_str)`.
  The module doc states verbatim: "Argument parsing is hand-rolled to keep
  the dependency graph tiny — clap would be the convention but a
  two-subcommand positional CLI does not earn it."
- **clap presence**: NONE. `Cargo.toml` dependencies are aegis, cinder,
  lumen, pulse, self-observe, serde_json. No clap, no structopt, no argh.
- **Subcommand handlers** (`run_read`, `run_stats`, `run_migrate`,
  `run_place`, `run_get_tier`, `run_list_items`, `run_evaluate_policy`,
  `run_ingest`) parse their own flags by scanning argv for a fixed known
  set: `parse_observe_otlp` looks only for `--observe-otlp`,
  `parse_flag_iso` looks only for `--since` / `--until`. Any token they do
  not recognise is simply skipped by the scan loop.

### Observed behaviour per scenario (derived from `fn main`, deterministic)

| Invocation | Code path | Exit | stderr | Verdict |
|---|---|---|---|---|
| `kaleidoscope-cli --bogus` | `Some(other)` arm (main.rs:70) — `--bogus` is not `--help`/`-h`, so it is treated as an unknown subcommand | `2` | `kaleidoscope-cli: unknown subcommand "--bogus"` + full usage block | Already rejected. Message wording says "subcommand", not "flag". |
| `kaleidoscope-cli bogus-subcommand` | `Some(other)` arm (main.rs:70) | `2` | `kaleidoscope-cli: unknown subcommand "bogus-subcommand"` + full usage block | Already rejected. |
| `kaleidoscope-cli read acme /tmp/x --bogus` | `run_read` → `parse_observe_otlp` / `parse_time_range` scan loops skip `--bogus` silently | `0` (or whatever the real read returns) | NO usage error; `--bogus` is ignored | **SILENT ACCEPTANCE — the real code gap.** |

> Runtime verification command to be run by DESIGN/DELIVER for the anchor:
> `cargo build -p kaleidoscope-cli` then
> `./target/debug/kaleidoscope-cli --bogus ; echo "exit=$?"`,
> `./target/debug/kaleidoscope-cli bogus-subcommand ; echo "exit=$?"`,
> `./target/debug/kaleidoscope-cli read acme /tmp/x --bogus ; echo "exit=$?"`.
> The first two are predicted exit=2 with a usage error; the third is
> predicted exit=0 with the flag silently ignored (the gap).

### Surviving K11 contract traces in docs

`grep -ri "unknown flag|unrecognized|unrecognised|K11|unknown subcommand"`
over `docs/` found NO surviving artefact from the reverted K11 contract.
The matches that exist (codex, query-api-label-matchers-v0, aperture)
belong to unrelated features. The contract must be rebuilt from scratch;
there is nothing to restore.

## CODE-to-fix vs RE-ANCHOR-only — the load-bearing distinction

This feature is **mixed**, and the split governs DESIGN/DELIVER size:

- **US-01 (unknown top-level flag) — RE-ANCHOR-only.** Behaviour already
  correct (exit 2 + usage). The only open question is whether the message
  saying "unknown subcommand" for a `--bogus` token is acceptable, or
  whether DESIGN wants flag-shaped tokens named as "unknown flag". An
  acceptance test is missing; that is the deliverable.
- **US-03 (unknown subcommand) — RE-ANCHOR-only.** Behaviour already
  correct (exit 2 + usage). Acceptance test missing.
- **US-02 (unknown flag on a real subcommand) — genuine CODE gap.** Today
  `read acme /tmp/x --bogus` silently ignores `--bogus` and exits 0. The
  subcommand flag scanners must learn to reject tokens they do not
  recognise. This is the only behavioural change the feature requires.
- **US-04 (regression) — RE-ANCHOR-only.** Known flags and subcommands
  must keep working; the US-02 fix must be additive.

Net: roughly 75% re-anchor (write the missing acceptance tests that give
K11 a fresh anchor) and one small additive code change in US-02 (teach the
hand-rolled subcommand scanners to detect unconsumed `--`-prefixed
tokens). No clap migration is proposed; the hand-rolled parser is honoured.

## Key Decisions

- [D1] Feature type: User-facing (CLI). Decision 1 = User-facing.
- [D2] No walking skeleton. Decision 2 = No. Brownfield CLI with an
  existing parser; the feature is a thin behavioural completion plus
  acceptance coverage, not an end-to-end skeleton.
- [D3] Research depth: Lightweight. Decision 3 = Lightweight. The journey
  is a single operator typing a wrong flag and reading the error.
- [D4] JTBD skipped. Decision 4 = No. The job is self-evident: "tell me I
  mistyped so I do not believe a command ran."
- [D5] Honour the hand-rolled parser. No clap dependency is introduced.
  Rationale: the grounding shows a deliberate, documented choice to avoid
  clap; the gap is narrow (one scan-loop class) and does not justify a
  dependency and parser rewrite. DESIGN may revisit, but the recommended
  shape is additive.
- [D6] No ADR. Rationale: this restores a CLI contract that already
  existed before the revert; it is not a new architectural decision.
  Recommended to DESIGN as "no ADR".

## Requirements Summary

- Primary operator need: when an operator mistypes a flag, option, or
  subcommand, the CLI must fail loudly (non-zero exit + usage error on
  stderr) rather than accept it silently or panic, so the operator knows
  the command did not do what they intended.
- Walking skeleton scope: not applicable (Decision 2 = No).
- Feature type: user-facing (CLI).

## Constraints Established

- The hand-rolled parser stays; no clap migration.
- The US-02 fix must be additive: every currently-accepted known flag and
  subcommand keeps working byte-for-byte (US-04 regression guard).
- The exact stderr message shape and exit code are observable contracts
  the verifier will assert on; DESIGN must pin them (see flags to DESIGN).
- British English in artefacts; no em dashes in body prose; no crate
  bumped to 1.0.0.

## Flags to DESIGN

1. **clap vs manual + code-gap vs re-anchor (most important).** Resolved
   by grounding: parser is manual, no clap; US-02 is a code gap, the rest
   is re-anchor. DESIGN decides the additive fix shape for US-02 (detect
   unconsumed `--`-prefixed tokens in each subcommand scanner, or a shared
   helper that validates argv against the known-flag set per subcommand).
2. **Exit code pin.** Current top-level rejection uses `ExitCode::from(2)`.
   US-02 today is the catch-all `ExitCode::FAILURE` (1) path once a fix
   makes it error. DESIGN pins whether unknown-flag rejection should be a
   uniform code (e.g. 2, matching the top-level and the clap convention)
   or whether subcommand-level rejection may use the generic failure code
   (1). The verifier will assert on this; it must be a single pinned value
   per scenario.
3. **stderr message shape.** Top-level today emits
   `kaleidoscope-cli: unknown subcommand "<x>"` plus the FULL usage block.
   DESIGN pins (a) whether flag-shaped unknown tokens say "unknown flag"
   vs "unknown subcommand", and (b) whether subcommand-level rejection
   prints the full usage block or a single usage line. The verifier
   asserts a substring; DESIGN must name the exact substring.
4. **ADR? Recommended NO** (D6) — restored CLI contract, not a new
   decision.

## Upstream Changes

- None. No DISCOVER or DIVERGE artefacts exist for this feature; it
  originates from the EDD verifier defect log, recorded above under Origin.
