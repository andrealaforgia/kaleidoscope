# Wave Decisions — `cli-migrate-subcommand-v0` / DISTILL

Author: `@nw-acceptance-designer` (Quinn), DISTILL wave, 2026-05-19.
Mode: PROPOSE. Six tests in `tests/migrate_subcommand.rs` covering
OK1/OK2/OK3/OK4 plus tenant isolation across two boundaries
(library-direct + subprocess). Zero edits to locked artefacts.

## DWD-01: Six tests, two boundary layers

| # | Name | Layer | KPI |
|---|------|-------|-----|
| 1 | `migrate_hot_to_cold_emits_transition_line_and_persists_new_tier` | library | OK1 |
| 2 | `migrate_same_tier_is_idempotent_and_emits_from_equals_to_line` | library | OK4 |
| 3 | `migrate_subcommand_unknown_item_exits_nonzero_with_stderr_naming_item` | subprocess | OK2 |
| 4 | `migrate_subcommand_invalid_tier_exits_nonzero_with_stderr_naming_value` | subprocess | OK3 |
| 5 | `migrate_for_one_tenant_does_not_affect_other_tenants_same_item_id` | library | tenant isolation |
| 6 | `migrate_library_direct_unknown_item_returns_err_without_mutating_store` | library | OK2 library companion |

Rationale: library-direct (4/6) calls into a `Vec<u8>` writer and
asserts exact stdout bytes — strictest possible discrimination,
zero fork overhead. Subprocess mandatory for OK2/OK3 because the
`kaleidoscope-cli: {e}` prefix (`main.rs:69`) and the non-zero
exit code are only visible at a real process boundary. Test #6 is
the library-direct OK2 companion (per user task brief): asserts
`Err`, empty writer, no silent insert.

## DWD-02: Inline-duplicated harness helpers (EIGHTH duplication)

`tenant`, `temp_root`, `cleanup`, `cinder_base`, `place_item`,
`read_entry`, `bin()` are duplicated inline. NO extraction to
`tests/common/mod.rs`. Rule-of-three extraction deferred per
DISCUSS D-NewTestFile and DEVOPS forward-compat note; this is the
EIGHTH inline duplication across the cluster. Conflating
acceptance-test landing with cross-file refactor risk is not worth
it in one commit. `record`, `ndjson`, `lumen_base` are omitted —
this feature does not touch Lumen (D-NoLumenTouch).

## DWD-03: Cinder seeding via direct `place()`, never via `ingest`

Every test seeds via `place_item(data_dir, tenant, item_id, tier)`
which opens a `FileBackedTieringStore`, issues one `place()`, then
drops the store (forcing WAL flush). NO test goes through
`kaleidoscope_cli::ingest()` because: (a) `ingest()` places one
Hot item per batch as a side effect of `flush()`
(`lib.rs:243-244`) — coupling the test's intent to batch
arithmetic, (b) it writes Lumen records, violating the
D-NoLumenTouch verification posture, (c) it forces a
`batch-NNNNN`-shaped item id rather than the operator-facing
`acme/batch-00042` shape from the user stories. No Fixture
Theater: `place_item` sets up PRECONDITIONS only; the OK1 post-
condition (`get_entry().tier == Cold`) would fail without the
`migrate()` body running.

## DWD-04: Mandate compliance + error path ratio

- **CM-A (Hexagonal Boundary)**: every library-direct test invokes
  `kaleidoscope_cli::migrate(...)` (the public driving port);
  subprocess tests invoke through the binary's driving port. No
  internal `InMemoryTieringStore::migrate` import. Setup-side
  `FileBackedTieringStore::open` + `place()` are FIXTURE
  infrastructure, not production under test.
- **CM-B (Business Language)**: test names read as operator
  outcomes; inline `// Given / // When / // Then` comments speak
  domain terms ("Priya has placed...", "stdout is the one-line
  transition report"). Technical terms confined to fixture-helper
  bodies.
- **CM-C (User Journey Completeness)**: each test exercises a
  complete operator journey from goal (move item) to observable
  outcome (stdout + post-call tier). Error-path tests #3/#4/#6
  cover fat-finger fail-fast recovery from Domain Examples 3/4.
- **CM-D (Pure Function Extraction)**: the pure function is
  `parse_tier(s) -> Result<Tier, ()>` (DESIGN DD3); acceptance
  exercises it transitively via `migrate()`. Direct unit witnesses
  belong with the inline `mod tests` block in `lib.rs` (mutation
  Gate 5 surface, parallel to `IsoParseError` Display tests).

**Error path ratio**: 4 of 6 tests (66%) target error or boundary
paths — exceeds the 40% gate. OK2 covered twice (boundary layers);
OK3 once; OK4 once; tenant isolation once; happy path once.

## DWD-05: WS strategy — Strategy C (real local adapters)

Walking Skeleton strategy is **Strategy C**: tests #1, #2, #5, #6
use real `FileBackedTieringStore` open + real WAL+snapshot on
`tmp_path`. Tests #3, #4 add the real binary subprocess + real
argv + real exit code + real stderr. NO `@in-memory` adapter
anywhere. The single driven adapter is `FileBackedTieringStore`;
every test exercises real `open()` + real `get_entry()` + real
`migrate()` against a real on-disk WAL.

Dimension 9d litmus test ("would the WS still pass if the real
adapter were deleted?"): NO — the post-call `read_entry()` oracle
reopens the same adapter that produced the WAL; deletion surfaces
as compile failure and runtime mismatch. Wiring is proven honestly.

## Locked-test posture and zero-edit invariants

This wave produces ONE new file and NO edits to:

- `src/lib.rs`, `src/main.rs` (DESIGN/DELIVER's surface)
- `Cargo.toml` (DELIVER adds one `[[test]]` block atomically)
- `.github/workflows/ci.yml` (DEVOPS A1/A2)
- All eight locked test files in `tests/`

The new file is RED until DELIVER lands the `migrate()` library
function, the `run_migrate` dispatcher arm, and the manifest entry
in one atomic commit (DEVOPS A2).

## Handoff

**Next agent**: `nw-software-crafter` (Crafty), DELIVER wave.

| Artefact | Path |
|----------|------|
| Acceptance test file | `crates/kaleidoscope-cli/tests/migrate_subcommand.rs` |
| Wave decisions (this file) | `docs/feature/cli-migrate-subcommand-v0/distill/wave-decisions.md` |
