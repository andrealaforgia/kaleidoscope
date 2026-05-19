# Slice 01 — `list-items` subcommand enumerates every item for a tenant in a tier

**Story**: US-01
**Outcome KPIs**: OK1-CLI-list-items-correctness (principal),
OK2-CLI-list-items-tenant-isolation,
OK3-CLI-list-items-invalid-tier-fail-fast
**Tag**: operator-visible (not `@infrastructure` — a real
user-invocable CLI subcommand)
**Estimated effort**: well under 1 day

## Goal

Add a new positional subcommand `kaleidoscope-cli list-items
<tenant_id> <data_dir> <tier>` that opens the Cinder store
under `<data_dir>/cinder.*` read-only, calls
`cinder::TieringStore::list_by_tier(&tenant, tier)`, sorts the
returned `Vec<ItemId>` lexicographically, and writes one item
id per line to stdout. On invalid `<tier>` value, exit non-
zero with a stderr line naming the invalid value. Lower-case
tier arguments only (`hot` / `warm` / `cold`). Empty stdout
for an empty result set (no placeholder line). Two successive
invocations produce byte-identical stdout (determinism).

## What ships in this slice

| Artifact | Change |
|----------|--------|
| `crates/kaleidoscope-cli/src/lib.rs` | NEW free function `list_items(tenant, data_dir, tier_arg, writer) -> Result<usize, Error>` (exact signature DESIGN's call per `wave-decisions.md` D-FunctionShape). Internally: calls the existing `parse_tier` helper at lines 475-482 (DESIGN's call on whether to promote it to `pub(crate)` or duplicate inline), opens `FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))` (same pattern as `stats_with_tiers` at lines 377-378), calls `list_by_tier(&tenant, tier)`, sorts the returned vec lexicographically, writes one `writeln!(writer, "{}", item_id.0)` per item, returns `Ok(items.len())`. NO new `Error` variant — the existing `InvalidTier`, `CinderOpen`, and `Io` variants cover all three error paths. |
| `crates/kaleidoscope-cli/src/main.rs` | NEW `Some("list-items") => run_list_items(&args)` arm in the match at lines 52-66. NEW `run_list_items(args: &[String]) -> Result<(), Box<dyn std::error::Error>>` function mirroring the shape of `run_migrate` at lines 264-294 (parse the three positional args, call the library function, propagate any error). Updated `print_usage` (`write_usage` at lines 83-145) to document the new subcommand's positional argument shape and lower-case tier contract. DESIGN's call on whether to emit a `list-items ok: items=N` stderr summary line per `wave-decisions.md` D-StderrSummary. |
| `crates/kaleidoscope-cli/tests/list_items_subcommand.rs` | NEW file. Mirrors the harness pattern in `tests/migrate_subcommand.rs` (the predecessor wave's locked file). Hosts five `#[test]` functions translating the five UAT scenarios from `user-stories.md` (happy-path enumeration with sort, empty-result, invalid-tier fail-fast, tenant-isolation, determinism). |
| `crates/kaleidoscope-cli/Cargo.toml` | NEW `[[test]] name = "list_items_subcommand", path = "tests/list_items_subcommand.rs"` entry. The `cinder` dependency is already present. |
| Locked test files (`tests/stats_subcommand.rs`, `tests/stats_cinder_tier_distribution.rs`, `tests/migrate_subcommand.rs`, `tests/observe_otlp_*.rs`) | NOT MODIFIED. Hard constraint from the task brief and from `wave-decisions.md` D-LockedTests. |

## IN scope

- One new CLI subcommand `list-items` with exactly three
  positional arguments: `<tenant_id> <data_dir> <tier>`.
- Lower-case tier argument only: `hot` / `warm` / `cold`. Any
  other spelling rejected with stderr naming the invalid value.
- One stdout line per item id, sorted lexicographically by
  byte sequence, each terminated by `\n`.
- Empty stdout (zero bytes) when the result set is empty (no
  header, no placeholder line).
- Non-zero exit + stderr line on invalid tier argument, stderr
  contains the verbatim invalid value.
- Tenant isolation: `list-items acme ...` surfaces ONLY
  `acme`'s items, never any other tenant's, even when item ids
  collide across tenants.
- Determinism: two successive invocations with the same
  `(tenant, data_dir, tier)` tuple produce byte-identical
  stdout (the lex-sort at the CLI boundary masks the `HashMap`
  iteration order randomisation in
  `cinder::InMemoryTieringStore::list_by_tier` at
  `crates/cinder/src/store.rs:190-198`).
- Read-only: the subcommand performs only `list_by_tier`. No
  `place`, no `migrate`, no `evaluate_at`. The Cinder
  WAL+snapshot is byte-equivalent before and after every call.
- Cinder-only: the subcommand opens ONLY the Cinder store
  under `<data_dir>/cinder.*`; the Lumen store under
  `<data_dir>/lumen.*` is never opened.

## OUT of scope

- JSON output / `--format` flag. v0 ships plain text only
  (`wave-decisions.md` D-OutOfScope-Json).
- `--observe-otlp` wiring. `list_by_tier` is a pure read with
  no operator-visible event to record — Cinder's
  `MetricsRecorder` trait has no `record_list` method per
  `crates/cinder/src/metrics.rs` (`wave-decisions.md`
  D-OutOfScope-Observe).
- Cross-tenant aggregate (`list-items <data_dir> <tier>`
  without `<tenant>`). Out of v0 scope; the per-tenant API at
  `crates/cinder/src/store.rs:102` is the v0 primitive
  (`wave-decisions.md` D-OutOfScope-CrossTenant).
- Time-bound historical state (`list-items ... --at
  <timestamp>`). Deferred to ADR-0039 §7 future feature
  (`wave-decisions.md` D-OutOfScope-Historical).
- Pagination (`--limit`, `--offset`). Cardinality is small in
  v0 (operator deployments have on the order of tens to
  hundreds of items per tier); a one-line-per-item shape with
  no pagination fits the operator's `xargs` pipeline naturally
  (`wave-decisions.md` D-OutOfScope-Pagination).
- Cinder mutation. The subcommand is purely read-only
  (`wave-decisions.md` D-ReadOnly).
- Lumen-side touch. `list-items` never opens
  `FileBackedLogStore::open(lumen_base(data_dir), ...)`
  (`wave-decisions.md` D-NoLumenTouch).
- Modification of any locked test file
  (`tests/stats_subcommand.rs`,
  `tests/stats_cinder_tier_distribution.rs`,
  `tests/migrate_subcommand.rs`, `tests/observe_otlp_*.rs`).
  `wave-decisions.md` D-LockedTests makes this a hard
  contract.

## Rejected alternatives

- **JSON output (`--json` flag)**: rejected in
  `wave-decisions.md` D-OutOfScope-Json. v0 ships plain text
  only; JSON becomes a v1 concern once the v0 shape proves it
  is the right thing to make machine-parseable. The
  one-bare-id-per-line shape is ALREADY machine-parseable by
  every Unix text tool (`grep`, `wc -l`, `xargs`, `sort`,
  `comm`, `diff`).
- **Cross-tenant aggregate (`list-items <data_dir> <tier>`
  without `<tenant>`)**: rejected in
  `wave-decisions.md` D-OutOfScope-CrossTenant. The Cinder API
  is per-tenant per `crates/cinder/src/store.rs:102`; a
  cross-tenant aggregate would require either a new trait
  method (`list_by_tier_all_tenants(tier) ->
  Vec<(TenantId, ItemId)>`) which the Cinder crate does not
  expose, or an iteration over all tenants (which would
  require a `list_tenants()` method which also does not
  exist). The operator's natural workaround for the cross-
  tenant case is a shell loop:

  ```bash
  for tenant in acme globex initech; do
    kaleidoscope-cli list-items "$tenant" /tmp/data cold | \
      awk -v t="$tenant" '{print t"\t"$0}'
  done
  ```

  v1 could introduce the cross-tenant aggregate if the loop
  pattern proves operationally inadequate.
- **Time-bound historical state (`--at <timestamp>`)**:
  rejected in `wave-decisions.md` D-OutOfScope-Historical and
  deferred to ADR-0039 §7 future feature. The current Cinder
  store has no historical reconstruction primitive; introducing
  one for `list-items` would require a separate large feature.
- **Pagination (`--limit N`, `--offset N`)**: rejected in
  `wave-decisions.md` D-OutOfScope-Pagination. Operator tier
  cardinality is small in v0 (tens to hundreds); pagination
  is a v1 concern.
- **`--observe-otlp` flag**: rejected in
  `wave-decisions.md` D-OutOfScope-Observe. `list_by_tier` is
  a pure read with no operator-visible event to record;
  Cinder's `MetricsRecorder` trait has no `record_list`
  method.
- **Mixed-case tier argument (`Cold` / `COLD`)**: rejected in
  `wave-decisions.md` D-LowerCase. The lower-case set is the
  established CLI convention enforced by `migrate` (parse
  side) and by `stats` (rendering side); accepting upper-case
  would create asymmetry across the four CLI subcommands and
  break operator muscle memory.

## Learning hypothesis

Confirms (or disproves) the assumption that the existing
`TieringStore::list_by_tier(tenant, tier) -> Vec<ItemId>` API is
sufficient for an operator-visible enumeration surface without
needing new methods or richer return types. The current trait
returns `Vec<ItemId>` per `crates/cinder/src/store.rs:102`; the
CLI needs no other information about each item (no tier-entry
metadata, no timestamps, no placement order) because the
operator's three decisions (manual rebalancing follow-up /
sanity check / scripted pipeline) all need only the bare item
id. If the assumption holds, the slice ships with one
`list_by_tier` call plus one `vec.sort()` plus N `writeln!`
calls per CLI invocation. If the assumption fails — the
operator says "I also want the `migrated_at` timestamp per
item to know HOW STALE each cold item is" — the failure mode
tells DESIGN to propose a follow-up `list-entries` subcommand
that surfaces `TierEntry` (`crates/cinder/src/tier.rs`'s
`TierEntry { tier, placed_at, migrated_at }` shape) at the CLI.
That follow-up is NOT pre-emptively designed here.

## Acceptance criteria (DISTILL translates each into a `#[test]` fn)

- `list_items_returns_cold_items_for_acme_sorted_lex`:
  pre-place items `acme/batch-00099`, `acme/batch-00007`, and
  `acme/batch-00041` in tier Cold for tenant `acme` in a fresh
  `data_dir` via direct `FileBackedTieringStore::open(...).place(...)`
  calls (placed_at = a fixed `SystemTime` value), in that
  insertion order — intentionally NOT lex-sorted, to exercise
  the sort step. Pre-place a decoy item `acme/batch-00050` in
  Hot for the same tenant (which MUST NOT appear in the cold
  output). Call the list-items library function with arguments
  `(acme, data_dir, "cold", &mut stdout_buf)`. Assert: the
  call returns `Ok(3)`; `stdout_buf` equals exactly the bytes
  `acme/batch-00007\nacme/batch-00041\nacme/batch-00099\n`;
  a follow-up `cinder.list_by_tier(acme, Tier::Cold)` returns
  a `Vec` of length 3 (no mutation occurred); the Lumen
  directory under `lumen_base(data_dir)` does NOT exist (no
  Lumen open).
- `list_items_empty_tier_produces_empty_stdout`:
  pre-place item `acme/batch-00050` in Hot for tenant `acme`
  (so the Cinder store opens cleanly with at least one entry).
  Do NOT place any item in Warm. Call the list-items library
  function with arguments `(acme, data_dir, "warm", &mut
  stdout_buf)`. Assert: the call returns `Ok(0)`; `stdout_buf`
  is empty (zero bytes); a follow-up
  `cinder.list_by_tier(acme, Tier::Warm)` returns an empty
  `Vec`.
- `list_items_invalid_uppercase_tier_argument_fails_fast_with_stderr_naming_the_value`:
  pre-place item `acme/batch-00042` in Hot for tenant `acme`.
  Snapshot the pre-call per-tier `list_by_tier(acme,
  Tier::Hot).len()` count. Call the list-items library
  function with arguments `(acme, data_dir, "COLD", &mut
  stdout_buf)` (assuming the binary's error printer is what
  surfaces the stderr substring — DESIGN's call per
  `wave-decisions.md` D-StderrWording on whether the library
  function returns an `Error::InvalidTier { value: "COLD" }`
  whose `Display` impl prints `COLD` verbatim, OR whether the
  binary's main.rs prints `kaleidoscope-cli: invalid tier
  "COLD": ...`). Assert: the call returns `Err`; `stdout_buf`
  is empty; the `Error`'s `Display` (or `to_string()`)
  contains the substring `COLD`; the post-call `list_by_tier(acme,
  Tier::Hot).len()` matches the pre-call count (no mutation).
  Companion sub-scenario with `tier_arg = "lukewarm"` (a
  typo) asserts the `Error`'s `Display` contains `lukewarm`.
- `list_items_for_acme_does_not_surface_globex_same_named_items`:
  pre-place item `shared/batch-00042` in Cold for tenant
  `acme` AND, separately, pre-place item `shared/batch-00042`
  in Cold for tenant `globex` in the SAME `data_dir`. Call the
  list-items library function with arguments `(acme, data_dir,
  "cold", &mut stdout_buf)`. Assert: the call returns
  `Ok(1)`; `stdout_buf` equals exactly the bytes
  `shared/batch-00042\n` (one line, one item id); a follow-up
  `cinder.list_by_tier(globex, Tier::Cold)` returns a `Vec`
  containing `ItemId::new("shared/batch-00042".to_string())`
  (unchanged from the pre-call state).
- `list_items_is_deterministic_across_two_successive_calls`:
  pre-place items `acme/batch-00099`, `acme/batch-00007`, and
  `acme/batch-00041` in Cold for tenant `acme` in that
  non-lex insertion order. Call the list-items library function
  TWICE in succession with arguments `(acme, data_dir, "cold",
  &mut stdout_buf_1)` then `(acme, data_dir, "cold", &mut
  stdout_buf_2)`. Assert: both calls return `Ok(3)`;
  `stdout_buf_1 == stdout_buf_2` (byte-equality of the two
  captures); both equal the lex-sorted bytes
  `acme/batch-00007\nacme/batch-00041\nacme/batch-00099\n`.
  This pins the determinism property at the CLI boundary.

## Dependencies

- `cinder::TieringStore::list_by_tier(tenant, tier)` already
  exists at `crates/cinder/src/store.rs:102`. It is the exact
  method `stats_with_tiers` already calls in production at
  `crates/kaleidoscope-cli/src/lib.rs:383`.
- `cinder::FileBackedTieringStore` already used by
  `stats_with_tiers` at
  `crates/kaleidoscope-cli/src/lib.rs:377-378` and by
  `migrate` at `lib.rs:445-446`.
- `cinder::CinderRecorder` (quiescent recorder) already used
  by `stats_with_tiers` at `lib.rs:377` and by `migrate`'s
  no-flag arm at `lib.rs:443`.
- `cinder::Tier` and `cinder::ItemId` already imported and
  used by `stats_with_tiers` and `migrate`.
- `parse_tier` helper already at
  `crates/kaleidoscope-cli/src/lib.rs:475-482`; this slice may
  promote it to `pub(crate)` (DESIGN's call) so the
  `list_items` function can reuse it directly, OR duplicate
  the four-line `match` inline.
- `cinder_base(data_dir)` helper already at
  `crates/kaleidoscope-cli/src/lib.rs:130-132`.
- `Error::InvalidTier { value: String }` variant already at
  `crates/kaleidoscope-cli/src/lib.rs:79-81` with `Display`
  impl at `lib.rs:98-100`. No new variant introduced.
- `Error::CinderOpen(MigrateError)` variant already at
  `crates/kaleidoscope-cli/src/lib.rs:77`. Reused for any
  Cinder store-open failure.
- `Error::Io(std::io::Error)` variant already at
  `crates/kaleidoscope-cli/src/lib.rs:82`. Reused for any
  `writeln!` I/O failure.
- `parse_positional` helper already at
  `crates/kaleidoscope-cli/src/main.rs:296-302`.
- `aegis::TenantId` already a dependency.
- No `std::time::SystemTime` call (no mutation).
- No new external dependencies. No new internal crate
  dependencies.

## Reference class

This is the EIGHTH small feature in a row in the
`kaleidoscope-cli` cluster (after
`cinder-to-pulse-bridge-v0`, `cinder-to-otlp-json-bridge-v0`,
`cli-cinder-otlp-wiring-v0`, `cli-read-observe-otlp-v0`,
`cli-stats-subcommand-v0`,
`cli-stats-cinder-tier-distribution-v0`, and
`cli-migrate-subcommand-v0`). STRICTLY THINNER than the
immediate predecessor (`cli-migrate-subcommand-v0`): the
structural surface area is the same shape (one new dispatch
arm + one new library function + one new acceptance test file
+ one new `[[test]]` manifest entry) but each piece is shorter
because (a) only three positional arguments instead of four,
(b) no `get_entry` pre-flight call, (c) no `from`/`to`
resolution, (d) no mutation, (e) no `SystemTime::now()`
argument, (f) no new `Error` variant (`InvalidTier` already
exists). The substantive difference is that this feature is
READ-ONLY (one `list_by_tier` call per invocation) whereas
the predecessor MUTATES (one `get_entry` + one `migrate` call
per invocation).

## Effort estimate

Well under 1 day for the crafter. Breakdown:

- 20 minutes for the dispatch wiring in `main.rs` (new
  `Some("list-items")` arm + `run_list_items` helper
  mirroring `run_migrate`).
- 20 minutes for the library function in `lib.rs` (one
  `parse_tier` + one open + one `list_by_tier` + one `sort` +
  one `writeln!` loop).
- 1-2 hours for the new acceptance test (five scenarios —
  happy path with sort, empty result, invalid tier, tenant
  isolation, determinism).
- 20 minutes for the `Cargo.toml` `[[test]]` entry, the
  `print_usage` update, and a local green run.

## Definition of Done for this slice

- All AC above green under `cargo test --package
  kaleidoscope-cli`.
- `cargo clippy --workspace --all-targets` clean (no new
  warnings).
- The existing locked test files
  (`tests/stats_subcommand.rs`,
  `tests/stats_cinder_tier_distribution.rs`,
  `tests/migrate_subcommand.rs`, `tests/observe_otlp_*.rs`)
  continue to pass green UNMODIFIED (`wave-decisions.md`
  D-LockedTests — the prior cluster's byte-level oracles
  preserved).
- The dogfood demo runs: `cargo run --bin kaleidoscope-cli --
  ingest acme /tmp/kdata < some_records.ndjson`, then `cargo
  run --bin kaleidoscope-cli -- migrate acme /tmp/kdata
  acme/batch-00000 cold`, then `cargo run --bin
  kaleidoscope-cli -- stats acme /tmp/kdata` (which shows
  `cold=1`), then `cargo run --bin kaleidoscope-cli --
  list-items acme /tmp/kdata cold` (which prints
  `acme/batch-00000` on stdout, exit 0). The scripted-pipeline
  form `cargo run --bin kaleidoscope-cli -- list-items acme
  /tmp/kdata cold | xargs -I {} cargo run --bin
  kaleidoscope-cli -- migrate acme /tmp/kdata {} warm` walks
  each cold item back to warm in one shell loop.
- The seven prior `tests/*_*` test files in the cluster
  continue to pass green (non-regression on the seven
  reference features).
