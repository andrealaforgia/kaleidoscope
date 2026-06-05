# ADR-0065 — Cinder surfaces WAL persistence failures: `place`/`evaluate_at` become fallible, write-ahead-ordered (amends ADR-0060 C1 byte-identity for `TieringStore`)

- **Status**: Accepted
- **Date**: 2026-06-05
- **Author**: `nw-solution-architect` (Morgan)
- **Feature**: `cinder-wal-error-surfacing-v0`
- **Supersedes**: none
- **Superseded by**: none
- **Amends**: **ADR-0060 §"Positive"/Decision-3 constraint C1** *only as it applies to the
  `TieringStore` trait*. ADR-0060 deliberately PRESERVED `TieringStore` byte-identity (the
  `FsyncBackend` was injected through an inherent `open_with_fsync_backend` constructor precisely
  so the trait surface stayed unchanged). This ADR makes the narrow, justified departure ADR-0060
  itself flagged was out of its scope: it changes the `TieringStore` trait so two of its
  state-mutating operations can report a persistence failure. ADR-0060 remains Accepted and is NOT
  edited (ADRs are immutable); this ADR records the amendment to its C1 constraint *for the cinder
  trait surface*. ADR-0060's C1 continues to hold byte-identically for every OTHER store trait
  (`LogStore`, `TraceStore`, `MetricStore`, beacon `RuleStateStore`) and for cinder's
  `open_with_fsync_backend` constructor, which stays an inherent (non-trait) method.
- **Related**: ADR-0049 (`earned-trust-fsync-probe-v0` — the per-record `sync_all` discipline and
  the `FsyncBackend`/`fsync_probe` family this ADR's failing-substrate seam injects through; cited,
  NOT modified). ADR-0059 (`wal-torn-tail-recovery-v0` — the read-back mirror; cinder is already on
  the shared `replay_wal_tolerating_torn_tail` routine, so this ADR's write-side surfacing produces
  exactly the residue that recovery already handles; cited, NOT modified). ADR-0060
  (`store-fsync-durability-v0` — the WAL fsync + atomic snapshot that this ADR now makes *reportable*
  at the trait boundary; cinder already has `open_with_fsync_backend` + `atomic_write_snapshot`
  wired from ADR-0060; amended re C1 as above). ADR-0064 (`cli-ingest-atomic-v0` — the all-or-nothing
  ingest commit discipline; D2 below extends that posture to the *write-failure* axis ADR-0064 §"Out
  of scope" explicitly LEFT to the ADR-0059/0060 line; cited and extended). ADR-0005 (the five CI
  gates — Gate 2 `cargo public-api` byte identity and Gate 3 semver WILL flag the trait change; that
  is expected and correct, see Decision D1; Gate 5 100% mutation kill on modified files).

## Context

Cinder's `FileBackedTieringStore` acknowledges a tier decision as durable when it was never written
to disk — the **acked-but-not-durable lie** the project's Earned-Trust posture exists to forbid
(brief Principle 12; the ADR-0049/0059/0060 durability lineage). Two swallow sites, both verified in
code on this branch:

| Site | Location | Defect |
|---|---|---|
| `place()` | `crates/cinder/src/file_backed.rs:270-278` | `if let Err(_e) = append_wal(...) { /* swallow */ }`, then `apply_to_entries` runs **unconditionally** — the in-memory map is mutated even when the WAL append failed. |
| `evaluate_at()` | `crates/cinder/src/file_backed.rs:364-368` | `let _ = append_wal(...)` per migration inside the loop, then memory mutated unconditionally; the returned `usize` count **overstates** what is durable. |

The trait cannot report the failure even if the adapter wanted to: `TieringStore::place(...) -> ()`
(`store.rs:81`) and `TieringStore::evaluate_at(...) -> usize` (`store.rs:107`) have no error channel.
A subsequent `get_tier` read returns a placement that was never persisted; it **vanishes on the next
restart**, and the operator (Priya) is never told the disk is failing.

The fix is two-part and they are inseparable:

1. **Surface** — the trait must be able to report the failure (a public-API change).
2. **Write-ahead order** — the in-memory mutation must happen ONLY after the WAL append succeeds, so
   a failure leaves memory consistent with disk AND is reported. Today `place` attempts the append
   first but does NOT gate the memory write on it — which is *backwards for a write-ahead log*:
   a failed WAL must ABORT the memory mutation, not be ignored after it.

`migrate()` (`file_backed.rs:295-321`) ALREADY does this correctly: `append_wal(...)?` BEFORE
`apply_to_entries`. It is the in-crate model the fix generalises to `place` and `evaluate_at`. The
error variant `MigrateError::PersistenceFailed { reason: String }` ALREADY EXISTS (`store.rs:49`) and
is already produced by `append_wal` via the `io`/`parse` helpers — **no new error type is needed**.

The failing-disk substrate is already simulable in-suite: ADR-0060 wired
`open_with_fsync_backend(base_path, recorder, fsync_backend)` and the shared `wal-recovery`
`FsyncBackend` family (`RealFsyncBackend` + a lying/failing variant). DISTILL injects a write/fsync
backend whose `fsync_file` (or the underlying write) returns `io::Error`, making the failure ACs
falsifiable with no host-level disk-fill (constraint C5).

This feature originates from the four-quadrants implementer assessment (Q2-MEDIUM) and the black-box
verifier triage; both read the cinder source directly. No DIVERGE wave preceded it (recorded as a
low-impact risk in DISCUSS `wave-decisions.md`); the defect and fix direction are verified in code,
so the absence does not block. ADR-0065 is the next free number (highest existing 0064, verified by
`ls docs/product/architecture/adr-*.md`).

## Decision

Make cinder's two swallowing tier operations **fallible and write-ahead-ordered**, surface the
failure through the existing `MigrateError::PersistenceFailed`, map every caller (D1), make the live
ingest path fail-the-ingest (D2), make the sweep fail-whole on the first WAL error (D3), and apply
the same surface-the-error fix to sluice's three swallow sites through sluice's own `Queue`-trait
change (D4). The full caller ripple is mapped and bounded below.

### D1 — The `TieringStore` trait signature change + the full caller ripple

**New signatures** (the `migrate` shape, generalised):

```text
// crate: cinder, file: src/store.rs  — the TieringStore trait
fn place(&self, tenant: &TenantId, item: &ItemId, tier: Tier, placed_at: SystemTime)
    -> Result<(), MigrateError>;            // was: -> ()

fn evaluate_at(&self, now: SystemTime, policy: &TierPolicy)
    -> Result<usize, MigrateError>;         // was: -> usize
```

Rationale for reusing `MigrateError` (not a new narrower error and not a type alias): the variant
`PersistenceFailed { reason }` already exists and is already what `append_wal` produces; `migrate`
already returns `Result<(), MigrateError>`; reusing it keeps the three state-mutating trait ops
(`place`, `migrate`, `evaluate_at`) error-type-uniform and adds ZERO new public types. The name
`MigrateError` is slightly broader than its origin but already documents `PersistenceFailed` as the
adapter-side I/O variant (`store.rs:46-49`); renaming it would be a gratuitous, larger public-API
churn for no behavioural gain. The crafter owns the exact `Result`-handling at each call site; DESIGN
pins the signatures and the per-caller handling policy.

**`FileBackedTieringStore` adapter behaviour** (write-ahead ordering, C2):

- `place`: build the `WalRecord::Place`, lock state, `append_wal(...)?` FIRST; only on `Ok` call
  `apply_to_entries(&mut state.entries, record)` and `self.recorder.record_place(...)`, then return
  `Ok(())`. On the `?` early-return the in-memory map is **untouched** — including the
  overwrite-an-existing-placement case: because the prior value is only replaced by `apply_to_entries`
  which never runs on failure, a failed overwrite leaves the prior durable value intact (US-01 #3).
- `evaluate_at`: see D3.

**`InMemoryTieringStore`** (`store.rs:139`): `place` returns `Ok(())`; `evaluate_at` returns
`Ok(count)`. It never persists, so it never produces `PersistenceFailed`. It is an *impl* (not a
caller) but MUST be updated to the new signatures.

**Full caller ripple** (verified by grep of `.place(` / `.evaluate_at(` across `crates/**/*.rs`;
20 files):

| Caller | Location | Live? | New handling |
|---|---|---|---|
| `flush()` gateway ingest | `kaleidoscope-cli/src/lib.rs:265` | **YES (live)** | **D2: propagate** — `cinder.place(...).map_err(Error::CinderPlace)?;` (fail-the-ingest). |
| `place()` CLI lib fn | `kaleidoscope-cli/src/lib.rs:543` | YES | `cinder.place(...).map_err(Error::CinderPlace)?;` before the `placed …` stdout line — on failure, nothing is printed, non-zero exit. |
| `evaluate_policy()` CLI lib fn | `kaleidoscope-cli/src/lib.rs:590` | YES | `let migrated = cinder.evaluate_at(...).map_err(Error::CinderEvaluate)?;` (D3: whole-sweep error → non-zero exit, no `evaluated migrated=` line). |
| `cinder_crash_target` bin | `cinder/src/bin/cinder_crash_target.rs:84` | No (test) | `.expect("place")` / propagate — crash-target's job is to crash, not to assert. |
| integration-suite restart test | `integration-suite/tests/v1_three_adapters_compose_under_restart.rs:140,141,234` | No (test) | `.unwrap()` on the healthy path. |
| cinder slice/lifecycle/durability tests | `cinder/tests/slice_01_*`, `slice_02_*`, `v1_slice_01..04_*` | No (test) | `.unwrap()` on the healthy path; new failing-substrate tests `assert!(matches!(.., Err(PersistenceFailed{..})))`. |
| CLI subcommand tests | `kaleidoscope-cli/tests/{place,evaluate_policy,get_tier,list_items,migrate,stats_*,observe_otlp_*}_*` | No (test) | `.unwrap()` on the healthy path. |
| self-observe bridge tests | `self-observe/tests/cinder_to_pulse.rs`, `cinder_to_otlp_json.rs` | No (test) | Drive `place`/`evaluate_at` through `InMemoryTieringStore`; `.unwrap()` (in-memory never fails). |

**NOT callers** (ripple does NOT reach them — confirmed): the self-observe `CinderToPulseRecorder`
and `CinderToOtlpJsonWriter` **bridges** consume the `cinder::MetricsRecorder` port
(`record_place`/`record_migrate`/`record_evaluate`), NOT `TieringStore::place`/`evaluate_at`. The
trait-signature change does not touch the bridge impls.

**CLI `Error` enum** (`kaleidoscope-cli/src/lib.rs:73`): add two thin variants for operator-message
clarity:

```text
CinderPlace(MigrateError),       // "cinder place: persistence failed: io: …"
CinderEvaluate(MigrateError),    // "cinder evaluate: persistence failed: io: …"
```

Reusing the existing `CinderMigrate(MigrateError)` variant is *behaviourally* sufficient (same
`MigrateError`, same exit path) and is an acceptable crafter shortcut; the two new variants only
sharpen the stderr prefix so the operator sees which operation failed. Either is Gate-2-visible on
the *cli* crate (a new enum variant), which is fine — the cli crate is pre-1.0 and the variant is
additive. The crafter owns the final choice; DESIGN's recommendation is the explicit pair.

**Public-API + semver impact (expected, correct)**: the `TieringStore` trait change is a breaking
change to cinder's public surface. **Gate 2 (`cargo public-api`) and Gate 3 (semver) WILL flag it —
that is the intended, correct signal**, not a regression: an operation that persists must be able to
fail. It is a deliberate **semver-MINOR** bump for the `cinder` crate (pre-1.0, where a minor bump
may carry breaking changes per Cargo's pre-1.0 semantics). **NEVER 1.0.0** — 1.0.0 is Andrea's call
(CLAUDE.md / MEMORY); this ADR does not authorise it. DEVOPS/DELIVER annotate the expected
`cargo public-api` diff and the cinder minor bump.

### D2 — Live gateway/CLI ingest behaviour on a tier-persist failure: **fail-the-ingest**

When `cinder.place(...)` returns `PersistenceFailed` inside `flush()`, the ingest **fails loudly**:
`flush` propagates the `Result` (`…map_err(Error::CinderPlace)?`), the ingest stops, `main.rs` prints
`error: cinder place: persistence failed: io: <reason>` to stderr and exits **non-zero**; the batch
whose tier metadata could not be persisted is NOT reported as ingested. Nothing is acked as durable
that is not on disk.

**Why fail-the-ingest, not log-and-continue** (Luna's non-binding lean, confirmed by DESIGN with the
architecture in view):

- **Earned-Trust + ADR-0064 consistency.** ADR-0064 made ingest all-or-nothing on a *parse* error
  and explicitly left mid-commit *write*-failure atomicity to "the ADR-0059/0060 line of work,
  unchanged by this wave." This feature IS that line of work for the cinder write path. The same
  posture — a command that cannot durably commit must not report a clean success — applies on the
  write-failure axis. Log-and-continue would re-introduce, on the durability axis, exactly the
  silently-degraded success ADR-0064 removed on the parse axis.
- **`flush` already returns `Result<(), Error>`** and `lumen.ingest(...)` already propagates with `?`
  (`lib.rs:262`). Propagating the cinder error is the *smaller*, more uniform change — it matches how
  the sibling store (Lumen) failure is already handled one line above. Log-and-continue would require
  a NEW non-fatal accumulation channel and a NEW ingest-summary field, a larger surface for the less
  Earned-Trust-consistent behaviour.
- **A failing disk is not a transient, per-record condition** the operator wants to ride through; it
  is a substrate fault the operator must learn about immediately and fix. Fail-fast surfaces it at
  the first failed batch, naming it (US-02 #2).

**Satisfies**: US-02 AC "a tier-placement persistence failure during ingest is never reported as a
clean success" and "the gateway's behaviour matches the D2 decision recorded in `wave-decisions.md`."
This branch is now a **locked AC for DISTILL**.

Consequence (recorded, accepted): under fail-the-ingest, batches BEFORE the failing one in the same
`flush`-per-chunk loop may already be committed to Lumen+Cinder durably — this is the same
mid-commit partial-progress property ADR-0064 §"Out of scope" already named for the write-failure
case; this ADR does not add cross-store rollback (that remains a saga-shaped non-goal). What it
guarantees is that the FAILED batch is never reported durable and the operator learns at the first
failure. Whole-ingest write-atomicity (rollback of earlier batches) is explicitly out of scope and
recorded as a future concern, consistent with ADR-0064.

### D3 — `evaluate_at` multi-item sweep: **fail-whole on the first WAL error**

When one migration's WAL append fails partway through the sweep, `evaluate_at` returns
`Err(MigrateError::PersistenceFailed { .. })` on that first error (the `?` on `append_wal`), exactly
as `migrate` does for a single item. It does NOT continue the loop and does NOT return a partial
count.

**Post-failure invariant (precise)**: for each item the loop reaches, write-ahead ordering holds —
`append_wal(...)?` BEFORE the per-item memory mutation (`entry.tier = to; entry.migrated_at = now`).
So at the moment of failure:

- every migration BEFORE the failing one is **durable on disk AND applied in memory** (memory ==
  disk for those — each was WAL-appended then applied);
- the FAILING migration is **neither on disk nor applied in memory** (the `?` returns before its
  memory mutation — no torn half-applied item);
- every migration AFTER the failing one is untouched (the loop never reaches it) — those items remain
  in their prior durable tier.

The function returns `Err`, carrying NO count. Therefore **the returned count never overstates
durability** — there is no returned count on failure at all; on success the returned `usize` equals
the number of WAL-appended-then-applied migrations, which is exactly the durable count. Memory is
consistent with disk at every instant (no torn migration).

**Why fail-whole, not partial-count**:

- **Simplest honest contract.** A `Result<usize, MigrateError>` where `Ok(n)` means "n durably
  migrated" and `Err` means "the sweep could not complete durably" is unambiguous and matches the
  `migrate`-shape the crate already uses. A partial-count variant (`Ok((durable_count, Some(error)))`
  or a custom struct) is a NEW public type carrying a "succeeded-but-also-failed" shape that every
  caller must then destructure — more surface, more ways to mis-handle, for an operator action
  (`evaluate-policy`) that is a periodic sweep the operator simply re-runs after fixing the disk.
- **The count's MEANING stays crisp.** The US-03 requirement is "the reported count never overstates
  durability." Fail-whole satisfies it by construction (there is no count on failure); the operator
  is never handed a number that mixes durable and lost migrations.
- A failing disk mid-sweep is, again, a substrate fault to fix and re-run, not a condition to
  partial-complete-through. The already-durable prefix is real and survives (the invariant above);
  re-running after the fix re-evaluates the remaining due items idempotently (`evaluate_at` is
  idempotent under stable `now`+policy per the trait doc, `store.rs:104-106`).

**Satisfies**: US-03 AC "on a failing disk, the sweep surfaces a persistence-failure error and never
counts a non-durable migration" and "matches the D3 decision recorded in `wave-decisions.md`." This
branch is now a **locked AC for DISTILL**.

### D4 — sluice's surfacing channel: change the `Queue` trait's three state-mutating ops to fallible (thinner, separately-justified, R3)

sluice's `Queue` trait differs from cinder's: `dequeue(&self, tenant) -> Option<Message>`,
`ack(&self, id) -> ()`, `nack(&self, id) -> ()` (`crates/sluice/src/queue.rs`). The cinder `Result`
shape does NOT transfer 1:1. Three swallow sites: `dequeue` (`file_backed.rs:346`), `ack` (`:356`),
`nack` (`:366`), each `let _ = append_wal(...)`. `enqueue` already does `append_wal(...)?`
(`:318`) — it is sluice's in-crate model, the mirror of cinder's `migrate`.

**Decision**: change the three `Queue` methods to surface the persistence failure through sluice's
existing `EnqueueError::PersistenceFailed` (`queue.rs:65` — already exists, no new type):

```text
// crate: sluice, file: src/queue.rs — the Queue trait
fn dequeue(&self, tenant: &TenantId) -> Result<Option<Message>, EnqueueError>;  // was: -> Option<Message>
fn ack(&self, id: MessageId)  -> Result<(), EnqueueError>;                      // was: -> ()
fn nack(&self, id: MessageId) -> Result<(), EnqueueError>;                      // was: -> ()
```

`dequeue` becomes `Result<Option<Message>, _>`: the `Option` (queue empty vs message present) is an
orthogonal axis from the I/O `Result` (persist succeeded vs failed), so they nest rather than
collapse — `Ok(None)` = empty queue, `Ok(Some(m))` = dequeued and persisted, `Err(_)` = could not
persist the `Dequeue` record. Write-ahead ordering (C2) applies identically: append the WAL record
FIRST, mutate the in-memory `pending`/`in_flight`/`total` ONLY on `Ok`. On `dequeue` failure the
popped message is NOT moved in-flight in memory while absent from the WAL (it stays pending,
consistent with disk); on `ack`/`nack` failure the in-flight/pending state is not mutated.

**Why this is the lower-risk, separately-justified slice (the carpaccio cut-line)**: sluice is
**UNWIRED** — verified by grep of `crates/**/src/**`: `FileBackedQueue` is referenced only by its own
crate (`sluice/src/{lib,file_backed,queue}.rs`), the `sluice_crash_target` bin, and the
integration-suite. NO gateway/server `src` path constructs or drives it. **Zero live blast radius
today.** The `Queue` trait change ripples only to sluice's own tests, the crash-target bin, and the
integration-suite — no live caller, no operator-visible CLI surface. It is grouped in R3 precisely so
the live-value cinder stories (US-01..US-03) are never gated on it; if the cinder ripple proves large
in DELIVER, US-04 may split into a separate follow-up feature with no loss of cinder value (the map
already isolates it). Its value is purely **uniformity**: preventing the cinder defect from being
re-shipped under the queue pillar when sluice is eventually wired.

**Note** (carried from ADR-0060 §6): sluice still carries the inline parse-or-die recovery loop; its
torn-tail migration is the tracked ADR-0059 §5 follow-up. The error-surfacing fix here is orthogonal
to that and lands independently.

### Failing-substrate test seam (makes the failure ACs falsifiable — the false-confidence guard)

The failure ACs MUST be proven by a substrate that makes the un-surfaced (buggy) path OBSERVABLY
fail, or the test would pass on the swallow bug (the ADR-0060 §1 / ADR-0049 false-confidence trap,
recorded as a HIGH-impact risk in DISCUSS). Mechanism:

- DISTILL injects, through the EXISTING `FileBackedTieringStore::open_with_fsync_backend(base_path,
  recorder, fsync_backend)` (and sluice's `FileBackedQueue::open_with_fsync_backend`), a
  **failing** `FsyncBackend` whose `fsync_file` returns `io::Error` (e.g. a `FailingFsyncBackend`, or
  the existing `LyingFsyncBackend` extended with a `failing` mode that errors instead of silently
  dropping). Because `append_wal` calls `fsync_backend.fsync_file(wal.get_ref()).map_err(io)?`
  (`file_backed.rs:418`), a failing backend makes `append_wal` return
  `Err(PersistenceFailed { reason: "io: …" })` deterministically, in-process, with no host disk-fill
  (constraint C5).
- **Falsifiability proof**: the failing-`place` test asserts BOTH that the call returns
  `Err(PersistenceFailed{..})` AND that a follow-up `get_tier` returns the prior value (or `None`)
  AND that a reopen confirms disk == memory. On the CURRENT swallow bug the call returns `()` (no
  error) and `get_tier` returns the un-persisted placement — so the test FAILS on the bug and PASSES
  only when the error is surfaced AND memory stayed consistent. This is the guard against inheriting
  a test that cannot fail on the defect it guards.
- The negative-control (healthy `RealFsyncBackend`) tests assert the existing happy-path behaviour is
  unchanged (places/migrates persist, readable AND durable across reopen) — the guardrail that the
  surfacing change does not regress the green path.

## Alternatives considered

### A (rejected): keep the `()`/`usize` signatures, log-only on WAL failure

For: no public-API change; Gate 2/Gate 3 stay green; no caller ripple. Against: it does NOT close the
lie. A logged-but-unreported failure still updates memory optimistically (or, if memory is left
untouched but no error returned, the caller cannot distinguish a placed item from a dropped one and
still reports success). The operator's `place`/ingest still returns success; a read still returns a
vanishing placement; the live ingest path (D2) cannot fail-the-ingest because it has no error to
propagate. Logging is observability, not a contract — it cannot make `flush` decline to ack a
non-durable batch. Rejected: the whole point is that the OPERATION reports the failure to its caller,
not that a log line exists somewhere. (This is precisely the "v2's job" comment the current
swallow-site carries — this feature IS that v2.)

### B (rejected): a side-channel error callback / out-of-band error sink on the store

For: the trait signature stays byte-identical (preserves ADR-0060 C1 for cinder too); failures are
delivered via a registered `on_persist_error(&MigrateError)` callback. Against: it makes the failure
*asynchronous to the operation* — the caller's `place(...)` returns before (or without) knowing
whether the callback fired, so `flush` STILL cannot decide synchronously whether to ack the batch.
It splits one atomic "did this persist?" question across two control-flow paths, is far harder to
test deterministically, and is a strictly more complex public surface (a registration API + a
callback type) than returning a `Result` the language already has. It also cannot express the
write-ahead *ordering* guarantee at the call site (the caller can't tell whether memory was mutated).
Rejected: a synchronous `Result` is the simplest mechanism that lets the caller decide, and it
matches `migrate` which the crate already ships.

### C (rejected): panic / abort the process on a WAL failure

For: maximally loud; impossible to ignore; no caller ripple (no `Result` to thread). Against: a panic
on a per-record I/O failure turns a recoverable, per-tenant, per-item operation into a whole-process
crash that takes down every other tenant's in-flight work in the live gateway. A failing disk on ONE
placement should fail THAT operation, not abort the gateway. It is also untestable as a *surfaced
error* (you cannot assert a structured `PersistenceFailed` on a panic without catch-unwind gymnastics)
and it removes the operator's ability to choose policy (D2's fail-the-ingest is a *decision*, not a
forced crash). Rejected: the failure must be a reportable, recoverable error value, not a process
abort.

### D3-alt (rejected): partial-count on the sweep

For: the operator sees how many migrated durably before the failure. Against: it needs a NEW public
return type (a `(usize, Option<MigrateError>)` or a struct) that every caller must destructure, for a
periodic operator sweep that is simply re-run after the disk is fixed. The fail-whole `Result<usize,
MigrateError>` already preserves the durable prefix (the post-failure invariant in D3) and the
re-run is idempotent, so the operator loses nothing by fail-whole except a number they would act on
identically. Rejected in favour of the simpler, uniform `migrate`-shape contract (D3).

### D4-alt (rejected): mirror cinder's `Result<(), MigrateError>` onto sluice verbatim

For: one uniform shape across both pillars. Against: sluice's `dequeue` legitimately returns an
`Option` (empty queue) and uses its OWN error type `EnqueueError` (`queue.rs`), not `MigrateError` —
forcing cinder's exact shape would either discard the `Option` semantics or import cinder's error
type into the queue pillar (a cross-pillar type dependency). Nesting `Result<Option<Message>,
EnqueueError>` and reusing sluice's existing `EnqueueError::PersistenceFailed` keeps each pillar's
error vocabulary its own (no cross-pillar coupling) while delivering the identical fail-loud-stay-
consistent posture. Rejected verbatim mirroring in favour of the pillar-local `Result` (D4).

## Consequences

### Positive

- **The acked-but-not-durable lie is closed on cinder's live path.** `place`/`evaluate_at` report the
  WAL failure; the live ingest fails-the-ingest (D2); a read never returns an un-persisted placement;
  the sweep count never overstates durability (D3). North-star: swallow sites 2/2 → 0/2 in cinder
  (and 3/3 → 0/3 in sluice), each with a falsifiable surfacing test.
- **Write-ahead ordering is now correct AND uniform** across `place`, `migrate`, `evaluate_at` — all
  three append-then-apply, the `migrate` discipline generalised. No torn memory on failure (C2);
  a failed overwrite preserves the prior durable value (US-01 #3) by construction.
- **Zero new types.** `MigrateError::PersistenceFailed` and `EnqueueError::PersistenceFailed` already
  exist; the fix reuses them. The only additive public surface is the two trait-signature changes
  (cinder), the `Queue` trait changes (sluice), and two thin CLI `Error` variants — all additive,
  pre-1.0, semver-MINOR.
- **The failure ACs are falsifiable in-suite** through the existing `open_with_fsync_backend` +
  `FsyncBackend` seam (ADR-0060), so the false-confidence trap is structurally avoided: the test
  fails on the swallow bug and passes only on the surfaced-and-consistent fix.
- **sluice's latent landmine is removed** while it is still unwired (zero live blast radius), so the
  next operator to wire it inherits a fail-loud queue from day one — uniform with cinder.

### Negative (accepted, flagged)

- **Public-API + semver break (expected).** Gate 2 (`cargo public-api`) and Gate 3 (semver) flag the
  cinder `TieringStore` and sluice `Queue` trait changes. This is the correct signal, not a
  regression: an operation that persists must be able to fail. Semver-MINOR, pre-1.0, NEVER 1.0.0.
  DEVOPS/DELIVER annotate the expected public-api diff.
- **Caller ripple touches ~15 files** (one live cinder caller `flush`, two CLI lib fns, the InMemory
  impl, the crash-target bin, the integration-suite, ~10 test files; sluice's ripple is its own tests
  + crash-target + integration-suite). Bounded and enumerated above; mostly mechanical
  `.unwrap()`/`?` additions on the healthy path. If it balloons, sluice (US-04) is the pre-defined
  carpaccio cut.
- **D2 fail-the-ingest does not roll back earlier durable batches.** Whole-ingest write-atomicity
  (cross-store rollback) stays out of scope, consistent with ADR-0064 §"Out of scope". The guarantee
  is that the FAILED batch is never acked durable and the operator learns at the first failure — not
  that the whole ingest is atomic on a write failure.
- **Amends ADR-0060 C1 for the cinder trait.** The byte-identity ADR-0060 preserved for
  `TieringStore` is deliberately broken here (only for cinder's trait; every other store trait and
  cinder's `open_with_fsync_backend` constructor keep byte-identity). Recorded as an explicit
  amendment; ADR-0060 stays immutable and Accepted.

### Trade-off summary

The feature trades "preserve the `TieringStore` byte-identity ADR-0060 kept (simpler CI, but the
store keeps lying)" against "make the persisting operations fallible and write-ahead-ordered (a
flagged-and-correct public-API break, but the store stops acking non-durable tier decisions)". v0/v1
takes the latter — making the durability promise *reportable* is the entire point of an Earned-Trust
hardening feature, and ADR-0060 itself flagged that the trait byte-identity was a constraint of THAT
feature's scope, not a permanent contract.

## Enforcement (Earned-Trust three orthogonal layers)

Per the methodology and the ADR-0049/0059/0060 precedent, the surfacing contract is enforced by three
semantically orthogonal layers; a single-layer bypass is caught by at least one of the other two:

- **(a) subtype/type check** — the `TieringStore` and `Queue` traits now have fallible signatures;
  `FileBackedTieringStore`/`FileBackedQueue`/`InMemoryTieringStore` satisfy them via `impl`. A caller
  that ignores the `Result` triggers `#[must_use]` on `Result` (Rust warns by default; the workspace
  treats warnings as errors in CI) — the equivalent of the mypy/Protocol composition-root check.
  `cargo check` is the build-time gate; removing the `?` in `flush` would leave an unused `Result` and
  fail the warnings-as-errors build.
- **(b) structural check** — an AST pre-commit check (the repo's existing structural-hook family,
  ADR-0060 §Verification (b)) asserts that `FileBackedTieringStore::place` and `evaluate_at` call
  `append_wal(...)?` (the `?`, the write-ahead ordering token) BEFORE any `apply_to_entries` /
  `entry.tier =` mutation, and that NO `let _ = append_wal` or `if let Err(_e) = append_wal` swallow
  pattern remains in `cinder/src/file_backed.rs` or `sluice/src/file_backed.rs`. This is the layer
  that catches a re-introduced swallow that still type-checks. `import-linter` was investigated and
  rejected (its contracts are import-graph only, with no API for method/call-presence enforcement);
  the AST hook covers the structural layer.
- **(c) behavioural gold-test** — the failing-substrate acceptance test per surfacing site (KPI-1/3/4)
  exercises the catalogued substrate lie (the failing `FsyncBackend`) and asserts the operation
  returns `PersistenceFailed` AND memory stayed consistent with disk across a reopen. This is the
  probe that verifies the adapter ACTUALLY surfaces (not merely claims to). **Self-application**: the
  gold-test is itself the probe that the swallow is gone — it FAILS on the swallow bug (the un-
  persisted placement is readable / the count overstates) and PASSES only on the surfaced-and-
  consistent fix, so it cannot be a no-op that passes on the defect.

**Mutation testing** (`cargo mutants --in-diff`, ADR-0005 Gate 5, 100% kill on modified files): the
primary targets are the `?` on `append_wal` in `place`/`evaluate_at`/sluice's three ops (must not be
deletable without a surviving test), and the append-before-apply ORDERING (a mutant that reorders
apply before append, or that drops the early-return on `Err`, must be killed by the failing-substrate
gold-test asserting memory == disk on failure).

## External-integration handoff

None. cinder and sluice read and write the in-process filesystem under their pillar root, not a
network service. No consumer-driven contract test recommendation. The failure is surfaced as a typed
Rust `Result` to the in-process caller and rendered to stderr by the CLI; no new metric, no new
dashboard, no third-party API/webhook/OAuth boundary.
