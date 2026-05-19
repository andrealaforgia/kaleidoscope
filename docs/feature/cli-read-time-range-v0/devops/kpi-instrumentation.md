# KPI Instrumentation - `cli-read-time-range-v0`

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19

All four KPIs are wired through the SAME new acceptance test file
`crates/kaleidoscope-cli/tests/read_time_range.rs`, executed under
ADR-0005 Gate 1 (`cargo test --workspace --all-targets --locked`,
ci.yml:182). No new collection infrastructure; no new dashboard;
no new alert. The CI exit code IS the KPI signal.

## Per-KPI verification

### OK1 - bounded-window-filter (principal / North Star)

| Aspect | Value |
|---|---|
| Probe | `tests/read_time_range.rs` - `bounded_window_returns_only_records_in_half_open_interval` scenario |
| Mechanism | Pre-ingest 5 records for tenant `acme` with `observed_time_unix_nano` values `{100, 200, 300, 400, 500}`; invoke `kaleidoscope_cli::read(&acme, &dir, &mut sink, None, TimeRange::new(200, 400))`; assert `sink` bytes equal records `{200, 300}` re-serialised as NDJSON (one per line, terminated by `\n`); assert returned count is `2`; assert record at exactly `400` is EXCLUDED (open-upper); assert record at exactly `200` is INCLUDED (closed-lower). |
| Gate | Gate 1 (`cargo test`) + Gate 5 (`gate-5-mutants-kaleidoscope-cli`) |
| Alerting | CI failure on Gate 1 or Gate 5 is the alert (trunk-based; CI is feedback per project doctrine). |
| Dashboard | None (no service surface). |

### OK2 - no-flag-byte-equivalent (guardrail)

| Aspect | Value |
|---|---|
| Probe | (a) `tests/read_time_range.rs` - `no_flag_scenario_byte_equivalent_to_time_range_all` asserts library-direct call with `TimeRange::all()` produces stdout bytes and return count equal to pre-feature behaviour. (b) Locked `tests/observe_otlp_read_flag.rs` and `tests/observe_otlp_flag.rs` continue to pass with ZERO edits - they invoke the binary as a subprocess WITHOUT `--since` / `--until`, hitting the no-flag default `TimeRange::all()` per DESIGN DD1 / DD4. |
| Mechanism | Library-direct call asserts byte-equality against re-serialised pre-ingested records; subprocess tests assert their original byte oracles pass unchanged. |
| Gate | Gate 1 (`cargo test`) - failure of either locked file is OK2 violation. |
| Alerting | CI failure on Gate 1 is the alert. The "zero edits to locked files" property is enforced by review: any diff to the two locked test files in the same commit as this feature's source delta is auto-rejected per DEVOPS DELIVER constraints. |
| Dashboard | None. |

### OK3 - half-bounded-supported (leading)

| Aspect | Value |
|---|---|
| Probe | `tests/read_time_range.rs` - `since_only_returns_records_from_lower_bound_onwards` AND `until_only_returns_records_strictly_before_upper_bound` scenarios. |
| Mechanism | Pre-ingest 4 records with `observed_time_unix_nano` values `{100, 200, 300, 400}`; invoke `read(..., TimeRange::new(250, u64::MAX))`, assert stdout contains exactly `{300, 400}`; symmetrically invoke `read(..., TimeRange::new(0, 250))`, assert stdout contains exactly `{100, 200}`. Per DEVOPS DELIVER constraints, the witness set MUST also include a record at exactly `observed_time_unix_nano = 0` AND a record near `u64::MAX` to kill the half-bounded-default mutation classes. |
| Gate | Gate 1 + Gate 5 |
| Alerting | CI failure is the alert. |
| Dashboard | None. |

### OK4 - invalid-iso8601-fails-fast (guardrail)

| Aspect | Value |
|---|---|
| Probe | `tests/read_time_range.rs` - `invalid_since_value_fails_fast_naming_flag`, `invalid_until_value_fails_fast_naming_flag`, `missing_z_suffix_fails_fast_naming_flag` scenarios. |
| Mechanism | Invoke `run_read_with(..., args)` (binary entry-point form testable in-process per `crates/kaleidoscope-cli/src/main.rs:155-165`) with argv lists `["--since", "yesterday"]`, `["--until", "2026-13-32T25:99:99Z"]`, `["--since", "2026-05-18T00:00:00"]`. For each: assert result is `Err`; assert stderr contains BOTH the offending flag name (`--since` or `--until`) AND the verbatim bad value; assert stdout is empty (zero bytes); assert the Lumen store under `data_dir` was NOT opened (filesystem-absence probe: no `data_dir/<tenant>/` subtree exists post-call). |
| Gate | Gate 1 + Gate 5 |
| Alerting | CI failure is the alert. The "fail-before-store-open" invariant is verified by filesystem-absence probe, not by mocking - the substrate IS the test oracle (Earned Trust). |
| Dashboard | None. |

## Why no dashboards or alerts

This feature is a query-shape change on `kaleidoscope-cli read`'s
stdout. There is no service to monitor, no SLO to track, no error
budget to burn. The CLI binary's "operator" runs it on their own
host; if the binary mis-behaves, the operator sees it directly in
their terminal. The CI gates are the only authority that this
feature's contract continues to hold across the codebase's life.

This matches the posture of all four prior `kaleidoscope-cli`
DEVOPS waves; no dashboard or alerting surface was created for
any of them, and none is needed here.
