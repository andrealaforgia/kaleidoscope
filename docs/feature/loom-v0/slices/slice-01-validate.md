# Slice 01 — `loom validate` walking skeleton (US-LO-01)

## Goal

The smallest unit of evidence that `loom validate --rules <dir>`
loads a Beacon-shaped TOML rule directory and exits with
operator-readable diagnostics.

## IN scope

- `loom` binary entry point with `clap` parsing for the
  `validate --rules <dir>` subcommand
- Calls `beacon::load_rules(dir)` and maps the outcome to:
  - exit 0 if all rules loaded
  - exit 1 if any rule failed (every diagnostic printed to stderr)
  - exit 2 if directory unreadable
- Stdout summary `validated N rules, rejected M`
- Acceptance test `tests/slice_01_validate.rs` walking a temp dir
  with mixed good and broken rules

## OUT scope

- Plan / apply (slice 02 / 03)
- JSON output flag (slice 04)

## Learning hypothesis

Disproves "Loom can wrap Beacon's loader as a CLI without
re-implementing parsing". Risk is low — Beacon's loader is already
public.
