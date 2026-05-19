<!-- markdownlint-disable MD024 -->

# User Stories — `cli-read-time-range-v0`

## System Constraints (apply to every story)

- Rust idiomatic per `CLAUDE.md`: data + free functions + traits where
  polymorphism is genuinely needed. The data type at the trait-port
  boundary is `lumen::TimeRange` (`crates/lumen/src/record.rs:97-120`);
  this feature changes the runtime construction site of the `TimeRange`
  passed to `LogStore::query` inside `kaleidoscope_cli::read`, not the
  trait itself.
- License: AGPL-3.0-or-later, matching the rest of the workspace.
- The acceptance idiom for this project is Rust `#[test]` functions with
  `// Given / // When / // Then` comment blocks, not Gherkin `.feature`
  files. The Given/When/Then text in the UAT Scenarios sections below is
  the specification; DISTILL translates it into `#[test]` functions in
  `crates/kaleidoscope-cli/tests/read_time_range.rs` mirroring the pattern
  already in `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs`.
- Half-open interval contract: `lumen::TimeRange` is `[start, end)` —
  closed-lower, open-upper. `contains(t)` returns `true` iff
  `start_unix_nano <= t < end_unix_nano`
  (`crates/lumen/src/record.rs:116-119`). The new `--since X --until Y`
  flag pair MUST honour this same half-open convention so the CLI
  surface matches the underlying storage semantics exactly. Records
  with `observed_time_unix_nano == until_ns` are EXCLUDED from output.
  Records with `observed_time_unix_nano == since_ns` are INCLUDED in
  output.
- Half-bounded contract: when only `--since X` is set, the implicit
  upper bound is `u64::MAX` (interval `[X, u64::MAX)`, mirroring the
  `TimeRange::all()` upper bound at `crates/lumen/src/record.rs:111-114`).
  When only `--until Y` is set, the implicit lower bound is `0`
  (interval `[0, Y)`, mirroring `TimeRange::all()`'s lower bound).
  When neither flag is set, the constructed range is exactly
  `TimeRange::all()` and the call site is byte-equivalent to today.
- ISO 8601 input contract: each flag value MUST parse as an ISO 8601
  UTC timestamp with the exact shape `YYYY-MM-DDTHH:MM:SSZ` or
  `YYYY-MM-DDTHH:MM:SS.NNNNNNNNNZ` (any 1..=9 fractional-second digits
  accepted; the `Z` suffix is the only timezone form accepted). This
  is the inverse of the `stats` subcommand's `format_iso8601_utc_nanos`
  output (`crates/kaleidoscope-cli/src/lib.rs:410-420`); a value
  produced by the formatter MUST round-trip through the parser. The
  parser converts the parsed value into a `u64` nanoseconds-since-Unix-
  epoch boundary suitable for `TimeRange::new(since_ns, until_ns)`.
- Invalid-input contract: when either flag's value fails to parse as a
  conformant ISO 8601 timestamp, the CLI exits with a non-zero exit
  code and writes to stderr a message naming WHICH flag carried the
  bad value (`--since` or `--until`) and the offending input
  verbatim. No Lumen store is opened in that case; no `read()` call is
  made; no records are written to stdout. Exit code is 1 (mirrors the
  existing `eprintln!("kaleidoscope-cli: {e}"); ExitCode::FAILURE`
  pattern at `crates/kaleidoscope-cli/src/main.rs:65-68`).
- No-flag non-regression: every assertion in the existing locked test
  files `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` and
  `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` MUST continue
  to pass byte-equivalently after this feature ships. Those tests
  exercise `read()` with the default (no time-range) shape; the new
  optional flags must default to `TimeRange::all()` so the existing
  call sites are unaffected. This is OK2.
- Flag interaction with `--observe-otlp`: the `--observe-otlp <path>`
  flag (shipped at commit `3af7e82` and extended in
  `cli-read-observe-otlp-v0`) remains independently usable on `read`.
  Composition with `--since` / `--until` is out of scope for this
  feature's acceptance test (D6 in `wave-decisions.md`); the
  `--observe-otlp` parsing helper at
  `crates/kaleidoscope-cli/src/main.rs:130-144` is order-independent and
  must remain so. The new time-range parsing helpers MUST share the
  same order-independent parsing posture so all three flags can appear
  in any order after the positional arguments.
- Scope is the `read` subcommand only. The `ingest` and `stats`
  subcommands are NOT touched. The `lumen::TimeRange` data type is
  NOT modified.

---

## US-01: Operator queries a tenant's records for a bounded time window

### Elevator Pitch

- **Before**: Priya wants yesterday's records for tenant `acme` during
  the incident window (`2026-05-18T00:00:00Z` to `2026-05-19T00:00:00Z`).
  The `read` subcommand only supports the full-tenant dump today
  (`crates/kaleidoscope-cli/src/lib.rs:283-285` always calls
  `lumen.query(tenant, TimeRange::all())`); for a populated tenant
  she has to stream the full ten gigabytes of NDJSON across stdout and
  pipe through `jq 'select(.observed_time_unix_nano >= 1747526400000000000
  and .observed_time_unix_nano < 1747612800000000000)'`, hand-converting
  the ISO 8601 window boundaries into nanoseconds. The query is dog-slow
  (stream all ten gigabytes, throw away >99% client-side), brittle (a
  typo in the nanosecond literal silently returns the wrong slice), and
  cognitively expensive at the worst possible moment (incident response
  with the on-call rotation watching).
- **After**: Priya runs:
  `kaleidoscope-cli read acme /tmp/data --since 2026-05-18T00:00:00Z --until 2026-05-19T00:00:00Z`.
  Stdout contains ONLY the records whose `observed_time_unix_nano` is
  in the half-open interval `[1747526400000000000, 1747612800000000000)`
  — yesterday's slice, computed by the storage layer's `TimeRange`
  query, not by client-side filtering of the full dump. The format on
  stdout is unchanged (NDJSON, one `lumen::LogRecord` per line,
  terminated by `\n`). She can also use either flag in isolation
  (`--since 2026-05-19T15:30:00Z` for "everything from the last 90
  minutes", `--until 2026-05-18T00:00:00Z` for "everything before
  yesterday"); when she omits both, behaviour is byte-equivalent to
  today (the existing
  `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` test still
  passes). A typo on either flag (e.g. `--since yesterday`) exits
  non-zero with a stderr message naming the flag and the bad value, so
  she gets corrected immediately instead of receiving an empty slice
  silently.
- **Decision enabled**: "What did `acme` write between 14:00 and 14:30
  UTC, in the half hour before the latency spike at 14:30?" — answered
  directly from the storage layer with one CLI invocation, without a
  multi-gigabyte stream-and-filter detour. Also: "Replay yesterday's
  slice through the test pipeline" — answered by a single CLI invocation
  whose output is a faithful subset of production for that window.

### Problem

Priya the platform operator runs a multi-tenant Kaleidoscope
deployment. When an incident hits — say, a latency spike at 14:30 UTC
— her first questions are "what did `acme` write in the 30 minutes
before the spike?" and "what did `acme` write during the 5 minutes the
spike lasted?". Both are time-bounded queries.

Today the `read` subcommand cannot answer either question directly.
The library function `kaleidoscope_cli::read` at
`crates/kaleidoscope-cli/src/lib.rs:261-294` always calls
`lumen.query(tenant, TimeRange::all())` (line 284), which returns every
record the tenant has ever ingested. For a tenant with ten gigabytes
of NDJSON, the only ways to extract the incident window are:

1. Stream the full dump to stdout and pipe through `jq` filtering on
   `observed_time_unix_nano`, hand-converting ISO 8601 window edges
   into nanosecond literals. Slow (full-table scan over the wire),
   error-prone (one wrong digit in the nanosecond literal returns the
   wrong slice silently), and cognitively expensive during incident
   response.
2. Write a one-off Rust binary that constructs `FileBackedLogStore`
   directly and calls `query` with a hand-built `TimeRange`. Not a
   production option for routine use; not something the on-call rota
   has the time to do mid-incident.

Priya finds it operationally hostile to need either workaround when
the underlying storage layer already supports `TimeRange::new(start,
end)` as a first-class query parameter
(`crates/lumen/src/record.rs:103-115`). The gap is purely at the CLI
surface: the `read` library function does not expose `TimeRange` as a
caller-controllable parameter, and the binary's `run_read` dispatcher
(`crates/kaleidoscope-cli/src/main.rs:146-165`) does not parse any
time-range flags.

### Who

Priya the platform operator | runs a multi-tenant Kaleidoscope
deployment for a fintech | already uses `kaleidoscope-cli read
<tenant> <data_dir>` for full-tenant dumps and
`kaleidoscope-cli read <tenant> <data_dir> --observe-otlp <path>`
for ingest-side observability | now needs time-bounded queries to
answer per-incident "what arrived in this window?" questions
without streaming the full tenant dump | thinks in ISO 8601 UTC (her
dashboards, alerting, and runbooks all render times that way; the
project's own `stats` subcommand emits ISO 8601 UTC with
nanosecond precision per `crates/kaleidoscope-cli/src/lib.rs:410-420`).

### Solution

Add two new optional CLI flags to `kaleidoscope-cli read`:

```text
kaleidoscope-cli read <tenant_id> <data_dir>
  [--since <ISO 8601 UTC>] [--until <ISO 8601 UTC>] [--observe-otlp <path>]
```

When `--since X --until Y` are both set, the library `read()` function
calls `lumen.query(tenant, TimeRange::new(parse(X), parse(Y)))` instead
of `lumen.query(tenant, TimeRange::all())`. When only `--since X` is
set, the call becomes `TimeRange::new(parse(X), u64::MAX)`. When only
`--until Y` is set, the call becomes `TimeRange::new(0, parse(Y))`.
When neither flag is set, the call is `TimeRange::all()` — the existing
shape, byte-equivalent to today.

The parser converts an ISO 8601 UTC timestamp of shape
`YYYY-MM-DDTHH:MM:SS[.NNNNNNNNN]Z` into a `u64` nanoseconds-since-Unix-
epoch value. It is the inverse of the `stats` subcommand's
hand-rolled `format_iso8601_utc_nanos` formatter (D5 in
`wave-decisions.md` confirms hand-rolled, no `chrono`/`time` dep).
Invalid input (any deviation from the accepted shape) → stderr error
message naming the offending flag (`--since` or `--until`) and the
verbatim bad value → CLI exits with code 1.

The new flags are order-independent with each other and with
`--observe-otlp`, mirroring the existing `parse_observe_otlp` pattern
at `crates/kaleidoscope-cli/src/main.rs:130-144`.

### Domain Examples

#### 1. Yesterday's incident window — both flags set

Priya investigates a latency spike that started at `2026-05-18T14:30:00Z`.
She wants `acme`'s records for the half-hour before the spike. She
runs:

```text
kaleidoscope-cli read acme /tmp/data \
  --since 2026-05-18T14:00:00Z \
  --until 2026-05-18T14:30:00Z
```

`--since 2026-05-18T14:00:00Z` parses to `since_ns = 1_747_578_000_000_000_000`
(1747578000 seconds since Unix epoch × 1e9). `--until 2026-05-18T14:30:00Z`
parses to `until_ns = 1_747_579_800_000_000_000`. The library call
becomes `lumen.query(&acme, TimeRange::new(1_747_578_000_000_000_000,
1_747_579_800_000_000_000))`. The returned `Vec<LogRecord>` contains
exactly the records whose `observed_time_unix_nano` is in the
half-open interval `[1_747_578_000_000_000_000, 1_747_579_800_000_000_000)`.
Stdout receives each such record as one NDJSON line, terminated by
`\n`. A record with `observed_time_unix_nano == 1_747_579_800_000_000_000`
(exactly the `until` boundary) is EXCLUDED — that's the half-open
contract from `crates/lumen/src/record.rs:116-119`. Stderr receives
the existing `read ok: records={count}` line where `{count}` equals
the number of matched records (mirrors the existing `run_read_with`
contract at `crates/kaleidoscope-cli/src/main.rs:155-165`).

#### 2. Last 90 minutes — only `--since` set

At `2026-05-19T17:00:00Z`, Priya wants every record `acme` produced
since `15:30Z` today. She runs:

```text
kaleidoscope-cli read acme /tmp/data --since 2026-05-19T15:30:00Z
```

`--since 2026-05-19T15:30:00Z` parses to
`since_ns = 1_747_668_600_000_000_000`. `--until` is absent, so the
upper bound defaults to `u64::MAX`. The call is
`lumen.query(&acme, TimeRange::new(1_747_668_600_000_000_000, u64::MAX))`,
which mirrors `TimeRange::all()`'s upper bound
(`crates/lumen/src/record.rs:111-114`). Stdout receives every record
with `observed_time_unix_nano >= 1_747_668_600_000_000_000`, as NDJSON.

Symmetric example for `--until` only: at `2026-05-19T08:00:00Z` Priya
wants everything `acme` produced BEFORE yesterday started. She runs
`kaleidoscope-cli read acme /tmp/data --until 2026-05-18T00:00:00Z`.
`since` defaults to `0`. The call is
`lumen.query(&acme, TimeRange::new(0, 1_747_526_400_000_000_000))`.
Stdout receives every record with `observed_time_unix_nano <
1_747_526_400_000_000_000`.

#### 3. Typo on `--since` — fail-fast with named flag

Mid-incident, Priya muscle-memories the wrong format:

```text
kaleidoscope-cli read acme /tmp/data --since yesterday --until 2026-05-19T00:00:00Z
```

The `--since` value `yesterday` fails the ISO 8601 shape check (no
`T` separator, no `Z` suffix, not digits in the expected slots). The
CLI:

1. Does NOT open the Lumen store.
2. Does NOT call `read()`.
3. Writes NOTHING to stdout.
4. Writes to stderr: `kaleidoscope-cli: invalid --since value
   "yesterday": expected ISO 8601 UTC (YYYY-MM-DDTHH:MM:SS[.NNNNNNNNN]Z)`.
5. Exits with code 1 (`ExitCode::FAILURE`).

Symmetric behaviour applies to `--until`: if she runs
`kaleidoscope-cli read acme /tmp/data --until 2026-13-32T25:99:99Z`,
the message names `--until` and the verbatim bad value
(`2026-13-32T25:99:99Z`), and the exit code is 1. Priya sees the
mistake immediately, corrects the value, and reruns — no silent empty
slice, no time wasted searching for missing records that never existed.

### UAT Scenarios (BDD)

#### Scenario: Bounded window query returns only records in `[since, until)`

```text
Given Priya has pre-ingested 5 records for tenant acme into /tmp/data
  with observed_time_unix_nano values 100, 200, 300, 400, 500 (in nanoseconds)
And the CLI accepts the equivalent `--since` and `--until` ISO 8601 values
  that parse to 200 and 400 respectively
When Priya invokes `kaleidoscope_cli::read` with
  `time_range = TimeRange::new(200, 400)` and a captured stdout sink
And the call returns Ok with `count == 2`
Then the captured stdout bytes equal the records with
  observed_time_unix_nano in {200, 300} re-serialised as NDJSON,
  one per line, terminated by `\n`
And the record with observed_time_unix_nano == 400 is EXCLUDED
  (half-open upper bound, per `crates/lumen/src/record.rs:116-119`)
And the record with observed_time_unix_nano == 200 is INCLUDED
  (closed lower bound)
```

#### Scenario: No flags is byte-equivalent to today's full-tenant dump

```text
Given Priya has pre-ingested N records for tenant acme into /tmp/data
When Priya invokes `kaleidoscope_cli::read` with `time_range = TimeRange::all()`
  and a captured stdout sink (the no-flag default)
And the call returns Ok with `count == N`
Then the captured stdout bytes equal all N records re-serialised as NDJSON,
  one per line, terminated by `\n`
And the bytes are byte-equivalent to the bytes the pre-feature `read()`
  function produces for the same inputs
And every assertion in `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs`
  continues to pass green when that test calls the new shape with the
  no-flag default
```

#### Scenario: `--since` alone uses `u64::MAX` as the implicit upper bound

```text
Given Priya has pre-ingested 4 records for tenant acme into /tmp/data
  with observed_time_unix_nano values 100, 200, 300, 400
When Priya invokes `kaleidoscope_cli::read` with
  `time_range = TimeRange::new(250, u64::MAX)` and a captured stdout sink
And the call returns Ok with `count == 2`
Then the captured stdout bytes equal the records with
  observed_time_unix_nano in {300, 400} re-serialised as NDJSON,
  one per line, terminated by `\n`
And the call exhibits the same shape as `TimeRange::all()` on its
  upper end (the records with observed_time_unix_nano up to the
  highest ingested value are included)
```

#### Scenario: `--until` alone uses `0` as the implicit lower bound

```text
Given Priya has pre-ingested 4 records for tenant acme into /tmp/data
  with observed_time_unix_nano values 100, 200, 300, 400
When Priya invokes `kaleidoscope_cli::read` with
  `time_range = TimeRange::new(0, 250)` and a captured stdout sink
And the call returns Ok with `count == 2`
Then the captured stdout bytes equal the records with
  observed_time_unix_nano in {100, 200} re-serialised as NDJSON,
  one per line, terminated by `\n`
And the call exhibits the same shape as `TimeRange::all()` on its
  lower end (the records with observed_time_unix_nano starting from
  the earliest ingested value are included)
```

#### Scenario: Invalid ISO 8601 on `--since` fails fast with stderr message

```text
Given Priya invokes the CLI with the argv list
  ["kaleidoscope-cli", "read", "acme", "/tmp/data", "--since", "yesterday"]
When the binary's `run_read` dispatcher attempts to parse the `--since` value
Then the dispatcher returns Err BEFORE opening the Lumen store
And stderr contains a message naming `--since` and the verbatim bad
  value `yesterday`
And stdout receives no bytes
And the process exits with code 1 (`ExitCode::FAILURE`)
```

#### Scenario: Invalid ISO 8601 on `--until` fails fast with stderr message

```text
Given Priya invokes the CLI with the argv list
  ["kaleidoscope-cli", "read", "acme", "/tmp/data",
   "--since", "2026-05-18T00:00:00Z", "--until", "2026-13-32T25:99:99Z"]
When the binary's `run_read` dispatcher attempts to parse the `--until` value
Then the dispatcher returns Err BEFORE opening the Lumen store
And stderr contains a message naming `--until` and the verbatim bad
  value `2026-13-32T25:99:99Z`
And stdout receives no bytes
And the process exits with code 1 (`ExitCode::FAILURE`)
```

#### Scenario: Existing locked tests continue to pass byte-equivalently

```text
Given the existing test files
  `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` and
  `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` are unmodified
When `cargo test --package kaleidoscope-cli` runs after this feature ships
Then all tests in both files pass green
And no assertion in either file is edited
```

### Acceptance Criteria

- [ ] The library function `kaleidoscope_cli::read` accepts a way for
      its caller to pass a `lumen::TimeRange` other than
      `TimeRange::all()`. The exact signature shape (a new parameter,
      a builder, an overload) is DESIGN's choice; the observable
      property is that the caller can drive any `TimeRange::new(s, e)`
      into the underlying `lumen.query` call.
- [ ] When the caller supplies `TimeRange::new(since_ns, until_ns)`,
      stdout contains exactly the records whose
      `observed_time_unix_nano` is in the half-open interval
      `[since_ns, until_ns)`, serialised as NDJSON (one record per
      line, terminated by `\n`). Records at exactly `until_ns` are
      excluded; records at exactly `since_ns` are included.
- [ ] When the caller supplies `TimeRange::all()` (the no-flag default),
      stdout bytes are byte-equivalent to what the pre-feature `read()`
      function produces, and the call exhibits the same return-value
      shape (`count: usize` equals the number of records the query
      matched).
- [ ] The binary's `run_read` dispatcher in
      `crates/kaleidoscope-cli/src/main.rs` parses `--since <value>`
      and `--until <value>` flags from the argv list. The two flags
      are order-independent with each other and with the existing
      `--observe-otlp` flag.
- [ ] When `--since` is absent the implicit lower bound is `0`. When
      `--until` is absent the implicit upper bound is `u64::MAX`. When
      both are absent the constructed `TimeRange` is `TimeRange::all()`
      and the call is structurally equivalent to today's call site at
      `crates/kaleidoscope-cli/src/lib.rs:283-285`.
- [ ] The `--since` and `--until` flag values are parsed as ISO 8601
      UTC timestamps of shape `YYYY-MM-DDTHH:MM:SSZ` or
      `YYYY-MM-DDTHH:MM:SS.NNNNNNNNNZ` (1..=9 fractional-second digits
      accepted; `Z` suffix is the only timezone form accepted).
- [ ] A timestamp produced by the `stats` subcommand's
      `format_iso8601_utc_nanos` formatter
      (`crates/kaleidoscope-cli/src/lib.rs:410-420`) round-trips
      through the parser: parse(format(ns)) == ns for every ns within
      the formatter's valid range. (Round-trip property AC; the slice
      file may discharge it via a property test or a small witness
      table.)
- [ ] When `--since`'s value fails to parse as a conformant ISO 8601
      timestamp, the binary writes a stderr message containing both
      the literal string `--since` and the verbatim bad value, does
      NOT open the Lumen store, does NOT write any bytes to stdout,
      and exits with code 1 (`ExitCode::FAILURE`).
- [ ] When `--until`'s value fails to parse as a conformant ISO 8601
      timestamp, the binary writes a stderr message containing both
      the literal string `--until` and the verbatim bad value, does
      NOT open the Lumen store, does NOT write any bytes to stdout,
      and exits with code 1 (`ExitCode::FAILURE`).
- [ ] `print_usage` in `crates/kaleidoscope-cli/src/main.rs` documents
      `--since <ISO 8601 UTC>` and `--until <ISO 8601 UTC>` on the
      `read` subcommand, mentioning the half-open `[since, until)`
      contract and that omitting either flag leaves that boundary
      unbounded.
- [ ] The existing test files
      `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` and
      `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` continue
      to pass green under `cargo test --package kaleidoscope-cli`
      with no edits to their assertions.

### Outcome KPIs

- **Who**: platform operator (Priya), observed at the byte level on
  stdout from `kaleidoscope-cli read`
- **Does what**: receives ONLY the records whose
  `observed_time_unix_nano` lies in the half-open `[since, until)`
  interval she specified via `--since` / `--until`, instead of the
  full-tenant dump
- **By how much**: 100% of records on stdout satisfy
  `since_ns <= observed_time_unix_nano < until_ns` when both flags
  set (OK1); 0% of records outside that interval appear on stdout;
  100% byte equivalence of stdout when neither flag set (OK2);
  half-bounded queries return the expected unbounded-side records
  (OK3); 100% of typo'd flag values exit non-zero with the offending
  flag name in stderr (OK4)
- **Measured by**: new acceptance test
  `crates/kaleidoscope-cli/tests/read_time_range.rs` (OK1, OK2, OK3,
  OK4) and existing locked tests
  `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` and
  `observe_otlp_flag.rs` (non-regression)
- **Baseline**: 0% — today the only way to obtain a time-bounded
  slice is to stream the full tenant dump (potentially ten gigabytes)
  and filter client-side via `jq`

Maps to OK1-bounded-window-filter, OK2-no-flag-byte-equivalent,
OK3-half-bounded-supported, and OK4-invalid-iso8601-fails-fast in
`outcome-kpis.md`.

### Technical Notes

- Modified file: `crates/kaleidoscope-cli/src/lib.rs` — the `read`
  function exposes a `TimeRange`-equivalent control to its caller
  (signature shape is DESIGN's choice; the property is that the
  caller can drive `TimeRange::new(s, e)` into the `lumen.query`
  call at line 284). The no-flag default MUST construct
  `TimeRange::all()` so existing call sites are byte-equivalent.
- Modified file: `crates/kaleidoscope-cli/src/main.rs` — `run_read`
  (lines 146-165) gains parsing of `--since <value>` and
  `--until <value>` flags via helpers structurally similar to
  `parse_observe_otlp` at lines 130-144 (order-independent, single-pass
  argv scan). The parsed values are converted to `u64` nanoseconds
  via a new ISO 8601 UTC parser (hand-rolled per D5 in
  `wave-decisions.md`). `print_usage` (lines 78-109) is updated to
  document both new flags on the `read` subcommand.
- New file: `crates/kaleidoscope-cli/tests/read_time_range.rs`.
  Mirrors the harness pattern from
  `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` (the
  `tenant`, `record`, `temp_root`, `cleanup`, `ndjson` helpers are
  duplicated inline at v0; rule-of-three extraction deferred per
  D7 in `wave-decisions.md`).
- Manifest: `crates/kaleidoscope-cli/Cargo.toml` gains a new
  `[[test]]` entry `name = "read_time_range", path =
  "tests/read_time_range.rs"`. No new external crate dependency
  required (the parser is hand-rolled per D5).
- DO NOT modify the existing test files
  `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` and
  `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` — they are
  locked OK2 protection.
- Lumen `TimeRange` semantics: `[start, end)` half-open closed-lower
  open-upper (`crates/lumen/src/record.rs:97-119`); `TimeRange::all()`
  is `[0, u64::MAX)`. The new flag pair MUST honour this convention
  exactly so the CLI surface matches the storage semantics.
- ISO 8601 input parser: hand-rolled per D5 in `wave-decisions.md`
  (inverse of `format_iso8601_utc_nanos` at
  `crates/kaleidoscope-cli/src/lib.rs:410-420`; no `chrono` / `time`
  dep). The parser accepts `YYYY-MM-DDTHH:MM:SSZ` and
  `YYYY-MM-DDTHH:MM:SS.NNNNNNNNNZ` (1..=9 fractional-second digits);
  any deviation (missing `T`, missing `Z`, non-digit in a digit slot,
  out-of-range month/day/hour/minute/second) → parse error → CLI
  fails fast per OK4. The `civil_from_days` algorithm at
  `crates/kaleidoscope-cli/src/lib.rs:426-438` (Howard Hinnant
  public-domain) has an inverse (`days_from_civil`) that the parser
  reuses to convert `(year, month, day)` into a signed day count.
- Composition with `--observe-otlp` is OUT of scope for this
  feature's acceptance test (D6 in `wave-decisions.md`). The
  `--observe-otlp` flag remains independently usable on `read`; if
  combined with `--since` / `--until`, the behaviour is the natural
  union (the OTLP file receives one `lumen.query.count` line whose
  `asInt` equals the matched count under the supplied `TimeRange`),
  but a dedicated composition test is deferred.
- Slice tag: not `@infrastructure` — this story directly enables an
  operator-visible decision on a real CLI surface
  (`kaleidoscope-cli read <tenant> <data_dir> --since X --until Y`).

### Dependencies

- `lumen::TimeRange` already exists with the correct `[start, end)`
  semantics (`crates/lumen/src/record.rs:97-120`). No change to the
  `lumen` crate is required.
- `kaleidoscope_cli::read` already opens the Lumen store and calls
  `query` with a `TimeRange` at
  `crates/kaleidoscope-cli/src/lib.rs:283-285`. The change is at the
  TimeRange-construction site.
- The hand-rolled `format_iso8601_utc_nanos` formatter (and its
  `civil_from_days` helper) at
  `crates/kaleidoscope-cli/src/lib.rs:410-438` is the precedent for
  the hand-rolled parser direction.
- `aegis::TenantId` (already a `kaleidoscope-cli` dependency).
- `serde_json` (already a dev-dependency on `kaleidoscope-cli`).
- No new external crates required.

### Slice

`slices/slice-01-read-time-range-filter.md`
