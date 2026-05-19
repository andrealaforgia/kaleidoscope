# Slice 01 — `get-tier` subcommand reports a single item's current tier

**Story**: US-01
**Outcome KPIs**: OK1-CLI-get-tier-success (principal),
OK2-CLI-get-tier-unknown-item-fail-fast,
OK3-CLI-get-tier-tenant-isolation
**Tag**: operator-visible (not `@infrastructure` — a real
user-invocable CLI subcommand)
**Estimated effort**: well under 1 day

## Goal

Add a new positional subcommand `kaleidoscope-cli get-tier
<tenant> <data_dir> <item_id>` that opens the Cinder store under
`<data_dir>/cinder.*`, calls
`cinder::TieringStore::get_tier(&tenant, &ItemId::new(item_id))`,
and writes `tier=<lowercase>\n` to stdout on success. On
`get_tier(...) -> None`, exit non-zero with a stderr line
containing the substrings `unknown item`, the verbatim item id,
and the verbatim tenant — mirroring `MigrateError::UnknownItem`'s
`Display` impl at `crates/cinder/src/store.rs:55-58`.

## What ships in this slice

| Artifact | Change |
|----------|--------|
| `crates/kaleidoscope-cli/src/lib.rs` | NEW free function `get_tier(tenant, data_dir, item_id, writer) -> Result<(), Error>` (exact signature DESIGN's call per `wave-decisions.md` D-FunctionShape). Possibly NEW `Error` variant `CinderUnknownItem { tenant: TenantId, item: ItemId }` (DESIGN's call per `wave-decisions.md` D-ErrorVariant; alternative is reusing `Error::CinderMigrate(MigrateError::UnknownItem)` off-label). |
| `crates/kaleidoscope-cli/src/main.rs` | NEW `Some("get-tier") => run_get_tier(&args)` arm in the dispatch match. NEW `run_get_tier(args: &[String]) -> Result<(), Box<dyn std::error::Error>>` function mirroring the shape of `run_list_items` (parse the three positional args, call the library function, propagate any error). Updated `print_usage` to document the new subcommand. |
| `crates/kaleidoscope-cli/tests/get_tier_subcommand.rs` | NEW file. Mirrors the harness pattern in `tests/list_items_subcommand.rs` / `tests/migrate_subcommand.rs`. Hosts five `#[test]` functions translating the five UAT scenarios from `user-stories.md` (happy-path-Hot, happy-path-Warm, happy-path-Cold, unknown-item-fail-fast, tenant-isolation). |
| `crates/kaleidoscope-cli/Cargo.toml` | NEW `[[test]] name = "get_tier_subcommand", path = "tests/get_tier_subcommand.rs"` entry. The `cinder` dependency is already present. |
| Locked test files (`tests/stats_subcommand.rs`, `tests/stats_cinder_tier_distribution.rs`, `tests/list_items_subcommand.rs`, `tests/migrate_subcommand.rs`, `tests/place_subcommand.rs`, `tests/observe_otlp_*.rs`) | NOT MODIFIED. Hard constraint from the task brief and from `wave-decisions.md` D-LockedTests. |

## IN scope

- One new CLI subcommand `get-tier` with exactly three positional
  arguments: `<tenant> <data_dir> <item_id>`.
- One-line stdout report on success: `tier=<lowercase>\n` where
  `<lowercase>` is `hot` / `warm` / `cold` rendered through the
  existing `tier_lowercase` helper.
- Non-zero exit + stderr line on `get_tier(...) -> None`. Stderr
  contains the substrings `unknown item`, the verbatim item id,
  and the verbatim tenant. Mirrors `MigrateError::UnknownItem`'s
  `Display` text at `crates/cinder/src/store.rs:55-58`.
- Tenant isolation: `get-tier(acme, ..., item)` and
  `get-tier(globex, ..., item)` for the same `ItemId` string
  return the respective per-tenant tiers; the placement key is
  `(TenantId, ItemId)` per `crates/cinder/src/store.rs:119`.
- Read-only: the subcommand opens ONLY the Cinder store under
  `<data_dir>/cinder.*`; the Cinder WAL+snapshot is byte-
  equivalent before and after every invocation. The Lumen store
  under `<data_dir>/lumen.*` is never opened and is byte-
  equivalent before and after the call.

## OUT of scope

- Bulk lookup (multi-item single call). One CLI invocation
  queries exactly one item (`wave-decisions.md`
  D-OutOfScope-Bulk).
- Structured output formats. No `--json` / `--csv` /
  `--format=...` (`wave-decisions.md` D-OutOfScope-Json).
- `--observe-otlp` wiring on `get-tier`. The flag is NOT accepted
  by this subcommand in v0 (`wave-decisions.md`
  D-OutOfScope-Observe / D-ReadOnly). `get_tier` is a read with
  no `MetricsRecorder` hook per
  `crates/cinder/src/store.rs:154-160`, so there is no
  operationally meaningful OTLP signal to attach.
- Full `get-entry` shape (placed_at, migrated_at). The richer
  question is OUT OF SCOPE for v0 and belongs to a future
  `cli-get-entry-subcommand-v0` feature
  (`wave-decisions.md` D-OutOfScope-FullEntry).
- Lumen-side touch. `get-tier` never opens
  `FileBackedLogStore::open(lumen_base(data_dir), ...)`. The
  Lumen WAL+snapshot is byte-equivalent before and after the
  call (`wave-decisions.md` D-NoLumenTouch).
- Modification of any locked test file
  (`tests/stats_subcommand.rs`,
  `tests/stats_cinder_tier_distribution.rs`,
  `tests/list_items_subcommand.rs`,
  `tests/migrate_subcommand.rs`,
  `tests/place_subcommand.rs`,
  `tests/observe_otlp_*.rs`). `wave-decisions.md`
  D-LockedTests makes this a hard contract.

## Rejected alternatives

- **Bulk lookup (`get-tier <tenant> <data_dir> <item_ids_file>`)**:
  rejected in `wave-decisions.md` D-OutOfScope-Bulk. v0 ships the
  single-item shape.
- **Full `get-entry` shape returning `TierEntry { tier, placed_at,
  migrated_at }`**: rejected in `wave-decisions.md`
  D-OutOfScope-FullEntry. v0 ships the narrowest answer to the
  operator's narrowest question.
- **`--observe-otlp` wiring**: rejected in `wave-decisions.md`
  D-OutOfScope-Observe / D-ReadOnly. `get_tier` is a read with no
  recorder hook in the underlying impl
  (`crates/cinder/src/store.rs:154-160`); no OTLP signal exists
  to attach.
- **JSON output (`--json`)**: rejected in `wave-decisions.md`
  D-OutOfScope-Json. The plain-text `tier=<lowercase>\n` shape
  is grep/cut-friendly and matches the `stats` aesthetic.
- **A new `TieringStore::get_tier_with_metadata(tenant, item) ->
  Option<TierEntry>` trait method**: not introduced in v0. The
  existing `get_entry(tenant, item) -> Option<TierEntry>` at
  `crates/cinder/src/store.rs:89` already returns the full triple
  if a future feature wants it. This feature consumes only
  `get_tier`, not `get_entry`.

## Learning hypothesis

Validates the assumption that `TieringStore::get_tier(tenant,
item) -> Option<Tier>` is sufficient for an operator-visible CLI
tier-query surface without needing the richer `get_entry`
(returning the full `TierEntry` triple including `placed_at` and
`migrated_at`). If the assumption holds, the slice ships with one
`get_tier` call per CLI invocation, returning `Option<Tier>`. If
the assumption fails — operators repeatedly ask "when was it
placed?" or "when was it last migrated?" alongside the tier
question — the failure mode tells DESIGN to propose a separate
`cli-get-entry-subcommand-v0` feature returning the full
`TierEntry` triple in a richer stdout shape (multiple lines or
multiple `key=value` pairs).

## Acceptance criteria (DISTILL translates each into a `#[test]` fn)

- `get_tier_returns_hot_for_an_item_placed_in_hot`:
  pre-place item `acme/batch-00042` for tenant `acme` in tier
  Hot via a direct `FileBackedTieringStore::open(...).place(...)`
  call (placed_at = a fixed `SystemTime` value). Call the
  get-tier library function with arguments `(acme, data_dir,
  "acme/batch-00042", &mut stdout_buf)`. Assert: the call
  returns Ok; `stdout_buf` equals exactly `tier=hot\n`;
  captured stderr is empty; a fresh
  `FileBackedTieringStore::open(cinder_base(data_dir),
  ...).get_tier(&acme, &ItemId::new("acme/batch-00042"))` returns
  `Some(Tier::Hot)` (unchanged from the pre-call state); the
  Lumen directory under `lumen_base(data_dir)` does NOT exist
  OR is byte-equivalent to its pre-call state.
- `get_tier_returns_warm_for_an_item_placed_in_warm`:
  pre-place item `acme/batch-00050` for tenant `acme` in tier
  Warm. Call the get-tier library function with arguments
  `(acme, data_dir, "acme/batch-00050", &mut stdout_buf)`.
  Assert: the call returns Ok; `stdout_buf` equals exactly
  `tier=warm\n`.
- `get_tier_returns_cold_for_an_item_placed_in_cold`:
  pre-place item `acme/batch-00007` for tenant `acme` in tier
  Cold. Call the get-tier library function with arguments
  `(acme, data_dir, "acme/batch-00007", &mut stdout_buf)`.
  Assert: the call returns Ok; `stdout_buf` equals exactly
  `tier=cold\n`.
- `get_tier_unknown_item_fails_fast_with_stderr_naming_item_and_tenant`:
  pre-place a DIFFERENT item (e.g. `acme/batch-00001` in Hot)
  for tenant `acme` so the Cinder store opens cleanly, but do
  NOT place `acme/batch-00099`. Snapshot the pre-call Cinder
  state (any deterministic observable — e.g.
  `list_by_tier(acme, Tier::Hot/Warm/Cold).len()` triple). Call
  the get-tier library function with arguments `(acme, data_dir,
  "acme/batch-00099", &mut stdout_buf)` using a subprocess
  invocation that captures both stdout and stderr (or via a
  library-direct call that returns Err so the assertion shape is
  inline). Assert: the call returns Err (or the subprocess exits
  non-zero with empty stdout and non-empty stderr); the stderr
  contains the substrings `unknown item`, `acme/batch-00099`,
  and `acme`; the post-call per-tier counts are identical to the
  pre-call counts (read-only — no mutation possible since
  `get_tier` does not mutate).
- `get_tier_for_acme_does_not_surface_globex_same_named_item`:
  pre-place item `acme/batch-00042` for tenant `acme` in tier
  Hot AND, separately, pre-place item `acme/batch-00042` for
  tenant `globex` in tier Warm in the SAME `data_dir`. Call the
  get-tier library function twice — first with `(acme, data_dir,
  "acme/batch-00042", &mut stdout_buf_a)` and then with `(globex,
  data_dir, "acme/batch-00042", &mut stdout_buf_b)`. Assert:
  both calls return Ok; `stdout_buf_a` equals exactly
  `tier=hot\n`; `stdout_buf_b` equals exactly `tier=warm\n`.

## Dependencies

- `cinder::TieringStore::get_tier(tenant, item) -> Option<Tier>`
  already exists at `crates/cinder/src/store.rs:85`.
- `cinder::MigrateError::UnknownItem { tenant, item }` `Display`
  text exists at `crates/cinder/src/store.rs:55-58` and is the
  canonical "unknown item" phrasing this feature mirrors on the
  read-side.
- `cinder::FileBackedTieringStore` already used by `list_items()`
  at `crates/kaleidoscope-cli/src/lib.rs:534`.
- `cinder::CinderRecorder` already used by `list_items` at
  `crates/kaleidoscope-cli/src/lib.rs:534`.
- `cinder::Tier` and `cinder::ItemId` already imported.
- `cinder_base(data_dir)` helper already at
  `crates/kaleidoscope-cli/src/lib.rs:122-124`.
- `tier_lowercase(tier) -> &'static str` helper already at
  `crates/kaleidoscope-cli/src/lib.rs:564-570`; this slice may
  promote it to `pub(crate)` if needed (DESIGN's call).
- `aegis::TenantId` already a dependency.
- No `std::time::SystemTime` call needed (this is a read).
- No new external dependencies. No new internal crate
  dependencies.

## Reference class

Next small feature in the `kaleidoscope-cli` cluster, after
`cli-migrate-subcommand-v0`. Strictly thinner than the
predecessor because (a) no tier-argument parser is needed
(`get-tier` accepts no tier argument), (b) no `from`/`to` pair to
render, (c) no pre-flight `get_entry` call — the underlying
`get_tier` returns `Option<Tier>` directly so the function body
is one `get_tier` call + one `writeln!` (on success) per
invocation.

## Effort estimate

Well under 1 day for the crafter. Breakdown:

- 20 minutes for the dispatch wiring in `main.rs` (new
  `Some("get-tier")` arm + `run_get_tier` helper mirroring
  `run_list_items`).
- 20 minutes for the library function in `lib.rs` (one
  `get_tier` + one `writeln!` for success).
- 1-2 hours for the new acceptance test (five scenarios —
  happy-path-Hot, happy-path-Warm, happy-path-Cold, unknown
  item, tenant isolation).
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
  `tests/list_items_subcommand.rs`,
  `tests/migrate_subcommand.rs`,
  `tests/place_subcommand.rs`,
  `tests/observe_otlp_*.rs`) continue to pass green
  UNMODIFIED (`wave-decisions.md` D-LockedTests — the prior
  cluster's byte-level oracles preserved).
- The dogfood demo runs: `cargo run --bin kaleidoscope-cli --
  ingest acme /tmp/kdata < some_records.ndjson`, then `cargo
  run --bin kaleidoscope-cli -- get-tier acme /tmp/kdata
  acme/batch-00000` returns `tier=hot`, then `cargo run --bin
  kaleidoscope-cli -- migrate acme /tmp/kdata acme/batch-00000
  cold`, then `cargo run --bin kaleidoscope-cli -- get-tier
  acme /tmp/kdata acme/batch-00000` returns `tier=cold`.
- The prior `tests/*_subcommand.rs` files in the cluster continue
  to pass green (non-regression on the predecessor features).
