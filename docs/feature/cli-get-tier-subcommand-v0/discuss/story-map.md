# Story Map: `cli-get-tier-subcommand-v0`

## User: Priya the platform operator

## Goal

When Priya runs
`kaleidoscope-cli get-tier acme /tmp/data acme/batch-00042`,
she sees on stdout — in milliseconds — exactly one line
`tier=hot\n` (or `tier=warm\n` or `tier=cold\n` depending on
the item's current tier), exit code 0. The Cinder store under
`/tmp/data/cinder.*` and the Lumen store under
`/tmp/data/lumen.*` are byte-equivalent before and after every
invocation (read-only). When the item id is unknown, she sees
exit non-zero and a single stderr line containing the substrings
`unknown item`, the verbatim item id, and the verbatim tenant;
stdout is empty; the Cinder store is unchanged. Three
operationally distinct uses are unified on this single CLI
invocation shape: pre-flight check before manual `migrate`,
scripted assertion in a deployment pipeline, and incident-time
audit on a single item id mentioned in an alert.

## Backbone

The journey has exactly one activity: the operator triggers a
single Cinder tier read without writing Rust and without running
three subprocess `list-items` invocations. The activity is a thin
extension on the existing CLI dispatch: a single
`kaleidoscope-cli get-tier <tenant> <data_dir> <item_id>`
invocation that internally opens
`FileBackedTieringStore::open(cinder_base(data_dir), recorder)`
with a quiescent `CinderRecorder`, calls `get_tier(tenant, &item)`
once (returning early with the unknown-item error if `None`), and
writes the one-line stdout report on `Some(tier)`. The CLI
substrate, the `TieringStore` trait, `FileBackedTieringStore`
adapter, the quiescent recorder pattern (`CinderRecorder`), the
`parse_positional` helper, and the `tier_lowercase` rendering
helper all already exist; this feature is a thin extension that
adds a new dispatch arm + a new library function on the existing
substrate.

| Activity 1: operator triggers a single Cinder tier read without writing Rust |
|---|
| `kaleidoscope-cli get-tier <tenant> <data_dir> <item_id>` is dispatched by the binary's `main.rs` argument matcher to the new `run_get_tier` helper. The helper parses the three positional arguments (the existing `parse_positional` returns `(TenantId, PathBuf)` for the first two; one extra `args.get(N)` call retrieves `<item_id>`) and calls the new library function `get_tier(...)`. The library function opens the Cinder store via `FileBackedTieringStore::open(cinder_base(data_dir), recorder)` with a quiescent `CinderRecorder`, calls `get_tier(tenant, &item)` once. On `Some(tier)` it writes one `writeln!(writer, "tier={}", tier_lowercase(tier))` line and returns Ok. On `None` it returns the typed unknown-item error. The Lumen store under `<data_dir>/lumen.*` is NEVER opened. Exit code 0 on success; non-zero on the unknown-item branch. |

## Walking Skeleton

Per `wave-decisions.md` D2 (and the task brief), the walking-
skeleton concept does not apply because:

- The CLI already exists, with six working subcommands (`ingest`,
  `read`, `stats`, `list-items`, `place`, `migrate`).
- The `TieringStore::get_tier(tenant, item) -> Option<Tier>`
  trait method already exists per `crates/cinder/src/store.rs:85`.
- The `FileBackedTieringStore::open` constructor is already used
  by `list_items()` at `crates/kaleidoscope-cli/src/lib.rs:534`.
- The `cinder_base(data_dir)` helper already exists at
  `crates/kaleidoscope-cli/src/lib.rs:122-124`.
- The quiescent `CinderRecorder` pattern is already used by
  `list_items` at `crates/kaleidoscope-cli/src/lib.rs:534`.
- The lower-case tier rendering helper `tier_lowercase` already
  exists at `crates/kaleidoscope-cli/src/lib.rs:564-570`.
- The `MigrateError::UnknownItem` `Display` text — the canonical
  "unknown item" phrasing — already exists at
  `crates/cinder/src/store.rs:55-58` and is locked by
  `tests/migrate_subcommand.rs:319`.

Equivalent statement: **the smallest valuable change is to add
one new `Some("get-tier") => run_get_tier(&args)` dispatch arm to
the `main.rs` match, one new `run_get_tier` helper in `main.rs`
mirroring the shape of `run_list_items`, one new `get_tier(...)`
library function in `lib.rs`, and one new acceptance test file
`tests/get_tier_subcommand.rs`.** Slice 01 ships exactly that.

## Release Slices

### Slice 01 — `get-tier` subcommand reports a single item's current tier

- **Outcome**: An operator running `kaleidoscope-cli get-tier acme
  /tmp/data acme/batch-00042` sees on stdout exactly one line
  `tier=hot\n` (or the appropriate lower-case tier), exit code 0.
  Unknown item ids produce non-zero exit + stderr containing the
  substrings `unknown item`, the verbatim item id, and the
  verbatim tenant (OK2). Tenant-isolated reads return the
  respective per-tenant tier for the same `ItemId` string under
  different tenants (OK3).
- **Stories**: `US-01` (single slice; all DoR-validated AC inside).
- **Learning hypothesis**: validates the assumption that
  `TieringStore::get_tier(tenant, item) -> Option<Tier>` is
  sufficient for an operator-visible CLI tier-query surface
  without needing the richer `get_entry` (returning the full
  `TierEntry` triple including `placed_at` and `migrated_at`).
  If the assumption holds, the slice ships with one `get_tier`
  call per CLI invocation, returning `Option<Tier>`. If the
  assumption fails — e.g. operators repeatedly ask "when was it
  placed?" or "when was it last migrated?" alongside the tier
  question — the failure mode tells DESIGN to propose a separate
  `cli-get-entry-subcommand-v0` feature returning the full
  `TierEntry` triple. That richer surface is OUT OF SCOPE here
  (`wave-decisions.md` D-OutOfScope-FullEntry).
- **Production-data-equivalent AC**: an end-to-end test invokes
  the CLI library function (the actual entry point the binary
  calls) with a `(tenant, data_dir, item_id, writer)` tuple
  against a real temp `data_dir`, against a Cinder store
  pre-populated by direct
  `FileBackedTieringStore::open(...).place(...)` setup calls per
  `(tenant, item_id, tier, placed_at)` triple, and reads back
  the captured stdout to assert the expected line content. This
  is the same data path the operator's `kaleidoscope-cli
  get-tier acme /tmp/data acme/batch-00042` invocation will
  exercise.
- **Dogfood moment**: After the slice ships, Andrea opens a
  terminal, runs `cargo run --bin kaleidoscope-cli -- ingest
  acme /tmp/kdata < some_records.ndjson` (which places one Hot
  Cinder item per batch via the existing `flush()`), then
  `cargo run --bin kaleidoscope-cli -- get-tier acme /tmp/kdata
  acme/batch-00000` (which returns `tier=hot` on stdout, exit 0),
  then `cargo run --bin kaleidoscope-cli -- migrate acme
  /tmp/kdata acme/batch-00000 cold` (which returns `migrated
  ... from=hot to=cold` on stdout, exit 0), then `cargo run
  --bin kaleidoscope-cli -- get-tier acme /tmp/kdata
  acme/batch-00000` again (which NOW returns `tier=cold` on
  stdout, exit 0). The four observations together — placement,
  read-back, mutation, post-mutation read-back — are the
  dogfood gate for the slice.
- **Effort**: well under 1 day. The change inside the library is
  structurally one `get_tier` call plus one `writeln!` (on
  success) per invocation; the dispatch helper in `main.rs` is
  structurally a mirror of `run_list_items`; the new acceptance
  test mirrors the existing `list_items_subcommand.rs` harness
  pattern; no tier-argument parser, no `from`/`to` pair to
  render, no pre-flight `get_entry` call.

## Priority Rationale

There is one slice and it is the only slice. The reference-class
sizing (this is the next small feature in the `kaleidoscope-cli`
cluster, comparable in size to `cli-migrate-subcommand-v0` but
strictly thinner because (a) no tier-argument parsing is needed,
(b) no `from`/`to` pair to render, (c) no pre-flight `get_entry`
call — the trait method already returns `Option<Tier>` directly)
means there is no benefit from further splitting:

- Slice 01 carries the new dispatch arm, the new library function,
  the OK1 happy-path test (three sub-scenarios, one per tier), the
  OK2 unknown-item fail-fast test, AND the OK3 tenant-isolation
  test all together. Splitting any one of the three KPIs into a
  separate slice would force a second PR for trivially the same
  wiring — net negative for the reviewer.
- The principal KPI (OK1) is the get-tier-success correctness;
  OK2 is the fail-fast guardrail on the only input-error path
  (unknown item — there are no other parse-side error paths for
  this subcommand since there is no tier argument and no flag);
  OK3 is the faithfulness-to-underlying-trait-key-invariant
  guardrail.  Shipping any without the others is meaningless: OK1
  alone with no unknown-item handling is dangerous (silent fall-
  through to a default tier or ambiguous diagnostic); OK1+OK2
  without OK3 ships a fail-fast but no protection against
  cross-tenant read leak.

If schedule pressure ever forces a partial ship, **the slice is
already as thin as it can be**: the function-level addition is
one `get_tier` call + one `writeln!`. There is no sub-slice
worth shipping in isolation.

The two-track choice between (a) reusing the existing
`Error::CinderMigrate(MigrateError::UnknownItem)` variant
off-label (semantically loose — no migrate was attempted — but
byte-equivalent to the OK2 stderr contract) and (b) introducing a
new dedicated `Error::CinderUnknownItem` variant cleanly
representing "tier-query found no placement" is DESIGN's call per
`wave-decisions.md` D-ErrorVariant. The wire-observable contract
(the stderr line content on the unknown-item branch) is what
matters here; the variant naming is a clarity concern for the
binary's error printer.

## Cross-feature alignment

This story-map intentionally mirrors the operator-facing posture
of `cli-migrate-subcommand-v0/discuss/story-map.md` (the
immediate predecessor) and inherits its persona (Priya),
positional-argument convention (`<tenant_id> <data_dir>
<item_id>` — three positional arguments, the same prefix as
`migrate`), quiescent-recorder convention on the Cinder side
(`CinderRecorder`), lower-case tier convention (`hot` / `warm` /
`cold` only — same set the rendering side at
`crates/kaleidoscope-cli/src/lib.rs:564-570` uses), unknown-item
stderr wording (mirroring `MigrateError::UnknownItem` at
`crates/cinder/src/store.rs:55-58`), no-Lumen-touch posture (the
`get-tier` subcommand opens ONLY the Cinder store, never the
Lumen store), and the fail-fast posture inherited from the
predecessor features.

The principal contractual difference is that this feature is
READ-ONLY. Unlike `migrate` (which mutates `entry.tier` and
`entry.migrated_at`) or `place` (which inserts a new placement
or overwrites an existing one), `get-tier` calls only
`TieringStore::get_tier(tenant, item)` which is read-only by
construction. The cross-feature read-only invariant (preserved
by `stats`, `list-items`, and inherited from earlier features)
applies here. The Cinder WAL+snapshot under
`<data_dir>/cinder.*` is byte-equivalent before and after every
invocation, in addition to the Lumen-side no-touch invariant.

## Scope Assessment: PASS — 1 story, 1 bounded context, estimated < 1 day

- 1 story (US-01).
- 1 bounded context (`kaleidoscope-cli` crate; the function-
  level change is in `lib.rs` and the dispatch is in `main.rs`;
  the new acceptance test lives in one new file).
- 2 modified files in `src/`
  (`crates/kaleidoscope-cli/src/lib.rs` for the new free
  function; `crates/kaleidoscope-cli/src/main.rs` for the new
  dispatch arm + new `run_get_tier` helper + the
  `print_usage` update); 1 new file
  (`crates/kaleidoscope-cli/tests/get_tier_subcommand.rs`); 1
  line-level modification
  (`crates/kaleidoscope-cli/Cargo.toml` for the new `[[test]]`
  entry).
- 1 integration point (the function calling
  `cinder::TieringStore::get_tier(tenant, &item)` once). No
  Lumen integration point. No mutation integration point.
- Estimated effort: well under 1 day for the crafter. Strictly
  thinner than `cli-migrate-subcommand-v0` because (a) no
  tier-argument parser is needed (`get-tier` accepts no tier
  argument), (b) no `from`/`to` pair to render, (c) no pre-
  flight `get_entry` call — the underlying `get_tier` returns
  `Option<Tier>` directly. No OTLP wiring, no concurrency test,
  no shared-handle ownership puzzle, no Cinder policy
  evaluation, no multi-item bulk shape.

The feature is right-sized. No splitting required, no thinning
possible.
