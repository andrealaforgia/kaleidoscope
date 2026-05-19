# Outcome KPIs — `cli-stats-time-range-v0`

## Feature

`cli-stats-time-range-v0` — extend the `kaleidoscope-cli stats`
subcommand with the same two optional flags `--since <ISO 8601 UTC>`
and `--until <ISO 8601 UTC>` shipped on `read` in
`cli-read-time-range-v0`. When set, the records / earliest / latest
Lumen lines reflect ONLY records within the half-open
`[since, until)` window. When absent, output is byte-equivalent to
today. The Cinder tier-distribution lines (`hot=` / `warm=` /
`cold=`) are state-snapshot, NOT time-bound (D-CinderScope in
`wave-decisions.md`) — they keep emitting CURRENT Cinder placements
regardless of the time-range flags. Today an operator who wants
yesterday's record count for tenant `acme` must dump the records via
`read --since X --until Y | jq -s 'length'` and aggregate
client-side. After this feature, the same operator runs:

```text
kaleidoscope-cli stats acme /tmp/data \
  --since 2026-05-18T00:00:00Z \
  --until 2026-05-19T00:00:00Z
```

and gets the count, earliest, and latest of that window directly in
two text lines on stdout.

## Objective

A single `kaleidoscope-cli stats acme /tmp/data --since X --until Y`
invocation prints (a) a `records=N` line where `N` is the count of
records whose `observed_time_unix_nano` lies in the half-open
interval `[parse(X), parse(Y))`, where `parse` is the existing
`kaleidoscope_cli::parse_iso8601_utc_nanos`
(`crates/kaleidoscope-cli/src/lib.rs:528-647`) shipped in
`cli-read-time-range-v0`, (b) an `earliest=<ISO>` and `latest=<ISO>`
line rendering the smallest and largest `observed_time_unix_nano`
WITHIN that window (omitted when `N == 0` per D-EmptyWindow), and
(c) the unchanged Cinder snapshot lines (`hot=H` / `warm=W` /
`cold=C`, each only when non-zero) reflecting the CURRENT
`TieringStore::list_by_tier(tenant, tier)` counts — independent of
the time-range flags (D-CinderScope). Half-bounded queries
(`--since` only or `--until` only) supply `u64::MAX` or `0`
respectively for the unbounded side. When neither flag is supplied,
behaviour is byte-equivalent to today (the locked
`crates/kaleidoscope-cli/tests/stats_subcommand.rs` and
`crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`
tests continue to pass). When either flag's value fails to parse,
the CLI exits non-zero with the offending flag named in stderr —
the same error path the `read` feature uses (D-NoNewError).

## Note on KPI granularity

This feature has four KPIs because the input/output behaviour
naturally factors into four orthogonal contracts: the bounded
positive case on the count (OK1, principal), the bounded positive
case on the earliest/latest derivation (OK2), the
Cinder-lines-unchanged guardrail that pins D-CinderScope (OK3), and
the no-flag non-regression guardrail (OK4). The half-bounded and
empty-window and fail-fast cases are exercised as additional test
shapes alongside OK1/OK2 — they are sub-scenarios of the same
acceptance test file rather than separate KPIs because they do not
expose a separate measurement surface.

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| OK1-bounded-window-records | Priya the platform operator, observed at the byte level on stdout from `kaleidoscope-cli stats` | Sees a `records=N` line where `N` exactly equals the count of records whose `observed_time_unix_nano` lies in the half-open interval `[since_ns, until_ns)` she specified via `--since` and `--until`, where `since_ns` / `until_ns` are the parser's output for the supplied ISO 8601 UTC values | 100% equality: `N == lumen.query(tenant, TimeRange::new(since_ns, until_ns)).len()`; the closed-lower boundary (record at exactly `since_ns` is INCLUDED) and the open-upper boundary (record at exactly `until_ns` is EXCLUDED) are exercised by witness records at those exact values | 0% — today `stats_with_tiers()` always calls `lumen.query(tenant, TimeRange::all())` (`crates/kaleidoscope-cli/src/lib.rs:359-361`); the only workaround is to dump records via `read --since X --until Y` and count them client-side via `jq -s 'length'` | New acceptance test `crates/kaleidoscope-cli/tests/stats_time_range.rs` — pre-ingest 5 records with `observed_time_unix_nano` values `{100, 200, 300, 400, 500}` for tenant `acme`; invoke `stats_with_tiers` with `TimeRange::new(200, 400)`; assert the `records=` line on stdout is `records=2` (the records at 200 and 300; 400 is excluded as the open upper bound, 100 and 500 are outside the window) | Leading (principal KPI — the operator-visible count the feature exists to deliver) |
| OK2-bounded-window-earliest-latest | Priya the platform operator, observed at the byte level on stdout | Sees an `earliest=<ISO 8601 UTC>` line and a `latest=<ISO 8601 UTC>` line whose values are `format_iso8601_utc_nanos` renderings of the smallest and largest `observed_time_unix_nano` WITHIN the half-open window (omitted entirely when the window contains zero records, per D-EmptyWindow) | 100% equality: when `N > 0`, the `earliest=` value equals `format_iso8601_utc_nanos(min_in_window)` where `min_in_window` is the smallest `observed_time_unix_nano` from `lumen.query(tenant, TimeRange::new(since_ns, until_ns))`; symmetrically the `latest=` value equals `format_iso8601_utc_nanos(max_in_window)`. When `N == 0`, neither line is emitted | 0% — today the `earliest=` / `latest=` lines reflect the full-tenant min/max, not the windowed min/max | Same test file — same pre-ingest as OK1; assert `earliest=1970-01-01T00:00:00.000000200Z` and `latest=1970-01-01T00:00:00.000000300Z` on stdout (the windowed min and max, not the global 100/500 min/max). Companion empty-window case: invoke with a range containing no records; assert stdout begins with exactly `records=0\n` and contains NO `earliest=` line and NO `latest=` line (D-EmptyWindow) | Leading (operator-visible time bracket — answers "what's the duration of the active window for this tenant?" directly) |
| OK3-cinder-lines-unchanged | Priya the platform operator, observed at the byte level on stdout | Sees `hot=H` / `warm=W` / `cold=C` lines (each selectively emitted when its count is non-zero, per Option B) that reflect the CURRENT `TieringStore::list_by_tier(tenant, tier)` counts — IDENTICAL byte-for-byte regardless of which `TimeRange` she supplies via `--since` / `--until`. The time range applies ONLY to the Lumen lines (D-CinderScope) | 100% byte-identity: for any two invocations of `stats_with_tiers` against the same `(tenant, data_dir)` pair with two DIFFERENT `TimeRange` values, the substring of stdout matching `/^(hot|warm|cold)=\d+$/` lines is byte-identical between the two invocations. The Cinder side does NOT change because of `--since` / `--until` | n/a — this guardrail cannot exist today since the time-range flags do not exist on `stats`; the KPI exists to pin D-CinderScope at acceptance time so a future reviewer cannot accidentally make the Cinder lines time-bound | Same test file — pre-ingest records spanning multiple days for tenant `acme`; seed Cinder with non-zero placements in all three tiers; invoke `stats_with_tiers` TWICE with two different bounded `TimeRange` values; assert the Cinder lines are byte-identical in both stdout captures while the Lumen lines differ between the two | Guardrail (pins D-CinderScope — the decision most likely to confuse a future reviewer who might assume the time range applies symmetrically across the whole output) |
| OK4-no-flag-byte-equivalence | Priya the platform operator, observed at the byte level on stdout and return value from `kaleidoscope_cli::stats_with_tiers` | Sees behaviour byte-equivalent to today when neither `--since` nor `--until` is supplied: same stdout (`records=N` / `earliest=` / `latest=` / `hot=` / `warm=` / `cold=` per the predecessor's contract), same return value (`count: usize` equal to the matched record count), and no behavioural change on any other observable surface | 100% byte equivalence vs the pre-feature `stats_with_tiers()` output for the same inputs; 100% equality of returned record count; 100% of the assertions in `crates/kaleidoscope-cli/tests/stats_subcommand.rs` and `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs` continue to pass green with no edits to any assertion (only a mechanical signature-match update at the `stats_with_tiers()` call sites to pass `TimeRange::all()` explicitly under DESIGN's likely 4-arg signature extension — same precedent as `observe_otlp_read_flag.rs` adopted in `cli-read-time-range-v0`) | n/a — baseline IS the current shipped behaviour of `kaleidoscope_cli::stats_with_tiers` at the commit this DISCUSS wave is written against | Same new test file (no-flag scenario) PLUS the existing locked test files continuing to pass green under `cargo test --package kaleidoscope-cli` with no assertion edits | Guardrail (non-regression on the existing `stats` subcommand behaviour and on the locked OK4 protection tests for the prior two stats waves) |

## Metric Hierarchy

- **North Star**: **OK1-bounded-window-records** — the principal KPI.
  Without it, the feature does not exist; with it alone the operator
  can already answer the principal incident-response question ("how
  many records did `acme` write in yesterday's window?").
- **Leading Indicators**: OK2 (earliest/latest derivation in
  window) — proves the operator gets the time bracket of the active
  window directly, without doing min/max client-side via `jq`.
- **Guardrail Metrics**:
  - OK3 (Cinder lines unchanged) — pins D-CinderScope. The Cinder
    side is state-snapshot, not time-bound; this KPI is the
    test-level guarantee that no future change accidentally makes
    the Cinder lines time-filtered. Reviewers may otherwise assume
    the time range applies symmetrically across the whole output.
  - OK4 (no-flag byte equivalence) — when the operator omits both
    flags, `stats_with_tiers()` continues to behave exactly as it
    does today; the two locked OK4-protection test files (the
    `cli-stats-subcommand-v0` oracle and the
    `cli-stats-cinder-tier-distribution-v0` oracle) continue to
    pass without assertion edits.

## Cross-feature alignment

The persona, sidecar/collector chain (none in this feature — `stats`
has no `--observe-otlp` flag), and operator-facing posture mirror
the prior `kaleidoscope-cli` features in the cluster. The KPI shape
mirrors `cli-read-time-range-v0`'s KPI shape exactly on the
input-side (OK1 / OK4 are the bounded-window correctness and no-flag
non-regression posture from that feature), DIFFERS on the output side
(this feature has a state-snapshot Cinder guardrail OK3 that the
`read` feature has no counterpart for, since `read` has no Cinder
output), and DIFFERS on the timestamp derivation (this feature has
OK2 for the windowed earliest/latest, while `read` has OK3 for the
half-bounded ranges — the half-bounded shape on `stats` is exercised
as a sub-scenario of OK1/OK2 rather than a separate KPI).

| KPI | Cross-feature precedent | This feature |
|-----|-------------------------|--------------|
| OK1 | OK1-bounded-window-filter in `cli-read-time-range-v0` (the bounded-window count posture inherited from the `read` feature) | bounded-window record count on `stats` via half-open `[since, until)` interval — same wiring, different output line (`records=N` vs the NDJSON stream) |
| OK2 | OK2-bounded-window-earliest-latest is new — `cli-read-time-range-v0` had no equivalent because `read` does not render `earliest=` / `latest=` | windowed min/max derivation that powers the `earliest=` / `latest=` lines |
| OK3 | OK3 is new — `cli-read-time-range-v0` had no Cinder output to guard | the state-snapshot Cinder lines remain time-independent; pins D-CinderScope |
| OK4 | OK2-no-flag-byte-equivalent in `cli-read-time-range-v0` (no-flag non-regression on `read`'s observable surface) | no-flag non-regression on `stats_with_tiers`'s stdout, return value, and locked test pass-through |

## Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| OK1-bounded-window-records | `crates/kaleidoscope-cli/tests/stats_time_range.rs` — bounded-window scenario | `cargo test --package kaleidoscope-cli --test stats_time_range` exit code. The test pre-ingests 5 records with `observed_time_unix_nano` values `{100, 200, 300, 400, 500}` for tenant `acme`, then invokes `stats_with_tiers` with `TimeRange::new(200, 400)`, then asserts (a) returned count is `2`, (b) the `records=` line on stdout is exactly `records=2`, (c) the record at exactly `400` is excluded (half-open upper bound), (d) the record at exactly `200` is included (closed lower bound) | At every commit touching the CLI stats path, the `lumen::TimeRange` type, or the ISO 8601 parser | `kaleidoscope-cli` maintainer (CI feedback per ADR-0005) |
| OK2-bounded-window-earliest-latest | Same test file — bounded-window scenario (same pre-ingest) and empty-window scenario | Same `cargo test` invocation. The bounded-window scenario asserts the `earliest=` line is `earliest=1970-01-01T00:00:00.000000200Z` and the `latest=` line is `latest=1970-01-01T00:00:00.000000300Z` — the windowed min/max, not the global 100/500. The empty-window scenario invokes `stats_with_tiers` with a `TimeRange` containing zero matching records and asserts stdout begins with exactly `records=0\n` and contains NO `earliest=` / `latest=` lines (D-EmptyWindow) | Same | Same |
| OK3-cinder-lines-unchanged | Same test file — Cinder invariance scenario | Same `cargo test` invocation. The test pre-ingests records spanning multiple days for tenant `acme`, seeds Cinder with non-zero placements in all three tiers, invokes `stats_with_tiers` TWICE with two different bounded `TimeRange` values, and asserts the Cinder lines (`hot=…`, `warm=…`, `cold=…`) on the two captured stdouts are byte-identical even though the Lumen lines (`records=…`, `earliest=…`, `latest=…`) differ between the two | Same | Same |
| OK4-no-flag-byte-equivalence | Same test file — no-flag scenario, PLUS the two locked test files | Same `cargo test` invocation. The test invokes `stats_with_tiers` with `TimeRange::all()` (the no-flag default) and asserts the stdout bytes equal the pre-feature shape. PLUS `cargo test --package kaleidoscope-cli --test stats_subcommand` exits `0` with no assertion edits, AND `cargo test --package kaleidoscope-cli --test stats_cinder_tier_distribution` exits `0` with no assertion edits (only a mechanical signature-match update at the `stats_with_tiers()` call sites if DESIGN decides on a 4-arg signature extension) | Same | Same |

## Hypothesis

We believe that **adding two optional CLI flags `--since <ISO 8601
UTC>` and `--until <ISO 8601 UTC>` to `kaleidoscope-cli stats`,
threading the parsed values into a `lumen::TimeRange::new(since_ns,
until_ns)` construction at the `stats_with_tiers()` library
function's call site (replacing the hard-coded `TimeRange::all()` at
`crates/kaleidoscope-cli/src/lib.rs:359-361`), with `u64::MAX` /
`0` as the implicit unbounded-side defaults, reusing the existing
`parse_iso8601_utc_nanos` parser shipped in
`cli-read-time-range-v0`, and explicitly NOT filtering the
Cinder-tier lines (D-CinderScope)** for the **platform operator
(Priya)** will achieve **direct, storage-layer time-bounded count and
earliest/latest queries that answer per-incident "how many records
arrived in this window?" and "what's the duration of the active
window?" questions in one CLI invocation, without dumping the
records and aggregating client-side, and without changing behaviour
for operators who omit the flags**.

We will know this is true when:

- The new acceptance test's bounded-window scenario passes green,
  asserting the `records=` line equals the windowed count (OK1).
- The new acceptance test's bounded-window and empty-window
  scenarios pass green, asserting the `earliest=` / `latest=` lines
  reflect the windowed min/max and are entirely omitted in the
  empty case (OK2 / D-EmptyWindow).
- The new acceptance test's Cinder invariance scenario passes
  green, asserting the Cinder lines are byte-identical across
  different `TimeRange` invocations (OK3 / D-CinderScope).
- The new acceptance test's no-flag scenario passes green AND the
  existing locked test files
  `crates/kaleidoscope-cli/tests/stats_subcommand.rs` and
  `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`
  continue to pass green with no assertion edits (OK4).

## Handoff to DESIGN

The DESIGN wave (`nw-solution-architect`) should preserve:

1. **The half-open `[start, end)` semantics of `lumen::TimeRange`**
   (`crates/lumen/src/record.rs:97-120`). The CLI flag semantics
   MUST mirror this exactly: `--since X` is the closed lower bound,
   `--until Y` is the open upper bound. DESIGN MUST NOT propose
   changes to `lumen::TimeRange`.
2. **The signature-extension precedent from `cli-read-time-range-v0`**.
   The `read()` library function was extended with an explicit
   `range: TimeRange` parameter; the `stats_with_tiers()` extension
   should follow the same shape (a new explicit `range: TimeRange`
   parameter, fourth in argument order). The default at the
   binary's `run_stats` site MUST construct `TimeRange::all()` when
   neither flag is set so existing callers are byte-equivalent (OK4).
3. **The reuse of the existing `parse_iso8601_utc_nanos` parser**
   (`crates/kaleidoscope-cli/src/lib.rs:528-647`). NO new parser
   code. NO new error code. NO new variant on `IsoParseError`. The
   same error path the `read` feature uses (D-NoNewError).
4. **The order-independent flag parsing posture** at
   `crates/kaleidoscope-cli/src/main.rs:188-214`. The new
   `parse_time_range(args)` invocation on the `stats` side reuses
   the existing helper unchanged (it scans from
   `args.iter().skip(2)`, which is past the bin name and the
   subcommand name — works identically for `read` and `stats`).
5. **D-CinderScope**: the Cinder lines are state-snapshot, NOT
   time-bound. The time range applies ONLY to the
   `lumen.query(tenant, TimeRange)` call at line 360; the
   `cinder.list_by_tier(tenant, tier)` calls at lines 375-380 are
   not touched.
6. **D-EmptyWindow**: when the bounded query returns zero records,
   stdout contains exactly one Lumen line `records=0\n` followed by
   the unchanged Cinder snapshot lines — no `earliest=`, no
   `latest=`. This is the existing empty-tenant contract from
   `crates/kaleidoscope-cli/src/lib.rs:362-369` carried over to the
   empty-window case (the same `if let (Some(first), Some(last))`
   match arm fires on the windowed result instead of the
   `TimeRange::all()` result).
7. **The locked test files**: `stats_subcommand.rs`,
   `stats_cinder_tier_distribution.rs`, the `observe_otlp_*` family,
   and `read_time_range.rs` continue to pass with NO assertion
   edits. The only edit any locked stats test file gets is a
   mechanical signature-match update at the `stats_with_tiers()`
   call sites if DESIGN adopts the 4-arg signature extension.

The DESIGN wave SHOULD decide:

- The exact signature shape for the new `TimeRange`-driving control
  on `kaleidoscope_cli::stats_with_tiers` (recommended: a new
  explicit `range: TimeRange` parameter; the acceptance test cares
  only that the caller can drive any `TimeRange::new(s, e)` into
  the underlying `lumen.query` call). The same shape on the
  untouched legacy `stats()` function is OUT of scope —
  `stats()` remains the byte-level OK4 oracle for the original
  `cli-stats-subcommand-v0` feature and stays unmodified.
- The exact `print_usage` block update for the `stats` subcommand
  documenting `--since <ISO 8601 UTC>` and `--until <ISO 8601 UTC>`,
  including the explicit D-CinderScope note (the time range applies
  to the Lumen lines only; the Cinder tier-distribution lines
  remain state-snapshot).

## DEVOPS instrumentation needs

No new collection infrastructure. The CI gate is the new
acceptance test's exit code, per ADR-0005 Gate 1 (the workspace
already runs `cargo test` on every commit). No new dashboard
panels required — this feature is a query-shape change on stdout,
not a new metric emission source. The existing `--observe-otlp`
flag is N/A for this feature (`stats` does not support
`--observe-otlp` today; out-of-scope per `wave-decisions.md`).
