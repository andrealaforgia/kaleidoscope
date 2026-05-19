# Story Map: `cli-list-items-subcommand-v0`

## User: Priya the platform operator

## Goal

When Priya runs
`kaleidoscope-cli list-items acme /tmp/data cold`, she sees on
stdout — in milliseconds — exactly N lines (where N is the count
`stats` already reports as `cold=N`), each carrying one item id
sorted lexicographically, terminated by `\n`. Exit code 0. The
Cinder store under `/tmp/data/cinder.*` is byte-equivalent before
and after (read-only); the Lumen store under `/tmp/data/lumen.*`
is byte-equivalent before and after (never opened). When the
tier argument is invalid, she sees exit non-zero and a single
stderr line naming the offending value; stdout is empty; the
Cinder store is unopened. When the queried tier is empty,
stdout is empty (zero bytes), exit 0. Three operationally
distinct decisions are unified on this single CLI invocation
shape: manual rebalancing follow-up (which cold items to migrate
back?), sanity check against tenant manifest (are these the
expected items?), and scripted pipelines (`list-items ... |
xargs migrate ...`).

## Backbone

The journey has exactly one activity: the operator enumerates
every item a tenant currently has in a given Cinder tier without
writing Rust. The activity is a thin extension on the existing
CLI dispatch: a single `kaleidoscope-cli list-items <tenant>
<data_dir> <tier>` invocation that internally parses
`<tier>` via the existing `parse_tier` helper at
`crates/kaleidoscope-cli/src/lib.rs:475-482` (fail-fast with
stderr line on mismatch — no Cinder store opened on this
branch), opens
`FileBackedTieringStore::open(cinder_base(data_dir), recorder)`
with a quiescent `CinderRecorder`, calls
`list_by_tier(&tenant, tier)` once, sorts the returned
`Vec<ItemId>` lexicographically, and writes one `writeln!`
line per item to stdout. The CLI substrate, the `TieringStore`
trait, `FileBackedTieringStore` adapter, the quiescent recorder
pattern, the `parse_tier` helper, and the `parse_positional`
helper all already exist; this feature is a thin extension that
adds a new dispatch arm + a new library function on the
existing substrate.

| Activity 1: operator enumerates every item a tenant currently has in a given Cinder tier without writing Rust |
|---|
| `kaleidoscope-cli list-items <tenant> <data_dir> <tier>` is dispatched by the binary's `main.rs` argument matcher to the new `run_list_items` helper. The helper parses the three positional arguments (the existing `parse_positional` returns `(TenantId, PathBuf)` for the first two; one extra `args.get(4)` call retrieves `<tier>`), then calls the new library function `list_items(...)`. The library function parses `<tier>` against the existing lower-case `hot`/`warm`/`cold` `parse_tier` helper (fail-fast with the `Error::InvalidTier { value }` variant on mismatch — Cinder store NOT opened on this branch), opens the Cinder store via `FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))` (read-only, quiescent recorder), calls `cinder.list_by_tier(&tenant, tier)` once to obtain a `Vec<ItemId>`, sorts the vec lexicographically (`vec.sort()` — `ItemId(String)` derives `Ord`), and writes one `writeln!(writer, "{}", item_id.0)` per item to stdout. The Lumen store under `<data_dir>/lumen.*` is NEVER opened. The Cinder store is opened READ-ONLY (no `place` / `migrate` / `evaluate_at` calls). Exit code 0 on success; non-zero on the invalid-tier branch. |

## Walking Skeleton

Per `wave-decisions.md` D2 (and the task brief), the walking-
skeleton concept does not apply because:

- The CLI already exists, with four working subcommands
  (`ingest`, `read`, `stats`, `migrate`).
- The `TieringStore::list_by_tier(tenant, tier)` trait method
  already exists per `crates/cinder/src/store.rs:102` and
  returns `Vec<ItemId>`. It is the EXACT method
  `stats_with_tiers` already calls in production at
  `crates/kaleidoscope-cli/src/lib.rs:383`.
- The `FileBackedTieringStore::open` constructor is already
  used by `stats_with_tiers` at
  `crates/kaleidoscope-cli/src/lib.rs:377-378` and by `migrate`
  at `lib.rs:445-446`.
- The `cinder_base(data_dir)` helper already exists at
  `crates/kaleidoscope-cli/src/lib.rs:130-132`.
- The quiescent `CinderRecorder` pattern is already used by
  `stats_with_tiers` at `lib.rs:377` and by `migrate`'s no-flag
  arm at `lib.rs:443`.
- The `parse_tier` helper already exists at
  `crates/kaleidoscope-cli/src/lib.rs:475-482` (currently
  private; promoting to `pub(crate)` or duplicating the four-
  line `match` is DESIGN's choice).
- The `Error::InvalidTier { value: String }` variant already
  exists at `crates/kaleidoscope-cli/src/lib.rs:79-81` with the
  `Display` impl that prints the verbatim invalid value at
  `lib.rs:98-100` — no new error variant needed.

Equivalent statement: **the smallest valuable change is to add
one new `Some("list-items") => run_list_items(&args)` dispatch
arm to the `main.rs` match (lines 52-66), one new
`run_list_items` helper in `main.rs` mirroring the shape of
`run_migrate` (lines 264-294), one new `list_items(...)`
library function in `lib.rs`, and one new acceptance test file
`tests/list_items_subcommand.rs`.** Slice 01 ships exactly
that. No new `Error` variant; no new trait method; no new
external dependency.

## Release Slices

### Slice 01 — `list-items` subcommand enumerates every item for a tenant in a tier

- **Outcome**: An operator running `kaleidoscope-cli list-items
  acme /tmp/data cold` sees on stdout exactly N lines (one item
  id per line, sorted lexicographically) representing every
  Cinder item tenant `acme` currently has in the Cold tier, exit
  code 0. After the call, the Cinder store under
  `/tmp/data/cinder.*` is byte-equivalent (read-only). Two
  successive invocations produce byte-identical stdout
  (determinism via lex-sort at the boundary, masking the
  `HashMap` iteration randomisation). When the tier is empty,
  stdout is empty (N=0 case). When the tier argument is invalid,
  exit non-zero + stderr naming the invalid value (OK3). When
  another tenant has same-named items, those are NOT included
  (OK2 tenant isolation).
- **Stories**: `US-01` (single slice; all DoR-validated AC
  inside).
- **Learning hypothesis**: confirms (or disproves) the assumption
  that the existing `TieringStore::list_by_tier(tenant, tier) ->
  Vec<ItemId>` API is sufficient for an operator-visible
  enumeration surface without needing new methods or richer
  return types. The current trait returns `Vec<ItemId>` per
  `crates/cinder/src/store.rs:102`; the CLI needs no other
  information about each item (no tier-entry metadata, no
  timestamps, no placement order) because the operator's three
  decisions all need only the bare item id. If the assumption
  holds, the slice ships with one `list_by_tier` call plus one
  sort plus N `writeln!` calls per CLI invocation. If the
  assumption fails — e.g. the operator asks "I also want the
  `migrated_at` timestamp per item" — the failure mode tells
  DESIGN to propose a follow-up `list-entries` subcommand
  exposing `TierEntry` (NOT pre-emptively designed here).
- **Production-data-equivalent AC**: an end-to-end test invokes
  the CLI library function (the actual entry point the binary
  calls) with a `(tenant, data_dir, tier_arg, writer)` tuple
  against a real temp `data_dir`, against a Cinder store
  pre-populated by direct
  `FileBackedTieringStore::open(...).place(...)` setup calls
  per `(tenant, item_id, tier, placed_at)` triple, and reads
  back the captured stdout to assert the expected byte
  sequence. This is the same data path the operator's
  `kaleidoscope-cli list-items acme /tmp/data cold` invocation
  will exercise.
- **Dogfood moment**: After the slice ships, Andrea opens a
  terminal, runs `cargo run --bin kaleidoscope-cli -- ingest
  acme /tmp/kdata < some_records.ndjson` (which places Hot
  Cinder items), then `cargo run --bin kaleidoscope-cli --
  migrate acme /tmp/kdata acme/batch-00000 cold` (which moves
  one item to Cold), then `cargo run --bin kaleidoscope-cli --
  stats acme /tmp/kdata` (which shows `cold=1`), then `cargo
  run --bin kaleidoscope-cli -- list-items acme /tmp/kdata cold`
  (which prints `acme/batch-00000` on stdout, exit 0). The
  scripted-pipeline dogfood: `cargo run --bin kaleidoscope-cli
  -- list-items acme /tmp/kdata cold | xargs -I {} cargo run
  --bin kaleidoscope-cli -- migrate acme /tmp/kdata {} warm`
  walks each cold item back to warm in one shell loop. The
  before/after sequence is the dogfood gate for the slice.
- **Effort**: well under 1 day. The change inside the library
  is structurally one `parse_tier` call plus one open plus one
  `list_by_tier` call plus one sort plus N `writeln!` calls per
  invocation; the dispatch helper in `main.rs` is structurally
  a mirror of `run_migrate`; the new acceptance test mirrors
  the existing `migrate_subcommand.rs` harness pattern; no
  concurrency probe, no OTLP wiring, no Cinder mutation, no
  policy evaluation.

## Priority Rationale

There is one slice and it is the only slice. The reference-class
sizing (this is the EIGHTH consecutive small feature in the
`kaleidoscope-cli` cluster, after `cinder-to-pulse-bridge-v0`,
`cinder-to-otlp-json-bridge-v0`, `cli-cinder-otlp-wiring-v0`,
`cli-read-observe-otlp-v0`, `cli-stats-subcommand-v0`,
`cli-stats-cinder-tier-distribution-v0`, and
`cli-migrate-subcommand-v0`, and STRICTLY THINNER than the
predecessor because the subcommand is read-only and no
`from`/`to` resolution is needed) means there is no benefit
from further splitting:

- Slice 01 carries the new dispatch arm, the new library
  function, the OK1 correctness test (with the lex-sort
  determinism sub-assertion AND the N=0 empty-result sub-
  scenario), the OK2 tenant-isolation test, AND the OK3
  invalid-tier fail-fast test all together. Splitting any one
  of the three KPIs into a separate slice would force a second
  PR for trivially the same wiring — net negative for the
  reviewer.
- The principal KPI (OK1) is the list-items correctness; OK2
  is the tenant-isolation guardrail (inherited from the
  per-tenant `list_by_tier` filter); OK3 is the fail-fast
  guardrail on the input-error path. Shipping any without the
  others is meaningless: OK1 alone with no invalid-tier
  handling is dangerous (silent fallback to a default tier
  risk); OK1+OK3 without OK2 omits the cross-tenant safety
  story; OK2 alone is meaningless without OK1 (no CLI surface
  to be tenant-safe on).

If schedule pressure ever forces a partial ship, **the slice is
already as thin as it can be**: the function-level addition is
one `parse_tier` + one open + one `list_by_tier` + one sort + N
`writeln!`s. There is no sub-slice worth shipping in isolation.

The single two-track choice — whether to make the existing
private `parse_tier` helper at
`crates/kaleidoscope-cli/src/lib.rs:475-482` `pub(crate)` so the
new `list_items` function can reuse it directly, OR to
duplicate the four-line `match` inline — is DESIGN's call. The
wire-observable contract is identical either way.

## Cross-feature alignment

This story-map intentionally mirrors the operator-facing posture
of `cli-migrate-subcommand-v0/discuss/story-map.md` (the
immediate predecessor) and inherits its persona (Priya),
positional-argument convention (`<tenant_id> <data_dir>`
unchanged, plus one new positional argument `<tier>` —
strictly fewer arguments than `migrate`'s four), quiescent-
recorder convention on the Cinder side (`CinderRecorder`),
lower-case tier convention (`hot` / `warm` / `cold` only — same
set the rendering side at
`crates/kaleidoscope-cli/src/lib.rs:489-495` uses), parse-then-
open ordering (lib.rs:432-446 in `migrate`), and the fail-fast
posture inherited from `cli-stats-subcommand-v0/wave-decisions.md`
D8 (the `--since`/`--until` parse error path at
`crates/kaleidoscope-cli/src/main.rs:214-240` — stderr names
the offending value).

The principal contractual difference is that this feature is
PURELY READ-ONLY on the Cinder side. Unlike `migrate` (which
mutates `entry.tier` and `entry.migrated_at`) but LIKE `stats`
(which only reads), this feature calls only
`TieringStore::list_by_tier(...)` which does NOT mutate any
Cinder state. The cross-feature read-only invariant (preserved
by `stats` and its tier-distribution extension) is the
invariant this feature DOES preserve unchanged. The
no-Lumen-touch posture (per `wave-decisions.md`
D-NoLumenTouch) is also inherited: the `list-items` subcommand
opens ONLY the Cinder store, never the Lumen store.

The output shape — one line per item id, NO `key=value`, NO
header — is the FIRST instance in the cluster of a one-record-
per-line stdout shape. `stats` (one line per stat) and
`migrate` (one line per invocation) use `key=value`; `read` (one
line per record) uses NDJSON. `list-items` adopts the
one-bare-id-per-line shape because the natural consumer is
`xargs -I {} ... {} ...` which expects one record per line on
stdin. This choice is documented in
`user-stories.md` Solution and in `wave-decisions.md`
D-OutputShape.

## Scope Assessment: PASS — 1 story, 1 bounded context, estimated < 1 day

- 1 story (US-01).
- 1 bounded context (`kaleidoscope-cli` crate; the function-
  level change is in `lib.rs` and the dispatch is in `main.rs`;
  the new acceptance test lives in one new file).
- 2 modified files in `src/`
  (`crates/kaleidoscope-cli/src/lib.rs` for the new free
  function; `crates/kaleidoscope-cli/src/main.rs` for the new
  dispatch arm + new `run_list_items` helper + the
  `print_usage` update); 1 new file
  (`crates/kaleidoscope-cli/tests/list_items_subcommand.rs`);
  1 line-level modification
  (`crates/kaleidoscope-cli/Cargo.toml` for the new `[[test]]`
  entry).
- 1 integration point (the function calling
  `cinder::TieringStore::list_by_tier(tenant, tier)` once per
  invocation — STRICTLY FEWER than `migrate`'s two integration
  points). No Lumen integration point. No Cinder mutation
  integration point.
- Estimated effort: well under 1 day for the crafter. STRICTLY
  THINNER than the predecessor (`cli-migrate-subcommand-v0`):
  no `get_entry` pre-flight call, no `from`/`to` resolution,
  no mutation, no timestamp argument. The structural surface
  area is the same shape (one new dispatch arm + one new
  library function + one new acceptance test + one new
  `[[test]]` manifest entry) but each piece is shorter.

The feature is right-sized. No splitting required, no thinning
possible.
