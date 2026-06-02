# Acceptance Design — wal-torn-tail-recovery-v0 (DISTILL)

British English. No em dashes in body.

## Wave: DISTILL (Quinn, nw-acceptance-designer)

## Autonomous run note

Autonomous overnight run. All interactive decisions were made by the
agent per the standing instruction; nothing was deferred to the user.
DISTILL only; does NOT proceed into DELIVER.

## Scope (bounded by the DISCUSS delta)

Single-story brownfield slice US-01: a crashed-then-restarted file-backed
store recovers its intact acked prefix and warns about the dropped torn
tail, while every OTHER parse failure stays fail-closed. Scope pillars
(ADR-0059 Decision 5, AC-9): lumen, ray, cinder, pulse. The SSOT provides
the port entry (lumen `GET /api/v1/logs`, verifier D04), the failure
modes (mid-file corruption, newline-terminated malformed final line), and
the structured WARN contract; the feature delta bounds the scope to US-01
only.

## Driving ports (from architecture brief "For Acceptance Designer")

- **AC-1 headline (verifier D04)**: the operator restart path through the
  COMPILED `log-query-api` binary. The binary opens
  `FileBackedLogStore::open(pillar_root/lumen, ..)` against a crashed
  `pillar_root`, binds its listener, and `GET /api/v1/logs` serves the
  intact prefix. Exercised port-to-port via a spawned child process and a
  real HTTP query (the same subprocess + stderr shape the EDD verifier
  uses; slice 07 established it).
- **Store-reopen driving port**: `FileBackedLogStore::open` /
  `FileBackedTraceStore::open` / `FileBackedTieringStore::open` /
  `FileBackedMetricStore::open` reopened directly on a crashed tmp
  `pillar_root`, then read through the store trait. This is a primary
  (driving) port: the operator-visible behaviour is the pillar starting
  and serving the durable prefix.
- **Secondary driving port (AC-3, the warning)**: the binary's structured
  `tracing` stderr. Assert exactly one
  `event="wal.recovery.torn_tail_dropped"` with `pillar`, `line`,
  `dropped_bytes` on a torn-tail recovery.

We do NOT enter through the shared `crates/wal-recovery` function
directly. It is a driven implementation detail created by DELIVER; its
behavioural gold-test (the Earned-Trust layer) is the crafter's concern,
NOT the user-visible acceptance. No test here references the
not-yet-existing `wal_recovery::replay_wal_tolerating_torn_tail` symbol,
so all tests compile against today's code with no scaffold.

## I-O strategy: Strategy C (real local I/O)

Recorded as DWD-1 in `wave-decisions.md`. These are real WAL files on a
real tmp directory (std `temp_dir()` + PID + nanos suffix, matching the
established v1 file-backed convention), real reopen, a real child process
for the binary path, and a real TCP query. No external services, no
containers, no test doubles for the filesystem. The torn tail is seeded by
writing real partial-JSON bytes with no trailing newline directly to the
on-disk WAL, exactly as a `kill -9` between `write_all(bytes)` and
`write_all(b"\n")` leaves it. All such scenarios are tagged `@real-io`.

No new dev-dependency was added (no `tempfile`): the established pillar
convention uses `std::env::temp_dir()` with manual cleanup, and adding a
new crate would touch `Cargo.lock` and risk Gate 4 churn for zero benefit.

## Test files and slice numbers

| Pillar / crate | File | Slice | Scenarios |
|---|---|---|---|
| log-query-api | `crates/log-query-api/tests/slice_08_torn_tail_recovery.rs` | 08 | AC-1 binary+HTTP (walking skeleton, verifier D04), AC-3 structured WARN |
| lumen | `crates/lumen/tests/v1_slice_03_torn_tail_recovery.rs` | v1_03 | AC-1 store reopen, AC-1 N=1 boundary, AC-5 mid-file, AC-6 newline-malformed |
| ray | `crates/ray/tests/v1_slice_03_torn_tail_recovery.rs` | v1_03 | AC-4 snapshot+torn-tail, AC-1/AC-9 torn-tail tolerated, AC-5 mid-file |
| cinder | `crates/cinder/tests/v1_slice_03_torn_tail_recovery.rs` | v1_03 | AC-1/AC-9 torn-tail tolerated, AC-5 mid-file (headline), AC-6 newline-malformed |
| pulse | `crates/pulse/tests/v1_slice_05_torn_tail_recovery.rs` | v1_05 | AC-1/AC-9 torn-tail + cardinality property, AC-5 mid-file, AC-6 newline-malformed |

Total: 15 acceptance scenarios across 5 test files.

## Scenario inventory (Given-When-Then, business framing)

### Walking skeleton (verifier D04) — log-query-api slice 08

1. **operator_restart_serves_the_intact_acked_prefix_after_a_torn_tail**
   (`@walking_skeleton @real-io @driving_port @AC-1`).
   Given a crashed pillar_root whose lumen WAL holds 10 acked records for
   tenant acme-corp followed by one torn final line with no trailing
   newline; When the operator restarts the log-query-api binary against
   it; Then the binary recovers, binds its listener, and a query over the
   full time range returns all 10 acked records (the torn 11th absent).

2. **recovery_emits_one_structured_warning_naming_pillar_line_and_dropped_bytes**
   (`@real-io @driving_port @AC-3`).
   Given a crashed pillar_root whose lumen WAL holds 3 acked records then
   one torn final line; When the operator restarts the binary; Then stderr
   carries exactly one `wal.recovery.torn_tail_dropped` event naming
   pillar=lumen, the 1-based line number, and the dropped byte length.

### Positive store-reopen (per pillar)

3. lumen **reopen_recovers_the_intact_prefix_and_drops_the_torn_tail**
   (`@real-io @adapter-integration @AC-1 @AC-2`): 5 acked records recover
   in order; torn tail dropped, not repaired.
4. lumen **reopen_recovers_a_single_acked_record_before_the_torn_tail**
   (`@AC-1` N=1 boundary): N >= 1 exercised at N=1.
5. ray **snapshot_plus_single_torn_tail_recovers_exactly_the_snapshot_state**
   (`@AC-4`): snapshot present, WAL is a single torn line on top; opens,
   recovers exactly the snapshot state, torn span absent.
6. ray **reopen_recovers_the_intact_prefix_after_a_torn_tail**
   (`@AC-1 @AC-9`): both acked spans recover via both indices.
7. cinder **reopen_recovers_the_intact_prefix_after_a_torn_tail**
   (`@AC-1 @AC-9`): three placements recover; torn fourth absent.
8. pulse **reopen_recovers_the_intact_prefix_and_cardinality_stays_consistent**
   (`@AC-1 @AC-9`, FLAG-1 property): two acked series recover; recovered
   cardinality equals the prefix cardinality (no torn series leaks in).

### Negative guards (per pillar)

9. lumen **mid_file_corruption_stays_fail_closed** (`@AC-5`).
10. lumen **newline_terminated_malformed_final_line_stays_fail_closed** (`@AC-6`).
11. ray **mid_file_corruption_stays_fail_closed** (`@AC-5 @AC-9`).
12. cinder **mid_file_corruption_stays_fail_closed_naming_the_offending_line** (`@AC-5`).
13. cinder **newline_terminated_malformed_final_line_stays_fail_closed** (`@AC-6`).
14. pulse **mid_file_corruption_stays_fail_closed** (`@AC-5 @AC-9`).
15. pulse **newline_terminated_malformed_final_line_stays_fail_closed** (`@AC-6`).

## Negative / edge coverage ratio

8 of 15 scenarios (53%) are negative or edge: 7 fail-closed negatives
(AC-5 x4, AC-6 x3) plus the N=1 boundary edge. This comfortably exceeds
the 40% target and is natural here given the two co-equal negative
criteria (AC-5, AC-6) that the whole value of the narrow tolerance
depends on (K4 guardrail, co-equal with K1).

## RED-not-BROKEN posture (Mandate 7)

Every one of the 15 scenarios is marked
`#[ignore = "RED until DELIVER: wal-torn-tail-recovery-v0 slice NN ..."]`.
The DISTILL commit's pre-commit hook (`cargo test --workspace
--all-targets --locked`, never `--no-verify`) sees zero live failures.
DELIVER removes the `#[ignore]` one scenario at a time as it implements
(Outside-In). Confirmed: all 15 tests COMPILE against today's public APIs
with NO scaffold required (no `panic!("__SCAFFOLD__")`, no `// SCAFFOLD:
true` marker anywhere), because they drive only existing public surface:
`FileBacked*Store::open`/`query`/`get_trace`/`get_tier`, the compiled
`log-query-api` binary, and on-disk WAL bytes.

## AC-7 (cinder doc) handling

AC-7 (the cinder module doc no longer makes the false "detected and
ignored" claim) is recorded as a **DELIVER-verified doc criterion**, not a
brittle runtime string-match on prose. Rationale and the substitute
behavioural coverage are stated inline in the cinder slice doc and in
`wave-decisions.md` DWD-2: the BEHAVIOUR the corrected doc describes is
proven by cinder's torn-tail-tolerated positive plus its two
fail-closed negatives; the crafter corrects the prose in the same DELIVER
commit and the reviewer reads it against AC-1..AC-6.

## Out of DISTILL scope (correctly not tested here)

- AC-8 (no trait change): Gate 2 `cargo public-api`, not an acceptance
  test.
- AC-10 (mutation kill): each pillar's `gate-5-mutants-*` job plus the new
  `gate-5-mutants-wal-recovery`, a DELIVER/CI concern.
- The `crates/wal-recovery` behavioural gold-test (Earned-Trust layer):
  the crafter's DELIVER concern; it probes the internal routine, not the
  user-visible behaviour.
