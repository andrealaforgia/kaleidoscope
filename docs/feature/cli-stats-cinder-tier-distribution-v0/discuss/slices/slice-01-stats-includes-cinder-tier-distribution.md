# Slice 01 — `stats` includes Cinder tier distribution

**Story**: US-01
**Outcome KPIs**: OK1-CLI-stats-tier-counts (principal),
OK2-CLI-stats-tier-tenant-isolation,
OK3-CLI-stats-no-records-no-timestamps-still,
OK4-CLI-stats-backwards-compatible-populated-then-cinder-zero
**Tag**: operator-visible (not `@infrastructure` — the CLI surface is
the real user-invocable entry point)
**Estimated effort**: well under 1 day

## Goal

Extend the existing `stats` subcommand on `kaleidoscope-cli` (shipped
in commit `75f15a6` from `cli-stats-subcommand-v0`) so that the same
invocation `kaleidoscope-cli stats <tenant_id> <data_dir>` ALSO emits
up to three additional plain-text key=value stdout lines reporting
the tenant's current Cinder tier distribution: `hot=H`, `warm=W`,
`cold=C`, where each `<tier>=<count>` line is emitted only when the
count is non-zero (Option B per `wave-decisions.md` D-EmptyRender).
The result: one CLI invocation gives the operator the records +
time-window + tier-distribution answer, with byte-equivalent
backwards-compatibility for any tenant with zero Cinder placements.

## What ships in this slice

| Artifact | Change |
|----------|--------|
| `crates/kaleidoscope-cli/src/lib.rs` | Either: (a) extend the existing `stats(tenant, data_dir, writer)` to take a third optional capability (e.g. `Option<&dyn TieringStore>`); or (b) add a parallel `stats_with_cinder(tenant, data_dir, writer)` function with the same signature shape. DESIGN locks the choice per `wave-decisions.md` D9. The new behaviour: open `FileBackedTieringStore::open(cinder_base(data_dir), recorder)` with a quiescent `cinder::NoopRecorder` (same pattern as `ingest`'s no-flag arm at lines 170-174), call `cinder.list_by_tier(tenant, Tier::Hot).len()`, `cinder.list_by_tier(tenant, Tier::Warm).len()`, `cinder.list_by_tier(tenant, Tier::Cold).len()`, and write one `writeln!(writer, "<key>={count}")` for each non-zero count in the order `hot` → `warm` → `cold`. |
| `crates/kaleidoscope-cli/src/main.rs` (possibly) | If DESIGN chooses the new-sibling-function shape, the `Some("stats")` dispatch arm calls the new function instead of (or in addition to) the existing `stats()`. If DESIGN chooses to extend the existing signature, the dispatch arm forwards the new capability. `print_usage` (lines 71-97) MAY be updated to mention the additional tier-distribution lines (DESIGN's call). |
| `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs` | NEW file. Mirrors the harness pattern in `crates/kaleidoscope-cli/tests/stats_subcommand.rs` (the predecessor's locked file). Hosts the five acceptance tests below (OK1 populated-multi-tier + hot-only, OK2 tenant-isolation, OK3 orphan-cinder, OK4 backwards-compat). |
| `crates/kaleidoscope-cli/Cargo.toml` | New `[[test]] name = "stats_cinder_tier_distribution", path = "tests/stats_cinder_tier_distribution.rs"`. The `cinder` dependency is already present. |
| `crates/kaleidoscope-cli/tests/stats_subcommand.rs` | NOT MODIFIED. `wave-decisions.md` D10 makes this a hard contract. The predecessor's locked OK1/OK2/OK3 byte-level oracle MUST continue to pass green unmodified — this is the supplementary oracle for OK4. |

## IN scope

- Three additional stdout lines on the existing `stats` subcommand:
  `hot=H` / `warm=W` / `cold=C` (lower-case keys), each emitted only
  when the count is non-zero.
- Three keys exactly: `hot`, `warm`, `cold`. No other keys, no header,
  no JSON, no CSV, no Unicode box drawing, no colour.
- Key ordering: lines appear in the order `hot` → `warm` → `cold`
  (forward-lifecycle order per `crates/cinder/src/tier.rs:38-44`).
- Lines appear AFTER the existing Lumen-side lines
  (`records=` / `earliest=` / `latest=`).
- The empty-render contract (Option B): a `<tier>=` line is emitted
  only when its count is non-zero. Zero-count tiers produce no line
  at all.
- Stdout (NOT stderr — inherited from the predecessor's stream
  contract).
- Exit code 0 in all cases (populated, empty-Lumen, empty-Cinder,
  empty-both).

## OUT of scope

- Cinder placement triggering. The function makes no `place()` call
  (`wave-decisions.md` D2).
- Policy evaluation. The function makes no `evaluate_at(now, policy)`
  call (`wave-decisions.md` D3).
- Per-item dump. The function reports counts (`H`, `W`, `C`), not
  item ids. No `--items` / `--list-items` flag
  (`wave-decisions.md` D4).
- JSON / CSV output. No `--json` / `--csv` / `--format=...`
  (`wave-decisions.md` D5).
- Cinder-only mode. The Lumen lines are emitted unconditionally
  (when their values are non-zero per the predecessor's contract);
  there is no `--cinder-only` / `--no-lumen` flag
  (`wave-decisions.md` D6).
- `--observe-otlp` wiring on `stats`. The flag is NOT accepted by
  this subcommand in v0 (inherited from
  `cli-stats-subcommand-v0/wave-decisions.md` D3, restated here).
- Modification of the existing
  `crates/kaleidoscope-cli/tests/stats_subcommand.rs` test file
  (`wave-decisions.md` D10 — the predecessor's locked OK1/OK2/OK3
  byte-level oracle is preserved unchanged).
- Sorting (`--sort-by-tier-count`), filtering (`--min-count=`,
  `--tier=hot`), multi-tenant aggregates (`stats <data_dir>` without a
  tenant), per-tier age-of-oldest-item histograms (all deferred to
  follow-up features).
- Extracting a shared test-helper module across the six `tests/*.rs`
  files that now use the same harness pattern (rule-of-three
  extraction trigger arrived at the predecessor feature and remains a
  separate follow-up — see `wave-decisions.md` D10).

## Rejected alternatives

- **Option A (strict OK3 compat — never emit Cinder lines for an
  empty-Lumen tenant)**: rejected in `wave-decisions.md`
  D-EmptyRender. Suppressing the Cinder lines for empty-Lumen tenants
  with non-zero Cinder placements would silently hide an operationally
  meaningful condition (orphan tier metadata) — exactly the thing an
  operator who runs `stats` wants to learn about.
- **Option C (always emit all six lines, with explicit `hot=0` /
  `warm=0` / `cold=0` for zero-count tiers)**: rejected in
  `wave-decisions.md` D-EmptyRender. Explicit zero lines would break
  byte-equivalent backwards-compatibility for the most common legacy
  case (a tenant with positive Lumen records and zero Cinder
  placements — every existing operator shell pipeline that
  `wc -l == 3` on the predecessor's output for those tenants would
  silently start failing).
- **In-place modification of the existing `stats()` function without
  a new parameter**: rejected in `wave-decisions.md` D9. The
  predecessor's locked test file
  (`crates/kaleidoscope-cli/tests/stats_subcommand.rs`) uses
  `ingest()` fixtures, and `ingest()` places one Hot Cinder item per
  batch (`crates/kaleidoscope-cli/src/lib.rs:243-244`). An in-place
  modification would change the byte-level stdout of the
  predecessor's test cases and break the locked file. DESIGN must
  choose between the two alternative shapes (extend `stats()`
  signature OR add `stats_with_cinder()`).
- **`-o json` / `--format=json`**: rejected in `wave-decisions.md`
  D5. Same rationale as the predecessor's D4 — v0 ships plain-text
  key=value, JSON output is a reasonable v1 once the v0 shape is
  validated.
- **A new `TieringStore::count_by_tier(tenant, tier) -> usize` trait
  method**: not introduced in v0. The existing
  `list_by_tier(tenant, tier)` returning `Vec<ItemId>` is sufficient
  at v0 tier sizes. If materialising the full per-tier item-id
  vectors proves operationally expensive at the operator's actual
  tier sizes, a follow-up feature can introduce a streaming/aggregate
  trait method without breaking the v0 `stats` subcommand's stdout
  contract.

## Learning hypothesis

Disproves the assumption that the existing `TieringStore` query API
is sufficient for per-tier counts without needing new methods. The
current `TieringStore::list_by_tier(tenant, tier)` returns
`Vec<ItemId>` per `crates/cinder/src/store.rs:101-102`, which is
sufficient to compute a per-tier count via three `.len()` calls. If
the assumption holds, the slice ships with three `list_by_tier()`
calls per `stats()` invocation plus three constant-time count
derivations. If the assumption fails — materialising three full
per-tier item-id vectors is too expensive at the operator's tier
sizes — the failure mode tells DESIGN to propose an aggregate
`TieringStore::count_by_tier(tenant, tier) -> usize` trait method
as a follow-up feature (NOT pre-emptively designed here).

## Acceptance criteria (DISTILL translates each into a `#[test]` fn)

- `stats_populated_lumen_and_multi_tier_cinder_emits_six_lines_in_order`:
  pre-populate Lumen for tenant `acme` with N > 0 records (e.g. via
  the existing `ingest()` library function with a deterministic seed
  set; this will also place Hot Cinder items per batch). Then,
  additionally, mutate the Cinder store to achieve a known final
  distribution — e.g. open `FileBackedTieringStore::open(cinder_base(data_dir),
  recorder)` and call `migrate(...)` on a subset of items to populate
  Warm and Cold tiers. Call the stats function with a captured
  stdout sink. Assert the captured stdout contains exactly 6 non-
  empty lines in order: line 1 equals `records=N`; lines 2/3 begin
  with `earliest=`/`latest=`; line 4 equals `hot=H`; line 5 equals
  `warm=W`; line 6 equals `cold=C`, where `H`, `W`, `C` are the
  seeded per-tier counts. Assert stdout ends with `\n`.
- `stats_populated_lumen_and_hot_only_cinder_emits_four_lines`:
  pre-populate Lumen for tenant `acme` with N > 0 records (via
  `ingest()`, which places only Hot Cinder items — no migration).
  Call the stats function with a captured stdout sink. Assert the
  captured stdout contains exactly 4 non-empty lines: `records=N`,
  `earliest=...`, `latest=...`, `hot=H` (with no `warm=`, no
  `cold=`). Assert stdout ends with `\n`.
- `stats_empty_lumen_with_orphan_cinder_emits_records_zero_plus_nonzero_cinder_lines`:
  open a fresh `data_dir`, do NOT call `ingest()`, but directly open
  `FileBackedTieringStore::open(cinder_base(data_dir), recorder)`
  and call `place()` to populate Cinder for tenant `acme` with `H=2`
  Hot items, `W=0` Warm, `C=1` Cold. Call the stats function with a
  captured stdout sink. Assert the captured stdout contains exactly 3
  non-empty lines in order: `records=0`, `hot=2`, `cold=1`. Assert
  no line begins with `earliest=`. Assert no line begins with
  `latest=`. Assert no line begins with `warm=`. Assert stdout ends
  with `\n`.
- `stats_populated_lumen_with_zero_cinder_is_byte_equivalent_to_predecessor`:
  open a fresh `data_dir`. Populate Lumen for tenant `legacy_acme`
  via a direct `lumen::FileBackedLogStore::open(...).ingest(...)`
  call (NOT via `kaleidoscope_cli::ingest()` which would also place
  Hot Cinder items). Open the Cinder store separately via
  `FileBackedTieringStore::open` and make NO `place()` call. Call
  the stats function with a captured stdout sink. Assert the
  captured stdout contains exactly 3 non-empty lines: `records=N`,
  `earliest=...`, `latest=...`. Assert no line begins with `hot=`,
  no line with `warm=`, no line with `cold=`. Assert stdout ends
  with `\n`. (The supplementary oracle for OK4 is the unmodified
  predecessor test file, which exercises a different test path
  using `ingest()` — both oracles must remain green.)
- `stats_for_acme_does_not_count_globex_cinder_placements_in_same_data_dir`:
  pre-populate Cinder for tenant `acme` with 5 Hot placements (via
  direct `FileBackedTieringStore::open(...).place(...)` calls) and
  separately pre-populate Cinder for tenant `globex` with 9 Hot
  placements in the same `data_dir`. Populate Lumen for `acme` with
  N > 0 records (to bypass the empty-Lumen branch). Call the stats
  function with a captured stdout sink. Assert the `hot=` line shows
  the count 5 (NOT 14, NOT 9). Assert the count 5 equals
  `list_by_tier(acme, Tier::Hot).len()` against the Cinder store at
  the same `data_dir`.

## Dependencies

- `cinder::TieringStore::list_by_tier(tenant, tier)` already exists
  and returns `Vec<ItemId>`
  (`crates/cinder/src/store.rs:101-102`).
- `cinder::FileBackedTieringStore` already implements `TieringStore`
  and is already constructed by `ingest()`
  (`crates/kaleidoscope-cli/src/lib.rs:179-180`).
- `cinder::NoopRecorder` already used by `ingest`'s no-flag arm
  (`crates/kaleidoscope-cli/src/lib.rs:170-174`); already a
  `kaleidoscope-cli` dependency.
- `cinder::Tier` enum already imported at
  `crates/kaleidoscope-cli/src/lib.rs:58`.
- `cinder_base(data_dir)` helper already at
  `crates/kaleidoscope-cli/src/lib.rs:122-124`; reused unchanged.
- The predecessor's `stats()` function at
  `crates/kaleidoscope-cli/src/lib.rs:313-332` provides the
  Lumen-side computation reused unchanged (or wrapped — DESIGN's
  call).
- `aegis::TenantId` already a dependency.
- No new external dependencies.

## Reference class

This is the sixth small feature in a row in the `kaleidoscope-cli`
cluster (after `cinder-to-pulse-bridge-v0`,
`cinder-to-otlp-json-bridge-v0`, `cli-cinder-otlp-wiring-v0`,
`cli-read-observe-otlp-v0`, and `cli-stats-subcommand-v0`).
Comparable in size to the predecessor (no OTLP wiring of any kind,
no concurrency probe, no shared-handle ownership puzzle, no Cinder
policy evaluation, no per-item dump). Structurally smaller than the
predecessor because the Lumen-side computation is reused unchanged
and only the Cinder-side `list_by_tier`+conditional `writeln!` work
is new.

## Effort estimate

Well under 1 day for the crafter. Breakdown: 30 minutes for the
function modification (open Cinder store, three `list_by_tier()`
calls, three conditional `writeln!`s); 30 minutes for any
`main.rs` / `print_usage` follow-on (depending on DESIGN's
function-shape choice); 1-2 hours for the new acceptance test
(five scenarios — populated-multi-tier, hot-only, orphan-cinder,
backwards-compat, tenant-isolation); 30 minutes for the
`Cargo.toml` `[[test]]` entry and a local green run.

## Definition of Done for this slice

- All AC above green under `cargo test --package kaleidoscope-cli`.
- `cargo clippy --workspace --all-targets` clean (no new warnings).
- The existing
  `crates/kaleidoscope-cli/tests/stats_subcommand.rs` test file
  continues to pass green UNMODIFIED (`wave-decisions.md` D10 —
  the predecessor's OK1/OK2/OK3 byte-level oracle preserved).
- The dogfood demo runs: `cargo run --bin kaleidoscope-cli --
  ingest acme /tmp/kdata < some_records.ndjson`, then (after
  optionally exercising Cinder migration), `cargo run --bin
  kaleidoscope-cli -- stats acme /tmp/kdata` shows up to six
  lines including the populated `hot=` line; `cargo run --bin
  kaleidoscope-cli -- stats acme /tmp/kdata | grep -E
  '^(hot|warm|cold)=' | wc -l` returns the count of non-empty
  tiers.
- The five prior `tests/observe_otlp_*` test files continue to
  pass green (non-regression on the five reference features).
