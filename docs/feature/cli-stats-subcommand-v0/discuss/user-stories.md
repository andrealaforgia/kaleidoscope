<!-- markdownlint-disable MD024 -->

# User Stories — `cli-stats-subcommand-v0`

## System Constraints (apply to every story)

- Rust idiomatic per `CLAUDE.md`: data + free functions + traits where
  polymorphism is genuinely needed. The new function is a free function
  on top of the existing `lumen::LogStore` trait; no new trait is
  introduced and no existing trait is modified.
- License: AGPL-3.0-or-later, matching the rest of the workspace.
- The acceptance idiom for this project is Rust `#[test]` functions with
  `// Given / // When / // Then` comment blocks, not Gherkin `.feature`
  files. The Given/When/Then text in the UAT Scenarios sections below is
  the specification; DISTILL translates it into `#[test]` functions in
  `crates/kaleidoscope-cli/tests/stats_subcommand.rs` mirroring the
  harness pattern already in
  `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` and
  `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs`.
- Output-shape contract: stdout is plain-text key=value lines, one
  stat per line, terminated by `\n`. v0 keys are exactly `records`,
  `earliest`, `latest`. No header line, no trailing blank line, no
  JSON, no CSV, no colour codes, no Unicode box drawing.
- Key ordering contract: when present, lines appear in the order
  `records`, `earliest`, `latest`. The empty-tenant case (D5 in
  `wave-decisions.md`) prints only `records=0` and omits the two
  timestamp lines.
- Timestamp format contract: timestamps are rendered as ISO 8601 UTC
  with `Z` suffix and (target) nanosecond precision, e.g.
  `2026-05-18T08:23:04.123456789Z`. The underlying field is
  `lumen::LogRecord::observed_time_unix_nano: u64`
  (`crates/lumen/src/record.rs:48`); the rendered string MUST be
  parseable as an ISO 8601 timestamp by any standard datetime
  library.
- Stream contract: the stats are written to **stdout**. Unlike `ingest`
  (which writes its `records=N batches=M tier_items=K` line to stderr
  because the principal output is the lack of an error), `stats`'s
  principal output IS the stats, so stdout is the right stream.
- No-flag-only: `stats` accepts NO optional flags in v0. Specifically,
  `--observe-otlp` is NOT accepted (`wave-decisions.md` D3); operators
  who want OTLP visibility of stats queries get it via a follow-up
  feature.
- Tenant-isolation contract: `stats` for tenant `acme` MUST NOT count
  records belonging to tenant `globex`. Inherited from
  `lumen::LogStore`'s per-tenant isolation invariant
  (`crates/lumen/src/store.rs:69-70`).
- Read-only contract: `stats` mutates nothing. No WAL writes, no
  snapshot updates, no Cinder placements. It is a pure read over the
  Lumen WAL+snapshot for the supplied tenant.

---

## US-01: Operator inspects a tenant's record count and time window without dumping every record

### Elevator Pitch

- **Before**: Priya wants to know "did `acme`'s overnight ingest run
  actually land records, and across what time window?" Today her only
  options are:

  ```text
  kaleidoscope-cli read acme /tmp/data > /dev/null
  kaleidoscope-cli read acme /tmp/data | wc -l
  kaleidoscope-cli read acme /tmp/data | head -1
  kaleidoscope-cli read acme /tmp/data | tail -1
  ```

  Four invocations, each one dumps the entire record set from Lumen
  through `serde_json::to_string` and back across a pipe just to be
  thrown away by `/dev/null`, `wc -l`, `head -1`, `tail -1`. For a
  tenant with 10 million records that is roughly 10 GB of NDJSON
  produced, piped, and discarded — four times. Worse, the time
  window answer (`head -1` and `tail -1`) requires the operator to
  parse a JSON line from the bash pipeline to extract
  `observed_time_unix_nano`, which means an `awk '{print $1}'`
  followed by a `jq` followed by a Unix-epoch-to-ISO-8601 conversion
  that nobody can remember the incantation for. The smoke-test is so
  expensive that operators stop running it after every ingest.

- **After**: Priya runs:

  ```text
  kaleidoscope-cli stats acme /tmp/data
  ```

  Stdout, in milliseconds, prints exactly three lines:

  ```text
  records=10000000
  earliest=2026-05-18T00:00:01.123456789Z
  latest=2026-05-19T03:45:12.987654321Z
  ```

  She knows immediately that `acme`'s ingest ran and landed 10 M
  records spanning the expected overnight window. If she wants just
  the count: `kaleidoscope-cli stats acme /tmp/data | grep ^records=
  | cut -d= -f2`. If she wants just the time range: `grep -E
  '^(earliest\|latest)='`. No pipeline that materialises every record;
  no JSON parsing; no Unix-epoch-to-ISO arithmetic. If the tenant has
  no data, stdout prints exactly one line, `records=0`, and the time
  range lines are absent (D5 in `wave-decisions.md`), so a `grep
  ^records=0` is the unambiguous "is this tenant empty?" check.

- **Decision enabled**: Priya can decide "did `acme`'s overnight
  ingest land records, and across what time window?" — the canonical
  post-ingest smoke-test and the canonical first audit/compliance
  question — in one CLI invocation, with stdout that pipes naturally
  through `grep` / `cut` / `awk`, without dumping or parsing the
  record set.

### Problem

Priya the platform operator runs a multi-tenant Kaleidoscope
deployment. She regularly needs to answer three closely-related
questions about a given tenant:

1. **Smoke-test after ingest**: "did the overnight `ingest` run
   actually land records for `acme`, or did the upstream feeder
   crash silently?"
2. **Capacity planning**: "how many records does `acme` generate per
   day on average — is the tenant growing, and how does that
   correlate with their storage cost line?"
3. **Audit/compliance**: "when did `acme` first write to our
   platform, and when did they last write — what is our retention
   window for this tenant?"

All three questions reduce to "give me the record count and the time
window for one tenant". Today she answers them by piping
`kaleidoscope-cli read acme /tmp/data` through `wc -l`, `head -1`,
`tail -1`. The `read` subcommand dumps every matching record (full
NDJSON) just to be thrown away by the downstream pipeline; on a
multi-million-record tenant this is operationally hostile — the
`read` invocation can run for minutes and consume gigabytes of
intermediate pipe buffers. She also has to parse the NDJSON lines
herself (`jq '.observed_time_unix_nano'`) and convert
nanoseconds-since-epoch to ISO 8601 by hand, which is error-prone
enough that she frequently skips the time-window check entirely and
just relies on the record count.

### Who

Priya the platform operator | runs a multi-tenant Kaleidoscope
deployment for a fintech | already uses `kaleidoscope-cli ingest` and
`kaleidoscope-cli read` daily | already uses `--observe-otlp` for
cross-process observability per the three reference features | wants
a one-shot, pipeable answer to "how much data does this tenant have
and what is its time window?" without paying the cost of materialising
the full record set | uses standard Unix text tools (`grep`, `cut`,
`awk`) on stdout output, not JSON parsers.

### Solution

Add a `stats` subcommand to `kaleidoscope-cli` invoked as:

```text
kaleidoscope-cli stats <tenant_id> <data_dir>
```

The binary's `main.rs` dispatcher gains a third `Some("stats")` arm
parallel to the existing `Some("ingest")` / `Some("read")` arms
(`crates/kaleidoscope-cli/src/main.rs:48-60`). The arm parses the
two positional arguments via the existing `parse_positional` helper
(`crates/kaleidoscope-cli/src/main.rs:155-161`), then calls a new
library function `kaleidoscope_cli::stats(&tenant, &data_dir,
stdout)`. `print_usage` (lines 71-97) gains a `stats` section
describing the subcommand and its key=value output shape.

The library function `stats` opens the same `FileBackedLogStore` that
`read` opens (using the same `lumen_base(data_dir)` helper,
`crates/kaleidoscope-cli/src/lib.rs:118-120`), with a quiescent
`LumenToPulseRecorder` over an in-process Pulse sink (the same
pattern `read` uses in its no-flag arm,
`crates/kaleidoscope-cli/src/lib.rs:275-279`, so no OTLP file is
created and the subcommand has no observable side effect other than
its stdout). It calls `lumen.query(tenant, TimeRange::all())` exactly
once, takes `records.len()` for the count, iterates the result once
to compute the min and max `observed_time_unix_nano`, and writes the
key=value lines to the supplied writer:

- Populated case (`records.len() > 0`): three lines —
  `records=<N>\nearliest=<ISO8601>\nlatest=<ISO8601>\n`.
- Empty case (`records.len() == 0`): one line —
  `records=0\n`. No `earliest=`, no `latest=`.

Exit code is `0` for both populated and empty cases (the empty case is
not an error). The function returns the data shape DESIGN locks in
(per `wave-decisions.md` D8 the name is
`kaleidoscope_cli::stats(tenant, data_dir, writer)` returning
`Result<(usize, Option<(SystemTime, SystemTime)>), Error>` or
similar).

The `Error` type reuses the existing `kaleidoscope_cli::Error`
variants — at minimum `LumenOpen(LogStoreError)` and
`LumenQuery(LogStoreError)` are already wired
(`crates/kaleidoscope-cli/src/lib.rs:73-83`); no new error variant is
introduced for v0.

### Domain Examples

#### 1. Happy path — Priya checks `acme` after its overnight ingest

Priya has previously run, sometime last night:

```text
kaleidoscope-cli ingest acme /tmp/k-data < acme-overnight.ndjson
```

where `acme-overnight.ndjson` contains 7 log records for tenant
`acme` with `observed_time_unix_nano` values spanning from
`1747526400000000000` (2026-05-18T00:00:00.000000000Z) to
`1747612800000000000` (2026-05-19T00:00:00.000000000Z). She runs at
07:00 the next morning:

```text
kaleidoscope-cli stats acme /tmp/k-data
```

Stdout contains exactly three lines, in order:

```text
records=7
earliest=2026-05-18T00:00:00Z
latest=2026-05-19T00:00:00Z
```

The output ends with a trailing `\n`. Exit code is `0`. Stderr is
empty. Priya pipes the output through `grep ^records= | cut -d= -f2`
in another invocation and gets `7`; she pipes through
`grep ^earliest= | cut -d= -f2-` and gets the ISO 8601 timestamp she
can feed to `date -d` for further arithmetic.

The Lumen store under `/tmp/k-data` is unchanged after the call (no
WAL writes, no snapshot updates). The library function returns
`Ok((7, Some((earliest_systime, latest_systime))))` to its caller (the
exact return shape is DESIGN-locked per `wave-decisions.md` D8).

#### 2. Edge case — Priya inspects a populated tenant with a single record

Priya wants to know the time window for tenant `globex`, which a test
ingest job populated last week with exactly one log record at
`observed_time_unix_nano = 1746921600000000000`
(2026-05-11T00:00:00.000000000Z):

```text
kaleidoscope-cli stats globex /tmp/k-data
```

Stdout contains exactly three lines:

```text
records=1
earliest=2026-05-11T00:00:00Z
latest=2026-05-11T00:00:00Z
```

`earliest` and `latest` are byte-identical because there is only one
record. This is intentional — single-record tenants have a degenerate
time window of one instant. Priya can `diff <(grep ^earliest=
output) <(grep ^latest= output | sed 's/^latest/earliest/')` to detect
the single-record case if her tooling needs to.

#### 3. Boundary case — Priya inspects an empty tenant (typo or fresh tenant)

Priya types the tenant id wrong (or the tenant is genuinely fresh):

```text
kaleidoscope-cli stats acmee /tmp/k-data
```

`acmee` has never been ingested (the typo of `acme`). The Lumen store
exists at `/tmp/k-data` and contains many records for `acme`, but
zero records for `acmee`. Stdout contains exactly one line:

```text
records=0
```

No `earliest=` line. No `latest=` line. Exit code is `0` (a tenant
with no records is a valid query result, not an error). Stderr is
empty. Priya's `grep ^records=0` script immediately tells her the
tenant has no data; the absence of `earliest=` and `latest=` lines is
the unambiguous signal that "min/max of an empty set" is correctly
reported as undefined rather than as some bogus sentinel string she
might accidentally parse as a real timestamp.

Same outcome (one line `records=0`) when the `data_dir` exists but has
never had any tenant ingested (`/tmp/empty-dir`), as long as the
Lumen WAL+snapshot at `lumen_base(data_dir)` can be opened (which the
`FileBackedLogStore::open` call handles transparently per its v1
behaviour).

### UAT Scenarios (BDD)

#### Scenario: Populated tenant — Priya sees count plus earliest plus latest in three lines

```text
Given Priya has pre-ingested 7 records for tenant acme into /tmp/k-data
And the records' observed_time_unix_nano values span 2026-05-18T00:00:00Z (earliest) to 2026-05-19T00:00:00Z (latest)
When Priya invokes `kaleidoscope_cli::stats` with tenant acme, data_dir /tmp/k-data, and a captured stdout sink
And the call returns Ok
Then the captured stdout contains exactly 3 non-empty lines, in order
And line 1 equals `records=7`
And line 2 equals `earliest=2026-05-18T00:00:00Z` (or the equivalent ISO 8601 UTC representation of the earliest record)
And line 3 equals `latest=2026-05-19T00:00:00Z` (or the equivalent ISO 8601 UTC representation of the latest record)
And the stdout ends with `\n`
```

#### Scenario: Empty tenant — Priya sees `records=0` and no timestamp lines

```text
Given the Lumen store at /tmp/k-data exists but contains zero records for tenant acmee
When Priya invokes `kaleidoscope_cli::stats` with tenant acmee, data_dir /tmp/k-data, and a captured stdout sink
And the call returns Ok
Then the captured stdout contains exactly 1 non-empty line
And that line equals `records=0`
And no line beginning with `earliest=` appears
And no line beginning with `latest=` appears
And the stdout ends with `\n`
```

#### Scenario: Single-record tenant — earliest equals latest

```text
Given Priya has pre-ingested exactly 1 record for tenant globex into /tmp/k-data
And the record's observed_time_unix_nano corresponds to 2026-05-11T00:00:00Z
When Priya invokes `kaleidoscope_cli::stats` with tenant globex, data_dir /tmp/k-data, and a captured stdout sink
And the call returns Ok
Then the captured stdout contains exactly 3 non-empty lines, in order
And line 1 equals `records=1`
And the `earliest=...` line and the `latest=...` line have the same timestamp value
And the stdout ends with `\n`
```

#### Scenario: Tenant isolation — stats for acme do not count globex records

```text
Given Priya has pre-ingested 7 records for tenant acme into /tmp/k-data
And Priya has separately pre-ingested 3 records for tenant globex into the same /tmp/k-data
When Priya invokes `kaleidoscope_cli::stats` with tenant acme, data_dir /tmp/k-data, and a captured stdout sink
And the call returns Ok
Then the line beginning with `records=` shows the count 7 (NOT 10)
And the `earliest=...` / `latest=...` lines reflect the time window of acme's records only (NOT the union with globex's records)
```

#### Scenario: Stats are consistent with `read` for the same tenant + data_dir

```text
Given Priya has pre-ingested N records for tenant acme into /tmp/k-data
When Priya invokes `kaleidoscope_cli::read` with tenant acme and a captured stdout sink (yielding count == N and N NDJSON lines on stdout)
And Priya then invokes `kaleidoscope_cli::stats` with the same tenant and data_dir and a separate captured stdout sink
Then the `records=...` line from `stats` shows the same N
And the earliest timestamp parsed out of the `stats` output equals the minimum `observed_time_unix_nano` across the N NDJSON records returned by `read`, rendered as ISO 8601 UTC
And the latest timestamp parsed out of the `stats` output equals the maximum `observed_time_unix_nano` across the same N records, rendered as ISO 8601 UTC
```

### Acceptance Criteria

- [ ] `kaleidoscope-cli stats <tenant_id> <data_dir>` is accepted by
  the binary's `main.rs` argument dispatcher and routes to a new
  `kaleidoscope_cli::stats(...)` library function (the binary
  dispatch arm uses the existing `parse_positional` helper at
  `crates/kaleidoscope-cli/src/main.rs:155-161`).
- [ ] When the tenant has N > 0 records under `data_dir`, the
  captured stdout contains exactly 3 non-empty lines: `records=N`,
  `earliest=<ISO 8601 UTC>`, `latest=<ISO 8601 UTC>`, in that order,
  terminated by `\n`.
- [ ] When the tenant has 0 records under `data_dir`, the captured
  stdout contains exactly 1 non-empty line: `records=0`, terminated
  by `\n`. No `earliest=` or `latest=` lines appear.
- [ ] The `records=N` count equals the count of records that
  `kaleidoscope_cli::read` returns for the same tenant and the same
  `data_dir` (consistency with the `read` subcommand for the same
  inputs).
- [ ] The `earliest=` value equals the ISO 8601 UTC representation of
  the minimum `observed_time_unix_nano` across the N records that the
  underlying `lumen.query(tenant, TimeRange::all())` call returns.
- [ ] The `latest=` value equals the ISO 8601 UTC representation of
  the maximum `observed_time_unix_nano` across the same N records.
- [ ] When N == 1, the `earliest=` and `latest=` lines have the
  identical timestamp value (degenerate single-record time window).
- [ ] When two tenants `acme` (7 records) and `globex` (3 records)
  coexist in one `data_dir`, `kaleidoscope_cli::stats(&acme, ...)`
  reports `records=7` and a time window covering only `acme`'s
  records, not 10 and not the union (tenant-isolation invariant).
- [ ] The library function `kaleidoscope_cli::stats` does not mutate
  the Lumen WAL or snapshot under `lumen_base(data_dir)` (read-only
  invariant — assertable by computing a checksum of the directory
  before and after, or by re-querying with `read` and observing
  identical output).
- [ ] No OTLP file is created at any path during the `stats` call
  (the function constructs a quiescent recorder identical to the one
  `read` uses in its no-flag arm; the `--observe-otlp` flag is NOT
  accepted by the `stats` subcommand in v0 per
  `wave-decisions.md` D3).
- [ ] `print_usage` in `crates/kaleidoscope-cli/src/main.rs`
  documents the `stats` subcommand alongside `ingest` and `read`,
  including the positional arguments and the key=value output shape.
- [ ] The existing test files `observe_otlp_flag.rs`,
  `observe_otlp_cinder_wiring.rs`, and `observe_otlp_read_flag.rs`
  continue to pass green under
  `cargo test --package kaleidoscope-cli` with no edits to their
  assertions (non-regression on the three prior `--observe-otlp`
  features).

### Outcome KPIs

- **Who**: platform operator (Priya), observed at the stdout byte
  level on a single `kaleidoscope-cli stats <tenant> <data_dir>`
  invocation.
- **Does what**: receives the tenant's record count plus, when N > 0,
  the earliest and latest record timestamps as ISO 8601 UTC strings,
  on three plain-text key=value stdout lines, in one CLI invocation
  that does not materialise the record set through any pipeline.
- **By how much**: 100% of `stats()` invocations against a populated
  tenant produce exactly 3 lines in order (records, earliest, latest)
  with the count equal to what `read` would return and the
  earliest/latest equal to the min/max `observed_time_unix_nano`
  across the same record set (OK1 + OK2); 100% of `stats()`
  invocations against an empty tenant produce exactly 1 line,
  `records=0`, with no timestamp lines (OK3).
- **Measured by**: new acceptance test
  `crates/kaleidoscope-cli/tests/stats_subcommand.rs` covering all
  three KPIs across the five UAT scenarios above.
- **Baseline**: 0% — today there is no `stats` subcommand at all;
  operators answer the same question by piping
  `kaleidoscope-cli read ...` through `wc -l`, `head -1`, `tail -1`
  with manual JSON parsing and nanoseconds-to-ISO 8601 conversion.

Maps to OK1-CLI-stats-record-count (principal),
OK2-CLI-stats-time-range, and OK3-CLI-stats-empty-tenant in
`outcome-kpis.md`.

### Technical Notes

- New library function: `kaleidoscope_cli::stats` in
  `crates/kaleidoscope-cli/src/lib.rs`. Likely signature (per
  `wave-decisions.md` D8; DESIGN locks the exact shape):
  `pub fn stats(tenant: &TenantId, data_dir: &Path, writer: impl
  Write) -> Result<(usize, Option<(SystemTime, SystemTime)>), Error>`.
  Reuses the existing `lumen_base(data_dir)` helper at
  `crates/kaleidoscope-cli/src/lib.rs:118-120`, the existing
  `LumenToPulseRecorder` quiescent recorder pattern at lines 275-279,
  the existing `Error` variants at lines 73-83 (specifically
  `LumenOpen` and `LumenQuery`), and the existing
  `FileBackedLogStore::open(...).query(tenant, TimeRange::all())`
  call shape.
- Modified file: `crates/kaleidoscope-cli/src/main.rs` — `match
  args.get(1).map(String::as_str)` (lines 48-60) gains a
  `Some("stats") => run_stats(&args)` arm; a new `run_stats` function
  parses the two positional args via `parse_positional` (line 155)
  and calls `stats(&tenant, &data_dir, io::stdout().lock())`. The
  `print_usage` helper (lines 71-97) gains a `stats` block.
- New test file:
  `crates/kaleidoscope-cli/tests/stats_subcommand.rs`. Mirrors the
  harness pattern from `observe_otlp_flag.rs` /
  `observe_otlp_read_flag.rs` (the `tenant`, `record`, `temp_root`,
  `cleanup` helpers are duplicated inline at v0 per
  `wave-decisions.md` D9; the rule-of-three extraction is a separate
  refactoring concern).
- Manifest: `crates/kaleidoscope-cli/Cargo.toml` gains a new
  `[[test]]` entry `name = "stats_subcommand", path =
  "tests/stats_subcommand.rs"`. The `lumen` and `aegis`
  dependencies needed by the test are already present.
- Timestamp formatting: per `wave-decisions.md` D6, ISO 8601 UTC
  with `Z` suffix and target nanosecond precision. DESIGN chooses
  the formatter (likely `chrono` or `time`; an existing workspace
  dependency is preferred). If nanosecond precision requires a new
  external dependency, DESIGN may downgrade to microsecond
  precision and document the choice as an ADR addendum — OK2 is
  robust to the downgrade as long as the min and max are preserved
  in order.
- Concurrency model: single thread per `stats()` call. The function
  drives exactly one `lumen.query(...)` call. No within-process
  concurrency to defend against; the Lumen recorder is quiescent
  (no file writes); no Cinder store is constructed.
- Slice tag: not `@infrastructure` — this story directly enables an
  operator-visible decision on a real CLI surface
  (`kaleidoscope-cli stats <tenant> <data_dir>`).

### Dependencies

- `lumen::LogStore::query(tenant, TimeRange::all())` already exists
  and returns `Result<Vec<LogRecord>, LogStoreError>`
  (`crates/lumen/src/store.rs:84`).
- `lumen::LogRecord::observed_time_unix_nano: u64` is the canonical
  sort key (`crates/lumen/src/record.rs:48`).
- `lumen::FileBackedLogStore` already implements `LogStore` and is
  already constructed by `read()` (`crates/kaleidoscope-cli/src/lib.rs:281-282`).
- `self_observe::LumenToPulseRecorder` is the quiescent recorder
  used by `read()`'s no-flag arm
  (`crates/kaleidoscope-cli/src/lib.rs:275-279`); already a
  `kaleidoscope-cli` dependency.
- `aegis::TenantId` already a dependency.
- An ISO 8601 timestamp formatter — `chrono` or `time` — is the only
  potential new external dependency. DESIGN chooses; the choice is
  recorded in `wave-decisions.md` D6.
- No new internal crate dependencies.

### Slice

`slices/slice-01-stats-subcommand-emits-record-count-and-time-range.md`
