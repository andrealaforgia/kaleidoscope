# Slice 01 — `stats` filters Lumen lines by `--since` / `--until` time window; Cinder lines unchanged

## Goal

Add two new optional flags `--since <ISO 8601 UTC>` and
`--until <ISO 8601 UTC>` to the `kaleidoscope-cli stats` subcommand
so that the underlying `lumen.query(tenant, TimeRange)` call inside
`stats_with_tiers()` uses a half-open `[since_ns, until_ns)`
interval derived from the flag values, instead of the hard-coded
`TimeRange::all()` at
`crates/kaleidoscope-cli/src/lib.rs:359-361`. The Cinder
`list_by_tier(tenant, tier)` calls at lines 375-380 are
NOT touched (D-CinderScope in `wave-decisions.md`).

## Stories included

- US-01 (single story; all DoR-validated AC inside `user-stories.md`)

## Acceptance shape

New acceptance test file:
`crates/kaleidoscope-cli/tests/stats_time_range.rs`. Six test
functions covering the four KPIs and the supporting sub-scenarios:

1. `bounded_window_records_and_earliest_and_latest_reflect_window` —
   OK1 + OK2. Pre-ingest 5 records with `observed_time_unix_nano`
   values `{100, 200, 300, 400, 500}` for tenant `acme`. Invoke the
   library `stats_with_tiers()` with `TimeRange::new(200, 400)`.
   Assert returned count is `2`, stdout begins with three Lumen lines
   in order: `records=2`, `earliest=1970-01-01T00:00:00.000000200Z`,
   `latest=1970-01-01T00:00:00.000000300Z`. The record at exactly
   `400` is EXCLUDED (open upper bound), the record at exactly `200`
   is INCLUDED (closed lower bound) and is the earliest in window.
2. `cinder_lines_are_byte_identical_across_different_time_ranges` —
   OK3 / D-CinderScope. Pre-ingest records spanning multiple days
   for tenant `acme`. Seed Cinder with non-zero placements in all
   three tiers (e.g. Hot=5, Warm=12, Cold=47). Invoke
   `stats_with_tiers` TWICE with two different bounded `TimeRange`
   values. Assert the Cinder lines (`hot=5`, `warm=12`, `cold=47`)
   are byte-identical in BOTH captured stdouts while the Lumen lines
   differ between the two.
3. `no_flag_default_is_byte_equivalent_to_today` — OK4. Pre-ingest
   N records and seed Cinder with the canonical post-ingest shape
   (Hot=1, Warm=0, Cold=0 — one Cinder Hot per Lumen batch).
   Invoke `stats_with_tiers` with `TimeRange::all()` and assert
   stdout bytes equal what the pre-feature `stats_with_tiers`
   produces for the same inputs. Reinforces the locked-test
   non-regression invariant.
4. `since_only_uses_u64_max_upper_bound` — OK1 half-bounded
   sub-scenario. Pre-ingest 4 records with `observed_time_unix_nano`
   values `{100, 200, 300, 400}`. Invoke `stats_with_tiers` with
   `TimeRange::new(250, u64::MAX)`. Assert `records=2` and
   `earliest=…300Z`, `latest=…400Z`.
5. `until_only_uses_zero_lower_bound` — OK1 half-bounded
   sub-scenario. Same pre-ingest as #4. Invoke with
   `TimeRange::new(0, 250)`. Assert `records=2` and
   `earliest=…100Z`, `latest=…200Z`.
6. `empty_window_emits_only_records_zero_then_cinder_lines` —
   D-EmptyWindow. Pre-ingest records with `observed_time_unix_nano`
   values entirely outside the chosen window. Seed Cinder with
   non-zero placements. Invoke `stats_with_tiers` with a range
   containing zero matching records. Assert stdout begins with
   exactly `records=0\n` (no `earliest=`, no `latest=`) followed
   by the unchanged Cinder snapshot lines.

The fail-fast `--since` / `--until` invalid-input cases (D-NoNewError)
are NOT re-asserted in this file — they are already covered for the
shared `parse_time_range(args)` helper by
`crates/kaleidoscope-cli/src/main.rs` inline tests
`parse_time_range_with_bad_since_names_flag_and_value_in_error` and
`parse_time_range_with_bad_until_names_flag_and_value_in_error` (lines
510-553) and by the locked `read_time_range.rs` subprocess tests #5
and #6. The same helper is reused unchanged on the `stats` side, so
re-asserting would be redundant test surface.

## Files touched

| File | Change |
|------|--------|
| `crates/kaleidoscope-cli/src/lib.rs` | `stats_with_tiers()` (lines 349-383) gains a way for its caller to drive a `TimeRange` other than `TimeRange::all()` (exact signature shape is DESIGN's choice; recommended: a new explicit `range: TimeRange` parameter, fourth in argument order, mirroring `read()`'s shape from `cli-read-time-range-v0`). No-flag default at the `run_stats_with` call site constructs `TimeRange::all()` so existing callers are byte-equivalent. The Cinder `list_by_tier` loop at lines 375-380 is NOT modified — Cinder lines remain state-snapshot per D-CinderScope. The legacy `stats()` function at lines 312-331 is NOT modified — it remains the byte-level OK4 oracle for the original `cli-stats-subcommand-v0` feature. |
| `crates/kaleidoscope-cli/src/main.rs` | `run_stats_with` (lines 226-235) gains a `let range = parse_time_range(args)?;` line and threads the parsed `range` into the `stats_with_tiers()` call. The `parse_time_range(args)` / `parse_flag_iso(args, flag)` helpers (lines 188-214) are REUSED unchanged — they scan from `args.iter().skip(2)`, which is past the bin name and the subcommand name, so they work identically for `read` and `stats`. `print_usage` (lines 81-119) is updated to document `--since` / `--until` on the `stats` subcommand block, including the D-CinderScope note that the time range applies to the Lumen lines only and the D-EmptyWindow note for the empty-window case. |
| `crates/kaleidoscope-cli/tests/stats_time_range.rs` | NEW. Six test functions per the shape above. Harness helpers (`tenant`, `record`, `temp_root`, `cleanup`, `ndjson`, `cinder_base`, `lumen_base`, `seed_cinder`, `cinder_count`) duplicated inline at v0 mirroring `stats_cinder_tier_distribution.rs`. |
| `crates/kaleidoscope-cli/Cargo.toml` | New `[[test]]` entry: `name = "stats_time_range"`, `path = "tests/stats_time_range.rs"`. |
| `crates/kaleidoscope-cli/tests/stats_subcommand.rs` | LOCKED — assertion-edit forbidden. Under DESIGN's likely 4-arg signature extension of `stats_with_tiers`, the call sites pass `TimeRange::all()` explicitly. Mechanical signature-match update only — no assertion text is changed. Same precedent as `observe_otlp_read_flag.rs` adopted in `cli-read-time-range-v0`. NOTE: `stats_subcommand.rs` exercises the legacy `stats()` function (not `stats_with_tiers`), so it may not need the signature update at all — DESIGN to confirm. |
| `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs` | LOCKED — assertion-edit forbidden. Mechanical signature-match update at the `stats_with_tiers()` call sites if DESIGN adopts the 4-arg signature extension. No assertion text changed. |

## Files explicitly NOT touched (locked)

- `crates/kaleidoscope-cli/tests/stats_subcommand.rs` — locked OK4
  oracle for the original `cli-stats-subcommand-v0` feature. Only
  mechanical signature-match update permitted (and likely not
  needed; this file exercises the legacy `stats()` function which
  this feature does not modify).
- `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`
  — locked OK4 oracle for the `cli-stats-cinder-tier-distribution-v0`
  feature. Only mechanical signature-match update permitted.
- `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` — locked OK2
  protection for the original `--observe-otlp` ingest feature.
- `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` — locked
  OK2 protection for `cli-read-observe-otlp-v0`.
- `crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs` —
  locked.
- `crates/kaleidoscope-cli/tests/read_time_range.rs` — locked OK2
  protection for the predecessor `cli-read-time-range-v0` feature.
- `crates/lumen/src/record.rs` — `TimeRange` API is correct as-is
  (`[start, end)` half-open; `TimeRange::all()` is `[0, u64::MAX)`).
  No change needed.
- The legacy `kaleidoscope_cli::stats()` function at
  `crates/kaleidoscope-cli/src/lib.rs:312-331` is NOT modified. It
  remains the byte-level OK4 oracle for the original
  `cli-stats-subcommand-v0` feature. Only `stats_with_tiers()` (the
  function the binary's `run_stats_with` dispatches to) is
  extended.
- `crates/kaleidoscope-cli/src/lib.rs:528-647`
  (`parse_iso8601_utc_nanos` and the `IsoParseError` type) are NOT
  modified. The parser is reused unchanged on the `stats` side per
  D-NoNewError.

## DoR cross-reference

- US-01 DoR: PASSED (see `dor-validation.md`).
- Feature-level DoR: PASSED with honestly-recorded artefact gaps
  (journey visual / YAML / Gherkin file / shared-artefact registry
  not produced; rationale in `dor-validation.md` F1-F4).

## DESIGN handoff one-pager

DESIGN must decide:

1. The exact signature shape of the new `TimeRange`-driving control
   on `kaleidoscope_cli::stats_with_tiers`. Strongly recommended: a
   new explicit `range: TimeRange` parameter, fourth in argument
   order, mirroring the `read()` extension precedent from
   `cli-read-time-range-v0`. The acceptance test cares only that the
   caller can drive any `TimeRange::new(s, e)` into the underlying
   `lumen.query` call; the rest is DESIGN's choice. Default at the
   `run_stats_with` call site MUST be `TimeRange::all()` so the
   locked test files continue to pass byte-equivalently (OK4).
2. Whether the locked `stats_subcommand.rs` file needs ANY signature
   update. It exercises the legacy `stats()` function (3-arg) which
   this feature does NOT modify — only `stats_with_tiers()` is
   extended. The likely answer is NO mechanical update needed for
   `stats_subcommand.rs`, only for `stats_cinder_tier_distribution.rs`.

DESIGN MUST NOT:

- Modify `lumen::TimeRange` (the `[start, end)` semantics are
  correct as-is; the CLI surface conforms to them).
- Modify the legacy `kaleidoscope_cli::stats()` function (3-arg) —
  it stays as-is, the byte-level OK4 oracle for the original
  feature.
- Modify the existing `parse_iso8601_utc_nanos` parser or the
  `IsoParseError` type. No new error code, no new variant
  (D-NoNewError).
- Modify the locked test files
  (`stats_subcommand.rs`, `stats_cinder_tier_distribution.rs`,
  `observe_otlp_*.rs`, `read_time_range.rs`) beyond mechanical
  signature-match updates at the `stats_with_tiers()` call sites.
  No assertion edit permitted on any locked file.
- Introduce a new external crate dependency for ISO 8601 parsing
  (the parser is the existing hand-rolled one).
- Change the Cinder-side calls inside `stats_with_tiers` (the
  `cinder.list_by_tier(tenant, tier)` loop at lines 375-380
  remains state-snapshot per D-CinderScope).
- Add a `--observe-otlp` flag to `stats` (out of scope for this
  feature; `stats` does not support `--observe-otlp` today and this
  feature does not change that).
- Add other Lumen query parameters to the CLI surface (severity
  filter, body substring filter — out of scope; only the existing
  `TimeRange` is wired).

Estimated effort for the crafter (DELIVER wave): <= 1 day. Strictly
thinner than `cli-read-time-range-v0`'s Slice 01 because the
parser-side work is already done in the prior wave; the only new
surface this feature introduces is the D-CinderScope decision and
its test-level guardrail (OK3) plus the D-EmptyWindow guardrail
inherited from the predecessor's empty-tenant contract.
