# Slice 01 — `migrate` subcommand moves a single item between Cinder tiers

**Story**: US-01
**Outcome KPIs**: OK1-CLI-migrate-success (principal),
OK2-CLI-migrate-unknown-item-fail-fast,
OK3-CLI-migrate-invalid-tier-fail-fast,
OK4-CLI-migrate-idempotent-same-tier
**Tag**: operator-visible (not `@infrastructure` — a real
user-invocable CLI subcommand)
**Estimated effort**: well under 1 day

## Goal

Add a new positional subcommand `kaleidoscope-cli migrate
<tenant_id> <data_dir> <item_id> <to_tier>` that opens the Cinder
store under `<data_dir>/cinder.*`, calls
`cinder::TieringStore::migrate(&tenant, &ItemId::new(item_id),
to_tier, SystemTime::now())`, and writes
`migrated tenant=<tenant> item=<item_id> from=<from> to=<to>\n`
to stdout on success. On `MigrateError::UnknownItem`, exit
non-zero with a stderr line naming the missing item. On an
invalid `<to_tier>` value, exit non-zero with a stderr line
naming the invalid value. Lower-case tier arguments only
(`hot` / `warm` / `cold`).

## What ships in this slice

| Artifact | Change |
|----------|--------|
| `crates/kaleidoscope-cli/src/lib.rs` | NEW free function `migrate(tenant, data_dir, item_id, to_tier_arg, writer) -> Result<(), Error>` (exact signature DESIGN's call per `wave-decisions.md` D-FunctionShape). NEW free function `tier_from_lowercase(s: &str) -> Result<Tier, _>` (or equivalent inline match) — the inverse of the existing `tier_lowercase` helper at lines 389-395. Possibly NEW `Error` variant `CinderMigrate(MigrateError)` and/or `InvalidTier { value: String }` (DESIGN's call per `wave-decisions.md` D-ErrorVariant). |
| `crates/kaleidoscope-cli/src/main.rs` | NEW `Some("migrate") => run_migrate(&args)` arm in the match at lines 50-64. NEW `run_migrate(args: &[String]) -> Result<(), Box<dyn std::error::Error>>` function mirroring the shape of `run_stats` at lines 226-246 (parse the four positional args, call the library function, propagate any error). Updated `print_usage` (`write_usage` at lines 81-129) to document the new subcommand's positional argument shape and lower-case tier contract. |
| `crates/kaleidoscope-cli/tests/migrate_subcommand.rs` | NEW file. Mirrors the harness pattern in `tests/stats_subcommand.rs` (the predecessor wave's locked file). Hosts five `#[test]` functions translating the five UAT scenarios from `user-stories.md` (happy-path Hot→Warm, idempotent same-tier Cold→Cold, unknown-item fail-fast, invalid-tier fail-fast, tenant-isolation). |
| `crates/kaleidoscope-cli/Cargo.toml` | NEW `[[test]] name = "migrate_subcommand", path = "tests/migrate_subcommand.rs"` entry. The `cinder` dependency is already present. |
| Locked test files (`tests/stats_subcommand.rs`, `tests/stats_cinder_tier_distribution.rs`, `tests/observe_otlp_*.rs`) | NOT MODIFIED. Hard constraint from the task brief and from `wave-decisions.md` D-LockedTests. |

## IN scope

- One new CLI subcommand `migrate` with exactly four positional
  arguments: `<tenant_id> <data_dir> <item_id> <to_tier>`.
- Lower-case tier argument only: `hot` / `warm` / `cold`. Any
  other spelling rejected with stderr naming the invalid value.
- One-line stdout report on success:
  `migrated tenant=<tenant> item=<item_id> from=<from> to=<to>\n`.
- Non-zero exit + stderr line on `MigrateError::UnknownItem`,
  stderr line contains the verbatim item id.
- Non-zero exit + stderr line on invalid tier argument, stderr
  line contains the verbatim invalid value.
- Idempotent same-tier migrate: the underlying API at
  `crates/cinder/src/store.rs:167-188` is idempotent; the CLI
  faithfully reports `from=X to=X` and exits 0; no special case.
- Tenant isolation: `migrate(acme, ...)` does NOT mutate
  `globex`'s same-named item.
- Cinder-only: the subcommand opens ONLY the Cinder store under
  `<data_dir>/cinder.*`; the Lumen store under
  `<data_dir>/lumen.*` is never opened and is byte-equivalent
  before and after the call.

## OUT of scope

- Bulk migration (multi-item single call). One CLI invocation
  migrates exactly one item (`wave-decisions.md`
  D-OutOfScope-Bulk).
- Policy preview / dry-run. No `--dry-run` flag
  (`wave-decisions.md` D-OutOfScope-Dryrun).
- `--observe-otlp` wiring on `migrate`. The flag is NOT accepted
  by this subcommand in v0 (`wave-decisions.md`
  D-OutOfScope-Observe). The Cinder recorder is a quiescent
  `NoopRecorder`.
- Structured output formats. No `--json` / `--csv` /
  `--format=...` (`wave-decisions.md` D-OutOfScope-Json).
- `--at <timestamp>` flag for testing. `SystemTime::now()` is
  hard-wired at the call site (`wave-decisions.md`
  D-Timestamp); deterministic-time testing belongs to a
  separate `TestKit` / spike feature.
- Lumen-side mutation. `migrate` never opens
  `FileBackedLogStore::open(lumen_base(data_dir), ...)`. The
  Lumen WAL+snapshot is byte-equivalent before and after the
  call (`wave-decisions.md` D-NoLumenTouch).
- Modification of any locked test file
  (`tests/stats_subcommand.rs`,
  `tests/stats_cinder_tier_distribution.rs`,
  `tests/observe_otlp_*.rs`). `wave-decisions.md`
  D-LockedTests makes this a hard contract.

## Rejected alternatives

- **Bulk migration shape (`migrate <tenant> <data_dir>
  <item_ids_file>`)**: rejected in `wave-decisions.md`
  D-OutOfScope-Bulk. v0 ships the single-item shape; bulk is a
  reasonable v1 once the single-item contract is validated.
- **Dry-run flag (`--dry-run`)**: rejected in
  `wave-decisions.md` D-OutOfScope-Dryrun. The operator's
  natural dry-run today is `kaleidoscope-cli stats <tenant>
  <data_dir>` BEFORE and AFTER the migrate; if a true dry-run
  is needed (preview without mutation), it becomes a separate
  feature.
- **Deterministic-time flag (`--at <timestamp>`)**: rejected in
  `wave-decisions.md` D-Timestamp. The `SystemTime::now()` is
  hard-wired at the call site; the acceptance test asserts the
  observable wire invariants (stdout report, post-call
  `get_entry().tier`), not the exact recorded `migrated_at`
  value. A `TestKit` / spike feature can introduce a
  deterministic clock later.
- **Mixed-case tier argument (`Hot` / `HOT`)**: rejected in
  `wave-decisions.md` D-LowerCase. The lower-case set is the
  established CLI convention from the rendering side at
  `crates/kaleidoscope-cli/src/lib.rs:389-395`; accepting upper-
  case would create asymmetry (read-side requires lower-case,
  write-side accepts mixed-case) and break operator muscle
  memory.
- **A new `TieringStore::migrate_observed(tenant, item,
  to_tier, migrated_at) -> Result<TierEntry, MigrateError>`
  trait method that returns the previous tier**: not introduced
  in v0. The existing `get_entry(tenant, item)` returning
  `Option<TierEntry>` is sufficient at v0; the read-then-write
  pattern is a known TOCTOU shape but the v0 use case (operator
  in a shell session) does not have a concurrent
  `evaluate_at` to race against. If a follow-up wave introduces
  concurrent policy evaluation that races with the CLI's
  `migrate`, that wave can introduce the atomic trait method.

## Learning hypothesis

Disproves the assumption that the existing `TieringStore` API is
sufficient for an operator-visible CLI tier-mutation surface
without needing new methods. The current trait returns
`Result<(), MigrateError>` from `migrate(...)` per
`crates/cinder/src/store.rs:93-99`; the CLI needs to discover the
`from` tier for the stdout report, which the trait method itself
does NOT return. The `get_entry(tenant, item)` trait method at
`crates/cinder/src/store.rs:89` fills this gap by returning the
full `TierEntry` BEFORE the migrate call. If the assumption holds,
the slice ships with one `get_entry` call plus one `migrate` call
per CLI invocation. If the assumption fails — a concurrent
`evaluate_at` policy evaluation races with the CLI's migrate so
the `from` tier captured BEFORE the migrate is stale by the time
the migrate actually fires — the failure mode tells DESIGN to
propose an atomic `migrate_observed` trait method as a follow-up.

## Acceptance criteria (DISTILL translates each into a `#[test]` fn)

- `migrate_existing_item_from_hot_to_warm_emits_one_stdout_line_and_updates_tier`:
  pre-place item `acme/batch-00042` for tenant `acme` in tier
  Hot via a direct `FileBackedTieringStore::open(...).place(...)`
  call (placed_at = a fixed `SystemTime` value). Call the
  migrate library function with arguments `(acme, data_dir,
  "acme/batch-00042", "warm", &mut stdout_buf, &mut stderr_buf)`.
  Assert: the call returns Ok; `stdout_buf` equals exactly
  `migrated tenant=acme item=acme/batch-00042 from=hot to=warm\n`;
  `stderr_buf` is empty; a fresh
  `FileBackedTieringStore::open(cinder_base(data_dir),
  ...).get_entry(&acme, &ItemId::new("acme/batch-00042"))` returns
  `Some(entry)` with `entry.tier == Tier::Warm`; the Lumen
  directory under `lumen_base(data_dir)` does NOT exist OR is
  byte-equivalent to its pre-call state.
- `migrate_existing_item_to_its_current_tier_succeeds_idempotently`:
  pre-place item `acme/batch-00007` for tenant `acme` in tier
  Cold via a direct `place(...)` call. Call the migrate library
  function with arguments `(acme, data_dir, "acme/batch-00007",
  "cold", &mut stdout_buf, &mut stderr_buf)`. Assert: the call
  returns Ok; `stdout_buf` equals exactly
  `migrated tenant=acme item=acme/batch-00007 from=cold to=cold\n`;
  `stderr_buf` is empty; `get_entry(acme, acme/batch-00007).unwrap().tier
  == Tier::Cold` (unchanged). Companion documentation in
  `user-stories.md` Domain Example 2 records that this is
  expected idempotent behaviour of the underlying API at
  `crates/cinder/src/store.rs:167-188`, NOT a special case.
- `migrate_unknown_item_fails_fast_with_stderr_naming_the_item`:
  pre-place a DIFFERENT item (e.g. `acme/batch-00001` in Hot)
  for tenant `acme` so the Cinder store opens cleanly, but do
  NOT place `acme/batch-00099`. Snapshot the pre-call per-tier
  counts via `list_by_tier(acme, Tier::Hot/Warm/Cold).len()`.
  Call the migrate library function with arguments `(acme,
  data_dir, "acme/batch-00099", "cold", &mut stdout_buf, &mut
  stderr_buf)`. Assert: the call returns Err; `stdout_buf` is
  empty; `stderr_buf` contains the substring
  `acme/batch-00099`; the post-call per-tier counts are
  identical to the pre-call counts (no mutation).
- `migrate_invalid_uppercase_tier_argument_fails_fast_with_stderr_naming_the_value`:
  pre-place item `acme/batch-00042` for tenant `acme` in tier
  Hot. Call the migrate library function with arguments `(acme,
  data_dir, "acme/batch-00042", "HOT", &mut stdout_buf, &mut
  stderr_buf)`. Assert: the call returns Err; `stdout_buf` is
  empty; `stderr_buf` contains the substring `HOT`; `get_entry(acme,
  acme/batch-00042).unwrap().tier == Tier::Hot` (unchanged).
  Companion sub-scenario with `to_tier_arg = "lukewarm"` (a
  typo) asserts `stderr_buf` contains `lukewarm`.
- `migrate_for_acme_does_not_touch_globex_same_named_item`:
  pre-place item `acme/batch-00042` for tenant `acme` in tier
  Hot AND, separately, pre-place item `acme/batch-00042` for
  tenant `globex` in tier Warm in the SAME `data_dir`. Call the
  migrate library function with arguments `(acme, data_dir,
  "acme/batch-00042", "cold", &mut stdout_buf, &mut stderr_buf)`.
  Assert: the call returns Ok; `stdout_buf` equals exactly
  `migrated tenant=acme item=acme/batch-00042 from=hot to=cold\n`;
  `get_entry(acme, acme/batch-00042).unwrap().tier == Tier::Cold`;
  `get_entry(globex, acme/batch-00042).unwrap().tier ==
  Tier::Warm` (unchanged).

## Dependencies

- `cinder::TieringStore::migrate(tenant, item, to_tier,
  migrated_at)` already exists at
  `crates/cinder/src/store.rs:93-99`.
- `cinder::TieringStore::get_entry(tenant, item)` already
  exists at `crates/cinder/src/store.rs:89`.
- `cinder::MigrateError::UnknownItem { tenant, item }`
  already exists at `crates/cinder/src/store.rs:43`.
- `cinder::FileBackedTieringStore` already used by `ingest()`
  at `crates/kaleidoscope-cli/src/lib.rs:179-180`.
- `cinder::NoopRecorder` already used by `ingest`'s no-flag arm
  at `crates/kaleidoscope-cli/src/lib.rs:170-174`.
- `cinder::Tier` and `cinder::ItemId` already imported at
  `crates/kaleidoscope-cli/src/lib.rs:58`.
- `cinder_base(data_dir)` helper already at
  `crates/kaleidoscope-cli/src/lib.rs:122-124`.
- `tier_lowercase(tier) -> &'static str` helper already at
  `crates/kaleidoscope-cli/src/lib.rs:389-395`; this slice may
  promote it to `pub(crate)` if needed (DESIGN's call) so the
  `migrate` function can render the `from`/`to` tier in the
  stdout report.
- `aegis::TenantId` already a dependency.
- `std::time::SystemTime::now()` for the `migrated_at`
  argument.
- No new external dependencies. No new internal crate
  dependencies.

## Reference class

This is the SEVENTH small feature in a row in the
`kaleidoscope-cli` cluster (after
`cinder-to-pulse-bridge-v0`, `cinder-to-otlp-json-bridge-v0`,
`cli-cinder-otlp-wiring-v0`, `cli-read-observe-otlp-v0`,
`cli-stats-subcommand-v0`, and
`cli-stats-cinder-tier-distribution-v0`). Comparable in size to
the immediate predecessor (`cli-stats-cinder-tier-distribution-v0`):
the structural surface area is the same (one new
dispatch arm + one new library function + one new acceptance
test file + one new `[[test]]` manifest entry). The substantive
difference is that this feature MUTATES the Cinder side (one
`migrate` call per invocation) whereas the predecessor was
read-only (three `list_by_tier` calls per invocation).

## Effort estimate

Well under 1 day for the crafter. Breakdown:

- 30 minutes for the dispatch wiring in `main.rs` (new
  `Some("migrate")` arm + `run_migrate` helper mirroring
  `run_stats`).
- 30 minutes for the library function in `lib.rs` (one
  `get_entry` + one `migrate` + one `writeln!` for success; the
  `tier_from_lowercase` parser is four lines).
- 1-2 hours for the new acceptance test (five scenarios — happy
  path, idempotent same-tier, unknown item, invalid tier,
  tenant isolation).
- 30 minutes for the `Cargo.toml` `[[test]]` entry, the
  `print_usage` update, and a local green run.

## Definition of Done for this slice

- All AC above green under `cargo test --package
  kaleidoscope-cli`.
- `cargo clippy --workspace --all-targets` clean (no new
  warnings).
- The existing locked test files
  (`tests/stats_subcommand.rs`,
  `tests/stats_cinder_tier_distribution.rs`,
  `tests/observe_otlp_*.rs`) continue to pass green
  UNMODIFIED (`wave-decisions.md` D-LockedTests — the prior
  cluster's byte-level oracles preserved).
- The dogfood demo runs: `cargo run --bin kaleidoscope-cli --
  ingest acme /tmp/kdata < some_records.ndjson`, then `cargo
  run --bin kaleidoscope-cli -- stats acme /tmp/kdata`, then
  `cargo run --bin kaleidoscope-cli -- migrate acme /tmp/kdata
  acme/batch-00000 cold`, then `cargo run --bin
  kaleidoscope-cli -- stats acme /tmp/kdata`. The before/after
  stats show one fewer Hot item and one more Cold item.
- The six prior `tests/*_*` test files in the cluster continue
  to pass green (non-regression on the six reference
  features).
