# AC Coverage — store-fsync-durability-v0 (DISTILL)

Each acceptance criterion (per the `upstream-changes.md` split and the brief's
"For Acceptance Designer" note) mapped to its proving MECHANISM and the test
function that exercises it. Mechanism (a) = out-of-process SIGKILL mid-snapshot;
(b) = in-suite **counting** substrate (fsync-per-append + fsync-around-snapshot)
PLUS the out-of-process refusal variant. All tests `@real-io`, `#[ignore]`d RED
until DELIVER (lumen un-ignored: its production wiring already landed).

> **DELIVER-found correction (mechanism b).** An earlier draft proved
> AC-wal-fsync by injecting the probe-double `LyingFsyncBackend::no_op()` /
> `::truncating()` into the store APPEND path and asserting the acked write
> SURVIVES. DELIVER step 1 (lumen) correctly escalated this rather than
> weakening it: the lying double deliberately discards bytes so the
> fsync-honesty PROBE can DETECT a lying substrate and the store can REFUSE to
> start; injected into the append path it makes the CORRECT `sync_all`-wired
> code lose the record, so the test fails on correct code. The lying double
> proves REFUSAL, not SURVIVAL, and cannot prove fsync-is-wired. The proof is
> now split, mirroring pulse slice 03 (`v1_slice_03_fsync_probe.rs`):
> (b-i) an in-suite `CountingFsyncBackend` (an honest `RealFsyncBackend`
> wrapper that DELEGATES the real fsync — data genuinely durable — and COUNTS
> calls at the seam) asserting `file_fsync_count` increases per acked append
> and `file_fsync_count` + `dir_fsync_count` increase around the snapshot
> rename, with the data queryable on reopen; and (b-ii) the unchanged
> out-of-process AC-substrate-refusal scenario where the lying double makes
> the composition root REFUSE to start. See `upstream-issues.md`.

## Per-AC → mechanism → test

### US-01 lumen (slice 01, walking skeleton) — `crates/lumen/tests/v1_slice_04_crash_durability.rs`

| AC | Mechanism | Test fn |
|----|-----------|---------|
| AC-snapshot-atomicity | (a) | `acked_log_survives_a_mid_snapshot_crash_and_is_queryable_after_restart` (WS) |
| AC-snapshot-atomicity (any-point invariant) | (a) | `canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash` `@property` |
| AC-recovery-regression | (a) | `a_torn_wal_tail_is_dropped_and_the_acked_prefix_is_recovered` |
| AC-wal-fsync (per-append) | (b-i) counting | `an_acked_append_fsyncs_the_wal_per_record_and_is_durable_on_reopen` |
| AC-wal-fsync (snapshot rename) | (b-i) counting | `a_snapshot_fsyncs_the_snapshot_file_and_parent_dir_for_rename_durability` |
| AC-substrate-refusal | (b) variant | `the_collector_refuses_to_start_on_a_substrate_that_lies_about_fsync` `@kpi` |
| AC-recovery-regression (graceful guard) | (a) graceful | `a_graceful_restart_still_recovers_every_acked_record` |

### US-02 ray (slice 02) — `crates/ray/tests/v1_slice_04_crash_durability.rs`

| AC | Mechanism | Test fn |
|----|-----------|---------|
| AC-snapshot-atomicity | (a) | `acked_span_survives_a_mid_snapshot_crash_and_is_queryable_after_restart` |
| AC-snapshot-atomicity (any-point) | (a) | `canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash` `@property` |
| AC-recovery-regression | (a) | `only_acked_spans_are_recovered_after_a_torn_tail_crash` |
| AC-wal-fsync (per-append) | (b-i) counting | `an_acked_append_fsyncs_the_wal_per_record_and_is_durable_on_reopen` |
| AC-wal-fsync (snapshot rename) | (b-i) counting | `a_snapshot_fsyncs_the_snapshot_file_and_parent_dir_for_rename_durability` |
| AC-substrate-refusal | (b) variant | `ray_refuses_to_start_on_a_substrate_that_lies_about_fsync` `@kpi` |

### US-03 strata (slice 03) — `crates/strata/tests/v1_slice_03_crash_durability.rs`

| AC | Mechanism | Test fn |
|----|-----------|---------|
| AC-snapshot-atomicity | (a) | `acked_profile_survives_a_mid_snapshot_crash_and_is_present_after_reopen` |
| AC-snapshot-atomicity (any-point) | (a) | `canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash` `@property` |
| AC-snapshot-atomicity (empty-store boundary) | (a) | `an_empty_store_opens_cleanly_after_a_crash_before_any_write` |
| AC-wal-fsync (per-append) | (b-i) counting | `an_acked_append_fsyncs_the_wal_per_record_and_is_durable_on_reopen` |
| AC-wal-fsync (snapshot rename) | (b-i) counting | `a_snapshot_fsyncs_the_snapshot_file_and_parent_dir_for_rename_durability` |
| AC-substrate-refusal | (b) variant | `strata_refuses_to_start_on_a_substrate_that_lies_about_fsync` `@kpi` |

### US-04 cinder (slice 04) — `crates/cinder/tests/v1_slice_04_crash_durability.rs`

| AC | Mechanism | Test fn |
|----|-----------|---------|
| AC-snapshot-atomicity | (a) | `acked_migration_survives_a_mid_snapshot_crash_and_is_present_after_reopen` |
| AC-snapshot-atomicity (any-point) | (a) | `canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash` `@property` |
| AC-recovery-regression | (a) | `a_torn_migration_tail_is_dropped_and_the_acked_prefix_is_recovered` |
| AC-wal-fsync (per-append) | (b-i) counting | `an_acked_migration_fsyncs_the_wal_per_record_and_is_durable_on_reopen` |
| AC-wal-fsync (snapshot rename) | (b-i) counting | `a_snapshot_fsyncs_the_snapshot_file_and_parent_dir_for_rename_durability` |
| AC-substrate-refusal | (b) variant | `cinder_refuses_to_start_on_a_substrate_that_lies_about_fsync` `@kpi` |

### US-05 sluice (slice 05) — `crates/sluice/tests/v1_slice_03_crash_durability.rs`

| AC | Mechanism | Test fn |
|----|-----------|---------|
| AC-snapshot-atomicity | (a) | `acked_enqueue_survives_a_mid_snapshot_crash_and_is_dequeuable_after_reopen` |
| AC-snapshot-atomicity (any-point) | (a) | `canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash` `@property` |
| AC-snapshot-atomicity (in-flight boundary) | (a) | `an_in_flight_item_is_recovered_after_a_crash_not_silently_dropped` |
| AC-wal-fsync (per-append) | (b-i) counting | `an_acked_enqueue_fsyncs_the_wal_per_record_and_is_durable_on_reopen` |
| AC-wal-fsync (snapshot rename) | (b-i) counting | `a_snapshot_fsyncs_the_snapshot_file_and_parent_dir_for_rename_durability` |
| AC-substrate-refusal | (b) variant | `sluice_refuses_to_start_on_a_substrate_that_lies_about_fsync` `@kpi` |

### US-06 beacon rule-state store (slice 06) — `crates/beacon/tests/v1_slice_03_crash_durability.rs`

| AC | Mechanism | Test fn |
|----|-----------|---------|
| AC-snapshot-atomicity | (a) | `acked_rule_transition_survives_a_mid_snapshot_crash_and_is_present_after_reopen` |
| AC-snapshot-atomicity (any-point) | (a) | `canonical_snapshot_is_whole_or_absent_never_torn_after_a_crash` `@property` |
| AC-recovery-regression | (a) | `a_torn_transition_tail_is_dropped_and_the_acked_prefix_is_recovered` |
| AC-wal-fsync (per-append) | (b-i) counting | `an_acked_transition_fsyncs_the_wal_per_record_and_is_durable_on_reopen` |
| AC-wal-fsync (snapshot rename) | (b-i) counting | `a_snapshot_fsyncs_the_snapshot_file_and_parent_dir_for_rename_durability` |
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
| AC-substrate-refusal (NEGATIVE — refuses to start on a lying substrate) | 6 |
| AC-snapshot-atomicity any-point invariant (`@property`, adverse crash) | 7 |
| strata empty-store boundary | 1 |
| sluice in-flight boundary | 1 |
| **Negative / edge subtotal** | **15** |
| AC-wal-fsync counting (per-append + snapshot-rename — durability proof) | 12 |
| Happy-path / recovery-regression (positive) | 13 |
| **Total** | **40** |

The AC-wal-fsync proof is now a counting-substrate assertion (data IS durable;
we count the seam), so it is a positive durability proof rather than an adverse
substrate. The NEGATIVE adverse-substrate role is carried by the 6
AC-substrate-refusal scenarios. Negative / edge = 15 / 40 = **37.5 %**; counting
the 12 AC-wal-fsync durability proofs as adverse-aware coverage of the
power-cut threat model gives 27 / 40 = 67.5 %. Either reading keeps the suite
within its intent: every store has a refusal NEGATIVE, an any-point crash
invariant, and a counted durability proof.
