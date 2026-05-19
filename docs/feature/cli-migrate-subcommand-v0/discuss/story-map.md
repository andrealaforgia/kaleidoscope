# Story Map: `cli-migrate-subcommand-v0`

## User: Priya the platform operator

## Goal

When Priya runs
`kaleidoscope-cli migrate acme /tmp/data acme/batch-00042 cold`,
she sees on stdout — in milliseconds — exactly one line
`migrated tenant=acme item=acme/batch-00042 from=hot to=cold\n`,
exit code 0. The Cinder store under `/tmp/data/cinder.*` reflects
the move (`get_entry(acme, acme/batch-00042).unwrap().tier ==
Tier::Cold`); the Lumen store under `/tmp/data/lumen.*` is byte-
equivalent before and after. When the item id is unknown or the
tier argument is invalid, she sees exit non-zero and a single stderr
line naming the offending value; stdout is empty; the Cinder store
is unchanged. Three operationally distinct decisions are unified on
this single CLI invocation shape: manual tier rebalancing,
compensating an over-aggressive auto-tiering policy decision, and
operator-driven lifecycle testing.

## Backbone

The journey has exactly one activity: the operator triggers a
single Cinder tier transition without writing Rust. The activity
is a thin extension on the existing CLI dispatch: a single
`kaleidoscope-cli migrate <tenant> <data_dir> <item_id> <to_tier>`
invocation that internally opens
`FileBackedTieringStore::open(cinder_base(data_dir), recorder)`
with a quiescent `NoopRecorder`, calls `get_entry(tenant, item)`
once to read the `from` tier (returning early with the unknown-
item error if `None`), calls `migrate(tenant, item, to_tier,
SystemTime::now())` once, and writes the one-line stdout report
on success. The CLI substrate, the `TieringStore` trait,
`FileBackedTieringStore` adapter, the quiescent recorder pattern
(`NoopRecorder`), and the `parse_positional` helper all already
exist; this feature is a thin extension that adds a new dispatch
arm + a new library function on the existing substrate.

| Activity 1: operator triggers a single Cinder tier transition without writing Rust |
|---|
| `kaleidoscope-cli migrate <tenant> <data_dir> <item_id> <to_tier>` is dispatched by the binary's `main.rs` argument matcher to the new `run_migrate` helper. The helper parses the four positional arguments (the existing `parse_positional` returns `(TenantId, PathBuf)` for the first two; two extra `args.get(N)` calls retrieve `<item_id>` and `<to_tier>`), validates `<to_tier>` against the lower-case `hot`/`warm`/`cold` set (fail-fast with stderr line on mismatch — no Cinder store opened on this branch), then calls the new library function `migrate(...)`. The library function opens the Cinder store via `FileBackedTieringStore::open(cinder_base(data_dir), recorder)` with a quiescent `NoopRecorder`, calls `get_entry(tenant, item)` once (fail-fast with the unknown-item error if `None`), captures the returned `entry.tier` as the `from` value, calls `migrate(tenant, item, to_tier, SystemTime::now())` once, and writes one `writeln!(writer, "migrated tenant={} item={} from={} to={}")` line on success. The Lumen store under `<data_dir>/lumen.*` is NEVER opened. Exit code 0 on success; non-zero on any error branch. |

## Walking Skeleton

Per `wave-decisions.md` D2 (and the task brief), the walking-
skeleton concept does not apply because:

- The CLI already exists, with three working subcommands
  (`ingest`, `read`, `stats`).
- The `TieringStore::migrate(tenant, item, to_tier, migrated_at)`
  trait method already exists per
  `crates/cinder/src/store.rs:93-99` and returns `Result<(),
  MigrateError>`.
- The `TieringStore::get_entry(tenant, item)` trait method
  already exists per `crates/cinder/src/store.rs:89` and returns
  `Option<TierEntry>`.
- The `MigrateError::UnknownItem { tenant, item }` variant
  already exists per `crates/cinder/src/store.rs:43` and is
  surfaced by both `InMemoryTieringStore::migrate`
  (`crates/cinder/src/store.rs:179-182`) and the file-backed
  adapter (which implements the same trait).
- The `FileBackedTieringStore::open` constructor is already used
  by `ingest()` at `crates/kaleidoscope-cli/src/lib.rs:179-180`.
- The `cinder_base(data_dir)` helper already exists at
  `crates/kaleidoscope-cli/src/lib.rs:122-124`.
- The quiescent `NoopRecorder` pattern is already used by
  `ingest`'s no-flag arm at
  `crates/kaleidoscope-cli/src/lib.rs:170-174`.
- The lower-case tier rendering helper `tier_lowercase` already
  exists at `crates/kaleidoscope-cli/src/lib.rs:389-395`.

Equivalent statement: **the smallest valuable change is to add
one new `Some("migrate") => run_migrate(&args)` dispatch arm to
the `main.rs` match (lines 50-64), one new `run_migrate` helper
in `main.rs` mirroring the shape of `run_stats` (lines 226-246),
one new `migrate(...)` library function in `lib.rs`, one new
`tier_from_lowercase(...)` parser helper (or inline match) in
`lib.rs`, and one new acceptance test file
`tests/migrate_subcommand.rs`.** Slice 01 ships exactly that.

## Release Slices

### Slice 01 — `migrate` subcommand moves a single item between Cinder tiers

- **Outcome**: An operator running `kaleidoscope-cli migrate acme
  /tmp/data acme/batch-00042 cold` sees on stdout exactly one line
  `migrated tenant=acme item=acme/batch-00042 from=hot to=cold\n`,
  exit code 0. After the call, `cinder.get_entry(acme,
  acme/batch-00042).unwrap().tier == Tier::Cold`. Unknown item ids
  produce non-zero exit + stderr naming the missing item (OK2);
  invalid tier values produce non-zero exit + stderr naming the
  invalid value (OK3); same-tier migrates succeed and the stdout
  report shows `from=X to=X` faithfully (OK4 — no special case in
  the CLI, the underlying API is idempotent).
- **Stories**: `US-01` (single slice; all DoR-validated AC inside).
- **Learning hypothesis**: disproves the assumption that the
  existing `TieringStore::migrate(tenant, item, to_tier,
  migrated_at)` API is sufficient for an operator-visible CLI
  surface without needing new methods. The current trait returns
  `Result<(), MigrateError>` per
  `crates/cinder/src/store.rs:93-99`; the CLI needs to discover
  the `from` tier for the stdout report, which the trait method
  itself does NOT return. The `get_entry(tenant, item)` trait
  method at `crates/cinder/src/store.rs:89` fills this gap by
  returning the full `TierEntry` BEFORE the migrate call. If the
  assumption holds, the slice ships with one `get_entry` call
  plus one `migrate` call per CLI invocation. If the assumption
  fails — e.g. the operator demands transactional "read the
  from tier and migrate in one atomic call so a concurrent
  policy evaluation cannot race" — the failure mode tells DESIGN
  to propose a new
  `TieringStore::migrate_observed(tenant, item, to_tier,
  migrated_at) -> Result<TierEntry, MigrateError>` trait method
  in a follow-up wave (NOT pre-emptively designed here).
- **Production-data-equivalent AC**: an end-to-end test invokes
  the CLI library function (the actual entry point the binary
  calls) with a `(tenant, data_dir, item_id, to_tier_arg,
  writer)` tuple against a real temp `data_dir`, against a
  Cinder store pre-populated by direct
  `FileBackedTieringStore::open(...).place(...)` setup calls per
  `(tenant, item_id, tier, placed_at)` triple, and reads back
  the captured stdout to assert the expected line content. This
  is the same data path the operator's `kaleidoscope-cli
  migrate acme /tmp/data acme/batch-00042 cold` invocation will
  exercise.
- **Dogfood moment**: After the slice ships, Andrea opens a
  terminal, runs `cargo run --bin kaleidoscope-cli -- ingest
  acme /tmp/kdata < some_records.ndjson` (which places one Hot
  Cinder item per batch via the existing `flush()`), then `cargo
  run --bin kaleidoscope-cli -- stats acme /tmp/kdata` (which
  shows `hot=N`), then `cargo run --bin kaleidoscope-cli --
  migrate acme /tmp/kdata acme/batch-00000 cold` (which returns
  `migrated tenant=acme item=acme/batch-00000 from=hot to=cold`
  on stdout, exit 0), then `cargo run --bin kaleidoscope-cli --
  stats acme /tmp/kdata` again (which now shows `hot=N-1
  cold=1`). The four observations together — placement,
  read-back, mutation, post-mutation read-back — are the
  dogfood gate for the slice.
- **Effort**: well under 1 day. The change inside the library is
  structurally one `get_entry` call plus one `migrate` call plus
  one `writeln!` (on success) per invocation; the dispatch
  helper in `main.rs` is structurally a mirror of `run_stats`;
  the new acceptance test mirrors the existing
  `stats_subcommand.rs` harness pattern; no concurrency probe,
  no OTLP wiring, no policy evaluation.

## Priority Rationale

There is one slice and it is the only slice. The reference-class
sizing (this is the SEVENTH consecutive small feature in the
`kaleidoscope-cli` cluster, after `cinder-to-pulse-bridge-v0`,
`cinder-to-otlp-json-bridge-v0`, `cli-cinder-otlp-wiring-v0`,
`cli-read-observe-otlp-v0`, `cli-stats-subcommand-v0`, and
`cli-stats-cinder-tier-distribution-v0`, and comparable in size
to the predecessor because no OTLP wiring of any kind is needed)
means there is no benefit from further splitting:

- Slice 01 carries the new dispatch arm, the new library function,
  the new tier-argument parser, the OK1 happy-path test, the OK2
  unknown-item fail-fast test, the OK3 invalid-tier fail-fast
  test, the OK4 idempotent-same-tier test, AND the tenant-
  isolation test all together. Splitting any one of the four
  KPIs into a separate slice would force a second PR for
  trivially the same wiring — net negative for the reviewer.
- The principal KPI (OK1) is the migrate-success correctness; OK2
  and OK3 are the fail-fast guardrails on the two distinct
  input-error paths (unknown item, invalid tier); OK4 is the
  faithfulness-to-underlying-API guardrail for the same-tier case.
  Shipping any without the others is meaningless: OK1 alone with
  no unknown-item handling is dangerous (silent insert risk);
  OK1+OK2 without OK3 ships a fail-fast for one error path and
  not the other; OK4 alone is meaningless without OK1 (no
  CLI surface to be faithful to).

If schedule pressure ever forces a partial ship, **the slice is
already as thin as it can be**: the function-level addition is
one `get_entry` + one `migrate` + one `writeln!` plus a four-
line tier parser. There is no sub-slice worth shipping in
isolation.

The two-track choice between (a) introducing a new
`Error::CinderMigrate(MigrateError)` variant cleanly separating
"store-open failure" from "migrate-call failure" and (b) reusing
the existing `Error::CinderOpen(MigrateError)` variant for both
is DESIGN's call per `wave-decisions.md` D-ErrorVariant. The
wire-observable contract (the stderr line content on each error
branch) is what matters here; the variant naming is a clarity
concern for the binary's error printer.

## Cross-feature alignment

This story-map intentionally mirrors the operator-facing posture
of `cli-stats-cinder-tier-distribution-v0/discuss/story-map.md`
(the immediate predecessor) and inherits its persona (Priya),
positional-argument convention (`<tenant_id> <data_dir>`
unchanged, plus two new positional arguments `<item_id>
<to_tier>`), quiescent-recorder convention on the Cinder side
(`NoopRecorder`), lower-case tier convention (`hot` / `warm` /
`cold` only — same set the rendering side at
`crates/kaleidoscope-cli/src/lib.rs:389-395` uses), and the
fail-fast posture inherited from
`cli-stats-subcommand-v0/wave-decisions.md` D8 (the `--since`
/ `--until` parse error path at
`crates/kaleidoscope-cli/src/main.rs:198-224` — stderr names the
flag AND the verbatim bad value).

The principal contractual difference is that this feature
MUTATES the Cinder side. Unlike `stats` (read-only) or the
predecessor `cli-stats-cinder-tier-distribution-v0` (read-only),
this feature calls `TieringStore::migrate(...)` which DOES
mutate `entry.tier` and `entry.migrated_at`. The cross-feature
read-only invariant (preserved by `stats` and its tier-
distribution extension) does NOT apply here. The cross-feature
invariant this feature DOES preserve unchanged is the
no-Lumen-touch posture (per `wave-decisions.md` D-NoLumenTouch):
the `migrate` subcommand opens ONLY the Cinder store, never the
Lumen store.

## Scope Assessment: PASS — 1 story, 1 bounded context, estimated < 1 day

- 1 story (US-01).
- 1 bounded context (`kaleidoscope-cli` crate; the function-
  level change is in `lib.rs` and the dispatch is in `main.rs`;
  the new acceptance test lives in one new file).
- 2 modified files in `src/`
  (`crates/kaleidoscope-cli/src/lib.rs` for the new free
  function + tier parser; `crates/kaleidoscope-cli/src/main.rs`
  for the new dispatch arm + new `run_migrate` helper + the
  `print_usage` update); 1 new file
  (`crates/kaleidoscope-cli/tests/migrate_subcommand.rs`); 1
  line-level modification
  (`crates/kaleidoscope-cli/Cargo.toml` for the new `[[test]]`
  entry).
- 2 integration points (the function calling
  `cinder::TieringStore::get_entry(tenant, item)` once and
  `cinder::TieringStore::migrate(tenant, item, to_tier,
  migrated_at)` once). No Lumen integration point.
- Estimated effort: well under 1 day for the crafter. No OTLP
  wiring, no concurrency test, no shared-handle ownership
  puzzle, no Cinder policy evaluation, no multi-item bulk
  shape. Strictly comparable in size to the predecessor (the
  Cinder-side reads are read-only there and mutating here, but
  the structural surface area is the same: one new dispatch
  arm + one new library function + one new acceptance test
  file + one new `[[test]]` manifest entry).

The feature is right-sized. No splitting required, no thinning
possible.
