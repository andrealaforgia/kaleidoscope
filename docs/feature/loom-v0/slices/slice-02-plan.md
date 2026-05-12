# Slice 02 — `loom plan` (US-LO-02)

## Goal

Compute the per-rule diff between a source directory (Git working
tree) and a destination directory (deployed rules). Output is
deterministic, operator-readable, and machine-parseable.

## IN scope

- `plan --from <src> --to <dst>` subcommand
- Load both directories via `beacon::load_rules`; key the diff by
  `rule.name`
- Output: per-rule `+ added` / `- removed` / `~ changed` lines
  plus a `summary:` footer
- `--diff` flag adds per-field deltas under each changed rule
- Determinism property test (KPI 2)

## OUT scope

- Apply (slice 03)
- JSON output (slice 04)

## Learning hypothesis

Disproves "the plan output is operator-readable AND
machine-parseable simultaneously". Risk: per-field diff
serialisation may turn ugly with deeply nested fields (sinks
inside rules). Mitigation: keep the format flat; deep nesting is
collapsed to one line with `key=value` pairs.
