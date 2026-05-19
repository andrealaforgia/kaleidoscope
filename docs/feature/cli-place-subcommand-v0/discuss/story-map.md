# Story Map: `cli-place-subcommand-v0`

## User: Priya the platform operator

## Goal

When Priya runs
`kaleidoscope-cli place acme /tmp/data acme/bootstrap-00001 hot`,
she sees on stdout — in milliseconds — exactly one line
`placed tenant=acme item=acme/bootstrap-00001 tier=hot\n`, exit
code 0. The Cinder store under `/tmp/data/cinder.*` reflects the
placement (`get_entry(acme, acme/bootstrap-00001).unwrap().tier
== Tier::Hot`); the Lumen store under `/tmp/data/lumen.*` is
byte-equivalent before and after. When the tier argument is
invalid, she sees exit non-zero and a single stderr line naming
the offending value; stdout is empty; the Cinder store is
unchanged. When the item already exists, the placement
overwrites the prior entry (faithful to the underlying API's
overwrite-semantics; no CLI special case). When
`--observe-otlp <path>` is set, exactly one `cinder.place.count`
OTLP-JSON line is appended to `<path>` per place call.

Three operationally distinct decisions are unified on this single
CLI invocation shape: bootstrap items that exist outside the
Lumen ingest flow, set up controlled test scenarios (place N
items in Hot then `evaluate-policy`), and recover the Cinder
catalog from a manifest after a snapshot corruption.

## Backbone

The journey has exactly one activity: the operator triggers a
single Cinder placement without writing Rust. The activity is a
thin extension on the existing CLI dispatch: a single
`kaleidoscope-cli place <tenant> <data_dir> <item_id> <tier>
[--observe-otlp <path>]` invocation that internally parses the
tier argument via the existing private `parse_tier` helper at
`crates/kaleidoscope-cli/src/lib.rs:505-512`, opens
`FileBackedTieringStore::open(cinder_base(data_dir), recorder)`
with either a quiescent `CinderRecorder` (no-flag arm) or a
`CinderToOtlpJsonWriter` (--observe-otlp arm), calls
`place(tenant, item, tier, SystemTime::now())` once, and writes
the one-line stdout report on success. The CLI substrate, the
`TieringStore` trait, `FileBackedTieringStore` adapter, the
quiescent recorder pattern (`CinderRecorder`), the
`CinderToOtlpJsonWriter` OTLP wiring, the `parse_positional` and
`parse_observe_otlp` helpers, the `parse_tier` parser, and the
`tier_lowercase` renderer all already exist; this feature is a
thin extension that adds a new dispatch arm + a new library
function on the existing substrate.

| Activity 1: operator triggers a single Cinder placement without writing Rust |
|---|
| `kaleidoscope-cli place <tenant> <data_dir> <item_id> <tier> [--observe-otlp <path>]` is dispatched by the binary's `main.rs` argument matcher to the new `run_place` helper. The helper parses the four positional arguments (the existing `parse_positional` returns `(TenantId, PathBuf)` for the first two; two extra `args.get(N)` calls retrieve `<item_id>` and `<tier>`), parses the optional `--observe-otlp <path>` flag via the existing `parse_observe_otlp` helper, then calls the new library function `place(...)`. The library function validates `<tier>` against the lower-case `hot`/`warm`/`cold` set via the existing `parse_tier` helper (fail-fast with `Error::InvalidTier { value }` on mismatch — no Cinder store opened on this branch), constructs the Cinder recorder via the same `match otlp_log_path { Some(p) => CinderToOtlpJsonWriter::new(file), None => CinderRecorder }` shape `migrate()` uses, opens the Cinder store, builds `ItemId::new(item_id.to_string())`, calls `place(tenant, item, tier, SystemTime::now())` once (the trait method returns `()` — no error matching needed), and writes one `writeln!(writer, "placed tenant={} item={} tier={}")` line on success. The Lumen store under `<data_dir>/lumen.*` is NEVER opened. Exit code 0 on success; non-zero on the invalid-tier error branch. |

## Walking Skeleton

Per `wave-decisions.md` D2 (and the task brief), the walking-
skeleton concept does not apply because:

- The CLI already exists, with five working subcommands
  (`ingest`, `read`, `stats`, `migrate`, `list-items`).
- The `TieringStore::place(tenant, item, tier, placed_at)` trait
  method already exists per `crates/cinder/src/store.rs:78-81`
  and returns `()` (overwrite-semantics, no failure modes).
- The `TieringStore::get_entry(tenant, item)` trait method already
  exists per `crates/cinder/src/store.rs:89` (used by the
  acceptance test as a post-call oracle, not by production code).
- The `FileBackedTieringStore::open` constructor is already used
  by `ingest()`, `migrate()`, `list_items()`,
  `stats_with_tiers()`.
- The `cinder_base(data_dir)` helper already exists at
  `crates/kaleidoscope-cli/src/lib.rs:130-132`.
- The quiescent `CinderRecorder` pattern is already used by the
  no-flag arm of `migrate()` at
  `crates/kaleidoscope-cli/src/lib.rs:443`.
- The `CinderToOtlpJsonWriter` OTLP wiring is already used by
  `ingest()` and `migrate()` --observe-otlp arms.
- The lower-case tier rendering helper `tier_lowercase` already
  exists at `crates/kaleidoscope-cli/src/lib.rs:519-525`.
- The lower-case tier parsing helper `parse_tier` already exists
  at `crates/kaleidoscope-cli/src/lib.rs:505-512`.
- The `Error::InvalidTier { value }` variant already exists at
  `crates/kaleidoscope-cli/src/lib.rs:79-81` with the right
  Display impl at lines 98-100.
- The `parse_observe_otlp` helper already exists at
  `crates/kaleidoscope-cli/src/main.rs:178-192`.

Equivalent statement: **the smallest valuable change is to add
one new `Some("place") => run_place(&args)` dispatch arm to the
`main.rs` match (lines 54-69), one new `run_place` /
`run_place_with` helper in `main.rs` mirroring the shape of
`run_migrate` / `run_migrate_with` (lines 276-306), one new
`place(...)` library function in `lib.rs`, and one new acceptance
test file `tests/place_subcommand.rs`.** Slice 01 ships exactly
that.

## Release Slices

### Slice 01 — `place` subcommand bootstraps a single Cinder item

- **Outcome**: An operator running `kaleidoscope-cli place acme
  /tmp/data acme/bootstrap-00001 hot` sees on stdout exactly one
  line `placed tenant=acme item=acme/bootstrap-00001 tier=hot\n`,
  exit code 0. After the call, `cinder.get_entry(acme,
  acme/bootstrap-00001).unwrap().tier == Tier::Hot`. Re-placing an
  existing item overwrites the prior entry (OK2 — faithful to
  overwrite-semantics, no CLI special case); invalid tier values
  produce non-zero exit + stderr naming the invalid value (OK3);
  `--observe-otlp <path>` appends exactly one `cinder.place.count`
  OTLP-JSON line per call (OK4).
- **Stories**: `US-01` (single slice; all DoR-validated AC inside).
- **Learning hypothesis**: disproves the assumption that the
  existing `TieringStore::place(tenant, item, tier, placed_at)`
  API is sufficient for an operator-visible CLI placement surface
  without needing new methods. The current trait returns `()` per
  `crates/cinder/src/store.rs:78-81` (no failure modes; overwrite-
  semantics); the CLI needs no pre-flight `get_entry` call (unlike
  `migrate` which needs to discover a `from` tier for its stdout
  report). If the assumption holds, the slice ships with one
  `place` call per CLI invocation — the thinnest possible mutation
  shape. If the assumption fails — e.g. the operator demands a
  pre-flight existence check so accidental overwrites are blocked
  — the failure mode tells DESIGN to propose either a new
  `TieringStore::place_if_absent(...) -> Result<(), AlreadyPlaced>`
  trait method or a `--no-overwrite` CLI flag in a follow-up wave
  (NOT pre-emptively designed here, per the task brief's
  explicit out-of-scope: "Verification that item_id doesn't
  already exist (place() is overwrite-semantics per Cinder API —
  document)").
- **Production-data-equivalent AC**: an end-to-end test invokes
  the CLI library function (the actual entry point the binary
  calls) with a `(tenant, data_dir, item_id, tier_arg, writer,
  otlp_log_path)` tuple against a real temp `data_dir`, optionally
  pre-populated by direct `FileBackedTieringStore::open(...).place(...)`
  setup calls per `(tenant, item_id, tier, placed_at)` triple,
  and reads back the captured stdout to assert the expected line
  content. The OTLP-JSON sidecar file is read back and asserted to
  contain exactly one `cinder.place.count` line per place call
  when the flag is set. This is the same data path the operator's
  `kaleidoscope-cli place acme /tmp/data acme/bootstrap-00001 hot`
  invocation will exercise.
- **Dogfood moment**: After the slice ships, Andrea opens a
  terminal, runs `cargo run --bin kaleidoscope-cli -- place acme
  /tmp/kdata acme/bootstrap-00001 hot` (returns `placed
  tenant=acme item=acme/bootstrap-00001 tier=hot` on stdout, exit
  0), then `cargo run --bin kaleidoscope-cli -- list-items acme
  /tmp/kdata hot` (which shows `acme/bootstrap-00001`), then
  `cargo run --bin kaleidoscope-cli -- stats acme /tmp/kdata`
  (which shows `hot=1`). Then `cargo run --bin kaleidoscope-cli
  -- place acme /tmp/kdata acme/bootstrap-00001 cold --observe-otlp
  /tmp/cinder.otlp.json` (returns `placed tenant=acme
  item=acme/bootstrap-00001 tier=cold`, the file gains one
  `cinder.place.count` line), then `cargo run --bin
  kaleidoscope-cli -- list-items acme /tmp/kdata hot` shows
  EMPTY (the overwrite took the item out of Hot) and
  `kaleidoscope-cli list-items acme /tmp/kdata cold` shows
  `acme/bootstrap-00001`. The five observations together — first
  placement, read-back, stats confirmation, overwrite (with
  OTLP), post-overwrite read-back — are the dogfood gate for the
  slice.
- **Effort**: well under 1 day. The change inside the library is
  structurally one `parse_tier` call plus one recorder
  construction plus one `FileBackedTieringStore::open` plus one
  `place` call plus one `writeln!` per invocation; the dispatch
  helper in `main.rs` is structurally a mirror of `run_migrate`;
  the new acceptance test mirrors the existing
  `migrate_subcommand.rs` and `migrate_observe_otlp_flag.rs`
  harness patterns; no concurrency probe, no policy evaluation.

## Priority Rationale

There is one slice and it is the only slice. The reference-class
sizing (this is the EIGHTH consecutive small feature in the
`kaleidoscope-cli` cluster, after `cinder-to-pulse-bridge-v0`,
`cinder-to-otlp-json-bridge-v0`, `cli-cinder-otlp-wiring-v0`,
`cli-read-observe-otlp-v0`, `cli-stats-subcommand-v0`,
`cli-stats-cinder-tier-distribution-v0`,
`cli-migrate-subcommand-v0`,
`cli-migrate-observe-otlp-v0`, and `cli-list-items-subcommand-v0`)
means there is no benefit from further splitting:

- Slice 01 carries the new dispatch arm, the new library function,
  the OK1 happy-path test, the OK2 overwrite-semantics test, the
  OK3 invalid-tier fail-fast test, the OK4 --observe-otlp emission
  test, AND the tenant-isolation test all together. Splitting any
  one of the four KPIs into a separate slice would force a second
  PR for trivially the same wiring — net negative for the
  reviewer.
- The principal KPI (OK1) is the place-success correctness; OK2
  is the faithfulness-to-underlying-API guardrail for the
  overwrite case; OK3 is the fail-fast guardrail on the parse-
  side error path; OK4 is the operator-facing observability
  emission. Shipping any without the others is meaningless: OK1
  alone with no invalid-tier handling is dangerous (silent
  fallback risk); OK1+OK3 without OK2 is incomplete (operator
  cannot rely on overwrite-semantics from the manifest-recovery
  use case); OK4 alone is meaningless without OK1 (no CLI
  surface to observe). The OK4 emission case naturally
  piggybacks on OK1's recorder-construction pattern (same
  `match otlp_log_path { Some(p) => OtlpJsonWriter, None =>
  Quiescent }` shape used by `migrate`).

If schedule pressure ever forces a partial ship, **the slice is
already as thin as it can be**: the function-level addition is
one `parse_tier` + one recorder construction + one `place` +
one `writeln!`. There is no sub-slice worth shipping in
isolation. The OK4 `--observe-otlp` emission is structurally
free: the recorder construction is the same `match` block as
`migrate()`'s, and the trait's `place` method already calls
`record_place(tenant, tier)` exactly once per call at
`crates/cinder/src/store.rs:151` — no additional wiring is
needed on the recording side.

## Cross-feature alignment

This story-map intentionally mirrors the operator-facing posture
of `cli-migrate-subcommand-v0/discuss/story-map.md` and
`cli-migrate-observe-otlp-v0/discuss/story-map.md` (the two
immediate predecessors that together established the precedent
for "mutation CLI subcommand + --observe-otlp wiring"). It
inherits its persona (Priya), positional-argument convention
(`<tenant_id> <data_dir>` unchanged, plus two new positional
arguments `<item_id> <tier>`), the `--observe-otlp <path>` optional
flag convention (mirrors `migrate --observe-otlp` and `ingest
--observe-otlp` exactly), quiescent-recorder convention on the
Cinder side (`CinderRecorder` for the no-flag arm), OTLP-JSON
writer convention on the Cinder side (`CinderToOtlpJsonWriter`
for the --observe-otlp arm), lower-case tier convention (`hot` /
`warm` / `cold` only — same set the rendering side at
`crates/kaleidoscope-cli/src/lib.rs:519-525` uses and the parse
side at lines 505-512 enforces), and the fail-fast posture
inherited from `cli-stats-subcommand-v0/wave-decisions.md` D8 and
`cli-migrate-subcommand-v0`'s OK3.

The principal contractual difference is that this feature creates
NEW Cinder placements (vs `migrate` which mutates existing
placements, or `list-items`/`stats` which read placements). The
underlying API (`TieringStore::place`) is overwrite-semantics
(returns `()`, no failure modes); `migrate` was `Result<(),
MigrateError>` (could return `UnknownItem`). The CLI surface
mirrors this difference: `place` has only ONE error branch
(invalid tier) vs `migrate`'s TWO (invalid tier + unknown item).
The cross-feature invariant this feature DOES preserve unchanged
is the no-Lumen-touch posture (per `wave-decisions.md`
D-NoLumenTouch): the `place` subcommand opens ONLY the Cinder
store, never the Lumen store. The cross-feature invariant on
`--observe-otlp` emission shape is preserved too: one
`cinder.place.count` line per call, byte-identical to the line
`ingest --observe-otlp` and the per-place portion of `migrate
--observe-otlp` already emit.

## Scope Assessment: PASS — 1 story, 1 bounded context, estimated < 1 day

- 1 story (US-01).
- 1 bounded context (`kaleidoscope-cli` crate; the function-level
  change is in `lib.rs` and the dispatch is in `main.rs`; the new
  acceptance test lives in one new file).
- 2 modified files in `src/`
  (`crates/kaleidoscope-cli/src/lib.rs` for the new free function;
  `crates/kaleidoscope-cli/src/main.rs` for the new dispatch arm
  + new `run_place` / `run_place_with` helpers + the
  `print_usage` update); 1 new file
  (`crates/kaleidoscope-cli/tests/place_subcommand.rs`); 1
  line-level modification (`crates/kaleidoscope-cli/Cargo.toml`
  for the new `[[test]]` entry).
- 2 integration points (the function calling
  `cinder::FileBackedTieringStore::open(cinder_base(data_dir),
  recorder)` once and `cinder::TieringStore::place(tenant, item,
  tier, SystemTime::now())` once). No Lumen integration point.
- Estimated effort: well under 1 day for the crafter. No
  policy evaluation, no concurrency test, no multi-item bulk
  shape, no pre-flight existence check (per the task brief's
  explicit out-of-scope). The OK4 `--observe-otlp` emission case
  piggybacks on `migrate`'s established recorder-construction
  pattern at zero marginal design cost. Strictly comparable in
  size to the immediate predecessors (`cli-migrate-subcommand-v0`,
  `cli-migrate-observe-otlp-v0`); the structural surface area is
  the same (one new dispatch arm + one new library function +
  one new acceptance test file + one new `[[test]]` manifest
  entry).

The feature is right-sized. No splitting required, no thinning
possible.
