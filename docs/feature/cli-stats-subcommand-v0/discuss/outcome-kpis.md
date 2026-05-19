# Outcome KPIs — `cli-stats-subcommand-v0`

## Feature

`cli-stats-subcommand-v0` — add a third subcommand `stats` to
`kaleidoscope-cli`, invoked as
`kaleidoscope-cli stats <tenant_id> <data_dir>`, that prints to stdout
the Lumen record count for the tenant plus, when the tenant is
populated, the earliest and latest `observed_time_unix_nano`
timestamps rendered as ISO 8601 UTC. v0 ships exactly three keys
(`records`, `earliest`, `latest`) on plain-text key=value lines, one
stat per line; no Cinder stats, no `--observe-otlp` wiring, no JSON
output, no filtering, no multi-tenant aggregates
(`wave-decisions.md` D2, D3, D4, D7).

## Objective

A single `kaleidoscope-cli stats acme /tmp/data` invocation prints,
to stdout, exactly the records-count line plus (when N > 0) the
earliest and latest record timestamps as ISO 8601 UTC strings. The
operator gets the canonical post-ingest smoke-test answer ("did data
land, and across what window?") in one CLI call, with stdout that
pipes naturally through `grep` / `cut` / `awk`, without paying the
cost of materialising the full record set through any pipeline. The
empty-tenant case is unambiguous (one line, `records=0`, no
timestamp lines).

## Note on KPI granularity

This feature is operator-visible at the CLI surface. The principal
KPI is OK1 (record count correctness), the second KPI is OK2 (time
range correctness — earliest and latest match the underlying record
set's min/max `observed_time_unix_nano`), and the third KPI is OK3
(empty-tenant case yields one line `records=0` with no timestamp
lines).

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| OK1-CLI-stats-record-count | Priya the platform operator, observed at the stdout byte level | Sees a single `records=N` line on stdout where N equals the exact count of records that `kaleidoscope_cli::read` would return for the same tenant and `data_dir`, terminated by `\n`. Consistency with `read` is the principal correctness invariant — operators rely on `stats` as the cheap replacement for `read \| wc -l`. | 100% of `stats()` invocations against any tenant report a `records=N` value where N equals the number of records that `read()` would dump for the same `(tenant, data_dir)` pair; 0% of `stats()` invocations report a count that disagrees with `read`'s | 0% (no `stats` subcommand exists today; the operator's only path to a record count is `kaleidoscope-cli read <tenant> <data_dir> \| wc -l`, which materialises the full record set through a pipe) | New acceptance test `crates/kaleidoscope-cli/tests/stats_subcommand.rs` — populated-tenant scenario asserts the `records=` line equals the count returned by a parallel `read()` call on the same `(tenant, data_dir)`; tenant-isolation scenario asserts `stats(acme, ...) == 7` when `acme` has 7 and `globex` (in the same `data_dir`) has 3 (proving stats does NOT include cross-tenant records) | Leading (operator-visible behaviour; principal KPI for this feature) |
| OK2-CLI-stats-time-range | Priya the platform operator, observed at the stdout byte level | Sees an `earliest=<ISO 8601 UTC>` line and a `latest=<ISO 8601 UTC>` line on stdout where the timestamps equal the ISO 8601 UTC rendering of the minimum and maximum `observed_time_unix_nano` across the record set the `lumen.query(tenant, TimeRange::all())` call returns | 100% of `stats()` invocations against a populated tenant produce an `earliest=` value equal to the ISO 8601 UTC rendering of `records.iter().map(\|r\| r.observed_time_unix_nano).min().unwrap()` (in conceptual terms) and a `latest=` value equal to the same expression with `.max()` instead of `.min()`; 100% of single-record-tenant invocations produce identical `earliest=` and `latest=` values (degenerate time window) | 0% (today operators parse the JSON output of `kaleidoscope-cli read <tenant> <data_dir> \| head -1` and `\| tail -1` and convert nanoseconds-since-epoch to ISO 8601 by hand — error-prone enough that the time-window check is often skipped) | Same new test file — populated-tenant scenario asserts the two timestamp lines reflect the seeded min/max nanos converted to ISO 8601 UTC; single-record scenario asserts `earliest=` and `latest=` lines have byte-identical timestamp values | Leading (operator-visible behaviour; second-priority KPI) |
| OK3-CLI-stats-empty-tenant | Priya the platform operator, observed at the stdout byte level | Sees exactly one stdout line, `records=0`, terminated by `\n`, and NO `earliest=` or `latest=` lines, when the tenant has zero records under `data_dir` (whether because the tenant has never been ingested, or because the operator typed the tenant id wrong, or because the `data_dir` is freshly initialised). Exit code is 0 (empty-tenant is a valid query result, not an error). | 100% of `stats()` invocations against a zero-record tenant produce exactly 1 stdout line equal to `records=0\n`; 0% of such invocations leave any `earliest=` or `latest=` line in stdout; 100% return exit code 0 | n/a (no `stats` subcommand exists today; the closest baseline behaviour — `kaleidoscope-cli read acmee /tmp/data \| wc -l` — returns `0` but also produces a 0-byte stdout from `read`, which is silently indistinguishable from "the read failed without an error code" if the operator does not also check the exit code) | Same new test file — empty-tenant scenario asserts (a) stdout contains exactly 1 non-empty line, (b) that line equals `records=0`, (c) no line begins with `earliest=`, (d) no line begins with `latest=`, (e) stdout ends with `\n` | Leading (operator-visible behaviour; disambiguates the empty case from the populated case in a `grep`-friendly way) |

## Metric Hierarchy

- **North Star**: **OK1-CLI-stats-record-count** — the count
  correctness KPI. Without it, the subcommand cannot replace the
  operator's existing `read | wc -l` workaround. With it alone, the
  operator already has the cheapest possible smoke-test for "did
  data land?" — the time range answer (OK2) is the additional
  enrichment that makes capacity planning and audit/compliance
  questions answerable in the same call.
- **Leading Indicators**: OK2 (time range correctness) — proves the
  earliest/latest are consistent with the underlying record set,
  which is the necessary correctness invariant for any operator
  decision that depends on the time window.
- **Guardrail Metrics**: OK3 (empty-tenant unambiguity) — when the
  tenant has no records, the output is unambiguous in both directions
  (operator can grep for `records=0` to detect the empty case, and
  the absence of `earliest=`/`latest=` lines is the unambiguous
  signal that "min/max of an empty set" is correctly reported as
  undefined rather than as a bogus sentinel string).

## Cross-feature alignment

OK1 in this feature is the inspection-side mirror of the ingest-side
record count `ingest` already writes to stderr
(`ingest ok: records=N batches=M tier_items=K`,
`crates/kaleidoscope-cli/src/main.rs:111-114`). The N from `stats` must
agree with the N from `ingest` for the same `(tenant, data_dir)` pair
(modulo any later ingest invocations adding more records).

OK2 in this feature is the first KPI in the `kaleidoscope-cli` cluster
to surface log-record timestamp data to the operator's stdout. The
underlying field `LogRecord::observed_time_unix_nano: u64`
(`crates/lumen/src/record.rs:48`) has been carried through every
Lumen layer since v0 but has never been exposed in a CLI-rendered
form; this feature is its first operator-facing rendering.

OK3 in this feature deliberately rejects the alternative sentinel
encoding (`earliest=<none>` / `latest=<none>`) per `wave-decisions.md`
D5. The rejected option would invite operators to parse the sentinel
as a real timestamp and silently get the wrong answer; the chosen
option (omit the lines entirely) is robust to that failure mode.

| KPI | Cross-feature precedent | This feature |
|-----|-------------------------|--------------|
| OK1 | `ingest` writes `records=N` (in `records=N batches=M tier_items=K`) to stderr after a successful ingest (`crates/kaleidoscope-cli/src/main.rs:111-114`) | `stats` writes `records=N` to stdout as the principal output |
| OK2 | (n/a — no prior feature surfaces log-record timestamps to operator stdout) | `stats` writes `earliest=` and `latest=` ISO 8601 UTC lines to stdout |
| OK3 | (n/a — no prior feature has an empty-tenant operator-facing output) | `stats` writes exactly `records=0` and omits timestamp lines for the empty case |

## Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| OK1-CLI-stats-record-count | `crates/kaleidoscope-cli/tests/stats_subcommand.rs` — populated-tenant scenario and tenant-isolation scenario | `cargo test --package kaleidoscope-cli --test stats_subcommand` exit code. The populated-tenant test pre-ingests 7 records for tenant `acme` (via a setup `ingest()` call) and asserts the captured stdout from a `stats()` call contains `records=7` as line 1. The tenant-isolation test pre-ingests 7 records for `acme` and 3 records for `globex` into the same `data_dir`, then calls `stats(&acme, ...)` and asserts `records=7` (NOT 10). The consistency-with-read sub-assertion: a parallel `read()` call on the same `(tenant, data_dir)` returns `count == 7` (already a property of `read`'s tested behaviour) | At every commit touching the CLI stats path or the Lumen `query` method | `kaleidoscope-cli` maintainer (CI feedback per ADR-0005) |
| OK2-CLI-stats-time-range | Same test file — populated-tenant scenario and single-record scenario | Same `cargo test` invocation. The populated-tenant test seeds the 7 records with deterministic `observed_time_unix_nano` values spanning a known window (e.g. 2026-05-18T00:00:00Z to 2026-05-19T00:00:00Z) and asserts the `earliest=` and `latest=` lines render the expected ISO 8601 UTC strings (or, more robustly, parses the lines and asserts the parsed timestamps equal the seeded min/max nanos). The single-record test seeds exactly 1 record and asserts `earliest=` and `latest=` lines have byte-identical values | Same | Same |
| OK3-CLI-stats-empty-tenant | Same test file — empty-tenant scenario | Same `cargo test` invocation. The test opens a fresh `data_dir` (or pre-ingests records for one tenant and queries a different never-ingested tenant) and asserts (a) `stats()` returns Ok, (b) captured stdout contains exactly 1 non-empty line equal to `records=0`, (c) no line begins with `earliest=`, (d) no line begins with `latest=`, (e) stdout ends with `\n` | Same | Same |

## Hypothesis

We believe that **adding a `stats` subcommand to `kaleidoscope-cli`
that calls `lumen.query(tenant, TimeRange::all())` exactly once,
takes `records.len()` for the count, iterates the result once to
compute the min and max `observed_time_unix_nano`, and prints the
key=value lines to stdout** for the **platform operator (Priya)**
will achieve **a one-shot, pipeable, grep-friendly answer to "is
there data for this tenant, and across what time window?" that
replaces the operator's current `read | wc -l` / `read | head -1` /
`read | tail -1` workaround without materialising the record set
through any pipeline**.

We will know this is true when:

- The new acceptance test's populated-tenant scenario passes green,
  asserting that `stats()` against a 7-record `acme` produces
  exactly 3 stdout lines (`records=7`, `earliest=...`, `latest=...`)
  with the count equal to what `read` returns and the timestamps
  equal to the seeded min/max nanos converted to ISO 8601 UTC
  (OK1 + OK2).
- The new acceptance test's tenant-isolation scenario passes green,
  asserting that `stats(&acme, ...)` reports `records=7` when
  `acme` (7 records) and `globex` (3 records) coexist in the same
  `data_dir`, NOT 10 and NOT the union time window (OK1
  reinforcement — per-tenant isolation).
- The new acceptance test's empty-tenant scenario passes green,
  asserting that `stats()` against a never-ingested tenant produces
  exactly 1 stdout line, `records=0`, with no `earliest=` or
  `latest=` lines (OK3).
- The dogfood demo runs: `kaleidoscope-cli stats acme /tmp/k-data |
  grep ^records= | cut -d= -f2` returns the same integer that
  `kaleidoscope-cli read acme /tmp/k-data | wc -l` returns (modulo
  any intervening ingests).

## Handoff to DESIGN

The DESIGN wave (`nw-solution-architect`) should preserve:

1. **The quiescent recorder pattern**: the `stats()` function
   constructs a `LumenToPulseRecorder` over an in-process Pulse
   sink identically to the way `read()` does in its no-flag arm
   (`crates/kaleidoscope-cli/src/lib.rs:275-279`); no OTLP file is
   created and no `--observe-otlp` flag is accepted in v0
   (`wave-decisions.md` D3).
2. **The `LogStore::query(tenant, TimeRange::all())` call shape**:
   exactly one query call per `stats()` invocation, taking
   `records.len()` for the count and iterating once for min/max.
   The library already returns a `Vec<LogRecord>` sorted by
   `observed_time_unix_nano` ascending
   (`crates/lumen/src/store.rs:69-70`), so the min is
   `records.first()` and the max is `records.last()` — a single
   iteration is not even required if DESIGN decides to lean on the
   sort order. DESIGN locks the choice.
3. **The stdout output contract**: plain-text key=value lines, one
   stat per line, terminated by `\n`. The keys are exactly
   `records`, `earliest`, `latest` and appear in that order when
   present. The empty-tenant case prints only `records=0`.
4. **ISO 8601 UTC timestamp rendering**: target nanosecond
   precision; DESIGN picks the formatter (chrono / time /
   hand-rolled) per `wave-decisions.md` D6. The wire-observable
   contract is the ISO 8601 UTC string with `Z` suffix.

The DESIGN wave should NOT introduce flags
(`--observe-otlp`, `--json`, `--since=`), additional output keys
(`severity_count=`, `tier_count=`), Cinder lookups, or any
multi-tenant aggregation.

## DEVOPS instrumentation needs

No new collection infrastructure. The `stats` subcommand is a pure
read over the existing Lumen WAL+snapshot and emits no OTLP, no
metrics, no logs of its own (the in-process Pulse sink is
intentionally quiescent). The CI gate is the new acceptance test's
exit code, per ADR-0005 Gate 1 (the workspace already runs
`cargo test` on every commit).
