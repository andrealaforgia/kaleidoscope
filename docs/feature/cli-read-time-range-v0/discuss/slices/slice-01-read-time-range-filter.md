# Slice 01 — `read` filters by `--since` / `--until` time window

## Goal

Add two new optional flags `--since <ISO 8601 UTC>` and
`--until <ISO 8601 UTC>` to the `kaleidoscope-cli read` subcommand
so that the underlying `lumen.query(tenant, TimeRange)` call uses a
half-open `[since_ns, until_ns)` interval derived from the flag
values, instead of the hard-coded `TimeRange::all()` at
`crates/kaleidoscope-cli/src/lib.rs:283-285`.

## Stories included

- US-01 (single story; all DoR-validated AC inside `user-stories.md`)

## Acceptance shape

New acceptance test file:
`crates/kaleidoscope-cli/tests/read_time_range.rs`. Six test
functions, one per UAT scenario in `user-stories.md`:

1. `bounded_window_returns_only_records_in_half_open_interval` —
   OK1. Pre-ingest 5 records with `observed_time_unix_nano` values
   `{100, 200, 300, 400, 500}` for tenant `acme`. Invoke the
   library `read()` with `TimeRange::new(200, 400)`. Assert returned
   count is `2`, stdout bytes equal the records with
   `observed_time_unix_nano` in `{200, 300}` re-serialised as NDJSON
   (one per line, terminated by `\n`), the record at `400` is
   EXCLUDED (open upper bound), the record at `200` is INCLUDED
   (closed lower bound).
2. `no_flags_is_byte_equivalent_to_today` — OK2. Pre-ingest N records.
   Invoke `read()` with `TimeRange::all()`. Assert returned count is
   N, stdout bytes equal all N records re-serialised as NDJSON.
3. `since_only_uses_u64_max_upper_bound` — OK3a. Pre-ingest 4
   records with `observed_time_unix_nano` values `{100, 200, 300,
   400}`. Invoke `read()` with `TimeRange::new(250, u64::MAX)`.
   Assert stdout contains exactly `{300, 400}`.
4. `until_only_uses_zero_lower_bound` — OK3b. Same pre-ingest as
   above. Invoke `read()` with `TimeRange::new(0, 250)`. Assert
   stdout contains exactly `{100, 200}`.
5. `invalid_since_fails_fast_with_named_flag_in_stderr` — OK4a.
   Invoke `run_read_with` (the in-process binary entry-point form at
   `crates/kaleidoscope-cli/src/main.rs:155-165`) with argv list
   `["kaleidoscope-cli", "read", "acme", "/tmp/data", "--since",
   "yesterday"]`. Assert result is `Err`, error message contains
   both `--since` and the verbatim bad value `yesterday`, stdout
   sink is empty (no records written).
6. `invalid_until_fails_fast_with_named_flag_in_stderr` — OK4b.
   Symmetric to #5 with argv list `["kaleidoscope-cli", "read",
   "acme", "/tmp/data", "--since", "2026-05-18T00:00:00Z",
   "--until", "2026-13-32T25:99:99Z"]`. Assert result is `Err`,
   error message contains both `--until` and the verbatim bad value
   `2026-13-32T25:99:99Z`, stdout sink is empty.

## Files touched

| File | Change |
|------|--------|
| `crates/kaleidoscope-cli/src/lib.rs` | `read()` gains a way for its caller to drive a `TimeRange` other than `TimeRange::all()` (exact signature shape is DESIGN's choice; the property is that the caller can pass an arbitrary `TimeRange::new(s, e)` into the `lumen.query` call at line 284). No-flag default constructs `TimeRange::all()` so existing callers are byte-equivalent. |
| `crates/kaleidoscope-cli/src/main.rs` | `run_read` (lines 146-165) gains parsing of `--since <value>` and `--until <value>` flags via helpers structurally similar to `parse_observe_otlp` at lines 130-144 (order-independent, single-pass argv scan). The parsed values are converted to `u64` nanoseconds via a new hand-rolled ISO 8601 UTC parser (inverse of `format_iso8601_utc_nanos` at `crates/kaleidoscope-cli/src/lib.rs:410-420`). `print_usage` (lines 78-109) gains a block documenting both flags. |
| `crates/kaleidoscope-cli/tests/read_time_range.rs` | NEW. Six test functions per the shape above; harness helpers (`tenant`, `record`, `temp_root`, `cleanup`, `ndjson`) duplicated inline at v0 mirroring the rule-of-three deferral from `cli-read-observe-otlp-v0` D6. |
| `crates/kaleidoscope-cli/Cargo.toml` | New `[[test]]` entry: `name = "read_time_range"`, `path = "tests/read_time_range.rs"`. |

## Files explicitly NOT touched (locked)

- `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` — locked
  OK2 protection (existing tests must continue to pass green).
- `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` — locked OK2
  protection.
- `crates/lumen/src/record.rs` — `TimeRange` API is correct as-is
  (`[start, end)` half-open; `TimeRange::all()` is `[0, u64::MAX)`).
  No change needed.
- `crates/self-observe/src/lumen_otlp_json.rs` — the OTLP-JSON writer
  is out of scope (composition with `--observe-otlp` is D6 in
  `wave-decisions.md`).
- `crates/kaleidoscope-cli/src/lib.rs:410-438` —
  `format_iso8601_utc_nanos` and `civil_from_days` are not modified;
  the new parser is the inverse direction and lives alongside them.

## DoR cross-reference

- US-01 DoR: PASSED (see `dor-validation.md`).
- Feature-level DoR: PASSED with honestly-recorded artefact gaps
  (journey visual / YAML / Gherkin file / shared-artefact registry
  not produced; rationale in `dor-validation.md` F1-F4).

## DESIGN handoff one-pager

DESIGN must decide:

1. The exact signature shape of the new `TimeRange`-driving control
   on `kaleidoscope_cli::read`. Options: a new explicit `time_range:
   TimeRange` parameter; a separate `read_range()` sibling function;
   a builder pattern. The acceptance test cares only that the caller
   can drive any `TimeRange::new(s, e)` into the underlying
   `lumen.query` call; the rest is DESIGN's choice. Default MUST be
   `TimeRange::all()` so existing callers are byte-equivalent (OK2).
2. The exact placement of the new ISO 8601 UTC parser. Options:
   inline in `crates/kaleidoscope-cli/src/main.rs` alongside the
   flag-parse helpers; in `crates/kaleidoscope-cli/src/lib.rs` next
   to its inverse `format_iso8601_utc_nanos`; in a new
   `crates/kaleidoscope-cli/src/iso8601.rs` module. The acceptance
   test calls only the library `read()` and the in-process
   `run_read_with`, so the parser's home is invisible to the test.
3. The exact handling of the no-`.` form. Per the System Constraints
   in `user-stories.md`, both `YYYY-MM-DDTHH:MM:SSZ` (no fractional
   seconds, parsed as `nanos_of_second = 0`) and
   `YYYY-MM-DDTHH:MM:SS.NNNNNNNNNZ` (1..=9 fractional-second digits)
   are accepted. DESIGN may decide whether the implementation accepts
   intermediate forms with 1..=8 digits (e.g.
   `YYYY-MM-DDTHH:MM:SS.NZ`) — recommended yes (left-pad to 9 digits
   internally before multiplication into the nanosecond field).

DESIGN MUST NOT:

- Modify `lumen::TimeRange` (the `[start, end)` semantics are correct
  as-is; the CLI surface conforms to them).
- Modify the existing locked test files
  (`observe_otlp_read_flag.rs`, `observe_otlp_flag.rs`).
- Introduce a new external crate dependency for ISO 8601 parsing
  (the hand-rolled posture is per `wave-decisions.md` D5).
- Change the `--observe-otlp` flag behaviour on `read` (composition
  with `--since` / `--until` is out of scope per D6; the
  `--observe-otlp` flag remains independently usable but is not
  jointly exercised in the new acceptance test).

Estimated effort for the crafter (DELIVER wave): <= 1 day.
