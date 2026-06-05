<!-- markdownlint-disable MD024 -->

# User Stories: cinder-wal-error-surfacing-v0

## Origin and Job Grounding

No DIVERGE artifacts exist for this feature (`docs/feature/cinder-wal-error-surfacing-v0/diverge/` is
absent). Origin is the **four-quadrants implementer assessment (Q2-MEDIUM)** and the **black-box
verifier's triage**, both of which read the cinder source directly. The job below is grounded in that
triage and the project's Earned-Trust durability posture (`docs/product/architecture/brief.md`
Principle 12; ADR-0049 / 0059 / 0060 lineage). Absence of DIVERGE is recorded as a risk in
`wave-decisions.md`; it does not block, because the defect and the fix direction are verified in code.

## The Operator Job (JTBD, Earned-Trust framing)

> **When** the tiering store cannot persist a placement or a migration because the disk is full or
> failing, **I want** the operation to FAIL LOUDLY and the in-memory state to stay consistent with what
> is actually on disk, **so that** a read never returns a placement that will vanish on restart, and I
> learn the disk is failing instead of silently losing tier decisions.

The current behaviour acks a tier change as durable when it was never written: the
acked-but-not-durable lie, the exact shape the Earned-Trust posture forbids.

## Verified Code Findings (confirming the verifier's read)

All confirmed by reading the source on this branch:

| Claim | Verified location | Finding |
|---|---|---|
| `TieringStore::place` returns `()` | `crates/cinder/src/store.rs:81` | Cannot report a persistence failure to its caller. |
| `TieringStore::evaluate_at` returns `usize` | `crates/cinder/src/store.rs:107` | Cannot report a persistence failure to its caller. |
| `TieringStore::migrate` returns `Result` | `crates/cinder/src/store.rs:93-99` | Already surfaces errors — the model for the fix. |
| `MigrateError::PersistenceFailed { reason: String }` exists | `crates/cinder/src/store.rs:49` | The error variant is already present; no new type needed. |
| cinder `place()` swallows the WAL error | `crates/cinder/src/file_backed.rs:270-278` | `if let Err(_e) = append_wal(...) { /* swallow */ }` then `apply_to_entries` runs UNCONDITIONALLY — memory is updated even when the WAL append failed. |
| cinder `evaluate_at()` swallows the WAL error | `crates/cinder/src/file_backed.rs:364-368` | `let _ = append_wal(...)` per migration inside the loop, then memory mutated unconditionally. |
| Ordering is memory-after-swallowed-WAL | `crates/cinder/src/file_backed.rs:270,278` | WAL append is attempted FIRST, but the failure does not gate the memory write — so it is effectively optimistic (backwards for a write-ahead log: a failed WAL must abort the memory mutation). |
| sluice has three swallow sites | `crates/sluice/src/file_backed.rs:346,356,366` | `let _ = append_wal(...)` in `dequeue` / `ack` / `nack` (NOT `place`/`evaluate_at` — sluice is the `Queue` trait, whose state-mutating ops also have no error channel). |
| sluice is UNWIRED (zero live blast radius) | grep of `crates/**/src/**` | `FileBackedQueue` is referenced only by its own crate, the `sluice_crash_target` bin, and the integration-suite. No gateway/server `src` path constructs or drives it. Confirmed not in the live ingest path. |
| `EnqueueError::PersistenceFailed { reason: String }` exists | `crates/sluice/src/queue.rs:65` | sluice's error variant is also already present. |

## Verified Caller List (who must handle the new `Result`)

### `place()` callers

| Caller | Location | Live? | Persistence-failure decision (DESIGN owns) |
|---|---|---|---|
| `flush()` in the gateway ingest path | `crates/kaleidoscope-cli/src/lib.rs:265` | **YES — live** | **Operator-visible**: does a failed tier placement fail the ingest, or log-and-continue? See US-02 + D2. |
| `place()` library fn (`place` CLI subcommand) | `crates/kaleidoscope-cli/src/lib.rs:543` | YES | Surface the error to the CLI exit code / stderr. |
| `cinder_crash_target` test binary | `crates/cinder/src/bin/cinder_crash_target.rs:84` | No (test) | `.expect(...)` or propagate. |
| integration-suite restart test | `crates/integration-suite/tests/v1_three_adapters_compose_under_restart.rs:140,141,234` | No (test) | Unwrap on the healthy path. |
| cinder unit/slice tests | `crates/cinder/tests/slice_01_*`, `slice_02_*`, `v1_slice_01_*`, `v1_slice_02_*`, `v1_slice_03_*`, `v1_slice_04_*` | No (test) | Unwrap on the healthy path. |
| CLI subcommand tests | `crates/kaleidoscope-cli/tests/*` (place, migrate, get_tier, list_items, stats_*, evaluate_policy, observe_otlp_*) | No (test) | Unwrap on the healthy path. |
| self-observe bridge tests | `crates/self-observe/tests/cinder_to_pulse.rs`, `cinder_to_otlp_json.rs` | No (test) | Drive `place`/`evaluate_at` through `InMemoryTieringStore`; unwrap (in-memory never fails). |

### `evaluate_at()` callers

| Caller | Location | Live? | Decision |
|---|---|---|---|
| `evaluate_policy()` library fn (`evaluate-policy` CLI subcommand) | `crates/kaleidoscope-cli/src/lib.rs:590` | YES | Surface the error; plus the **partial-vs-fail-whole** question (D3). |
| cinder lifecycle / durability tests | `crates/cinder/tests/slice_02_lifecycle.rs`, `v1_slice_01_wal_durability.rs` | No (test) | Unwrap on the healthy path. |
| self-observe bridge tests | `crates/self-observe/tests/cinder_to_pulse.rs`, `cinder_to_otlp_json.rs` | No (test) | Unwrap. |

### NOT callers (ripple does NOT reach them)

- The self-observe `CinderToPulseRecorder` and `CinderToOtlpJsonWriter` **bridges** consume the
  `cinder::MetricsRecorder` port (`record_place` / `record_migrate` / `record_evaluate`), NOT
  `TieringStore::place` / `evaluate_at`. The trait-signature change does not touch the bridge impls.
- `InMemoryTieringStore` (`crates/cinder/src/store.rs:139`) is an **impl** of the trait, not a caller —
  but it MUST be updated to the new signatures (it always returns `Ok`, never persisting).

## System Constraints

- **C1 — cinder is LIVE.** The gateway ingest path (`flush()` at `kaleidoscope-cli/src/lib.rs:265`)
  calls `place()` on every flushed batch. The caller-handling decision is operator-visible and
  load-bearing; it belongs in this feature, not deferred.
- **C2 — Write-ahead ordering, no torn memory.** The fix MUST append to the WAL FIRST and update
  in-memory state ONLY if the append succeeded. On failure the in-memory map must be left exactly as it
  was before the operation (no partial / torn mutation). `migrate()` already demonstrates this ordering
  (`file_backed.rs:316` — `append_wal(...)?` before `apply_to_entries`).
- **C3 — Public trait change is expected.** Changing `place` -> `Result<(), MigrateError>` and
  `evaluate_at` -> `Result<usize, MigrateError>` (or similar) is a public-API change. Gate 2
  (`cargo public-api`) and Gate 3 (semver) WILL flag it. This is correct and deliberate: an operation
  that persists must be able to fail. A semver-MINOR at most (these crates are pre-1.0). **Do NOT touch
  1.0.0** (CLAUDE.md / MEMORY: semver 1.0.0 is Andrea's call).
- **C4 — `MigrateError::PersistenceFailed` already exists** (`store.rs:49`); no new error type. The
  same is true for sluice's `EnqueueError::PersistenceFailed` (`queue.rs:65`).
- **C5 — Failing-disk is simulable in-suite.** The durability features already provide a
  `FsyncBackend` seam and `open_with_fsync_backend(...)` constructor (ADR-0060 §3) plus a lying /
  failing backend. The error-surfacing AC can be made falsifiable in-suite by injecting a backend whose
  `append_wal` I/O fails — no host-level disk-fill needed.
- **C6 — Mutation testing 100%** on modified files (ADR-0005 Gate 5; CLAUDE.md). The swallow-to-surface
  change and the write-ahead ordering are the primary mutation targets (the `?` must not be deletable
  without a surviving test; the abort-before-memory-write must not be reorderable).
- **C7 — Trunk-based, no CI gates** (MEMORY). CI is feedback, not a merge gate.

---

## US-01: Cinder place() surfaces a WAL persistence failure instead of swallowing it

### Problem

Priya runs a multi-tenant Kaleidoscope gateway. Cinder's `FileBackedTieringStore::place()` updates its
in-memory tier map even when the write-ahead-log append fails (a full disk, an I/O error): the failure
is dropped on the floor at `crates/cinder/src/file_backed.rs:270-278`. A subsequent `get_tier` read
returns a placement that was never written to disk, and that placement VANISHES on the next gateway
restart. Priya is never told the disk is failing. She finds it impossible to trust a read, because the
store will ack a tier decision as durable when it was never persisted — the acked-but-not-durable lie
the Earned-Trust posture exists to forbid.

### Elevator Pitch

- **Before**: `place()` on a failing disk silently updates memory, returns no error, and the
  un-persisted placement is readable until it vanishes on restart. The operator learns nothing.
- **After**: the operator-invocable path is `kaleidoscope place <tenant> <item> <tier>` (and the
  gateway ingest path that calls `place()`); on a WAL write failure the operation returns a
  `PersistenceFailed` error — stderr shows `error: persistence failed: io: <reason>` and a non-zero exit
  — AND a subsequent `kaleidoscope get-tier <tenant> <item>` does NOT return the un-persisted placement.
- **Decision enabled**: Priya decides to investigate the failing disk NOW (the operation told her it
  failed) instead of discovering vanished tier decisions after a restart. She can trust that any
  placement a read returns is on disk.

### Who

- Priya the platform operator | runs a live multi-tenant gateway with cinder in the ingest path |
  motivated to trust that an acked tier placement is actually durable, and to be told immediately when
  the disk cannot persist.

### Solution

Change `TieringStore::place` to return `Result<(), MigrateError>`. In `FileBackedTieringStore::place`,
follow write-ahead ordering: append to the WAL FIRST; only call `apply_to_entries` (the in-memory
mutation) and `record_place` if the append succeeded; on failure return
`Err(MigrateError::PersistenceFailed { .. })` and leave the in-memory map untouched.
`InMemoryTieringStore::place` returns `Ok(())` (it never persists). DESIGN owns the exact signature and
the caller ripple.

### Domain Examples

#### 1: Happy Path — Priya places a healthy-disk item

Priya runs `kaleidoscope place acme trade-001 hot` against a data dir on a healthy disk. The WAL append
succeeds, the in-memory map records `(acme, trade-001) -> Hot`, stdout prints
`placed tenant=acme item=trade-001 tier=hot`, exit code 0. A later
`kaleidoscope get-tier acme trade-001` prints `hot`, and after reopening the store the placement is
still `hot` — readable AND durable.

#### 2: Error/Boundary — Priya places onto a failing disk

Priya's data dir is on a disk whose WAL append fails (simulated in-suite by a failing `FsyncBackend` /
write backend). She runs `kaleidoscope place acme trade-002 warm`. The operation returns
`MigrateError::PersistenceFailed { reason: "io: no space left on device" }`; stderr shows
`error: persistence failed: io: no space left on device`, exit code non-zero. A subsequent
`get-tier acme trade-002` returns `None` (prints nothing / "not placed") — the un-persisted placement
is NOT readable. After a reopen, the store has no record of `trade-002`. Memory matched disk throughout.

#### 3: Edge Case — overwrite of an existing placement fails to persist

`(globex, batch-007)` is already durably `Hot`. Priya runs `kaleidoscope place globex batch-007 cold`
while the disk is failing. The WAL append fails; the operation returns `PersistenceFailed`; the
in-memory map STILL reads `Hot` for `(globex, batch-007)` (the failed overwrite did not torn-mutate the
prior durable value). A reopen confirms `Hot`. The operator's read is never corrupted by a failed write.

### UAT Scenarios (BDD)

#### Scenario: A failing disk makes a placement fail loudly

```gherkin
Given Priya has a cinder store whose WAL append fails (a failing-disk substrate)
When Priya places item "trade-002" for tenant "acme" in tier "warm"
Then the operation returns a persistence-failure error naming the disk reason
And no success is reported
```

#### Scenario: A placement that failed to persist is not readable

```gherkin
Given Priya has placed item "trade-002" for tenant "acme" against a failing disk
And the placement returned a persistence-failure error
When Priya reads the tier for "acme" / "trade-002"
Then the read returns no placement
And the in-memory state matches what is on disk (nothing was written)
```

#### Scenario: A failed overwrite preserves the prior durable placement

```gherkin
Given "globex" / "batch-007" is durably placed in tier "hot"
And the disk subsequently begins failing on WAL append
When Priya places "globex" / "batch-007" in tier "cold"
Then the operation returns a persistence-failure error
And reading the tier for "globex" / "batch-007" still returns "hot"
And after reopening the store the tier is still "hot"
```

#### Scenario: A healthy disk places and persists normally (negative control)

```gherkin
Given Priya has a cinder store on a healthy disk
When Priya places item "trade-001" for tenant "acme" in tier "hot"
Then the operation succeeds
And reading the tier for "acme" / "trade-001" returns "hot"
And after reopening the store the tier is still "hot"
```

### Acceptance Criteria

- [ ] On a failing-disk WAL append, `place()` returns `MigrateError::PersistenceFailed` (from scenario 1).
- [ ] After a failed `place()`, the item is absent from both memory and disk; a read returns no placement (from scenario 2).
- [ ] A failed overwrite leaves the prior durable placement intact in memory and on disk (from scenario 3).
- [ ] On a healthy disk, `place()` succeeds and the placement is readable and durable across a reopen (negative control).

### Outcome KPIs

- **Who**: cinder file-backed tiering store, in the live gateway ingest path.
- **Does what**: surfaces WAL persistence failures on `place` instead of swallowing them.
- **By how much**: swallowed-place WAL failures move from 1 site (today) to 0; the failing-disk `place`
  AC is falsifiable in-suite (passes only when the error is surfaced AND memory stays consistent).
- **Measured by**: the lying/failing-substrate acceptance test; `cargo mutants` kill rate on the
  `place` swallow site.
- **Baseline**: today `place()` swallows the error and updates memory optimistically (1 swallow site, 0 surfacing tests).

### Technical Notes

- Public trait change (C3): Gate 2 + Gate 3 will flag; expected, semver-MINOR at most, pre-1.0.
- Write-ahead ordering (C2): append before memory mutation; on failure leave memory untouched.
- `InMemoryTieringStore::place` returns `Ok(())`.
- Caller ripple: `flush()` (US-02), `place()` CLI fn, and all tests listed in the caller table.
- Reuse the existing `open_with_fsync_backend` seam (C5) to drive the failing-disk path in-suite.

---

## US-02: The live gateway ingest path handles a cinder tier-placement persistence failure

### Problem

The gateway ingest path's `flush()` (`crates/kaleidoscope-cli/src/lib.rs:265`) calls
`cinder.place(...)` once per flushed batch and ignores the (currently absent) error channel. Once
`place()` returns a `Result` (US-01), `flush()` must DECIDE what an ingest does when the tier placement
cannot be persisted: fail the ingest loudly, or log-and-continue. Today the question cannot even be
asked because the error does not exist. This is the operator-visible heart of the feature: Priya needs
the gateway's behaviour on a tier-persist failure to be a deliberate, documented choice — not an
accident of a swallowed `Result`.

### Elevator Pitch

- **Before**: the gateway ingest path drops the tier-placement persistence failure silently; the batch
  is reported as ingested even though its tier metadata was never persisted.
- **After**: the operator-invocable path is `kaleidoscope ingest <tenant> <file>` (the ingest
  subcommand that drives `flush()`); on a tier-placement persistence failure the gateway behaves per the
  documented decision (D2) — the operator sees either a failed ingest with a `persistence failed` reason
  on stderr, or a structured WARN plus a non-fatal continue, but NEVER a silent success that hides
  un-persisted tier metadata.
- **Decision enabled**: Priya knows, from the ingest output, whether her tier metadata is durable; she
  decides whether to retry the ingest or address the disk, instead of trusting a falsely-green ingest.

### Who

- Priya the platform operator | drives data into the gateway via the ingest path | motivated to know
  whether an ingest that "succeeded" actually persisted its tier metadata durably.

### Solution

In `flush()`, handle the `Result` from `cinder.place(...)`. DESIGN decides (D2) the exact policy:
fail-the-ingest (propagate as an ingest error, the batch is reported as not durably tiered) vs
log-and-continue (emit a structured WARN, continue ingesting, surface the failure in the ingest
summary). Whichever is chosen, the gateway must NOT report a clean success when a tier placement was not
persisted. The decision is recorded in `wave-decisions.md` D2 and crystallised by DESIGN.

### Domain Examples

#### 1: Happy Path — healthy-disk ingest

Priya runs `kaleidoscope ingest acme /var/data/acme-2026-06-05.ndjson` on a healthy disk. Every batch
flush places its `acme/batch-NNNNN` item in Hot tier; all WAL appends succeed; the ingest reports
`records_ingested=4200 batches_flushed=5 tier_items_placed=5`, exit 0. Every placed item is durable
across a reopen.

#### 2: Error/Boundary — disk fails mid-ingest (fail-the-ingest policy)

The disk fills after batch 3. On batch 4, `cinder.place(...)` returns `PersistenceFailed`. Under the
fail-the-ingest policy, `flush()` propagates the error; the ingest stops; stderr shows
`error: persistence failed: io: no space left on device`; exit non-zero. Priya sees exactly which batch
could not be tiered durably and retries after freeing space.

#### 3: Edge Case — disk fails mid-ingest (log-and-continue policy)

Same failure on batch 4. Under the log-and-continue policy, `flush()` emits a structured
`event=cinder.place.persist_failed` WARN naming the tenant, item, and reason; ingest continues; the
final ingest summary reports the count of batches whose tier metadata could NOT be persisted (non-zero),
so the operator still sees the failure — it is never silently green.

### UAT Scenarios (BDD)

#### Scenario: A healthy-disk ingest places all tier metadata durably (negative control)

```gherkin
Given Priya ingests a file for tenant "acme" on a healthy disk
When the ingest flushes its batches
Then every batch's tier placement is persisted durably
And the ingest reports success with the count of tier items placed
And after reopening the store every placed item is present
```

#### Scenario: A tier-placement persistence failure is never reported as a clean ingest success

```gherkin
Given Priya ingests a file for tenant "acme" and the disk begins failing on a batch flush
When the gateway attempts to place that batch's tier metadata and the WAL append fails
Then the gateway does NOT report a clean ingest success for that batch
And the operator is informed of the persistence failure with its disk reason
```

#### Scenario: The gateway's failure behaviour follows the documented decision

```gherkin
Given the gateway is configured per the D2 tier-persist-failure policy
When a tier placement fails to persist during ingest
Then the gateway behaves exactly as D2 specifies (fail-the-ingest OR log-and-continue with a non-silent summary)
And no un-persisted tier placement is presented to a later read as durable
```

### Acceptance Criteria

- [ ] On a healthy disk, ingest places all tier metadata durably and reports the placed count (negative control).
- [ ] A tier-placement persistence failure during ingest is never reported as a clean success (from scenario 2/3).
- [ ] The gateway's behaviour on a tier-persist failure matches the D2 decision recorded in `wave-decisions.md`.

### Outcome KPIs

- **Who**: Priya operating the live gateway ingest path.
- **Does what**: learns whether an ingest durably persisted its tier metadata (instead of a falsely-green success).
- **By how much**: silent tier-persist failures in the ingest path move from "always silent" to "always surfaced" (0% -> 100% surfaced).
- **Measured by**: the ingest-with-failing-disk acceptance test asserting the D2 behaviour; absence of any code path that reports success on a swallowed `place` error.
- **Baseline**: today `flush()` ignores the (absent) error channel; an ingest is reported green even when tier metadata was never persisted.

### Technical Notes

- Depends on US-01 (the `Result` must exist before `flush()` can handle it).
- D2 (fail-the-ingest vs log-and-continue) is the operator-visible decision DESIGN owns; this story
  encodes the requirement that the behaviour be deliberate and non-silent, not which branch is chosen.
- The `place()` and `evaluate_policy()` CLI library fns also gain `Result` handling (surface to exit
  code / stderr); they are simpler than the ingest path and ride US-01's signature change.

---

## US-03: Cinder evaluate_at() surfaces a WAL persistence failure during a policy sweep

### Problem

`FileBackedTieringStore::evaluate_at()` migrates many items in a loop, each with
`let _ = append_wal(...)` (`crates/cinder/src/file_backed.rs:364`), then mutates memory unconditionally.
A policy sweep on a failing disk silently migrates items in memory that were never persisted; the
sweep's returned count overstates what is durable, and the migrations vanish on restart. Priya runs
`kaleidoscope evaluate-policy` expecting the reported migration count to mean "this many items were
durably migrated" — today it can mean "this many were migrated in memory, an unknown number of which are
on disk." There is also a real un-decided question: when one migration's WAL append fails partway
through a multi-item sweep, does the whole sweep fail, or does it report a partial result?

### Elevator Pitch

- **Before**: `evaluate-policy` on a failing disk returns a migration count that includes migrations
  that were never persisted; they vanish on restart and the operator is told nothing.
- **After**: the operator-invocable path is `kaleidoscope evaluate-policy --hot-to-warm <s>
  --warm-to-cold <s>`; on a WAL write failure during the sweep the command surfaces a
  `persistence failed` error (per the D3 partial-vs-fail-whole decision) instead of returning a
  count that overstates durability, and every migration the count DOES report is on disk.
- **Decision enabled**: Priya trusts that the reported migration count equals the number of durably
  migrated items, and she learns immediately when the disk cannot persist a sweep — deciding to fix the
  disk rather than discovering vanished migrations after a restart.

### Who

- Priya the platform operator | runs periodic age-based tiering sweeps via `evaluate-policy` | motivated
  to trust that the reported migration count reflects what is actually durable on disk.

### Solution

Change `TieringStore::evaluate_at` to return `Result<usize, MigrateError>` (or a result that carries the
durable count). In `FileBackedTieringStore::evaluate_at`, for each migration append to the WAL FIRST and
mutate memory only on success. DESIGN decides D3: fail-the-whole-sweep on the first WAL error (return
`Err`, leaving already-applied migrations as-is or documented), OR report a partial durable count.
`InMemoryTieringStore::evaluate_at` returns `Ok(count)`. DESIGN owns the signature and the partial-vs-
fail-whole semantics; this story requires only that the count never overstate durability and that a
persistence failure be surfaced.

### Domain Examples

#### 1: Happy Path — healthy-disk sweep

Priya has 30 acme items aged past the hot->warm threshold on a healthy disk. She runs
`kaleidoscope evaluate-policy --hot-to-warm 86400 --warm-to-cold 604800`. All 30 WAL appends succeed;
stdout prints `evaluated migrated=30`; a reopen confirms all 30 are durably Warm. The reported count
equals the durable count.

#### 2: Error/Boundary — disk fails mid-sweep (fail-whole policy)

20 items are due for migration; the disk fails on the 8th. Under the fail-whole policy, `evaluate_at`
returns `MigrateError::PersistenceFailed`; the command exits non-zero with
`error: persistence failed: io: ...`. Priya learns the sweep could not complete durably and addresses
the disk before re-running. No reported count overstates durability.

#### 3: Edge Case — disk fails mid-sweep (partial-count policy)

Same failure on the 8th of 20. Under the partial-count policy, the command reports the durable count (7)
AND signals the failure (non-zero exit and/or a `persistence failed` note), so the operator sees both
how many were durably migrated and that the sweep was cut short. The 7 reported are all on disk after a
reopen; the un-migrated 13 remain in their prior durable tier.

### UAT Scenarios (BDD)

#### Scenario: A healthy-disk sweep reports a count equal to the durably-migrated items (negative control)

```gherkin
Given Priya has 30 items due for migration on a healthy disk
When Priya runs the policy evaluation sweep
Then the sweep reports 30 migrated
And after reopening the store all 30 items are durably in their new tier
```

#### Scenario: A sweep on a failing disk does not report migrations that were never persisted

```gherkin
Given Priya has items due for migration and the disk fails partway through the sweep
When Priya runs the policy evaluation sweep
Then the sweep surfaces a persistence-failure error
And the reported migration count never includes a migration that is not on disk
```

#### Scenario: The sweep's partial-vs-fail-whole behaviour follows the documented decision

```gherkin
Given the sweep is configured per the D3 partial-vs-fail-whole policy
When a migration's WAL append fails during a multi-item sweep
Then the sweep behaves exactly as D3 specifies
And every migration counted as done is durable on disk after a reopen
```

### Acceptance Criteria

- [ ] On a healthy disk, the sweep reports a count equal to the durably-migrated items (negative control).
- [ ] On a failing disk, the sweep surfaces a persistence-failure error and never counts a non-durable migration (from scenario 2/3).
- [ ] The sweep's partial-vs-fail-whole behaviour matches the D3 decision recorded in `wave-decisions.md`.

### Outcome KPIs

- **Who**: cinder file-backed tiering store under a policy sweep, driven by `evaluate-policy`.
- **Does what**: surfaces WAL persistence failures during a sweep and never reports a non-durable migration count.
- **By how much**: swallowed-sweep WAL failures move from per-migration silent (1 swallow site in a loop) to 0; reported count becomes equal to durable count.
- **Measured by**: the failing-substrate sweep acceptance test; `cargo mutants` on the `evaluate_at` swallow site and the count logic.
- **Baseline**: today `evaluate_at` swallows each migration's WAL error and the count overstates durability.

### Technical Notes

- Public trait change (C3): `evaluate_at -> Result<usize, MigrateError>`; Gate 2 + Gate 3 flag, expected, semver-MINOR.
- D3 (fail-whole vs partial count) is a real decision DESIGN owns; this story requires only non-overstatement and surfacing.
- Write-ahead ordering (C2) per migration: append before memory mutation; on failure do not torn-mutate that item.
- Caller: `evaluate_policy()` (`kaleidoscope-cli/src/lib.rs:590`) + the lifecycle/durability tests.

---

## US-04: Sluice surfaces its three swallowed WAL failures (uniformity; zero live blast radius)

### Problem

`FileBackedQueue` swallows WAL append failures at three sites — `dequeue`
(`crates/sluice/src/file_backed.rs:346`), `ack` (`:356`), and `nack` (`:366`) — each with
`let _ = append_wal(...)`. These are the same acked-but-not-durable lie as cinder's, in the `Queue`
trait's state-mutating ops that have no error channel. sluice is UNWIRED today (no gateway/server `src`
path constructs `FileBackedQueue`; only its own crate, the crash-target bin, and the integration-suite
reference it), so the live blast radius is ZERO. But leaving sluice's swallow sites in place while
fixing cinder's would create an inconsistent durability posture across the storage pillars: the next
operator to wire sluice would inherit the exact bug this feature exists to kill. For uniformity, sluice
should surface the same way.

### Elevator Pitch

- **Before**: sluice's `dequeue` / `ack` / `nack` silently swallow WAL persistence failures; a future
  wiring of sluice would inherit the acked-but-not-durable lie.
- **After**: there is no operator-invocable sluice path today (sluice is unwired), so the observable is
  at the library/test seam: sluice's three state-mutating ops surface a `PersistenceFailed` outcome on a
  failing-disk substrate instead of swallowing it, proven by a failing-substrate test — the same posture
  cinder now has.
- **Decision enabled**: when sluice IS eventually wired into a live path, its operator inherits a
  fail-loud-stay-consistent queue from day one, not a silent-data-loss queue to be discovered later.

> Note: this story is `@uniformity` — it has no live operator-invocable entry point today because sluice
> is unwired. Its value is preventing the cinder defect from being re-shipped under a different pillar.
> It is grouped in a separate release slice (R3) precisely so that the live-value cinder stories (US-01..
> US-03) are not gated on it.

### Who

- The future operator who wires sluice into a live path | inherits sluice's durability posture | motivated
  (transitively) to NOT inherit a silent-data-loss queue.
- The platform maintainer | keeps the storage pillars' durability posture uniform | motivated to avoid a
  known-bug-shaped landmine in an unwired pillar.

### Solution

Apply the same surface-the-error fix to sluice's three swallow sites. The `Queue` trait's `dequeue` /
`ack` / `nack` return `Option<Message>` / `()` / `()` respectively and have no error channel — DESIGN
decides the exact surfacing shape for sluice (its trait change, if any, and whether the fix mirrors
cinder's `Result` shape or uses a different channel given the `Option`/`()` returns). `EnqueueError::
PersistenceFailed` already exists (`queue.rs:65`). The write-ahead ordering and consistency-on-failure
requirements (C2) apply identically.

### Domain Examples

#### 1: Happy Path — healthy-disk dequeue/ack/nack

A sluice `FileBackedQueue` on a healthy disk: `dequeue` moves a message in-flight and its WAL record is
persisted; `ack` removes it durably; `nack` returns it to the head durably. All three persist; a reopen
reflects the final state. (Driven via the library/test seam, as sluice is unwired.)

#### 2: Error/Boundary — failing disk on dequeue

On a failing-disk substrate, `dequeue` cannot persist its `Dequeue` WAL record. Instead of swallowing,
the operation surfaces the persistence failure (per DESIGN's chosen shape) and does NOT leave the
in-memory queue state inconsistent with disk (the message is not silently moved in-flight in memory
while absent from the WAL).

#### 3: Edge Case — failing disk on ack/nack

On a failing-disk substrate, `ack` / `nack` cannot persist their WAL record. The operation surfaces the
failure rather than swallowing it; the in-flight / pending state stays consistent with what is durable.

### UAT Scenarios (BDD)

#### Scenario: Healthy-disk queue operations persist durably (negative control)

```gherkin
Given a sluice queue on a healthy disk with a message enqueued
When the consumer dequeues, then acks the message
Then both operations persist their WAL records durably
And after reopening the queue the final state is reflected
```

#### Scenario: A failing disk on dequeue is surfaced, not swallowed

```gherkin
Given a sluice queue whose WAL append fails (a failing-disk substrate)
When the consumer dequeues a message
Then the persistence failure is surfaced rather than swallowed
And the in-memory queue state stays consistent with what is on disk
```

#### Scenario: A failing disk on ack/nack is surfaced, not swallowed

```gherkin
Given a sluice queue with an in-flight message and a failing-disk substrate
When the consumer acks (or nacks) the message
Then the persistence failure is surfaced rather than swallowed
And the in-flight/pending state stays consistent with what is on disk
```

### Acceptance Criteria

- [ ] Healthy-disk dequeue/ack/nack persist durably and survive a reopen (negative control).
- [ ] A failing-disk dequeue surfaces the persistence failure and keeps memory consistent with disk.
- [ ] A failing-disk ack/nack surfaces the persistence failure and keeps memory consistent with disk.

### Outcome KPIs

- **Who**: sluice file-backed queue (unwired today; future live consumers).
- **Does what**: surfaces WAL persistence failures at dequeue/ack/nack instead of swallowing them.
- **By how much**: sluice swallow sites move from 3 to 0; storage-pillar durability posture becomes uniform (cinder + sluice both fail-loud).
- **Measured by**: the failing-substrate sluice tests; `cargo mutants` on the three swallow sites.
- **Baseline**: today sluice swallows at 3 sites (`dequeue`/`ack`/`nack`); zero live blast radius but a latent landmine.

### Technical Notes

- sluice is UNWIRED (zero live blast radius today) — verified by grep of `crates/**/src/**`.
- The `Queue` trait surfacing shape (`dequeue`->`Option`, `ack`/`nack`->`()`) differs from cinder's; DESIGN owns sluice's exact channel.
- `EnqueueError::PersistenceFailed` already exists (`queue.rs:65`).
- Grouped in R3 so the live-value cinder stories are not gated on this uniformity work.
- Per ADR-0060 §6, sluice still carries the parse-or-die recovery loop (its torn-tail migration is the
  tracked ADR-0059 §5 follow-up); the error-surfacing fix is orthogonal to that and lands independently.
