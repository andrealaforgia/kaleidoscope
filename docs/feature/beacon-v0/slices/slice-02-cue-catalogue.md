# Slice 02 — CUE rule catalogue (US-BE-02)

## Goal

Scale the CUE loader from one rule to a catalogue. Defensive
diagnostics on broken rules; reloading on SIGHUP.

## IN scope

- CUE schema documented in `crates/beacon/cue/rule.cue`
- Multi-file loader walking the `--rules` directory tree
- Per-rule validation with file + line + field diagnostics
- Edit-distance "did you mean" suggestion for unknown fields
  (re-uses Codex's nearest-blessed-match helper, or duplicates the
  pattern if cross-crate dependency is unwanted)
- `SIGHUP` triggers reload; the previous catalogue stays active
  until the new one validates
- Exit code 0 if at least one rule loaded; non-zero if all rejected
- Integration test `slice_02_cue_catalogue.rs` with a 50-file
  corpus (5 broken, 45 valid)

## OUT scope

- Anything beyond rule loading (eval / inhibition / sinks remain
  unchanged from slice 01)

## Learning hypothesis

Disproves "the CUE schema scales to a real-world catalogue without
silent failures". Risk: the CUE binding library's diagnostic
shape may not expose line numbers, forcing us to fork or replace.
Mitigation: spike before slice 02 to confirm.

## Acceptance criteria

US-BE-02 AC-2.1 through AC-2.6.

## Dependencies

- A robust CUE library. Candidate: `cue` Rust binding via FFI. If
  unworkable, fall back to a hand-written CUE subset parser that
  supports the documented schema only (no general CUE expressions).
