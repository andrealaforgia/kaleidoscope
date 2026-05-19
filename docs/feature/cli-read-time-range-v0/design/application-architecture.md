# Application Architecture — `cli-read-time-range-v0`

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-19.
Mode: PROPOSE.

**The architectural question**: the `kaleidoscope-cli read`
subcommand today always queries `lumen.query(tenant, TimeRange::all())`
at `crates/kaleidoscope-cli/src/lib.rs:284`. How does the operator
drive an arbitrary `TimeRange::new(s, e)` into that call site via
two optional CLI flags `--since` and `--until` that take ISO 8601 UTC
strings, WITHOUT breaking the locked OK2 tests (`observe_otlp_read_flag.rs`,
`observe_otlp_flag.rs`) and WITHOUT pulling in `chrono` / `time` /
`jiff`?

**The decision**: extend `read()` from 4 args to 5 by appending an
explicit `range: TimeRange` parameter (DD1); add a private library
function `parse_iso8601_utc_nanos` next to its inverse
`format_iso8601_utc_nanos` plus a private library helper
`days_from_civil` next to its inverse `civil_from_days` (DD2);
add a binary-side `parse_time_range` helper that does the argv
scan and constructs the stderr error message naming the offending
flag (DD2). The parser accepts the two shapes `YYYY-MM-DDTHH:MM:SSZ`
and `YYYY-MM-DDTHH:MM:SS.D..DZ` with 1..=9 fractional digits;
calendar validation rejects malformed dates at the parser boundary
(DD3). No new external crate; the no-`chrono`-no-`time` posture
inherited from `cli-stats-subcommand-v0` DD1 is preserved. Full
rationale and the Reuse Analysis in `design/wave-decisions.md`.

## C4 — System Context (Level 1)

```mermaid
C4Context
  title System Context — cli-read-time-range v0
  Person(operator, "Priya the platform operator", "Runs kaleidoscope-cli read with --since and --until to obtain a tenant's records for a specific time window without streaming the full tenant dump.")
  System(cli, "kaleidoscope-cli", "Operator CLI for Lumen v1 + Cinder v1. read subcommand gains two optional flags --since and --until accepting ISO 8601 UTC timestamps; values are parsed to u64 nanos-since-Unix-epoch and threaded into lumen::TimeRange::new(s, e). Half-open [since, until) semantics inherited from lumen::TimeRange. AGPL-3.0-or-later.")
  System_Ext(filesystem, "POSIX filesystem", "Hosts Lumen WAL+snapshot under <data_dir>/lumen.*. read is a pure read on Lumen; this feature does not touch Cinder.")
  System_Ext(unix_tools, "Unix text tools (jq, grep)", "Consume the NDJSON stdout via shell pipelines. Replaces the pre-feature workaround of streaming the full tenant dump and filtering client-side via jq on observed_time_unix_nano.")

  Rel(operator, cli, "Invokes `read <tenant> <data_dir> [--since X] [--until Y]` at, capturing NDJSON stdout from")
  Rel(cli, filesystem, "Opens Lumen WAL+snapshot read-only and calls query(tenant, TimeRange::new(s, e)) once on")
  Rel(operator, unix_tools, "Optionally pipes NDJSON stdout through jq for downstream transformation on")
```

The change is confined to the `kaleidoscope-cli` node. Before this
feature, Priya answered the bounded-window question via
`read <tenant> <data_dir> | jq 'select(.observed_time_unix_nano >= NNN
and .observed_time_unix_nano < MMM)'`, hand-converting the ISO 8601
window edges into nanosecond literals. After it, the same answer
falls out of the existing `read` invocation directly from the
storage layer.

## C4 — Container View (Level 2)

```mermaid
C4Container
  title Container Diagram — cli-read-time-range v0
  Person(operator, "Priya the platform operator")
  Container_Boundary(cli, "kaleidoscope-cli crate") {
    Container(main, "main.rs (binary)", "Rust, src/main.rs", "run_read_with gains one new line that calls parse_time_range(args)? before invoking read(..); the parsed TimeRange is threaded into read() as the new 5th parameter. write_usage gains documentation lines for --since and --until on the read subcommand.")
    Container(parse_time_range, "parse_time_range helper (new)", "Rust, src/main.rs (new, ~25 lines)", "Single-pass argv scan mirroring parse_observe_otlp's shape. Pulls --since and --until values if present; calls parse_iso8601_utc_nanos on each; constructs stderr error message naming the offending flag (--since or --until) and the verbatim bad value on parse failure. Returns TimeRange with 0 / u64::MAX defaults for absent flags.")
    Container(read_fn, "read function", "Rust, src/lib.rs (extended)", "Signature gains 5th param: range: TimeRange. The lumen.query(tenant, TimeRange::all()) call site becomes lumen.query(tenant, range). No body restructuring on the I/O side. Existing otlp_log_path parameter unchanged.")
    Container(parser, "parse_iso8601_utc_nanos (new, private)", "Rust, src/lib.rs (new, ~50 lines)", "Inverse of format_iso8601_utc_nanos. Accepts YYYY-MM-DDTHH:MM:SSZ and YYYY-MM-DDTHH:MM:SS.D..DZ (1..=9 fractional digits). Calendar validation rejects malformed dates at the parser boundary. Returns Result<u64, IsoParseError>; the typed error does NOT carry flag-name context (DD2). Mutation-killing unit tests cohabit with the formatter's tests at lib.rs:457-651.")
    Container(days_from_civil, "days_from_civil (new, private)", "Rust, src/lib.rs (new, ~15 lines)", "Inverse of civil_from_days. Public-domain Howard Hinnant. Round-trip property: days_from_civil(civil_from_days(z)) == z.")
    Container(format_iso, "format_iso8601_utc_nanos (unchanged)", "Rust, src/lib.rs", "Pre-existing hand-rolled formatter. Reused unchanged as the parser's round-trip oracle (parse(format(ns)) == ns AC).")
    Container(civil_from_days_existing, "civil_from_days (unchanged)", "Rust, src/lib.rs", "Pre-existing Hinnant helper. Reused unchanged. Cohabits with its new inverse.")
  }
  Container_Boundary(stores, "Storage adapters") {
    Container(lumen_store, "FileBackedLogStore", "Rust, lumen crate", "Honours LogStore port: per-tenant isolation, observed-time ascending order. query(tenant, TimeRange::new(s, e)) returns ascending vector filtered by the half-open [s, e) interval. TimeRange::contains at lumen/src/record.rs:116-119 IS the half-open semantics.")
  }
  ContainerDb(lumen_files, "<data_dir>/lumen.*", "POSIX files, read-only access", "Lumen v1 WAL + snapshot. Opened read-only by read().")

  Rel(operator, main, "Invokes `read <tenant> <data_dir> [--since X] [--until Y]` at")
  Rel(main, parse_time_range, "Calls before invoking read(), passing argv slice")
  Rel(parse_time_range, parser, "Calls twice (one per supplied flag) on")
  Rel(parser, days_from_civil, "Converts parsed (year, month, day) into signed day count via")
  Rel(parser, format_iso, "Is the inverse of (round-trip AC oracle)")
  Rel(days_from_civil, civil_from_days_existing, "Is the inverse of (round-trip property)")
  Rel(main, read_fn, "Threads parsed TimeRange (or TimeRange::all() when no flags) into 5th parameter of")
  Rel(read_fn, lumen_store, "Calls query(tenant, range) once on")
  Rel(lumen_store, lumen_files, "Reads WAL + snapshot from")
  Rel(read_fn, operator, "Writes NDJSON records matching range to the supplied writer back to (via stdout)")
```

The parser pair (`parse_iso8601_utc_nanos`, `days_from_civil`)
cohabits in `lib.rs` next to its already-shipped inverse pair so
the round-trip property AC is a single-file local check.
`parse_time_range` is the binary wrapper that adds CLI-flag-name
context to the parser's typed error (same split shape as
`parse_observe_otlp` + `Option<&Path>` on `read()`). The `read()`
body changes by one token: `TimeRange::all()` → `range`.

## C4 — Component View (Level 3)

**Not produced.** The parser body is a fixed-position digit-walker
+ calendar validation + `days_from_civil` call + fractional-digit
left-pad; the `read()` body change is one token; `run_read_with` is
one line. L3 reification conditions: (a) parser extended to accept
non-`Z` offsets (DD3 forward-compat hook); (b) `IsoParseError`
escapes private surface (future library caller wants typed errors);
(c) `parse_time_range` generalised across multiple flag pairs.
