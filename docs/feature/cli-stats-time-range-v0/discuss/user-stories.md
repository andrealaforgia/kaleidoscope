<!-- markdownlint-disable MD024 -->

# User Stories — `cli-stats-time-range-v0`

## System Constraints (apply to every story)

- Rust idiomatic per `CLAUDE.md`: data + free functions + traits where
  polymorphism is genuinely needed. The data type at the trait-port
  boundary is `lumen::TimeRange` (`crates/lumen/src/record.rs:97-120`);
  this feature changes the runtime construction site of the `TimeRange`
  passed to `LogStore::query` inside `kaleidoscope_cli::stats_with_tiers`
  (the function the CLI dispatches to at
  `crates/kaleidoscope-cli/src/lib.rs:349-383`), not the trait itself.
- License: AGPL-3.0-or-later, matching the rest of the workspace.
- The acceptance idiom for this project is Rust `#[test]` functions with
  `// Given / // When / // Then` comment blocks, not Gherkin `.feature`
  files. The Given/When/Then text in the UAT Scenarios section below is
  the specification; DISTILL translates it into `#[test]` functions in
  the new file `crates/kaleidoscope-cli/tests/stats_time_range.rs`
  mirroring the patterns already in `tests/stats_subcommand.rs`,
  `tests/stats_cinder_tier_distribution.rs`, and `tests/read_time_range.rs`.
- Half-open interval contract: `lumen::TimeRange` is `[start, end)` —
  closed-lower, open-upper. `contains(t)` returns `true` iff
  `start_unix_nano <= t < end_unix_nano`
  (`crates/lumen/src/record.rs:116-119`). The new `--since X --until Y`
  flag pair on `stats` MUST honour this same half-open convention so
  the CLI surface matches both the underlying storage semantics and the
  prior `read --since / --until` precedent shipped in
  `cli-read-time-range-v0`. A record with
  `observed_time_unix_nano == until_ns` is EXCLUDED from the
  `records=` count and from the `earliest=` / `latest=` derivation.
  A record with `observed_time_unix_nano == since_ns` is INCLUDED.
- Half-bounded contract: when only `--since X` is set, the implicit
  upper bound is `u64::MAX` (interval `[X, u64::MAX)`, mirroring the
  `TimeRange::all()` upper bound at `crates/lumen/src/record.rs:111-114`).
  When only `--until Y` is set, the implicit lower bound is `0`
  (interval `[0, Y)`). When neither flag is set, the constructed
  range is exactly `TimeRange::all()` and the call site is
  byte-equivalent to today.
- ISO 8601 input contract: each flag value MUST parse as an ISO 8601
  UTC timestamp with the exact shape `YYYY-MM-DDTHH:MM:SSZ` or
  `YYYY-MM-DDTHH:MM:SS.D..DZ` (any 1..=9 fractional-second digits
  accepted; the `Z` suffix is the only timezone form accepted). The
  parser is the existing `kaleidoscope_cli::parse_iso8601_utc_nanos`
  shipped in `cli-read-time-range-v0`
  (`crates/kaleidoscope-cli/src/lib.rs:528-647`). No new parser code
  is required; this feature reuses the parser unchanged. This is
  D-NoNewError in `wave-decisions.md`.
- Invalid-input contract: when either flag's value fails to parse,
  the CLI exits with a non-zero exit code and writes to stderr a
  message naming WHICH flag carried the bad value (`--since` or
  `--until`) and the offending input verbatim — the exact contract
  the predecessor `read` feature established at
  `crates/kaleidoscope-cli/src/main.rs:197-214`. No Lumen store is
  opened in that case; no `stats_with_tiers()` call is made; no
  records are written to stdout. Exit code is 1
  (`ExitCode::FAILURE`).
- Cinder lines are state-snapshot, NOT time-bound (D-CinderScope in
  `wave-decisions.md`). The Cinder `hot=` / `warm=` / `cold=` lines
  reflect the CURRENT per-tenant placement counts from
  `TieringStore::list_by_tier(tenant, tier)` at
  `crates/kaleidoscope-cli/src/lib.rs:375-380`; the `--since` /
  `--until` flag pair filters the Lumen-side records only. Cinder
  exposes `placed_at` per `TierEntry` but no "currently in tier at
  time T" query, so a time-bound Cinder projection is out of scope.
  This is the DECISION point most likely to confuse reviewers and
  is documented explicitly in `wave-decisions.md` D-CinderScope.
- Empty-window contract: when the chosen window contains zero
  records, the Lumen-side output mirrors the predecessor's
  empty-tenant contract — exactly one Lumen line `records=0\n`, no
  `earliest=` line, no `latest=` line. The Cinder lines follow
  unchanged from their current snapshot (per D-CinderScope above).
  This is D-EmptyWindow in `wave-decisions.md`.
- No-flag non-regression: every assertion in the existing locked test
  files `crates/kaleidoscope-cli/tests/stats_subcommand.rs` (OK4
  oracle for the original `stats` feature) and
  `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`
  (OK4 oracle for the tier feature) MUST continue to pass
  byte-equivalently after this feature ships. Those tests exercise
  `stats()` and `stats_with_tiers()` with the implicit
  `TimeRange::all()` shape; the new optional flags must default to
  `TimeRange::all()` so the existing call sites are unaffected. The
  test files invoke `stats_with_tiers()` via its current 3-arg
  signature; under DESIGN's likely extension to a 4-arg signature
  with an explicit `range: TimeRange` parameter, the locked test
  files are updated only at the call-site to pass `TimeRange::all()`
  explicitly (mechanical signature-match update, same precedent as
  `observe_otlp_read_flag.rs` adopted in `cli-read-time-range-v0`).
  No assertion in either locked file is edited. This is OK4.
- Flag interaction posture: `stats` does NOT support `--observe-otlp`
  today, so there is no composition concern (out-of-scope, see
  `wave-decisions.md`). The new `--since` / `--until` flag-parse
  helpers are order-independent with each other and with the
  positional arguments.
- Scope is the `stats` subcommand only via its library dispatcher
  `stats_with_tiers()`. The `ingest`, `read`, and the legacy `stats()`
  library function are NOT touched (`stats()` is the byte-level OK4
  oracle for the original `cli-stats-subcommand-v0` feature; it
  remains untouched as the historical reference). The `lumen::TimeRange`
  data type is NOT modified. The Cinder placement model is NOT
  modified.

---

## US-01: Operator counts a tenant's records within an ISO 8601 time window

### Elevator Pitch

- **Before**: Priya wants to know how many records `acme` wrote in
  yesterday's incident window (`2026-05-18T00:00:00Z` to
  `2026-05-19T00:00:00Z`) and what the earliest/latest seen instants
  inside that window were. The `stats` subcommand today
  (`kaleidoscope-cli stats acme /tmp/data`) only summarises the full
  tenant since-the-beginning-of-time — the `stats_with_tiers()` body
  always calls `lumen.query(tenant, TimeRange::all())` at
  `crates/kaleidoscope-cli/src/lib.rs:359-361`. To get the windowed
  count Priya has to either (1) run
  `kaleidoscope-cli read acme /tmp/data --since 2026-05-18T00:00:00Z
  --until 2026-05-19T00:00:00Z | jq -s 'length'` (which dumps every
  matching record across stdout just to count them), or (2) skip the
  CLI entirely and write a one-off Rust binary that constructs
  `FileBackedLogStore::open(...)` directly. Both workarounds are
  noisy at the worst possible moment (mid-incident, on-call rota
  watching) and the first one wastes the operator's time scrolling
  through every record just to read the line count at the end.
- **After**: Priya runs:
  `kaleidoscope-cli stats acme /tmp/data --since 2026-05-18T00:00:00Z --until 2026-05-19T00:00:00Z`.
  Stdout contains the three Lumen lines reflecting ONLY the
  half-open window: `records=N` (where `N` is the count of records
  whose `observed_time_unix_nano` lies in
  `[1_779_062_400_000_000_000, 1_779_148_800_000_000_000)`),
  `earliest=<ISO 8601 UTC>` and `latest=<ISO 8601 UTC>` derived from
  the min/max `observed_time_unix_nano` WITHIN that window, followed
  by the unchanged Cinder snapshot lines `hot=H` / `warm=W` /
  `cold=C` for the tiers with non-zero per-tenant placements (the
  Cinder counts are time-independent — D-CinderScope in
  `wave-decisions.md`). She can also use either flag in isolation
  (`--since 2026-05-19T15:30:00Z` for "records since 15:30",
  `--until 2026-05-18T00:00:00Z` for "records before yesterday");
  when she omits both, behaviour is byte-equivalent to today (the
  locked `stats_subcommand.rs` and `stats_cinder_tier_distribution.rs`
  tests still pass). A typo on either flag (e.g. `--since yesterday`)
  exits non-zero with the offending flag named in stderr — the same
  fail-fast contract `read --since / --until` already ships.
- **Decision enabled**: "How many records did `acme` write in
  yesterday's incident window?" — answered directly with one CLI
  invocation, in two text lines on stdout, without scrolling
  through every record. Also: "Is yesterday's slice empty for
  tenant `X`?" — answered by the `records=0\n` shape (D-EmptyWindow).
  Also: "What's the duration of the active window for this tenant?"
  — answered by `latest - earliest` of the two timestamp lines.

### Problem

Priya the platform operator runs a multi-tenant Kaleidoscope
deployment. When an incident hits, her first triage questions are
quantitative: "how many records did `acme` write between 14:00 and
14:30 UTC?" and "what was the earliest and latest record observed
in that window?". Both are time-bounded count-and-range queries —
the same shape `kaleidoscope-cli stats acme /tmp/data` already
answers for the full tenant since-the-beginning-of-time, just
restricted to a half-open interval she chooses at the keyboard.

Today the `stats` subcommand cannot answer either question
directly. The dispatcher in `main.rs` calls
`kaleidoscope_cli::stats_with_tiers(...)`
(`crates/kaleidoscope-cli/src/main.rs:226-235`), which always calls
`lumen.query(tenant, TimeRange::all())` at
`crates/kaleidoscope-cli/src/lib.rs:359-361`. For a tenant with ten
gigabytes of NDJSON, the only ways to extract the windowed count
and the windowed earliest/latest are:

1. Run `kaleidoscope-cli read acme /tmp/data --since X --until Y |
   jq -s 'length'` to count, and a separate `... | jq -s 'min_by /
   max_by'` invocation to get the earliest and latest seen instants.
   Slow (full filtered stream across stdout just to count it), noisy
   (the records are scrolling past during incident response), and
   redundant (the storage layer already knows the count and the
   min/max from its sorted-by-time query result — `read` does the
   work and throws it away).
2. Write a one-off Rust binary that opens `FileBackedLogStore`
   directly and inspects the `Vec<LogRecord>` result of
   `query(tenant, TimeRange::new(s, e))`. Not a production option
   for routine mid-incident use.

Priya finds it operationally hostile to need either workaround when
the storage layer already supports `TimeRange::new(start, end)` as
a first-class query parameter (`crates/lumen/src/record.rs:103-115`)
and the prior `read --since / --until` feature
(`cli-read-time-range-v0`) already shipped the exact CLI surface and
the exact ISO 8601 UTC parser (`parse_iso8601_utc_nanos` at
`crates/kaleidoscope-cli/src/lib.rs:528-647`). The gap is purely at
the `stats_with_tiers()` call site: line 360 hard-codes
`TimeRange::all()` where it could take a caller-driven `TimeRange`.

### Who

Priya the platform operator | runs a multi-tenant Kaleidoscope
deployment for a fintech | already uses `kaleidoscope-cli stats
<tenant> <data_dir>` for full-tenant summary counts and
`kaleidoscope-cli read <tenant> <data_dir> --since X --until Y`
for windowed record dumps | now wants the same windowing on the
`stats` subcommand so she can count and bracket the window without
streaming the records through `jq` | thinks in ISO 8601 UTC (the
project's `stats` output already renders timestamps that way per
`format_iso8601_utc_nanos` at
`crates/kaleidoscope-cli/src/lib.rs:409-419`).

### Solution

Add two new optional CLI flags to `kaleidoscope-cli stats`:

```text
kaleidoscope-cli stats <tenant_id> <data_dir>
  [--since <ISO 8601 UTC>] [--until <ISO 8601 UTC>]
```

When `--since X --until Y` are both set, `stats_with_tiers()` calls
`lumen.query(tenant, TimeRange::new(parse(X), parse(Y)))` instead of
`lumen.query(tenant, TimeRange::all())`. When only `--since X` is
set, the call becomes `TimeRange::new(parse(X), u64::MAX)`. When
only `--until Y` is set, the call becomes `TimeRange::new(0,
parse(Y))`. When neither flag is set, the call is
`TimeRange::all()` — the existing shape, byte-equivalent to today.

The three Lumen lines on stdout (`records=N`, `earliest=<ISO>`,
`latest=<ISO>`) reflect the bounded query result: `N` is the count
of matching records, `earliest` is `format_iso8601_utc_nanos` of
the smallest `observed_time_unix_nano` IN the window, `latest` is
the same for the largest. When the window contains zero records,
output mirrors the predecessor's empty-tenant contract:
exactly `records=0\n` with no `earliest=` and no `latest=` line
(D-EmptyWindow).

The Cinder `hot=` / `warm=` / `cold=` lines remain unchanged from
their state-snapshot semantics — the `--since` / `--until` flag pair
does NOT apply to them (D-CinderScope). Each is still selectively
emitted under Option B (zero-count tiers emit no line) so the
backwards-compatibility invariant holds for tenants whose Cinder
side is empty.

The parser is the existing `parse_iso8601_utc_nanos` shipped in
`cli-read-time-range-v0`; no new parser code is required
(D-NoNewError). The same flag-parse helper structure
(`parse_flag_iso(args, "--since")` / `parse_flag_iso(args,
"--until")`) at `crates/kaleidoscope-cli/src/main.rs:197-214` is
the precedent for the new `stats`-side helper.

### Domain Examples

#### 1. Yesterday's window for `acme` — both flags set

Priya investigates `acme`'s record volume for yesterday (the 24-hour
window starting at `2026-05-18T00:00:00Z`). The tenant has cumulative
records spanning multiple days. She runs:

```text
kaleidoscope-cli stats acme /tmp/data \
  --since 2026-05-18T00:00:00Z \
  --until 2026-05-19T00:00:00Z
```

`--since 2026-05-18T00:00:00Z` parses to
`since_ns = 1_779_062_400_000_000_000`. `--until 2026-05-19T00:00:00Z`
parses to `until_ns = 1_779_148_800_000_000_000`. The library call
becomes `lumen.query(&acme, TimeRange::new(1_779_062_400_000_000_000,
1_779_148_800_000_000_000))`. Suppose 7 records lie in that
half-open window with `observed_time_unix_nano` evenly spaced at
4-hour intervals from `1_779_062_400_000_000_000`
(=`2026-05-18T00:00:00Z`) to `1_779_148_800_000_000_000 - 1`
(strictly less than the upper bound — the record at exactly the
upper bound would be EXCLUDED per the half-open contract). Stdout
receives, in order:

```text
records=7
earliest=2026-05-18T00:00:00.000000000Z
latest=2026-05-18T20:00:00.000000000Z
hot=H
warm=W
cold=C
```

Where `H`, `W`, `C` are the CURRENT per-tenant Cinder placement
counts from `list_by_tier(&acme, Tier::*)` at the time of the call
— independent of the `--since` / `--until` flag values, per
D-CinderScope. Tiers with a zero count emit no line (Option B per
the predecessor `stats_with_tiers` contract at
`crates/kaleidoscope-cli/src/lib.rs:375-380`). Stderr receives the
existing `stats ok: records=7` line where the number equals the
Lumen-side count (mirrors the existing `run_stats_with` contract at
`crates/kaleidoscope-cli/src/main.rs:226-235`).

#### 2. Records since 15:30 — only `--since` set

At `2026-05-19T17:00:00Z`, Priya wants the count, earliest, and
latest of `acme`'s records since `15:30Z` today. She runs:

```text
kaleidoscope-cli stats acme /tmp/data --since 2026-05-19T15:30:00Z
```

`--since 2026-05-19T15:30:00Z` parses to
`since_ns = 1_779_205_800_000_000_000`. `--until` is absent so the
upper bound defaults to `u64::MAX`. The call is
`lumen.query(&acme, TimeRange::new(1_779_205_800_000_000_000,
u64::MAX))`, which mirrors `TimeRange::all()`'s upper bound at
`crates/lumen/src/record.rs:111-114`. Stdout receives the three
Lumen lines reflecting all records with `observed_time_unix_nano >=
1_779_205_800_000_000_000`, followed by the unchanged Cinder
snapshot lines.

Symmetric for `--until` only: at `2026-05-19T08:00:00Z` Priya wants
the count of `acme` records before yesterday started. She runs
`kaleidoscope-cli stats acme /tmp/data --until 2026-05-18T00:00:00Z`.
The call is
`lumen.query(&acme, TimeRange::new(0, 1_779_062_400_000_000_000))`.
Stdout receives the three Lumen lines reflecting all records with
`observed_time_unix_nano < 1_779_062_400_000_000_000`, followed by
the unchanged Cinder snapshot lines.

#### 3. Empty window for `acme` — D-EmptyWindow contract

Priya checks whether `acme` was active in the early hours of a quiet
Sunday (`2026-05-17T02:00:00Z` to `2026-05-17T03:00:00Z`). She runs:

```text
kaleidoscope-cli stats acme /tmp/data \
  --since 2026-05-17T02:00:00Z \
  --until 2026-05-17T03:00:00Z
```

The bounded `lumen.query` returns zero records. Per D-EmptyWindow,
stdout receives exactly one Lumen line `records=0\n` (no
`earliest=`, no `latest=`), then the unchanged Cinder snapshot
lines `hot=H` / `warm=W` / `cold=C` for the tiers with non-zero
per-tenant placements (still time-independent, per D-CinderScope).
For a tenant whose Cinder side is also empty, the entire stdout is
exactly `records=0\n` — byte-equivalent to the predecessor's
empty-tenant contract from `cli-stats-subcommand-v0` /
`cli-stats-cinder-tier-distribution-v0`. Stderr receives
`stats ok: records=0`. Exit code is 0 (an empty window is a valid
query result, not an error).

#### 4. Typo on `--since` — fail-fast with named flag

Mid-incident, Priya muscle-memories the wrong format:

```text
kaleidoscope-cli stats acme /tmp/data --since yesterday --until 2026-05-19T00:00:00Z
```

The `--since` value `yesterday` fails the ISO 8601 shape check at
`parse_iso8601_utc_nanos`
(`crates/kaleidoscope-cli/src/lib.rs:528-647`). The CLI:

1. Does NOT open the Lumen store.
2. Does NOT open the Cinder store.
3. Does NOT call `stats_with_tiers()`.
4. Writes NOTHING to stdout.
5. Writes to stderr a message containing both `--since` and the
   verbatim bad value `yesterday`, prefixed by the
   `kaleidoscope-cli: ` prefix the binary already adds at
   `crates/kaleidoscope-cli/src/main.rs:68-72`.
6. Exits with code 1 (`ExitCode::FAILURE`).

Symmetric for `--until`: if Priya runs
`kaleidoscope-cli stats acme /tmp/data --until 2026-13-32T25:99:99Z`,
the message names `--until` and the verbatim bad value
(`2026-13-32T25:99:99Z`), and the exit code is 1. The error path
is the same one `cli-read-time-range-v0` already ships — no new
error code, no new error variant (D-NoNewError).

### UAT Scenarios (BDD)

#### Scenario: Bounded window query reflects only records in `[since, until)`

```text
Given Priya has pre-ingested 7 records for tenant acme into /tmp/data
  with observed_time_unix_nano values evenly spaced 4 hours apart
  starting at 2026-05-18T00:00:00Z (= 1_779_062_400_000_000_000 ns)
  and ending at 2026-05-19T00:00:00Z (= 1_779_148_800_000_000_000 ns,
  inclusive of the last record at 2026-05-19T00:00:00Z exactly)
And Cinder has placements for acme such that list_by_tier counts
  are Hot=H, Warm=W, Cold=C at the call time
When Priya invokes `kaleidoscope_cli::stats_with_tiers` with
  range = TimeRange::new(1_779_062_400_000_000_000,
  1_779_148_800_000_000_000) and a captured stdout sink
And the call returns Ok with `count == 6`
Then the captured stdout begins with three Lumen lines in order:
  `records=6`, `earliest=2026-05-18T00:00:00.000000000Z`,
  `latest=2026-05-18T20:00:00.000000000Z`
And the record at exactly until_ns (2026-05-19T00:00:00Z) is EXCLUDED
  from the count (half-open upper bound, per
  `crates/lumen/src/record.rs:116-119`)
And the record at exactly since_ns (2026-05-18T00:00:00Z) is INCLUDED
  in the count and is the earliest= timestamp (closed lower bound)
And the captured stdout then continues with the unchanged Cinder
  snapshot lines (`hot=H`, `warm=W`, `cold=C`, each only when its
  count is non-zero, per Option B) — the Cinder counts are NOT
  filtered by the time range (per D-CinderScope in `wave-decisions.md`)
```

#### Scenario: No flags is byte-equivalent to today's full-tenant summary

```text
Given Priya has pre-ingested N records for tenant acme into /tmp/data
And Cinder has placements for acme such that list_by_tier counts are
  Hot=1, Warm=0, Cold=0 (the canonical post-ingest shape)
When Priya invokes `kaleidoscope_cli::stats_with_tiers` with
  range = TimeRange::all() (the no-flag default) and a captured
  stdout sink
And the call returns Ok with `count == N`
Then the captured stdout bytes are byte-equivalent to the bytes the
  pre-feature `stats_with_tiers()` produces for the same inputs
And every assertion in `crates/kaleidoscope-cli/tests/stats_subcommand.rs`
  continues to pass green (OK4 oracle for the original stats feature)
And every assertion in
  `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`
  continues to pass green (OK4 oracle for the tier feature)
```

#### Scenario: `--since` alone uses `u64::MAX` as the implicit upper bound

```text
Given Priya has pre-ingested 4 records for tenant acme into /tmp/data
  with observed_time_unix_nano values 100, 200, 300, 400
When Priya invokes `kaleidoscope_cli::stats_with_tiers` with
  range = TimeRange::new(250, u64::MAX) and a captured stdout sink
And the call returns Ok with `count == 2`
Then the captured stdout begins with three Lumen lines in order:
  `records=2`, `earliest=1970-01-01T00:00:00.000000300Z`,
  `latest=1970-01-01T00:00:00.000000400Z`
And the call exhibits the same shape as `TimeRange::all()` on its
  upper end (the record with observed_time_unix_nano == 400, the
  highest ingested value, is INCLUDED — strictly less than u64::MAX)
```

#### Scenario: `--until` alone uses `0` as the implicit lower bound

```text
Given Priya has pre-ingested 4 records for tenant acme into /tmp/data
  with observed_time_unix_nano values 100, 200, 300, 400
When Priya invokes `kaleidoscope_cli::stats_with_tiers` with
  range = TimeRange::new(0, 250) and a captured stdout sink
And the call returns Ok with `count == 2`
Then the captured stdout begins with three Lumen lines in order:
  `records=2`, `earliest=1970-01-01T00:00:00.000000100Z`,
  `latest=1970-01-01T00:00:00.000000200Z`
And the call exhibits the same shape as `TimeRange::all()` on its
  lower end (the record with observed_time_unix_nano == 100, the
  lowest ingested value, is INCLUDED — greater than or equal to 0)
```

#### Scenario: Empty window emits `records=0` with no earliest/latest (D-EmptyWindow)

```text
Given Priya has pre-ingested records for tenant acme into /tmp/data
  with observed_time_unix_nano values entirely outside the chosen window
And Cinder has placements for acme such that list_by_tier counts are
  Hot=H, Warm=W, Cold=C (still time-independent per D-CinderScope)
When Priya invokes `kaleidoscope_cli::stats_with_tiers` with a
  range that contains zero matching records and a captured stdout
  sink
And the call returns Ok with `count == 0`
Then the captured stdout begins with exactly one Lumen line
  `records=0` — no `earliest=` line, no `latest=` line (the
  predecessor's empty-tenant contract from
  `crates/kaleidoscope-cli/src/lib.rs:362-369` carried over to the
  empty-window case)
And the captured stdout then continues with the unchanged Cinder
  snapshot lines (`hot=H`, `warm=W`, `cold=C`, each only when its
  count is non-zero)
```

#### Scenario: Cinder lines are state-snapshot, NOT time-bound (D-CinderScope)

```text
Given Priya has pre-ingested records for tenant acme into /tmp/data
  spanning multiple days
And Cinder has placements for acme such that list_by_tier counts are
  Hot=5, Warm=12, Cold=47 at the call time
When Priya invokes `kaleidoscope_cli::stats_with_tiers` twice in
  succession with two DIFFERENT bounded ranges (one for yesterday,
  one for the day before) and two captured stdout sinks
Then the Lumen lines (`records=`, `earliest=`, `latest=`) differ
  between the two captured outputs to reflect the per-window count
  and timestamps
And the Cinder lines (`hot=5`, `warm=12`, `cold=47`) are identical
  byte-for-byte in BOTH captured outputs — the time range does NOT
  apply to the Cinder snapshot
```

#### Scenario: Invalid ISO 8601 on `--since` fails fast with stderr message (D-NoNewError)

```text
Given Priya invokes the CLI with the argv list
  ["kaleidoscope-cli", "stats", "acme", "/tmp/data", "--since", "yesterday"]
When the binary's `run_stats` dispatcher attempts to parse the
  `--since` value via the same `parse_iso8601_utc_nanos` path the
  read feature uses
Then the dispatcher returns Err BEFORE opening the Lumen store or
  the Cinder store
And stderr contains a message naming `--since` and the verbatim bad
  value `yesterday`
And stdout receives no bytes
And the process exits with code 1 (`ExitCode::FAILURE`) — the same
  error path the read feature uses, no new error code introduced
```

### Acceptance Criteria

- [ ] The library function `kaleidoscope_cli::stats_with_tiers`
      accepts a way for its caller to pass a `lumen::TimeRange` other
      than the implicit `TimeRange::all()`. The exact signature
      shape is DESIGN's choice; the observable property is that the
      caller can drive any `TimeRange::new(s, e)` into the underlying
      `lumen.query` call at line 360. (Likely shape: a new explicit
      `range: TimeRange` parameter, mirroring the
      `cli-read-time-range-v0` precedent on `read()`.)
- [ ] When the caller supplies `TimeRange::new(since_ns, until_ns)`,
      the `records=` line equals the count of records whose
      `observed_time_unix_nano` lies in the half-open interval
      `[since_ns, until_ns)`. Records at exactly `until_ns` are
      excluded; records at exactly `since_ns` are included.
- [ ] When the caller supplies `TimeRange::new(since_ns, until_ns)`
      and the bounded query result is non-empty, the `earliest=`
      line equals `format_iso8601_utc_nanos` of the smallest
      `observed_time_unix_nano` IN the window, and the `latest=`
      line equals `format_iso8601_utc_nanos` of the largest.
- [ ] When the caller supplies `TimeRange::all()` (the no-flag
      default), stdout bytes are byte-equivalent to what the
      pre-feature `stats_with_tiers()` produces for the same
      inputs, and the call exhibits the same return-value shape
      (`count: usize` equal to the matched record count).
- [ ] When the bounded query result is empty, stdout contains
      exactly one Lumen line `records=0\n` followed by the unchanged
      Cinder snapshot lines (no `earliest=`, no `latest=`) —
      D-EmptyWindow contract.
- [ ] The Cinder `hot=` / `warm=` / `cold=` lines reflect the CURRENT
      per-tenant `TieringStore::list_by_tier(tenant, tier)` counts,
      independent of the supplied `TimeRange`. The time range does
      NOT apply to the Cinder snapshot. Each line is selectively
      emitted under Option B (zero-count tiers emit no line) — same
      contract as the predecessor `stats_with_tiers`.
- [ ] The binary's `run_stats` dispatcher in
      `crates/kaleidoscope-cli/src/main.rs` parses `--since <value>`
      and `--until <value>` flags from the argv list via the same
      `parse_flag_iso(args, flag)` helper used by `run_read` at
      `crates/kaleidoscope-cli/src/main.rs:197-214`. The two flags
      are order-independent with each other and with the positional
      arguments.
- [ ] When `--since` is absent the implicit lower bound is `0`. When
      `--until` is absent the implicit upper bound is `u64::MAX`.
      When both are absent the constructed `TimeRange` is
      `TimeRange::all()` and the call is structurally equivalent to
      today's call site at
      `crates/kaleidoscope-cli/src/lib.rs:359-361`.
- [ ] The `--since` and `--until` flag values are parsed by the
      existing `kaleidoscope_cli::parse_iso8601_utc_nanos` shipped
      in `cli-read-time-range-v0` (`crates/kaleidoscope-cli/src/lib.rs:528-647`).
      No new parser code is required. (D-NoNewError.)
- [ ] When `--since`'s value fails to parse, the binary writes a
      stderr message containing both the literal string `--since`
      and the verbatim bad value, does NOT open the Lumen store,
      does NOT open the Cinder store, does NOT write any bytes to
      stdout, and exits with code 1 (`ExitCode::FAILURE`) — the
      same error path the read feature uses, no new error code.
- [ ] When `--until`'s value fails to parse, the binary writes a
      stderr message containing both the literal string `--until`
      and the verbatim bad value, does NOT open the Lumen store,
      does NOT open the Cinder store, does NOT write any bytes to
      stdout, and exits with code 1 — symmetric to `--since`.
- [ ] `print_usage` in `crates/kaleidoscope-cli/src/main.rs`
      documents `--since <ISO 8601 UTC>` and `--until <ISO 8601 UTC>`
      on the `stats` subcommand block, mentioning the half-open
      `[since, until)` contract on the Lumen lines and the
      D-CinderScope decision that the Cinder lines remain
      state-snapshot.
- [ ] The existing test files
      `crates/kaleidoscope-cli/tests/stats_subcommand.rs` and
      `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`
      continue to pass green under `cargo test --package
      kaleidoscope-cli` with NO assertion edits (only a mechanical
      signature-match update at the `stats_with_tiers()` call sites
      to pass `TimeRange::all()` explicitly under DESIGN's likely
      4-arg signature extension — same precedent as
      `observe_otlp_read_flag.rs` adopted in
      `cli-read-time-range-v0`).
- [ ] New test file
      `crates/kaleidoscope-cli/tests/stats_time_range.rs` is added,
      covering OK1 (bounded window), OK2 (no-flag byte equivalence
      via cross-check with `stats_with_tiers` under
      `TimeRange::all()` and the locked test files), OK3 (Cinder
      lines unchanged under different time ranges), and the
      half-bounded / fail-fast / empty-window scenarios above.

### Outcome KPIs

- **Who**: platform operator (Priya), observed at the byte level on
  stdout from `kaleidoscope-cli stats`.
- **Does what**: receives `records=N`, `earliest=<ISO>`, and
  `latest=<ISO>` lines reflecting ONLY the records whose
  `observed_time_unix_nano` lies in the half-open `[since, until)`
  interval she specified via `--since` / `--until`; the Cinder
  lines remain unchanged from their current state-snapshot semantics.
- **By how much**: 100% of records counted in the `records=N` line
  satisfy `since_ns <= observed_time_unix_nano < until_ns` when
  both flags set (OK1); the `earliest=` and `latest=` lines reflect
  the windowed min/max (OK2); the Cinder lines are byte-identical
  across different time-range invocations on the same data
  (OK3); 100% byte equivalence of stdout when neither flag set
  (OK4).
- **Measured by**: new acceptance test
  `crates/kaleidoscope-cli/tests/stats_time_range.rs` (OK1, OK2,
  OK3, and the half-bounded / fail-fast / empty-window cases) and
  existing locked tests `crates/kaleidoscope-cli/tests/stats_subcommand.rs`
  + `tests/stats_cinder_tier_distribution.rs` (OK4 non-regression).
- **Baseline**: 0% — today the only way to obtain a time-bounded
  count + earliest + latest is to dump the records via `read --since
  X --until Y` and aggregate client-side via `jq`.

Maps to OK1-bounded-window-records, OK2-bounded-window-earliest-latest,
OK3-cinder-lines-unchanged, and OK4-no-flag-byte-equivalence in
`outcome-kpis.md`.

### Technical Notes

- Modified file: `crates/kaleidoscope-cli/src/lib.rs` — the
  `stats_with_tiers` function (lines 349-383) exposes a
  `TimeRange`-equivalent control to its caller. The exact signature
  shape (a new parameter, a builder, an overload) is DESIGN's
  choice; the property is that the caller can drive
  `TimeRange::new(s, e)` into the `lumen.query` call at line 360.
  The no-flag default MUST construct `TimeRange::all()` so existing
  call sites are byte-equivalent.
- Modified file: `crates/kaleidoscope-cli/src/main.rs` — `run_stats`
  (lines 216-235) gains parsing of `--since <value>` and
  `--until <value>` flags via the same `parse_flag_iso(args, flag)`
  / `parse_time_range(args)` helpers already used by `run_read`
  (lines 188-214). These helpers are factored so they are
  subcommand-neutral (they scan from `args.iter().skip(2)`, which is
  past the bin name and the subcommand name — works identically for
  `read` and `stats`). `print_usage` (lines 81-119) is updated to
  document both new flags on the `stats` subcommand block, including
  the D-CinderScope note that the time range applies only to the
  Lumen lines.
- New file: `crates/kaleidoscope-cli/tests/stats_time_range.rs`.
  Mirrors the harness pattern from
  `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`
  (the `tenant`, `record`, `temp_root`, `cleanup`, `ndjson`,
  `cinder_base`, `lumen_base`, `seed_cinder`, `cinder_count` helpers
  are duplicated inline at v0; rule-of-three extraction deferred —
  this is the seventh test file in the cluster using the same
  shape).
- Manifest: `crates/kaleidoscope-cli/Cargo.toml` gains a new
  `[[test]]` entry `name = "stats_time_range", path =
  "tests/stats_time_range.rs"`. No new external crate dependency
  required (the parser is the existing `parse_iso8601_utc_nanos`
  from `cli-read-time-range-v0`).
- DO NOT modify the existing test files
  `crates/kaleidoscope-cli/tests/stats_subcommand.rs`,
  `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`,
  the `observe_otlp_*` family, or
  `crates/kaleidoscope-cli/tests/read_time_range.rs` — they are
  locked OK4 protection. The only edit any of them gets is a
  mechanical signature-match update at the `stats_with_tiers()` /
  `stats()` call sites if DESIGN decides on a 4-arg signature
  extension — same precedent as `observe_otlp_read_flag.rs`
  adopted in `cli-read-time-range-v0`. No assertion is changed.
- Lumen `TimeRange` semantics: `[start, end)` half-open closed-lower
  open-upper (`crates/lumen/src/record.rs:97-119`); `TimeRange::all()`
  is `[0, u64::MAX)`. The new flag pair MUST honour this convention
  exactly so the CLI surface matches the storage semantics.
- ISO 8601 parser: reuse `kaleidoscope_cli::parse_iso8601_utc_nanos`
  (`crates/kaleidoscope-cli/src/lib.rs:528-647`) — no new parser
  code, no new error code, no new variant on `IsoParseError`. The
  parser already handles all the calendar-out-of-range cases the
  fail-fast scenarios exercise.
- Cinder placement model is NOT modified: this feature only
  changes the construction of the `TimeRange` passed to
  `lumen.query`. The `cinder.list_by_tier(tenant, tier)` calls at
  `crates/kaleidoscope-cli/src/lib.rs:375-380` are not touched; the
  Cinder lines remain state-snapshot per D-CinderScope.
- Composition with `--observe-otlp`: N/A. `stats` does not support
  `--observe-otlp` today, so there is no composition concern.
  Out-of-scope per `wave-decisions.md`.
- Slice tag: not `@infrastructure` — this story directly enables an
  operator-visible decision on a real CLI surface
  (`kaleidoscope-cli stats <tenant> <data_dir> --since X --until Y`).

### Dependencies

- `lumen::TimeRange` already exists with the correct `[start, end)`
  semantics (`crates/lumen/src/record.rs:97-120`). No change to
  the `lumen` crate is required.
- `kaleidoscope_cli::parse_iso8601_utc_nanos` already exists from
  `cli-read-time-range-v0` (`crates/kaleidoscope-cli/src/lib.rs:528-647`).
  No new parser code required.
- `kaleidoscope_cli::stats_with_tiers` already opens Lumen and
  calls `query` with a `TimeRange` at
  `crates/kaleidoscope-cli/src/lib.rs:357-361`. The change is at
  the TimeRange-construction site.
- `kaleidoscope_cli::stats` (the byte-level OK4 oracle for the
  original `cli-stats-subcommand-v0` feature) is NOT modified; it
  remains the untouched historical reference at
  `crates/kaleidoscope-cli/src/lib.rs:312-331`.
- `crates/kaleidoscope-cli/src/main.rs`'s
  `parse_time_range(args)` / `parse_flag_iso(args, flag)` helpers
  already exist (lines 188-214) and are subcommand-neutral; the new
  `stats` dispatcher reuses them unchanged.
- `aegis::TenantId`, `cinder::*`, `lumen::*`, `serde_json` — all
  existing dependencies.
- No new external crates required.

### Slice

`slices/slice-01-stats-time-range-filter.md`
