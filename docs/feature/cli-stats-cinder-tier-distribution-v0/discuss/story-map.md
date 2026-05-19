# Story Map: `cli-stats-cinder-tier-distribution-v0`

## User: Priya the platform operator

## Goal

When Priya runs `kaleidoscope-cli stats acme /tmp/data`, she sees on
stdout — in milliseconds — the existing Lumen-side answer
(records count + time window from the predecessor wave) AND the new
Cinder-side answer (current per-tier item counts) on up to six
plain-text key=value lines, terminated by `\n`. The output pipes
naturally through `grep` / `cut` / `awk` and answers three
operationally distinct tier-related questions in the same invocation
("are warm-to-cold migrations happening?", "should I expect reads to
be slow?", "is the hot tier ballooning?"). Crucially, for any tenant
with zero Cinder placements, the stdout is byte-equivalent to the
predecessor's output — no existing operator shell pipeline breaks
(OK4).

## Backbone

The journey has exactly one activity: the operator inspects a
tenant's tier distribution without writing Rust or inspecting
`cinder.*` snapshots. The activity is a thin extension on the
predecessor's single-call shape: a single `kaleidoscope-cli stats
<tenant> <data_dir>` invocation that internally calls
`lumen.query(tenant, TimeRange::all())` exactly once (inherited
unchanged from the predecessor) AND calls
`cinder.list_by_tier(tenant, Tier::Hot/Warm/Cold)` exactly three
times (new in this feature). The CLI substrate, `LogStore` and
`TieringStore` traits, `FileBackedLogStore` and
`FileBackedTieringStore` adapters, quiescent recorder patterns
(`LumenToPulseRecorder` on the Lumen side, `NoopRecorder` on the
Cinder side), and `parse_positional` helper all already exist; this
feature is a thin extension that adds three new computed lines on
the existing substrate.

| Activity 1: operator inspects a tenant's tier distribution without writing Rust |
|---|
| `kaleidoscope-cli stats <tenant> <data_dir>` is dispatched by the binary's `main.rs` argument matcher to the library function. The function opens the Lumen store (inherited from predecessor), opens the Cinder store via `FileBackedTieringStore::open(cinder_base(data_dir), recorder)` with a quiescent `NoopRecorder`, queries Lumen once for records+time-range, calls `list_by_tier(tenant, tier)` three times (one per `Tier::Hot`/`Warm`/`Cold`), and writes plain-text key=value lines to stdout: the existing `records=N` always; the existing `earliest=`/`latest=` lines only when N > 0; the new `hot=H` / `warm=W` / `cold=C` lines only when the respective count is non-zero (Option B per `wave-decisions.md`). Neither store is mutated (read-only). No OTLP file created. Exit code 0 in all cases. |

## Walking Skeleton

Per `wave-decisions.md` (no explicit decision needed; the answer is
N/A), the walking-skeleton concept does not apply because:

- The CLI already exists, with three working subcommands (`ingest`,
  `read`, `stats`).
- The `stats` subcommand already exists (shipped in commit `75f15a6`
  from `cli-stats-subcommand-v0`).
- The `TieringStore::list_by_tier(tenant, tier)` trait method already
  returns `Vec<ItemId>` per `crates/cinder/src/store.rs:101-102`.
- The `FileBackedTieringStore::open` constructor is already used by
  `ingest()` at `crates/kaleidoscope-cli/src/lib.rs:179-180`.
- The `cinder_base(data_dir)` helper already exists at
  `crates/kaleidoscope-cli/src/lib.rs:122-124`.
- The quiescent `NoopRecorder` pattern is already used by `ingest`'s
  no-flag arm (`crates/kaleidoscope-cli/src/lib.rs:170-174`).

Equivalent statement: **the smallest valuable change is to extend the
existing `stats()` library function (or add a parallel
`stats_with_cinder()` function — DESIGN's decision per
`wave-decisions.md` D9) so that the function also opens the
`FileBackedTieringStore` at `cinder_base(data_dir)`, calls
`list_by_tier(tenant, tier)` three times, and writes the three new
key=value lines to the supplied writer when the respective counts are
non-zero.** Slice 01 ships exactly that.

## Release Slices

### Slice 01 — `stats` includes Cinder tier distribution

- **Outcome**: An operator running
  `kaleidoscope-cli stats acme /tmp/data` sees up to six stdout lines
  for a tenant with both Lumen records and Cinder placements
  (`records=N`, `earliest=...`, `latest=...`, `hot=H`, `warm=W`,
  `cold=C`), where each `<tier>=<count>` line is emitted only when
  the count is non-zero. Each per-tier count equals
  `cinder.list_by_tier(tenant, tier).len()` for the corresponding
  `Tier::Hot/Warm/Cold` (OK1); per-tenant isolation is honoured (OK2);
  the empty-Lumen-orphan-Cinder case yields `records=0\n<hot/warm/cold
  lines>\n` (OK3 with Option B); the zero-Cinder-placement case is
  byte-equivalent to the predecessor's output (OK4).
- **Stories**: `US-01` (single slice; all DoR-validated AC inside).
- **Learning hypothesis**: disproves the assumption that the existing
  `TieringStore::list_by_tier(tenant, tier)` API is sufficient for
  tier-distribution stats without needing new methods. The current
  trait returns a `Vec<ItemId>` per tier per tenant
  (`crates/cinder/src/store.rs:101-102`); the assumption is that
  materialising the full vector and reducing to `.len()` is
  acceptable at v0 tier-size volumes (an aggregate
  `count_by_tier(tenant, tier) -> usize` method might be more
  efficient at huge tier sizes, but premature without measurement).
  If the assumption holds, the slice ships with three
  `list_by_tier().len()` calls per `stats()` invocation. If the
  assumption fails — materialising three full per-tier item-id
  vectors is operationally hostile — the failure mode tells DESIGN to
  propose a new `TieringStore::count_by_tier(tenant, tier) -> usize`
  trait method as a follow-up feature (NOT pre-emptively designed
  here).
- **Production-data-equivalent AC**: an end-to-end test invokes the
  CLI library function (the actual entry point the binary calls)
  with a `(tenant, data_dir, writer)` triple against a real temp
  `data_dir`, against a Cinder store pre-populated by direct
  `FileBackedTieringStore::open(...).place(...)` setup calls per
  `(tenant, item_id, tier, placed_at)` triple, and reads back the
  captured stdout to assert the expected line count and per-line
  content. This is the same data path the operator's
  `kaleidoscope-cli stats acme /tmp/data` invocation will exercise.
- **Dogfood moment**: After the slice ships, Andrea opens a terminal,
  runs a sequence of `cargo run --bin kaleidoscope-cli -- ingest acme
  /tmp/kdata < some_records.ndjson` invocations (each one places one
  Hot Cinder item per batch via the existing `flush()`), then
  manually exercises tier migration (or simulates it by direct
  Cinder calls), then `cargo run --bin kaleidoscope-cli -- stats acme
  /tmp/kdata`. Stdout shows up to six lines. `cargo run --bin
  kaleidoscope-cli -- stats acme /tmp/kdata | grep -E
  '^(hot|warm|cold)=' | wc -l` returns the count of non-empty tiers.
  For comparison: a fresh tenant `globex` with only Lumen records and
  zero Cinder placements (verified by direct inspection) yields the
  byte-equivalent three-line predecessor output. The three observations
  together are the dogfood gate for the slice.
- **Effort**: well under 1 day. The change inside the library is
  structurally three `list_by_tier()` calls plus three conditional
  `writeln!`s appended to the existing `stats()` (or to a new sibling
  function `stats_with_cinder()` — DESIGN locks the choice); the new
  acceptance test mirrors the existing
  `stats_subcommand.rs` harness pattern; no concurrency probe, no
  OTLP wiring, no policy evaluation.

## Priority Rationale

There is one slice and it is the only slice. The reference-class
sizing (this is the sixth consecutive small feature in the
`kaleidoscope-cli` cluster, after `cinder-to-pulse-bridge-v0`,
`cinder-to-otlp-json-bridge-v0`, `cli-cinder-otlp-wiring-v0`,
`cli-read-observe-otlp-v0`, and `cli-stats-subcommand-v0`, and
comparable to the predecessor because no OTLP wiring of any kind is
needed) means there is no benefit from further splitting:

- Slice 01 carries the library function modification (extend `stats()`
  with a third capability OR add `stats_with_cinder()` — DESIGN locks
  the choice), the OK1 tier-counts test, the OK2 tenant-isolation
  test, the OK3 orphan-Cinder test, and the OK4 backwards-compat test
  all together. Splitting any one of the four KPIs into a separate
  slice would force a second PR for trivially the same wiring — net
  negative for the reviewer.
- The principal KPI (OK1) is the tier-count correctness; OK2 is the
  per-tenant isolation guardrail; OK3 is the empty-Lumen-orphan-Cinder
  case that surfaces operationally meaningful state without breaking
  the predecessor's empty-tenant contract for the never-touched case;
  OK4 is the backwards-compat guarantee against breaking existing
  operator shell pipelines. Shipping any without the others is
  meaningless: OK1 alone with no isolation guarantee is dangerous
  (cross-tenant leak); OK2 alone without OK1 is structurally
  impossible; OK3 alone without OK1 is also structurally impossible
  (the empty-Lumen-orphan-Cinder case is defined in terms of the
  non-zero Cinder lines that OK1 produces); OK4 alone is the
  predecessor's behaviour unchanged, which is not a feature.

If schedule pressure ever forces a partial ship, **the slice is
already as thin as it can be**: the function-level modification is
three `list_by_tier()` calls plus three conditional `writeln!`s on
top of the existing `stats()` shape (or a parallel sibling function
of the same structure). There is no sub-slice worth shipping in
isolation.

The choice between the three empty-render options (Option A: never
emit Cinder lines for empty Lumen; Option B: selectively emit non-zero
Cinder lines; Option C: always emit all six lines with explicit
zeros) is locked in `wave-decisions.md` D-EmptyRender: **Option B**.
Rationale documented in the wave-decisions file and recapped in the
slice brief's "Rejected alternatives" section.

## Cross-feature alignment

This story-map intentionally mirrors the operator-facing posture of
`cli-stats-subcommand-v0/discuss/story-map.md` (the predecessor) and
inherits its persona (Priya), Lumen-store substrate
(`FileBackedLogStore::open(lumen_base(data_dir), recorder)`),
quiescent-recorder convention on the Lumen side
(`LumenToPulseRecorder`), and the plain-text key=value output shape
(`<key>=<value>\n` lines). The principal contractual difference is
that this feature ADDS the Cinder side: it opens
`FileBackedTieringStore::open(cinder_base(data_dir), recorder)` with
a quiescent `NoopRecorder`, calls `list_by_tier(tenant, tier)` three
times, and conditionally emits the three new lines per Option B.

The cross-feature contract this feature DOES inherit is the
positional-argument convention (`<tenant_id> <data_dir>`,
unchanged), the stream contract (stdout for the principal output,
unchanged), the no-flag posture (no `--observe-otlp` accepted by
`stats` in v0, unchanged from `cli-stats-subcommand-v0/wave-decisions.md`
D3), the tenant-isolation invariant (now extended from
`lumen::LogStore` to `cinder::TieringStore` as well), the read-only
invariant (now extended to also leave the Cinder WAL+snapshot
unchanged), and the byte-equivalent backwards-compatibility
contract (NEW guardrail, encoded as OK4).

## Scope Assessment: PASS — 1 story, 1 bounded context, estimated < 1 day

- 1 story (US-01).
- 1 bounded context (`kaleidoscope-cli` crate; the function-level
  change is either an extension of `stats()` in `lib.rs` or a new
  parallel function `stats_with_cinder()` in `lib.rs`; the new
  acceptance test lives in one new file).
- 1 modified file (`crates/kaleidoscope-cli/src/lib.rs`) — possibly
  a second modified file (`crates/kaleidoscope-cli/src/main.rs`) if
  DESIGN chooses the new-sibling-function shape; 1 new file
  (`crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`);
  1 line-level modification
  (`crates/kaleidoscope-cli/Cargo.toml` for the new `[[test]]` entry).
- 1 integration point (the function calling the existing
  `cinder::TieringStore::list_by_tier(tenant, tier)` trait method
  three times). Plus the existing integration point with
  `lumen::LogStore::query(tenant, TimeRange::all())` inherited
  unchanged from the predecessor.
- Estimated effort: well under 1 day for the crafter. No OTLP wiring,
  no concurrency test, no shared-handle ownership puzzle, no Cinder
  policy evaluation. Strictly comparable in size to the predecessor
  (the Lumen-side work is reused unchanged and only the Cinder-side
  computation is new).

The feature is right-sized. No splitting required, no thinning
possible.
