# Slice 04 — Pre-commit + CI integration (US-LO-04)

## Goal

Make Loom usable in pre-commit hooks and CI workflows without
ad-hoc parsing of its output.

## IN scope

- `--json` flag on `plan` and `validate`: structured output for
  tooling
- `--help` text covering every command, flag, and exit code
- Exit-code stability across versions (documented in `--help`)
- Acceptance test parsing the JSON output via `serde_json`

## OUT scope

- GitHub Action / GitLab CI config snippets (operator owns)
- PR-comment-posting integration (operator owns; Loom outputs the
  payload)

## Learning hypothesis

Disproves "Loom's diagnostic format integrates with standard CI
tooling". Risk: JSON output schema may turn unstable across
versions if not carefully versioned. Mitigation: bump a `schema:
"loom.v0"` field at the top of every JSON output.
