# Slice 01 — `place` subcommand bootstraps a single Cinder item

**Story**: US-01
**Outcome KPIs**: OK1-CLI-place-success (principal),
OK2-CLI-place-overwrite-semantics (guardrail),
OK3-CLI-place-invalid-tier-fail-fast,
OK4-CLI-place-observe-otlp-emission
**Tag**: operator-visible (not `@infrastructure` — a real
user-invocable CLI subcommand)
**Estimated effort**: well under 1 day

## Goal

Add a new positional subcommand `kaleidoscope-cli place <tenant>
<data_dir> <item_id> <tier> [--observe-otlp <path>]` that opens
the Cinder store under `<data_dir>/cinder.*`, calls
`cinder::TieringStore::place(&tenant, &ItemId::new(item_id),
tier, SystemTime::now())`, and writes
`placed tenant=<tenant> item=<item_id> tier=<tier>\n` to stdout
on success. On an invalid `<tier>` value, exit non-zero with a
stderr line naming the invalid value. With `--observe-otlp <path>`
set, append exactly one `cinder.place.count` OTLP-JSON line per
place call to `<path>`. Lower-case tier arguments only (`hot` /
`warm` / `cold`). Overwrite-semantics: re-placing an existing
item updates the entry to the new tier with no error and no CLI
special case.

## What ships in this slice

| Artifact | Change |
|----------|--------|
| `crates/kaleidoscope-cli/src/lib.rs` | NEW free function `place(tenant, data_dir, item_id, tier_arg, writer, otlp_log_path) -> Result<(), Error>` (exact signature DESIGN's call per `wave-decisions.md` D-FunctionShape). Reuses the existing private `parse_tier` helper at lines 505-512 (DESIGN's call: lift to `pub(crate)` or duplicate inline) and the existing `tier_lowercase` helper at lines 519-525. No new `Error` variant required (`Error::InvalidTier { value }` at lines 79-81 already covers the parse-side failure; `Error::CinderOpen(MigrateError)` at line 77 covers the store-open failure; `Error::Io(std::io::Error)` at line 82 covers `--observe-otlp` file-open failure). |
| `crates/kaleidoscope-cli/src/main.rs` | NEW `Some("place") => run_place(&args)` arm in the match at lines 54-69. NEW `run_place(args: &[String])` outer function delegating to NEW `run_place_with<O: Write>(args, stdout)` inner function, mirroring the shape of `run_migrate` / `run_migrate_with` at lines 276-306 (parse the four positional args, parse the optional `--observe-otlp` via the existing `parse_observe_otlp` helper at lines 178-192, call the library function, propagate any error). Updated `print_usage` (`write_usage` at lines 86-157) to document the new subcommand's positional argument shape, the lower-case tier contract, the overwrite-semantics contract, and the optional `--observe-otlp <path>` flag. |
| `crates/kaleidoscope-cli/tests/place_subcommand.rs` | NEW file. Mirrors the harness pattern in `tests/migrate_subcommand.rs` and `tests/migrate_observe_otlp_flag.rs` (the predecessor waves' locked files). Hosts five `#[test]` functions translating the five UAT scenarios from `user-stories.md` (happy-path bootstrap, overwrite-semantics over an existing item, invalid-tier fail-fast, --observe-otlp emission, tenant-isolation). |
| `crates/kaleidoscope-cli/Cargo.toml` | NEW `[[test]] name = "place_subcommand", path = "tests/place_subcommand.rs"` entry. The `cinder` and `self-observe` dependencies are already present. |
| Locked test files (all 12 existing `tests/*.rs`) | NOT MODIFIED. Hard constraint from the task brief and from `wave-decisions.md` D-LockedTests. |

## IN scope

- One new CLI subcommand `place` with exactly four positional
  arguments: `<tenant> <data_dir> <item_id> <tier>`, plus one
  optional flag `--observe-otlp <path>`.
- Lower-case tier argument only: `hot` / `warm` / `cold`. Any
  other spelling rejected with stderr naming the invalid value.
- One-line stdout report on success:
  `placed tenant=<tenant> item=<item_id> tier=<tier>\n`.
- Non-zero exit + stderr line on invalid tier argument, stderr
  line contains the verbatim invalid value (via the existing
  `Error::InvalidTier` Display impl at
  `crates/kaleidoscope-cli/src/lib.rs:98-100`).
- Overwrite-semantics: re-placing an existing item updates the
  entry to the new tier with no error, no CLI special case. The
  underlying API at `crates/cinder/src/store.rs:78-81, :140-152`
  is overwrite-semantics; the CLI faithfully reflects this.
- `--observe-otlp <path>` emission: exactly one
  `cinder.place.count` OTLP-JSON line per place call appended to
  `<path>`. File opened ONCE with
  `OpenOptions::create(true).append(true)` per ADR-0039 §8.
  Mirrors the `--observe-otlp` arm of `migrate()` at
  `crates/kaleidoscope-cli/src/lib.rs:435-444`.
- Tenant isolation: `place(acme, ...)` does NOT mutate `globex`'s
  same-named item.
- Cinder-only: the subcommand opens ONLY the Cinder store under
  `<data_dir>/cinder.*`; the Lumen store under
  `<data_dir>/lumen.*` is never opened and is byte-equivalent
  before and after the call.

## OUT of scope

- Bulk placement (multi-item single call). One CLI invocation
  places exactly one item (`wave-decisions.md`
  D-OutOfScope-Bulk).
- Pre-flight existence check / `--no-overwrite` flag. The CLI
  MUST NOT verify that `item_id` doesn't already exist; the
  underlying API is overwrite-semantics by design
  (`wave-decisions.md` D-Overwrite / D-OutOfScope-ExistsCheck).
- `--placed-at <timestamp>` flag for testing. `SystemTime::now()`
  is hard-wired at the call site (`wave-decisions.md`
  D-Timestamp / D-OutOfScope-PlacedAt); deterministic-time
  testing belongs to a separate `TestKit` / spike feature.
- Structured output formats. No `--json` / `--csv` /
  `--format=...`.
- Lumen-side mutation. `place` never opens
  `FileBackedLogStore::open(lumen_base(data_dir), ...)`. The
  Lumen WAL+snapshot is byte-equivalent before and after the
  call (`wave-decisions.md` D-NoLumenTouch /
  D-OutOfScope-LumenMutation).
- Modification of any locked test file (all 12 existing
  `tests/*.rs`). `wave-decisions.md` D-LockedTests makes this a
  hard contract.

## Rejected alternatives

- **Bulk placement shape (`place <tenant> <data_dir>
  <item_ids_file> <tier>`)**: rejected in `wave-decisions.md`
  D-OutOfScope-Bulk. v0 ships the single-item shape; bulk is a
  reasonable v1 once the single-item contract is validated.
- **Pre-flight existence check (`--no-overwrite` flag or implicit
  reject on existing item)**: rejected in `wave-decisions.md`
  D-Overwrite / D-OutOfScope-ExistsCheck. The underlying API is
  overwrite-semantics by design; introducing a CLI-side guard
  would diverge from the trait contract and defeat the catalog-
  recovery use case (which requires idempotent re-runs against a
  manifest). If a true no-overwrite mode is needed later, it
  becomes a separate feature.
- **Deterministic-time flag (`--placed-at <timestamp>`)**:
  rejected in `wave-decisions.md` D-Timestamp /
  D-OutOfScope-PlacedAt. The `SystemTime::now()` is hard-wired
  at the call site; the acceptance test asserts the observable
  wire invariants (stdout report, post-call `get_entry().tier`),
  not the exact recorded `placed_at` value. A `TestKit` / spike
  feature can introduce a deterministic clock later.
- **Mixed-case tier argument (`Hot` / `HOT`)**: rejected in
  `wave-decisions.md` D-LowerCase. The lower-case set is the
  established CLI convention from both the rendering side at
  `crates/kaleidoscope-cli/src/lib.rs:519-525` AND the parse
  side at lines 505-512 (used by `migrate` and `list-items`);
  accepting mixed-case would break symmetry and operator muscle
  memory.
- **A new `TieringStore::place_if_absent(tenant, item, tier,
  placed_at) -> Result<(), AlreadyPlaced>` trait method**: not
  introduced in v0. The existing overwrite-semantics
  `place(...)` returning `()` is sufficient at v0; the explicit
  out-of-scope item in the task brief blocks adding such a
  method speculatively. If a follow-up wave introduces a
  no-overwrite contract, that wave can introduce the new trait
  method.

## Learning hypothesis

Disproves the assumption that the existing `TieringStore::place`
API is sufficient for an operator-visible CLI placement surface
without needing new methods. The current trait returns `()` per
`crates/cinder/src/store.rs:78-81` (no failure modes; overwrite-
semantics). The CLI needs:

1. A tier-argument parser (existing `parse_tier` at lines
   505-512).
2. A recorder constructor (existing `match otlp_log_path {
   Some(p) => CinderToOtlpJsonWriter::new(file), None =>
   CinderRecorder }` pattern from `migrate()` at lines 435-444).
3. A `FileBackedTieringStore::open` call (existing usage
   pattern).
4. A single `place(tenant, item, tier, SystemTime::now())` call.
5. A `writeln!` for the stdout report.

If the assumption holds, the slice ships with the above five
ingredients composed into one new free function — the thinnest
possible mutation shape. If the assumption fails — e.g. the
operator demands a pre-flight existence check so accidental
overwrites are blocked — the failure mode tells DESIGN to propose
either a new `TieringStore::place_if_absent(...) -> Result<(),
AlreadyPlaced>` trait method or a `--no-overwrite` CLI flag in a
follow-up wave (NOT pre-emptively designed here).

## Acceptance criteria (DISTILL translates each into a `#[test]` fn)

- `place_fresh_item_in_hot_emits_one_stdout_line_and_records_entry`:
  open a fresh temp `data_dir`. Call the place library function
  with arguments `(acme, data_dir, "acme/bootstrap-00001", "hot",
  &mut stdout_buf, None)`. Assert: the call returns Ok;
  `stdout_buf` equals exactly `placed tenant=acme
  item=acme/bootstrap-00001 tier=hot\n`; `stderr_buf` is empty;
  a fresh `FileBackedTieringStore::open(cinder_base(data_dir),
  ...).get_entry(&acme, &ItemId::new("acme/bootstrap-00001"))`
  returns `Some(entry)` with `entry.tier == Tier::Hot`; the
  Lumen directory under `lumen_base(data_dir)` does NOT exist OR
  is byte-equivalent to its pre-call state.
- `place_over_existing_item_overwrites_to_new_tier_without_error`:
  pre-place item `acme/bootstrap-00007` for tenant `acme` in tier
  Hot via a direct `FileBackedTieringStore::open(...).place(...)`
  call (placed_at = a fixed `SystemTime` value). Call the place
  library function with arguments `(acme, data_dir,
  "acme/bootstrap-00007", "cold", &mut stdout_buf, None)`.
  Assert: the call returns Ok; `stdout_buf` equals exactly
  `placed tenant=acme item=acme/bootstrap-00007 tier=cold\n` (the
  NEW tier, not the old `hot`); `stderr_buf` is empty;
  `get_entry(acme, acme/bootstrap-00007).unwrap().tier ==
  Tier::Cold` (overwrite-semantics — the previous Hot entry is
  gone). Companion documentation in `user-stories.md` Domain
  Example 2 records that this is expected overwrite-semantics of
  the underlying API at `crates/cinder/src/store.rs:78-81,
  :140-152`, NOT a special case.
- `place_invalid_uppercase_tier_argument_fails_fast_with_stderr_naming_the_value`:
  open a fresh `data_dir` (or alternatively, pre-place a seed item
  `acme/seed-00001` in Hot to prove the parse error short-circuits
  before any store mutation). Call the place library function
  with arguments `(acme, data_dir, "acme/bootstrap-00001", "HOT",
  &mut stdout_buf, &mut stderr_buf, None)`. Assert: the call
  returns Err; `stdout_buf` is empty; `stderr_buf` contains the
  substring `HOT`; the seed item (if present) is byte-equivalent
  before and after; no `acme/bootstrap-00001` entry was created.
  Companion sub-scenario with `tier_arg = "lukewarm"` (a typo)
  asserts `stderr_buf` contains `lukewarm`.
- `place_with_observe_otlp_appends_exactly_one_cinder_place_count_line`:
  open a fresh temp `data_dir`. Choose an OTLP-JSON sidecar path
  `<tmp>/observe.log` and assert it does NOT exist before the
  call. Call the place library function with arguments `(acme,
  data_dir, "acme/bootstrap-00001", "hot", &mut stdout_buf,
  Some(<tmp>/observe.log))`. Assert: the call returns Ok; the
  file at `<tmp>/observe.log` exists; the file contains exactly
  ONE line; that line contains the substrings
  `cinder.place.count`, `acme`, and `hot`. Companion sub-scenario:
  call the place library function with `otlp_log_path = None`
  against a different candidate path and assert the candidate
  path is NOT created (no implicit file).
- `place_for_acme_does_not_touch_globex_same_named_item`:
  pre-place item `acme/bootstrap-00001` for tenant `acme` in tier
  Warm AND, separately, pre-place item `acme/bootstrap-00001`
  for tenant `globex` in tier Cold in the SAME `data_dir`. Call
  the place library function with arguments `(acme, data_dir,
  "acme/bootstrap-00001", "hot", &mut stdout_buf, None)`. Assert:
  the call returns Ok; `stdout_buf` equals exactly `placed
  tenant=acme item=acme/bootstrap-00001 tier=hot\n`;
  `get_entry(acme, acme/bootstrap-00001).unwrap().tier ==
  Tier::Hot` (the new tier — overwrite-semantics for `acme`);
  `get_entry(globex, acme/bootstrap-00001).unwrap().tier ==
  Tier::Cold` (unchanged from the pre-call state — tenant
  isolation).

## Dependencies

- `cinder::TieringStore::place(tenant, item, tier, placed_at)`
  already exists at `crates/cinder/src/store.rs:78-81`, returns
  `()` (no failure modes; overwrite-semantics).
- `cinder::TieringStore::get_entry(tenant, item)` already exists
  at `crates/cinder/src/store.rs:89` (used by the acceptance
  test as the post-call oracle).
- `cinder::FileBackedTieringStore` already used by `ingest()` at
  `crates/kaleidoscope-cli/src/lib.rs:185-188`, `migrate()` at
  lines 445-446, `list_items()` at lines 489-490,
  `stats_with_tiers()` at lines 377-378.
- `cinder::CinderRecorder` (quiescent) already used by
  `migrate()`'s no-flag arm at line 443.
- `self_observe::CinderToOtlpJsonWriter` already imported at
  `crates/kaleidoscope-cli/src/lib.rs:65`; used by `ingest()`'s
  `--observe-otlp` arm at lines 172-174 and `migrate()`'s
  `--observe-otlp` arm at line 441.
- `cinder::Tier` and `cinder::ItemId` already imported at
  `crates/kaleidoscope-cli/src/lib.rs:58`.
- `cinder_base(data_dir)` helper already at
  `crates/kaleidoscope-cli/src/lib.rs:130-132`.
- `parse_tier(s: &str) -> Result<Tier, ()>` helper already at
  `crates/kaleidoscope-cli/src/lib.rs:505-512`; this slice may
  promote it to `pub(crate)` if needed (DESIGN's call) so the
  `place` function can reuse it without duplication.
- `tier_lowercase(tier) -> &'static str` helper already at
  `crates/kaleidoscope-cli/src/lib.rs:519-525`; this slice may
  promote it to `pub(crate)` if needed (DESIGN's call).
- `Error::InvalidTier { value: String }` variant already at
  `crates/kaleidoscope-cli/src/lib.rs:79-81` with Display at
  98-100 (`invalid tier "<value>": expected one of hot, warm,
  cold`).
- `parse_positional` helper at
  `crates/kaleidoscope-cli/src/main.rs:328-334`.
- `parse_observe_otlp` helper at
  `crates/kaleidoscope-cli/src/main.rs:178-192`.
- `aegis::TenantId` already a dependency.
- `std::time::SystemTime::now()` for the `placed_at` argument.
- No new external dependencies. No new internal crate
  dependencies.

## Reference class

This is the EIGHTH small feature in a row in the
`kaleidoscope-cli` cluster (after `cinder-to-pulse-bridge-v0`,
`cinder-to-otlp-json-bridge-v0`, `cli-cinder-otlp-wiring-v0`,
`cli-read-observe-otlp-v0`, `cli-stats-subcommand-v0`,
`cli-stats-cinder-tier-distribution-v0`,
`cli-migrate-subcommand-v0`,
`cli-migrate-observe-otlp-v0`, and
`cli-list-items-subcommand-v0`). Comparable in size to the
immediate predecessors: the structural surface area is the same
(one new dispatch arm + one new library function + one new
acceptance test file + one new `[[test]]` manifest entry). The
substantive difference is that this feature CREATES Cinder
placements directly (via `place`), whereas `migrate` mutates an
existing placement (`get_entry` + `migrate`) and `list-items` /
`stats` read placements (`list_by_tier`).

## Effort estimate

Well under 1 day for the crafter. Breakdown:

- 30 minutes for the dispatch wiring in `main.rs` (new
  `Some("place")` arm + `run_place` / `run_place_with` helpers
  mirroring `run_migrate` / `run_migrate_with`).
- 30 minutes for the library function in `lib.rs` (one
  `parse_tier` call + one recorder construction match + one
  `FileBackedTieringStore::open` + one `place` call + one
  `writeln!`; the parse_tier and tier_lowercase helpers are
  already there).
- 1-2 hours for the new acceptance test (five scenarios —
  happy-path bootstrap, overwrite-semantics, invalid tier,
  `--observe-otlp` emission, tenant isolation).
- 30 minutes for the `Cargo.toml` `[[test]]` entry, the
  `print_usage` update, and a local green run.

## Definition of Done for this slice

- All AC above green under `cargo test --package
  kaleidoscope-cli`.
- `cargo clippy --workspace --all-targets` clean (no new
  warnings).
- The existing 12 locked test files
  (`tests/migrate_subcommand.rs`,
  `tests/migrate_observe_otlp_flag.rs`,
  `tests/list_items_subcommand.rs`,
  `tests/stats_subcommand.rs`,
  `tests/stats_cinder_tier_distribution.rs`,
  `tests/stats_time_range.rs`,
  `tests/read_time_range.rs`,
  `tests/ingest_and_read_roundtrip.rs`,
  `tests/cli_binary_smoke.rs`,
  `tests/observe_otlp_flag.rs`,
  `tests/observe_otlp_read_flag.rs`,
  `tests/observe_otlp_cinder_wiring.rs`) continue to pass green
  UNMODIFIED (`wave-decisions.md` D-LockedTests — the prior
  cluster's byte-level oracles preserved).
- The dogfood demo runs: `cargo run --bin kaleidoscope-cli --
  place acme /tmp/kdata acme/bootstrap-00001 hot` returns
  `placed tenant=acme item=acme/bootstrap-00001 tier=hot` on
  stdout, exit 0; the immediately-following `cargo run --bin
  kaleidoscope-cli -- list-items acme /tmp/kdata hot` includes
  `acme/bootstrap-00001`; `cargo run --bin kaleidoscope-cli --
  stats acme /tmp/kdata` shows `hot=1`. Then `cargo run --bin
  kaleidoscope-cli -- place acme /tmp/kdata acme/bootstrap-00001
  cold --observe-otlp /tmp/cinder.otlp.json` returns `placed
  tenant=acme item=acme/bootstrap-00001 tier=cold`; the file at
  `/tmp/cinder.otlp.json` gains one `cinder.place.count` line;
  `kaleidoscope-cli list-items acme /tmp/kdata hot` shows empty;
  `kaleidoscope-cli list-items acme /tmp/kdata cold` shows
  `acme/bootstrap-00001`.
- The other prior `tests/*_*` test files in the cluster
  continue to pass green (non-regression on the eight reference
  features).
