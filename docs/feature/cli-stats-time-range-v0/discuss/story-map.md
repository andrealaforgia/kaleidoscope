# Story Map: `cli-stats-time-range-v0`

## User: Priya the platform operator

## Goal

When Priya runs
`kaleidoscope-cli stats acme /tmp/data --since 2026-05-18T00:00:00Z --until 2026-05-19T00:00:00Z`,
stdout receives a three-line Lumen summary (`records=N`,
`earliest=<ISO 8601 UTC>`, `latest=<ISO 8601 UTC>`) reflecting ONLY
the records whose `observed_time_unix_nano` lies in the half-open
interval `[1_779_062_400_000_000_000, 1_779_148_800_000_000_000)`
— yesterday's slice for tenant `acme`, computed by the storage
layer's `TimeRange` query at `crates/lumen/src/record.rs:97-120`, not
by client-side `jq` aggregation of a `read` dump. The Cinder
`hot=` / `warm=` / `cold=` lines that follow remain state-snapshot
(CURRENT per-tenant placements, NOT time-bound — D-CinderScope in
`wave-decisions.md`). Half-bounded forms (`--since X` alone or
`--until Y` alone) work. Empty-window queries emit exactly
`records=0\n` with no `earliest=` / `latest=` lines (D-EmptyWindow).
Omitting both flags is byte-equivalent to today (the locked
`crates/kaleidoscope-cli/tests/stats_subcommand.rs` and
`crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`
tests still pass with no assertion edits). A typo on either flag
exits non-zero with the offending flag named in stderr — the same
fail-fast path the `read --since / --until` feature uses
(D-NoNewError).

## Backbone

The journey has exactly one activity: the operator counts and
brackets a tenant's records by an ISO 8601 time window via two new
CLI flags on the existing `stats` subcommand. The activity is a thin
end-to-end slice: a single
`kaleidoscope-cli stats <tenant> <data_dir> [--since X] [--until Y]`
invocation produces a bounded Lumen summary on stdout followed by
the unchanged Cinder snapshot. The underlying `lumen::TimeRange`
data type, the storage-layer `LogStore::query(tenant, TimeRange)`
entry point, the binary's positional argument parsing, the
existing `parse_time_range(args)` / `parse_flag_iso(args, flag)`
helpers at `crates/kaleidoscope-cli/src/main.rs:188-214`, the
existing `parse_iso8601_utc_nanos` parser at
`crates/kaleidoscope-cli/src/lib.rs:528-647`, the Cinder placement
model, and the `stats_with_tiers()` function shape all already
exist. This feature is the wire that lets the operator drive a
non-`all()` `TimeRange` from the CLI surface into the Lumen-side
query, while explicitly leaving the Cinder-side calls untouched
(D-CinderScope).

| Activity 1: operator counts and brackets a tenant's records by ISO 8601 time window on the `stats` subcommand |
|---|
| CLI's `stats` path constructs `TimeRange::new(since_ns, until_ns)` from the operator-supplied `--since` and `--until` flag values (instead of always calling `TimeRange::all()`). Stdout begins with the three Lumen lines reflecting the half-open window count and earliest/latest, followed by the unchanged Cinder snapshot lines. Empty window → `records=0` with no `earliest=` / `latest=`. No-flag default is `TimeRange::all()` (byte-equivalent to today). Invalid ISO 8601 on either flag fails fast with the offending flag name in stderr via the existing read-feature error path. |

## Walking Skeleton

Per `wave-decisions.md` D2 (`walking_skeleton = no`), the
walking-skeleton concept is N/A: the CLI already exists, the
`stats` subcommand already exists, the `lumen::TimeRange` data type
already exists with the correct `[start, end)` semantics
(`crates/lumen/src/record.rs:97-120`), the
`LogStore::query(tenant, TimeRange)` entry point already exists,
the binary's argument parser already knows how to parse `--since`
and `--until` (via the shared helpers introduced for `read` in
`cli-read-time-range-v0` at
`crates/kaleidoscope-cli/src/main.rs:188-214`), the project
already hand-rolls an ISO 8601 UTC parser
(`crates/kaleidoscope-cli/src/lib.rs:528-647`), and the
`stats_with_tiers()` function (the dispatcher target) already
exists at `crates/kaleidoscope-cli/src/lib.rs:349-383`. This
feature extends one subcommand's library function with a
`TimeRange`-driving control and threads the existing
`parse_time_range(args)` helper through the `run_stats` dispatcher.

Equivalent statement: **the smallest valuable change is to expose
`TimeRange` to `kaleidoscope_cli::stats_with_tiers`'s caller (with
`TimeRange::all()` as the default), thread the existing
`parse_time_range(args)` helper through the binary's `run_stats`
dispatcher mirroring the existing `run_read` posture, and explicitly
NOT touch the Cinder-side calls inside `stats_with_tiers` (the
`cinder.list_by_tier(tenant, tier)` loop at lines 375-380 remains
state-snapshot per D-CinderScope).** Slice 01 ships exactly that.

## Release Slices

### Slice 01 — `stats` filters Lumen lines by `--since` / `--until` time window; Cinder lines unchanged

- **Outcome**: An operator running `kaleidoscope-cli stats acme
  /tmp/data --since 2026-05-18T00:00:00Z --until 2026-05-19T00:00:00Z`
  sees the three Lumen lines reflecting ONLY yesterday's half-open
  window (OK1 + OK2), followed by the unchanged Cinder snapshot
  lines (OK3 / D-CinderScope). Half-bounded forms work
  (sub-scenario of OK1). Empty-window queries emit exactly
  `records=0\n` (D-EmptyWindow). Omitting both flags is
  byte-equivalent to today (OK4). Typos fail fast via the existing
  read-feature error path (D-NoNewError).
- **Stories**: `US-01` (single slice; all DoR-validated AC inside).
- **Learning hypothesis**: disproves the assumption that the
  `stats` extension is structurally harder than the `read`
  extension. The wiring is, in fact, mechanically the same on the
  input side (one new `range: TimeRange` parameter on the library
  function, same `parse_time_range(args)` helper on the binary
  side) and STRICTLY THINNER on the parser side (the parser is the
  existing `parse_iso8601_utc_nanos` shipped in the prior wave; no
  new parser code). The only additional surface this feature
  introduces is the D-CinderScope decision and its test-level
  guardrail (OK3). If the assumption holds, the slice ships in
  comparable time to the `read` extension. If it fails, the
  failure mode tells DESIGN whether the D-CinderScope decision
  needs a different shape (e.g. an additional flag like
  `--cinder-at <ISO>` that explicitly takes a Cinder snapshot time
  — out of scope for this feature, documented as a future
  candidate in `wave-decisions.md`).
- **Production-data-equivalent AC**: an end-to-end test invokes
  `kaleidoscope_cli::stats_with_tiers` (the actual library
  function the binary dispatches to) with a `TimeRange::new(s, e)`
  other than `TimeRange::all()` against a Lumen store pre-populated
  with witness records at known `observed_time_unix_nano` boundary
  values; asserts the stdout `records=` line, the `earliest=` /
  `latest=` lines, and the Cinder lines all behave per the OK1 /
  OK2 / OK3 KPIs. This is the same data path the operator's
  `kaleidoscope-cli stats acme /tmp/data --since X --until Y`
  invocation exercises.
- **Dogfood moment**: After the slice ships, Andrea opens a
  terminal, runs:

  ```text
  cargo run --bin kaleidoscope-cli -- ingest acme /tmp/kdata < some_records.ndjson
  cargo run --bin kaleidoscope-cli -- stats acme /tmp/kdata
  cargo run --bin kaleidoscope-cli -- stats acme /tmp/kdata \
    --since 2026-05-18T00:00:00Z --until 2026-05-19T00:00:00Z
  ```

  and observes that the second invocation's `records=` line is
  less than or equal to the first invocation's (the bounded window
  is a subset of the full tenant), while both invocations' Cinder
  lines (`hot=…` / `warm=…` / `cold=…`) are byte-identical
  (D-CinderScope). Then
  `cargo run --bin kaleidoscope-cli -- stats acme /tmp/kdata --since yesterday`
  exits non-zero with
  `kaleidoscope-cli: --since "yesterday": invalid ISO 8601 …`
  on stderr (same error format the `read` feature uses). The three
  demonstrations together are the dogfood gate for the slice.
- **Effort**: comparable to or less than `cli-read-time-range-v0`'s
  Slice 01. The change inside `kaleidoscope_cli::stats_with_tiers`
  is one new parameter defaulted to `TimeRange::all()`; the change
  inside `crates/kaleidoscope-cli/src/main.rs` is one new
  `parse_time_range(args)?` call inside `run_stats_with` plus the
  threading of the parsed `range` into the `stats_with_tiers()`
  call; the new acceptance test mirrors the
  `stats_cinder_tier_distribution.rs` harness. No new parser code,
  no new flag-parse helper, no Cinder code change.

## Priority Rationale

There is one slice and it is the only slice. The reference-class
sizing (this is the sixth consecutive small feature in the
`kaleidoscope-cli` / `self-observe` cluster, after
`cinder-to-pulse-bridge-v0`, `cinder-to-otlp-json-bridge-v0`,
`cli-cinder-otlp-wiring-v0`, `cli-read-observe-otlp-v0`, and
`cli-read-time-range-v0`, and STRICTLY THINNER than
`cli-read-time-range-v0` because the parser-side work is already
done) means there is no benefit from further splitting:

- Slice 01 carries the wiring change (one new `range: TimeRange`
  parameter on `stats_with_tiers`, one new `parse_time_range(args)?`
  call in `run_stats_with`, one new `print_usage` block update),
  the OK1 bounded-window record-count test, the OK2 bounded-window
  earliest/latest test, the OK3 Cinder-invariance test, the OK4
  no-flag-byte-equivalent test, and the half-bounded / empty-window
  / fail-fast sub-scenarios all together. Splitting any one of the
  four KPIs into a separate slice would force a second PR for
  trivially the same wiring — net negative for the reviewer.
- The principal KPI (OK1) is the bounded-window record count; OK2
  is the windowed earliest/latest derivation that operates on the
  same query result; OK3 is the D-CinderScope guardrail that pins
  the most reviewer-confusing design decision; OK4 is the no-flag
  non-regression guardrail. Shipping any without the others is
  meaningless: OK1 alone gives the count but not the time bracket
  (operator still needs `read` to find earliest/latest); OK2 alone
  is structurally impossible without OK1 (the earliest/latest lines
  use the same `lumen.query` result); OK3 alone is "we did nothing
  useful but at least we pinned a decision"; OK4 alone is "we did
  nothing and we didn't break anything".

If schedule pressure ever forces a partial ship, the slice is
already as thin as it can be: one new parameter, one new function
call, one new `print_usage` block update. The parser is reused
unchanged from the prior wave; no new error code; no Cinder code
touched.

## Cross-feature alignment

This story-map intentionally mirrors the operator-facing posture of
`cli-read-time-range-v0/discuss/story-map.md` and inherits its
persona (Priya), CLI surface (`kaleidoscope-cli <subcommand> <tenant>
<data_dir> [--since X] [--until Y]`), and ISO 8601 input contract.
The principal contractual differences are:

- the OUTPUT side is plain-text `key=value\n` lines (not NDJSON);
- there are additional output lines (the Cinder `hot=` / `warm=` /
  `cold=` snapshot) that are EXPLICITLY excluded from the time
  filter (D-CinderScope in `wave-decisions.md`);
- there is no `read`-equivalent empty-window contract on the
  Lumen-side output today; this feature carries the predecessor's
  empty-tenant contract (`crates/kaleidoscope-cli/src/lib.rs:362-369`)
  over to the empty-window case (D-EmptyWindow);
- the parser is REUSED unchanged from the prior wave; no new error
  code is introduced (D-NoNewError).

## Scope Assessment: PASS — 1 story, 1 bounded context, estimated <= 1 day

- 1 story (US-01).
- 1 bounded context (`kaleidoscope-cli` crate; the wiring change is
  at one call site in `lib.rs`, one helper-call addition in
  `main.rs`'s `run_stats_with`, and the new acceptance test lives
  in one new file).
- 2 modified files (`crates/kaleidoscope-cli/src/lib.rs`,
  `crates/kaleidoscope-cli/src/main.rs`), 1 new file
  (`crates/kaleidoscope-cli/tests/stats_time_range.rs`), 1
  line-level modification (`crates/kaleidoscope-cli/Cargo.toml` for
  the new `[[test]]` entry), and 1 mechanical signature-match
  update across the locked stats test files
  (`tests/stats_subcommand.rs` and
  `tests/stats_cinder_tier_distribution.rs`) at the
  `stats_with_tiers()` call sites if DESIGN adopts the 4-arg
  signature extension. No assertion in any locked file is edited.
- 1 integration point (the `lumen.query(tenant, TimeRange::new(s,
  e))` call site at `crates/kaleidoscope-cli/src/lib.rs:359-361`).
  No second integration point: the Cinder side is explicitly NOT
  touched (D-CinderScope).
- Estimated effort: <= 1 day for the crafter. STRICTLY THINNER than
  `cli-read-time-range-v0` because the parser-side work is already
  done; the only new surface this feature introduces is the
  D-CinderScope decision and its test-level guardrail (OK3).

The feature is right-sized. No splitting required, no thinning
possible (the slice is already at the minimum: one new parameter,
one new function call, one `print_usage` update, one new test file).
