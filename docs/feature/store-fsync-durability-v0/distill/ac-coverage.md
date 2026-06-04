# AC Coverage — store-fsync-durability-v0 (DISTILL)

Each acceptance criterion (per the `upstream-changes.md` split and the brief's
"For Acceptance Designer" note) mapped to its proving MECHANISM and the test
function that exercises it. Mechanism (a) = out-of-process SIGKILL mid-snapshot;
(b) = in-suite lying substrate. All tests `@real-io`, `#[ignore]`d RED until
DELIVER.

## Per-AC → mechanism → test

### US-01 lumen (slice 01, walking skeleton) — `crates/lumen/tests/v1_slice_04_crash_durability.rs`

| AC | Mechanism | Test fn |
|----|-----------|---------|
| AC-snapshot-atomicity | (a) | `acked_log_survives_a_mid_snapshot_crash_and_is_queryable_after_restart` (WS) |
| AC-snapshot-atomicity (any-point invariant) | (a) | `canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash` `@property` |
| AC-recovery-regression | (a) | `a_torn_wal_tail_is_dropped_and_the_acked_prefix_is_recovered` |
| AC-wal-fsync (no_op) | (b) | `an_acked_write_survives_a_substrate_that_discards_unsynced_bytes` |
| AC-wal-fsync (truncating) | (b) | `an_acked_write_survives_a_truncating_substrate` |
| AC-substrate-refusal | (b) variant | `the_collector_refuses_to_start_on_a_substrate_that_lies_about_fsync` `@kpi` |
| AC-recovery-regression (graceful guard) | (a) graceful | `a_graceful_restart_still_recovers_every_acked_record` |

### US-02 ray (slice 02) — `crates/ray/tests/v1_slice_04_crash_durability.rs`

| AC | Mechanism | Test fn |
|----|-----------|---------|
| AC-snapshot-atomicity | (a) | `acked_span_survives_a_mid_snapshot_crash_and_is_queryable_after_restart` |
| AC-snapshot-atomicity (any-point) | (a) | `canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash` `@property` |
| AC-recovery-regression | (a) | `only_acked_spans_are_recovered_after_a_torn_tail_crash` |
| AC-wal-fsync (no_op) | (b) | `an_acked_span_survives_a_substrate_that_discards_unsynced_bytes` |
| AC-wal-fsync (truncating) | (b) | `an_acked_span_survives_a_truncating_substrate` |
| AC-substrate-refusal | (b) variant | `ray_refuses_to_start_on_a_substrate_that_lies_about_fsync` `@kpi` |

### US-03 strata (slice 03) — `crates/strata/tests/v1_slice_03_crash_durability.rs`

| AC | Mechanism | Test fn |
|----|-----------|---------|
| AC-snapshot-atomicity | (a) | `acked_profile_survives_a_mid_snapshot_crash_and_is_present_after_reopen` |
| AC-snapshot-atomicity (any-point) | (a) | `canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash` `@property` |
| AC-snapshot-atomicity (empty-store boundary) | (a) | `an_empty_store_opens_cleanly_after_a_crash_before_any_write` |
| AC-wal-fsync (no_op) | (b) | `an_acked_profile_survives_a_substrate_that_discards_unsynced_bytes` |
| AC-wal-fsync (truncating) | (b) | `an_acked_profile_survives_a_truncating_substrate` |
| AC-substrate-refusal | (b) variant | `strata_refuses_to_start_on_a_substrate_that_lies_about_fsync` `@kpi` |

### US-04 cinder (slice 04) — `crates/cinder/tests/v1_slice_04_crash_durability.rs`

| AC | Mechanism | Test fn |
|----|-----------|---------|
| AC-snapshot-atomicity | (a) | `acked_migration_survives_a_mid_snapshot_crash_and_is_present_after_reopen` |
| AC-snapshot-atomicity (any-point) | (a) | `canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash` `@property` |
| AC-recovery-regression | (a) | `a_torn_migration_tail_is_dropped_and_the_acked_prefix_is_recovered` |
| AC-wal-fsync (no_op) | (b) | `an_acked_migration_survives_a_substrate_that_discards_unsynced_bytes` |
| AC-wal-fsync (truncating) | (b) | `an_acked_migration_survives_a_truncating_substrate` |
| AC-substrate-refusal | (b) variant | `cinder_refuses_to_start_on_a_substrate_that_lies_about_fsync` `@kpi` |

### US-05 sluice (slice 05) — `crates/sluice/tests/v1_slice_03_crash_durability.rs`

| AC | Mechanism | Test fn |
|----|-----------|---------|
| AC-snapshot-atomicity | (a) | `acked_enqueue_survives_a_mid_snapshot_crash_and_is_dequeuable_after_reopen` |
| AC-snapshot-atomicity (any-point) | (a) | `canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash` `@property` |
| AC-snapshot-atomicity (in-flight boundary) | (a) | `an_in_flight_item_is_recovered_after_a_crash_not_silently_dropped` |
| AC-wal-fsync (no_op) | (b) | `an_acked_enqueue_survives_a_substrate_that_discards_unsynced_bytes` |
| AC-wal-fsync (truncating) | (b) | `an_acked_enqueue_survives_a_truncating_substrate` |
| AC-substrate-refusal | (b) variant | `sluice_refuses_to_start_on_a_substrate_that_lies_about_fsync` `@kpi` |

### US-06 beacon rule-state store (slice 06) — `crates/beacon/tests/v1_slice_03_crash_durability.rs`

| AC | Mechanism | Test fn |
|----|-----------|---------|
| AC-snapshot-atomicity | (a) | `acked_rule_transition_survives_a_mid_snapshot_crash_and_is_present_after_reopen` |
| AC-snapshot-atomicity (any-point) | (a) | `canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash` `@property` |
| AC-recovery-regression | (a) | `a_torn_transition_tail_is_dropped_and_the_acked_prefix_is_recovered` |
| AC-wal-fsync (no_op) | (b) | `an_acked_transition_survives_a_substrate_that_discards_unsynced_bytes` |
| AC-wal-fsync (truncating) | (b) | `an_acked_transition_survives_a_truncating_substrate` |
| AC-substrate-refusal | (b) variant | `beacon_rule_state_store_refuses_to_start_on_a_substrate_that_lies_about_fsync` `@kpi` |

### US-07 pulse (slice 07, SNAPSHOT-ONLY) — `crates/pulse/tests/v1_slice_06_snapshot_atomicity.rs`

| AC | Mechanism | Test fn |
|----|-----------|---------|
| AC-snapshot-atomicity | (a) | `pulse_opens_cleanly_after_a_crash_during_a_snapshot` |
| AC-snapshot-atomicity (any-point: temp-never-canonical + rename boundary) | (a) | `canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash` `@property` |
| AC-recovery-regression (ADR-0049 WAL durability preserved) | (a) | `acked_metrics_written_after_the_snapshot_also_survive_a_crash` |

pulse carries NO AC-wal-fsync (WAL already crash-durable, ADR-0049) and NO
AC-substrate-refusal scenario in this slice (its fsync probe is already wired;
adding it here would duplicate `v1_slice_03_fsync_probe.rs`).

## Story coverage

Every story US-01..US-07 has at least one scenario tagged `@US-0N`. No story
is uncovered. KPI mapping: K3 (snapshot atomicity, mechanism a), K2 (wal
fsync, mechanism b), K4 (substrate refusal, 1/7 → 7/7).

## Negative / edge ratio (Mandate: ≥ 40 %)

| Category | Count |
|----------|-------|
| AC-substrate-refusal (NEGATIVE — refuses to start) | 6 |
| AC-wal-fsync (lying substrate discards/truncates — adverse substrate) | 12 |
| AC-snapshot-atomicity any-point invariant (`@property`, adverse crash) | 7 |
| strata empty-store boundary | 1 |
| sluice in-flight boundary | 1 |
| **Negative / edge subtotal** | **27** |
| Happy-path / recovery-regression (positive) | 13 |
| **Total** | **40** |

Negative / edge = 27 / 40 = **67.5 %**, well above the 40 % target. (Even the
strictest reading — counting only refusal + lying-substrate as "negative" —
gives 18 / 40 = 45 %.)
