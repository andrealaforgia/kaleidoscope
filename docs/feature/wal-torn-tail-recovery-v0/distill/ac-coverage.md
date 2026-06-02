# AC / Adapter Coverage Table — wal-torn-tail-recovery-v0 (DISTILL)

British English. No em dashes in body.

## Per-AC coverage

| AC | Behaviour | Scenario(s) | File |
|---|---|---|---|
| AC-1 | Intact-prefix recovery, end-to-end (verifier D04) | `operator_restart_serves_the_intact_acked_prefix_after_a_torn_tail` | `log-query-api/tests/slice_08_torn_tail_recovery.rs` |
| AC-1 | Intact-prefix recovery, store-reopen port (N>=1) | `reopen_recovers_the_intact_prefix_and_drops_the_torn_tail`; `reopen_recovers_a_single_acked_record_before_the_torn_tail` (N=1) | `lumen/tests/v1_slice_03_torn_tail_recovery.rs` |
| AC-1 | Intact-prefix recovery, ray + cinder + pulse | `reopen_recovers_the_intact_prefix_after_a_torn_tail` (ray, cinder); `reopen_recovers_the_intact_prefix_and_cardinality_stays_consistent` (pulse) | ray / cinder / pulse slice files |
| AC-2 | Torn tail dropped, not repaired | `reopen_recovers_the_intact_prefix_and_drops_the_torn_tail` asserts exactly N records, original order, none partial | `lumen/tests/v1_slice_03_torn_tail_recovery.rs` |
| AC-3 | Structured WARN (event, pillar, line, dropped_bytes) | `recovery_emits_one_structured_warning_naming_pillar_line_and_dropped_bytes` | `log-query-api/tests/slice_08_torn_tail_recovery.rs` |
| AC-4 | Snapshot + single torn tail recovers snapshot state | `snapshot_plus_single_torn_tail_recovers_exactly_the_snapshot_state` | `ray/tests/v1_slice_03_torn_tail_recovery.rs` |
| AC-5 | NEGATIVE: mid-file corruption stays fail-closed | `mid_file_corruption_stays_fail_closed` (lumen, ray, pulse); `mid_file_corruption_stays_fail_closed_naming_the_offending_line` (cinder) | all four pillar slice files |
| AC-6 | NEGATIVE: newline-terminated malformed final line fail-closed | `newline_terminated_malformed_final_line_stays_fail_closed` (lumen, cinder, pulse) | lumen / cinder / pulse slice files |
| AC-7 | cinder doc correction | DELIVER-verified doc criterion (DWD-2); behaviour proven by cinder torn-tail-tolerated + two negatives | cinder slice doc + `wave-decisions.md` DWD-2 |
| AC-8 | No trait change | NOT a DISTILL acceptance test (Gate 2 `cargo public-api`) | n/a |
| AC-9 | Scope = lumen, ray, cinder, pulse | torn-tail-tolerated + mid-file-fail-closed present for ALL four pillars | all four pillar slice files |
| AC-10 | Mutation kill 100% | NOT a DISTILL acceptance test (Gate 5 jobs, DELIVER) | n/a |

## Per-adapter real-I/O coverage (Mandate: every adapter exercised through real I/O)

The driven adapters in this feature are the four pillars' file-backed WAL
replay-on-open paths plus the lumen read-API binary. Each has at least one
`@real-io` scenario exercising REAL filesystem I/O (real WAL bytes, real
reopen) and, for lumen, a real child process + real TCP.

| Driven adapter (open/replay) | Real-I/O scenario | Tag |
|---|---|---|
| lumen `FileBackedLogStore::open` | `reopen_recovers_the_intact_prefix_and_drops_the_torn_tail` (+ N=1, AC-5, AC-6) | `@real-io @adapter-integration` |
| lumen via `log-query-api` binary | `operator_restart_serves_the_intact_acked_prefix_after_a_torn_tail` (real process + TCP) | `@real-io @driving_port` |
| ray `FileBackedTraceStore::open` | `reopen_recovers_the_intact_prefix_after_a_torn_tail` (+ AC-4 snapshot, AC-5) | `@real-io @adapter-integration` |
| cinder `FileBackedTieringStore::open` | `reopen_recovers_the_intact_prefix_after_a_torn_tail` (+ AC-5, AC-6) | `@real-io @adapter-integration` |
| pulse `FileBackedMetricStore::open` | `reopen_recovers_the_intact_prefix_and_cardinality_stays_consistent` (+ AC-5, AC-6) | `@real-io @adapter-integration` |

No adapter relies solely on an in-memory double. Per Strategy C, there are
no `@in-memory` walking-skeleton scenarios.

## KPI observability coverage

`docs/product/kpi-contracts.yaml` was checked. The feature emits exactly
one structured signal (`event="wal.recovery.torn_tail_dropped"`); DEVOPS
D5 and `environments.yaml` confirm it rides the existing tracing stream
with NO new metric, dashboard, or alert at v0. AC-3
(`recovery_emits_one_structured_warning_...`) is the observability
scenario verifying the metric/event is emittable through the operator's
real stderr port. No separate `@kpi`-tagged scenario is warranted: the
KPIs K1/K2 are measured by the AC-1 recovery scenarios, K3 by AC-3, K4 by
AC-5/AC-6, K5 by AC-7; all are covered above. (If a top-level
`kpi-contracts.yaml` enumerates a distinct emittable-metric contract for
this event, AC-3 already satisfies it; no gap.)

## Story-to-scenario traceability (Dim 8 Check A)

Story IDs in `discuss/user-stories.md`: US-01 (the only story). Every one
of the 15 scenarios carries an `@US-01` tag in its leading comment and
references its AC(s). Zero untraceable scenarios; zero stories with no
scenario.

## Environment-to-scenario traceability (Dim 8 Check B)

`devops/environments.yaml` declares two build-and-test environments:
`clean` and `ci` (deploy_target: none; this is a library-internal change
with no install surface). Both run the SAME validation: `cargo test
--workspace --all-targets --locked`. All 15 scenarios run under both
unchanged (no environment-specific preconditions: `preconditions: []` for
both). The walking-skeleton binary scenario spawns the compiled binary
with a controlled env (`KALEIDOSCOPE_PILLAR_ROOT`,
`KALEIDOSCOPE_LOG_QUERY_TENANT`, `KALEIDOSCOPE_LOG_QUERY_ADDR=127.0.0.1:0`,
`RUST_LOG=info`) on a real tmp directory, which is exactly the `clean` /
`ci` precondition set (no external services). No environment is left
without coverage.
