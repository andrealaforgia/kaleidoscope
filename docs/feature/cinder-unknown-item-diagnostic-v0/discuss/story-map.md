# Story Map: cinder-unknown-item-diagnostic-v0

## User: Priya Raman — Platform SRE operating Cinder via `kaleidoscope-cli`

## Goal: When I name an item id that does not exist, the error names that id back to me exactly as I typed it (quoted), so I read it as a not-found and not an internal fault

## Backbone

| Invoke a cinder command with an item id | Hit a not-found | Read the diagnostic | Recover |
|---|---|---|---|
| `migrate <t> <dir> <id> <tier>` | id was never placed | stderr names the id | re-run with corrected id |
| `get-tier <t> <dir> <id>` | id was never placed | stderr names the id | check tenant / placement |

---

### Walking Skeleton

The single end-to-end slice that connects all four backbone activities:
operator invokes `migrate` (or `get-tier`) with an unplaced id ->
fail-closed exit 1 -> stderr renders the bare quoted id (`"ghost"`)
matching the CLI-help contract, with no `ItemId(` leak -> operator
recognises not-found and recovers. This IS the whole feature — there is
exactly one slice.

### Release 1 (= the only release): The diagnostic names the id you typed

- **Tasks**: fix the single `MigrateError::UnknownItem` Display arm so
  both `migrate` and `get-tier` unknown-item paths emit the bare quoted
  id; confirm known-item and exit-code behaviour unchanged.
- **Outcome KPI**: zero internal-type-name leaks in the unknown-item
  diagnostic; doc-vs-code contract mismatch on this message closed
  (1 -> 0). Verifier K18 (UC-TIER-008/009) flips GREEN.
- **Rationale**: both subcommands share one Display arm, so they ship
  together as one coherent contract fix. Splitting them would create two
  partial slices of the same one-line change — strictly worse.

---

## Priority Rationale

One slice, one priority. The migrate and get-tier paths are NOT separate
deliverables: they are two callers of a single
`MigrateError::UnknownItem` Display arm (`crates/cinder/src/store.rs:57`).
Fixing the arm fixes both atomically. Slicing by subcommand would split
an indivisible one-line change into two dependent halves with no
independent value — an anti-pattern (feature-first / technical-layer
slicing). The correct slice is by user outcome: "the diagnostic names the
id you typed", which spans both subcommands by construction.

## Scope Assessment: PASS — 1 story, 1 bounded context (cinder error Display, surfaced by kaleidoscope-cli), estimated < 1 day

Elephant Carpaccio taste tests:

- **Oversized signals**: 0 of 5. 1 story (< 10), 1 module touched
  (< 3 contexts — the change is in `cinder`; `kaleidoscope-cli` only
  re-renders), walking skeleton needs 0 new integration points, effort
  well under 2 weeks, single user outcome.
- **One-slice correctness**: the change is a single Display-arm string.
  It cannot be meaningfully thinned further without shipping a
  non-working partial message. One slice is the right grain — do NOT
  over-slice.
