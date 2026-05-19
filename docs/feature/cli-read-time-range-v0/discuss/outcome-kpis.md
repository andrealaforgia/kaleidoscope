# Outcome KPIs — `cli-read-time-range-v0`

## Feature

`cli-read-time-range-v0` — extend the `kaleidoscope-cli read`
subcommand with two new optional flags `--since <ISO 8601 UTC>` and
`--until <ISO 8601 UTC>` so that the `read` subcommand can drive any
`lumen::TimeRange::new(since_ns, until_ns)` query instead of the
hard-coded `TimeRange::all()` at
`crates/kaleidoscope-cli/src/lib.rs:283-285`. Today an operator who
wants yesterday's records for tenant `acme` must stream the full
tenant dump (potentially ten gigabytes of NDJSON) and pipe through
`jq` filtering on `observed_time_unix_nano`. After this feature, the
same operator runs:

```text
kaleidoscope-cli read acme /tmp/data \
  --since 2026-05-18T00:00:00Z \
  --until 2026-05-19T00:00:00Z
```

and gets exactly yesterday's slice directly from the storage layer.

## Objective

A single `kaleidoscope-cli read acme /tmp/data --since X --until Y`
invocation returns ONLY records whose `observed_time_unix_nano` lies
in the half-open interval `[parse(X), parse(Y))`, where `parse` is
the inverse of the project's existing hand-rolled
`format_iso8601_utc_nanos` formatter at
`crates/kaleidoscope-cli/src/lib.rs:410-420`. Half-bounded queries
(`--since` only or `--until` only) supply `u64::MAX` or `0`
respectively for the unbounded side, mirroring `TimeRange::all()`'s
construction at `crates/lumen/src/record.rs:111-114`. When neither
flag is supplied, behaviour is byte-equivalent to today (the
existing `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs`
no-flag test continues to pass). When either flag's value fails to
parse, the CLI exits non-zero with a stderr message naming which
flag carried the bad value.

## Note on KPI granularity

This feature has four KPIs because the input/output behaviour
naturally factors into four orthogonal contracts: the bounded
positive case (OK1, principal), the no-flag non-regression
guardrail (OK2), the half-bounded case (OK3), and the invalid-input
failure mode (OK4). There is no cross-writer concurrency probe, no
cross-subcommand symmetry probe, and no cross-process probe — those
shapes do not apply to this feature.

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| OK1-bounded-window-filter | Priya the platform operator, observed at the byte level on stdout from `kaleidoscope-cli read` | Sees ONLY the records whose `observed_time_unix_nano` lies in the half-open interval `[since_ns, until_ns)` she specified via `--since` and `--until`, where `since_ns` and `until_ns` are the parser's output for the supplied ISO 8601 UTC values | 100% of records on stdout satisfy `since_ns <= observed_time_unix_nano < until_ns` when both flags are set; 0% of records outside that interval appear on stdout; the closed-lower boundary (record with `observed_time_unix_nano == since_ns` is INCLUDED) and the open-upper boundary (record with `observed_time_unix_nano == until_ns` is EXCLUDED) are exercised by witness records | 0% — today `read()` always calls `lumen.query(tenant, TimeRange::all())` (`crates/kaleidoscope-cli/src/lib.rs:283-285`) and emits every record the tenant has ever ingested; the only workaround is to stream the full dump and filter client-side via `jq` on the nanosecond field | New acceptance test `crates/kaleidoscope-cli/tests/read_time_range.rs` — pre-ingest 5 records with `observed_time_unix_nano` values `{100, 200, 300, 400, 500}` for tenant `acme`; invoke `read` with the equivalent of `--since` mapping to `200` and `--until` mapping to `400`; assert stdout contains exactly the records with `observed_time_unix_nano` in `{200, 300}` re-serialised as NDJSON (the record at `200` is included, the record at `400` is excluded — the half-open contract) | Leading (principal KPI — the operator-visible behaviour the feature exists to deliver) |
| OK2-no-flag-byte-equivalent | Priya the platform operator, observed at the byte level on stdout and return value from `kaleidoscope_cli::read` | Sees behaviour byte-equivalent to today when neither `--since` nor `--until` is supplied: the function returns the same NDJSON stdout (all matched records, one per line, terminated by `\n`), the same return value (`count: usize` equal to the number of matched records), and no behavioural change on any other observable surface | 100% byte equivalence of stdout records vs the pre-feature `read()` output for the same inputs; 100% equality of returned record count; 100% of the assertions in `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` and `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` continue to pass green with no edits | n/a — baseline IS the current shipped behaviour of `kaleidoscope_cli::read` at the commit this DISCUSS wave is written against | Same new test file — no-flag scenario asserts stdout bytes and return count under `TimeRange::all()`. PLUS the existing locked test files continue to pass green under `cargo test --package kaleidoscope-cli` with no edits | Guardrail (non-regression on the existing `read` subcommand behaviour and on the locked OK2 protection tests) |
| OK3-half-bounded-supported | Priya the platform operator, observed at the byte level on stdout | Sees the correct records when only ONE of `--since` / `--until` is supplied: `--since X` alone returns every record with `observed_time_unix_nano >= since_ns` (upper bound `u64::MAX`); `--until Y` alone returns every record with `observed_time_unix_nano < until_ns` (lower bound `0`) | 100% of `--since`-only invocations: stdout contains the records from `since_ns` onwards, matching `TimeRange::new(since_ns, u64::MAX)`. 100% of `--until`-only invocations: stdout contains the records strictly before `until_ns`, matching `TimeRange::new(0, until_ns)` | n/a — this scenario cannot exist today | Same test file — pre-ingest 4 records with `observed_time_unix_nano` values `{100, 200, 300, 400}`; invoke `read` with `--since` mapping to `250` (no `--until`); assert stdout contains exactly `{300, 400}`. Symmetric assertion for `--until` mapping to `250` (no `--since`); assert stdout contains exactly `{100, 200}` | Leading (the half-bounded shapes are direct operator value: "last 90 minutes" and "everything before yesterday" are common incident-response queries) |
| OK4-invalid-iso8601-fails-fast | Priya the platform operator, observed at the byte level on stderr and exit code from `kaleidoscope-cli read` | Sees a fail-fast error response when either flag's value cannot be parsed as a conformant ISO 8601 UTC timestamp: the binary writes a stderr message naming WHICH flag (`--since` or `--until`) carried the bad value AND the verbatim bad value, does NOT open the Lumen store, writes NOTHING to stdout, and exits with code 1 (`ExitCode::FAILURE`) | 100% of invocations with a non-conformant `--since` value exit non-zero with `--since` and the verbatim bad value in stderr; 100% of invocations with a non-conformant `--until` value exit non-zero with `--until` and the verbatim bad value in stderr; 0% of such invocations produce any bytes on stdout; 0% of such invocations open the Lumen store | 0% — today neither flag exists, so the parser cannot fail; conversion errors do not currently propagate from any caller-supplied time-range input | Same test file — invoke `run_read` (or the binary entry point) with argv lists containing (a) `["--since", "yesterday"]`, (b) `["--until", "2026-13-32T25:99:99Z"]`, (c) `["--since", "2026-05-18T00:00:00"]` (missing `Z`). For each: assert the result is `Err`, assert stderr contains both the flag name and the verbatim bad value, assert stdout is empty | Leading (operator-facing failure-mode contract: typos at the keyboard are the most common error class for a flag accepting free-form input) |

## Metric Hierarchy

- **North Star**: **OK1-bounded-window-filter** — the principal KPI.
  Without it, the feature does not exist; with it alone the operator
  can already answer the principal incident-response question ("what
  did `acme` write between 14:00 and 14:30 UTC?").
- **Leading Indicators**: OK3 (half-bounded support) — proves the
  operator can express the common one-sided queries ("last 90
  minutes", "everything before yesterday") without needing to invent
  a sentinel upper/lower bound by hand.
- **Guardrail Metrics**:
  - OK2 (no-flag byte equivalence) — when the operator omits both
    flags, `read` continues to behave exactly as it does today
    (stdout NDJSON of all matched records, return value equals
    matched count, no change to any other observable surface, all
    locked OK2-protection tests continue to pass).
  - OK4 (invalid ISO 8601 fails fast) — when the operator typos a
    flag value, the failure is immediate, loud, and self-locating
    (the offending flag is named in stderr), so the operator does
    not silently receive an empty slice and waste time chasing
    missing records that never existed.

## Cross-feature alignment

The persona, sidecar/collector/dashboard chain, and operator-facing
posture mirror the three prior `kaleidoscope-cli` features in the
cluster (the original `--observe-otlp` ingest wiring at commit
`3af7e82`, `cli-cinder-otlp-wiring-v0`, and `cli-read-observe-otlp-v0`).
The KPI shape is DIFFERENT — this feature has no cross-writer
concurrency probe and no cross-subcommand symmetry probe — because
the change is purely an input-side parameter on a single library
function, not a new emission source. OK2's non-regression posture
is shared with `cli-read-observe-otlp-v0`'s OK2 (same locked test
files protect the no-flag default).

| KPI | Cross-feature precedent | This feature |
|-----|-------------------------|--------------|
| OK1 | (no direct precedent — first feature in the cluster to change query semantics) | bounded-window filter on `observed_time_unix_nano` via half-open `[since, until)` interval |
| OK2 | OK2 in `cli-read-observe-otlp-v0` (no-flag non-regression on `read`'s observable surface) | no-flag non-regression on `read`'s stdout, return value, and locked test pass-through |
| OK3 | (no direct precedent — half-bounded queries are a new shape for this CLI) | half-bounded with implicit `u64::MAX` / `0` bounds |
| OK4 | (no direct precedent — first feature in the cluster to take free-form parseable values on a flag) | named-flag fail-fast for invalid ISO 8601 input |

## Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| OK1-bounded-window-filter | `crates/kaleidoscope-cli/tests/read_time_range.rs` — bounded-window scenario | `cargo test --package kaleidoscope-cli --test read_time_range` exit code. The test pre-ingests 5 records with `observed_time_unix_nano` values `{100, 200, 300, 400, 500}` for tenant `acme`, then invokes `read` with `TimeRange::new(200, 400)`, then asserts (a) returned count is `2`, (b) stdout bytes equal the records with `observed_time_unix_nano` in `{200, 300}` re-serialised as NDJSON (one per line, terminated by `\n`), (c) the record at exactly `400` is excluded (half-open upper bound), (d) the record at exactly `200` is included (closed lower bound) | At every commit touching the CLI read path, the Lumen `TimeRange` type, or the new ISO 8601 parser | `kaleidoscope-cli` maintainer (CI feedback per ADR-0005) |
| OK2-no-flag-byte-equivalent | Same test file — no-flag scenario | Same `cargo test` invocation. The test invokes `read` with `TimeRange::all()` (the no-flag default) and asserts (a) returned count equals the number of pre-ingested records, (b) stdout bytes equal the pre-ingested records re-serialised as NDJSON. PLUS `cargo test --package kaleidoscope-cli --test observe_otlp_read_flag` and `--test observe_otlp_flag` exit code is `0` with no edits to either file | Same | Same |
| OK3-half-bounded-supported | Same test file — half-bounded scenarios (one each for `--since`-only and `--until`-only) | Same `cargo test` invocation. The test pre-ingests 4 records with `observed_time_unix_nano` values `{100, 200, 300, 400}`, invokes `read` with `TimeRange::new(250, u64::MAX)`, asserts stdout contains exactly `{300, 400}`; symmetrically invokes `read` with `TimeRange::new(0, 250)`, asserts stdout contains exactly `{100, 200}` | Same | Same |
| OK4-invalid-iso8601-fails-fast | Same test file — invalid-input scenarios | Same `cargo test` invocation. The test invokes `run_read` (the binary entry point form testable in-process per the inline tests at `crates/kaleidoscope-cli/src/main.rs:155-165`) with argv lists containing `["--since", "yesterday"]`, `["--until", "2026-13-32T25:99:99Z"]`, and `["--since", "2026-05-18T00:00:00"]` (missing `Z`); asserts the result is `Err`, stderr contains both the offending flag name and the verbatim bad value, and stdout is empty | Same | Same |

## Hypothesis

We believe that **adding two optional CLI flags `--since <ISO 8601 UTC>`
and `--until <ISO 8601 UTC>` to `kaleidoscope-cli read`, threading the
parsed values into a `lumen::TimeRange::new(since_ns, until_ns)`
construction at the `read()` library function's call site (replacing
the hard-coded `TimeRange::all()` at
`crates/kaleidoscope-cli/src/lib.rs:283-285`), with `u64::MAX` /
`0` as the implicit unbounded-side defaults and a hand-rolled ISO 8601
UTC parser inverse to the existing `format_iso8601_utc_nanos` formatter**
for the **platform operator (Priya)** will achieve **direct,
storage-layer time-bounded queries that answer per-incident "what
arrived in this window?" questions in one CLI invocation, without
streaming the full tenant dump and without changing behaviour for
operators who omit the flags**.

We will know this is true when:

- The new acceptance test's bounded-window scenario passes green,
  asserting that `read` returns ONLY records whose
  `observed_time_unix_nano` lies in `[since_ns, until_ns)` (OK1).
- The new acceptance test's no-flag scenario passes green AND the
  existing locked test files
  `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` and
  `observe_otlp_flag.rs` continue to pass green with no edits (OK2).
- The new acceptance test's half-bounded scenarios pass green,
  asserting correct behaviour for `--since`-only and `--until`-only
  (OK3).
- The new acceptance test's invalid-input scenarios pass green,
  asserting fail-fast non-zero exit with the offending flag named in
  stderr (OK4).

## Handoff to DESIGN

The DESIGN wave (`nw-solution-architect`) should preserve:

1. **The half-open `[start, end)` semantics of `lumen::TimeRange`**
   (`crates/lumen/src/record.rs:97-120`). The CLI flag semantics MUST
   mirror this exactly: `--since X` is the closed lower bound,
   `--until Y` is the open upper bound. DESIGN MUST NOT propose
   changes to `lumen::TimeRange`.
2. **The `Option<&Path>` parameter idiom** already established by
   `ingest`'s and (per the prior feature) `read`'s `--observe-otlp`
   parameter. The new time-range control on `read`'s library
   signature should follow a similar Option-shaped pattern OR an
   explicit `TimeRange` parameter; the choice is DESIGN's, but the
   default MUST be `TimeRange::all()` so existing callers are
   byte-equivalent.
3. **The order-independent flag parsing posture** at
   `crates/kaleidoscope-cli/src/main.rs:130-144`. The new `--since` and
   `--until` parsing helpers should mirror that shape so all three
   `read` subcommand flags (`--since`, `--until`, `--observe-otlp`)
   can appear in any order after the positional arguments.
4. **The hand-rolled, no-`chrono`-no-`time` posture** established by
   the existing `format_iso8601_utc_nanos` formatter at
   `crates/kaleidoscope-cli/src/lib.rs:410-420` (D5 in
   `wave-decisions.md`). The new parser is the inverse of the existing
   formatter; round-trip property `parse(format(ns)) == ns` MUST hold.
5. **The fail-fast invariant**: invalid input on either flag MUST NOT
   open the Lumen store. The parser runs before any I/O.

The DESIGN wave SHOULD decide:

- The exact signature shape for the new control on
  `kaleidoscope_cli::read` (new parameter? builder pattern?
  pre-construction by caller?). The acceptance test cares only that
  the caller can drive any `TimeRange::new(s, e)` into the underlying
  `lumen.query` call; the rest is DESIGN's choice.
- Whether the ISO 8601 parser's range of fractional-second digits is
  exactly `0..=9` (zero fractional digits allowed when no `.` is
  present) or `1..=9` (fractional digits required if `.` is present).
  Round-trip with the formatter requires `9` digits exact when present;
  zero-digit form (no `.` at all) MUST round-trip via the format
  shape `YYYY-MM-DDTHH:MM:SSZ`.

## DEVOPS instrumentation needs

No new collection infrastructure. The CI gate is the new acceptance
test's exit code, per ADR-0005 Gate 1 (the workspace already runs
`cargo test` on every commit). No new dashboard panels required —
this feature is a query-shape change on stdout, not a new metric
emission source. The existing `--observe-otlp` instrumentation
remains independently usable on `read` and is out of scope for this
feature's acceptance test (D6 in `wave-decisions.md`).
