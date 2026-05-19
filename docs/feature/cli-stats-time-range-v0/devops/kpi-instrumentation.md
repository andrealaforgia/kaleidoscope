# KPI Instrumentation - `cli-stats-time-range-v0`

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19

All four KPIs are wired through the SAME new acceptance test file
`crates/kaleidoscope-cli/tests/stats_time_range.rs`, executed under
ADR-0005 Gate 1 (`cargo test --workspace --all-targets --locked`).
No new collection infrastructure; no new dashboard; no new alert.
The CI exit code IS the KPI signal.

## Per-KPI verification

### OK1 - bounded-window-records (principal / North Star)

| Aspect | Value |
|---|---|
| Probe | `tests/stats_time_range.rs` - `bounded_window_records_line_equals_windowed_count` scenario. |
| Mechanism | Pre-ingest 5 records for tenant `acme` with `observed_time_unix_nano` values `{100, 200, 300, 400, 500}`; invoke `kaleidoscope_cli::stats_with_tiers(&acme, &dir, &mut sink, TimeRange::new(200, 400))`; assert the `records=` line on captured stdout is exactly `records=2`; assert returned `count: usize` is `2`; assert record at exactly `400` is EXCLUDED (open-upper) and record at exactly `200` is INCLUDED (closed-lower). |
| Gate | Gate 1 (`cargo test`) + Gate 5 (`gate-5-mutants-kaleidoscope-cli`). |
| Alerting | CI failure on Gate 1 or Gate 5 is the alert (trunk-based; CI is feedback per project doctrine). |
| Dashboard | None (no service surface). |

### OK2 - bounded-window-earliest-latest (leading)

| Aspect | Value |
|---|---|
| Probe | `tests/stats_time_range.rs` - `bounded_window_earliest_latest_reflect_windowed_min_max` and `empty_window_omits_earliest_and_latest_lines` scenarios. |
| Mechanism | Bounded-window scenario: same pre-ingest as OK1; assert `earliest=1970-01-01T00:00:00.000000200Z` and `latest=1970-01-01T00:00:00.000000300Z` - the windowed min/max, NOT the global `100`/`500` min/max. Empty-window scenario: invoke `stats_with_tiers` with a `TimeRange` containing zero records; assert stdout begins with exactly `records=0\n` and contains NEITHER `earliest=` NOR `latest=` lines (D-EmptyWindow per DESIGN DD3). |
| Gate | Gate 1 + Gate 5. |
| Alerting | CI failure is the alert. |
| Dashboard | None. |

### OK3 - cinder-lines-unchanged (guardrail; pins D-CinderScope)

| Aspect | Value |
|---|---|
| Probe | `tests/stats_time_range.rs` - `cinder_lines_byte_identical_across_different_time_ranges` scenario. |
| Mechanism | Pre-ingest records spanning multiple disjoint time windows for tenant `acme`; seed Cinder with non-zero placements in all three tiers (hot, warm, cold) via `seed_cinder` harness helper. Invoke `stats_with_tiers` TWICE with two materially different bounded `TimeRange` values (e.g. `TimeRange::new(100, 200)` and `TimeRange::new(300, 400)`). Capture stdout from both. Assert the substring of stdout matching `/^(hot\|warm\|cold)=\d+$/` is byte-identical between the two captures, WHILE the Lumen lines (`records=...`, `earliest=...`, `latest=...`) differ between the two captures. This empirically pins D-CinderScope: the `range` parameter applies to Lumen only; Cinder is state-snapshot. |
| Gate | Gate 1 + Gate 5. |
| Alerting | CI failure is the alert. Specifically, if a future change accidentally threads `range` into the Cinder loop at `src/lib.rs:375-380`, OK3 fails immediately. |
| Dashboard | None. |

### OK4 - no-flag-byte-equivalence (guardrail)

| Aspect | Value |
|---|---|
| Probe | (a) `tests/stats_time_range.rs` - `no_flag_default_byte_equivalent_to_pre_feature_stats_with_tiers` asserts library-direct call with `TimeRange::all()` produces stdout bytes and return value equal to pre-feature behaviour. (b) Locked `tests/stats_subcommand.rs` continues to pass green with ZERO edits - it exercises only the legacy 3-arg `stats()` function which this feature does NOT modify (DESIGN DD6 item 1 out-of-scope). (c) Locked `tests/stats_cinder_tier_distribution.rs` continues to pass green with ONLY a mechanical 4th-arg update at its five `stats_with_tiers(...)` call sites (DESIGN DD4) - no assertion text edited. |
| Mechanism | Library-direct call asserts byte-equality against the pre-feature `stats_with_tiers` stdout for the same inputs; locked subprocess and library-direct tests assert their original byte oracles pass unchanged. |
| Gate | Gate 1 - failure of either locked file OR of the no-flag scenario is OK4 violation. |
| Alerting | CI failure on Gate 1 is the alert. The "zero assertion edits to locked files" property is enforced by review: any diff to assertion text in the two locked test files in the same commit as this feature's source delta is auto-rejected per DEVOPS DELIVER constraints. |
| Dashboard | None. |

## Why no dashboards or alerts

This feature is a query-shape change on `kaleidoscope-cli stats`'s
stdout. There is no service to monitor, no SLO to track, no error
budget to burn. The CLI binary's "operator" runs it on their own
host; if the binary mis-behaves, the operator sees it directly in
their terminal. The CI gates are the only authority that this
feature's contract continues to hold across the codebase's life.

This matches the posture of all five prior `kaleidoscope-cli`
DEVOPS waves; no dashboard or alerting surface was created for
any of them, and none is needed here. This is the sixth
realisation of the same posture.
