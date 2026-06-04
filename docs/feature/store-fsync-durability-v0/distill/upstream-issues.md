# Upstream issues — store-fsync-durability-v0 (DISTILL)

DELIVER-found defects in the DISTILL acceptance design, and their
resolution. Recorded here so the conflation is not re-introduced.

## UI-01 — AC-wal-fsync conflated the probe-double with a durability-discard double

**Found by**: DELIVER step 1 (lumen, commit `ec82270`). Crafty correctly
ESCALATED rather than weakening the scenario.

### The flaw

The two AC-wal-fsync scenarios per store injected the probe-double
`LyingFsyncBackend::no_op()` / `::truncating()` into the **store append
path** through `open_with_fsync_backend` and asserted the acked write
**SURVIVES** on reopen.

But the lying backends are the **probe doubles**: their `fsync_file`
deliberately discards bytes (`no_op` truncates the file to 0,
`truncating` drops the last byte) so that the fsync-honesty **probe**
(ADR-0049) detects a lying substrate and the store **REFUSES to start**.
The lie semantics are required verbatim by the ADR-0049 probe tests.

Injecting them into the append path makes the **CORRECT** `sync_all`-wired
code lose the record on every append: `no_op` truncates the live WAL to 0;
`truncating` de-newlines the just-synced final line so torn-tail recovery
(ADR-0059) drops it. So the acked write is **ABSENT on the fixed code**,
and the test fails on correct code. A SIGKILL cannot prove fsync-is-wired
either (the page cache survives a same-host kill, ADR-0060 §1).

**The lying double proves REFUSAL, not SURVIVAL.** The probe-double and a
durability-discard double cannot be one type/constructor: the probe needs
fsync to corrupt-to-be-detected; the store needs fsync to preserve.

### The resolution

Mirror the pattern already present in this codebase —
`crates/pulse/tests/v1_slice_03_fsync_probe.rs` — which proves pulse's WAL
fsync with a `CountingFsyncBackend`: a wrapper around `RealFsyncBackend`
that **delegates** the real fsync (so data is genuinely durable) and
**counts** calls at the seam.

1. Generalised pulse's per-test wrapper into ONE shared
   `CountingFsyncBackend` in `crates/wal-recovery/src/lib.rs`, beside the
   `FsyncBackend` family (which moved there under ADR-0060 §4). It is a
   `pub struct` (the same exposure as `LyingFsyncBackend`), re-exported by
   all seven stores.

2. Rewrote the two AC-wal-fsync scenarios in each of the six non-pulse
   stores' crash-durability test files to the counting pattern:
   - `an_acked_<verb>_fsyncs_the_wal_per_record_and_is_durable_on_reopen` —
     inject `CountingFsyncBackend` via `open_with_fsync_backend`, assert
     `file_fsync_count` increased after an acked append, and the record is
     queryable on reopen (the delegated real fsync made it durable).
   - `a_snapshot_fsyncs_the_snapshot_file_and_parent_dir_for_rename_durability` —
     assert `file_fsync_count` and `dir_fsync_count` both increased across
     `snapshot()`, and the snapshotted data is queryable on reopen.

   Assertions are observable and deterministic (count delta + reopen-read),
   no wall-clock, no p95.

3. AC-wal-fsync is now proven by:
   - **(b-i)** the in-suite `CountingFsyncBackend` fsync-per-append +
     fsync-around-snapshot scenarios (counts the seam), AND
   - **(b-ii)** the unchanged out-of-process AC-substrate-refusal scenario
     where the lying double makes the composition root REFUSE to start.

   It is NOT proven by injecting a lying double into the append path and
   asserting survival.

### State after the correction

- **lumen** production code (`open_with_fsync_backend` wiring `sync_all`
  per append + `atomic_write_snapshot` for the snapshot) was already
  landed by DELIVER step 1, so lumen's two corrected scenarios are
  **un-ignored and pass**: `cargo test -p lumen --test
  v1_slice_04_crash_durability` is **7/7 green, zero ignored**.
- The other five stores (ray, strata, cinder, sluice, beacon) keep their
  corrected scenarios **`#[ignore]`d** (their durability wiring is their
  own slice). Their `open_with_fsync_backend` is still a RED scaffold that
  panics — fine for an `#[ignore]`d test as long as it COMPILES. They
  reference the real `CountingFsyncBackend` symbol from `wal-recovery`.
- No production `file_backed.rs` of the five pending stores was touched —
  only their test files. Only `wal-recovery` gained the shared
  `CountingFsyncBackend` (plus a unit test); lumen/pulse production
  behaviour is unchanged.
