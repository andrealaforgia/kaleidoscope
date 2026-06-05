# Evolution archive — cinder-wal-error-surfacing-v0

British English. No em dashes. This is the archival evolution record for
the feature. It is the factual ledger of what changed, why, and what is
left open. The narrative prose for this feature lives in
`docs/presentation/narrative.md`; this file does not duplicate it.

Sibling to `wal-torn-tail-recovery-v0-evolution.md`,
`store-fsync-durability-v0-evolution.md`,
`tls-config-reject-v0-evolution.md`,
`claims-honesty-pass-v0-evolution.md`,
`beacon-sighup-reload-v0-evolution.md` and
`cli-ingest-atomic-v0-evolution.md`, which established the per-file
convention: one file per feature, named `<feature-id>-evolution.md`, with
the sections below.

## Status

- State: DELIVERED and pushed on `main`.
- Wave model: full nWave (DISCUSS, DESIGN, DEVOPS, DISTILL, DELIVER),
  every wave dispatched to its own agent.
- ADR: ADR-0065
  (`docs/product/architecture/adr-0065-cinder-wal-error-surfacing-trait-signature.md`),
  in the Earned-Trust durability lineage of ADR-0049 (per-record
  `sync_all`), ADR-0059 (WAL torn-tail recovery), ADR-0060 (store fsync
  durability) and ADR-0064 (CLI ingest all-or-nothing).
- Closes: the four-quadrants backlog item 4 (the cinder/sluice swallowed
  WAL write failure), carried forward as a named follow-up on every
  archive since `wal-torn-tail-recovery-v0`. It was the NEXT item on the
  carried project-wide list.

## Commit ledger (in order, on `main`)

| Wave / step | SHA | Subject |
|---|---|---|
| discuss | `01ff72c` | stop swallowing the WAL write failure |
| design | `5e00f56` | surface the WAL error via a trait signature change |
| devops | `d873966` | existing CI covers it, with one correction |
| distill | `e735fe9` | RED tests that pin the write-ahead ordering |
| deliver | `e271ddd` | surface WAL persistence failures, write-ahead ordered |

## The problem, in Earned-Trust framing

cinder's `FileBackedTieringStore` acknowledged a tier decision as durable
when it was never written to disk. Two swallow sites, both verified in
code on this branch:

- `place()` (`crates/cinder/src/file_backed.rs:270-278`) ran
  `if let Err(_e) = append_wal(...) { /* swallow */ }`, then
  `apply_to_entries` ran UNCONDITIONALLY: the in-memory map was mutated
  even when the WAL append failed.
- `evaluate_at()` (`:364-368`) ran `let _ = append_wal(...)` per
  migration inside the sweep loop, mutated memory unconditionally, and
  returned a `usize` count that OVERSTATED what was durable.

The trait could not report the failure even if the adapter had wanted to:
`place(...) -> ()` and `evaluate_at(...) -> usize` had no error channel. A
subsequent `get_tier` returned a placement that was never persisted; it
vanished on the next restart, and the operator was never told the disk was
failing. This is the acked-but-not-durable lie the project's Earned-Trust
principle exists to forbid: the store said a placement was durable, exited
clean, and lost it on reopen.

The feature originates from the four-quadrants implementer assessment
(Q2-MEDIUM) and the black-box verifier triage; both read the cinder source
directly. It sits in the same Earned-Trust lineage as the fsync, WAL
recovery and atomic-ingest features before it: ADR-0049, ADR-0059,
ADR-0060 and ADR-0064. The contract the feature restores is simple: an
operation that persists must be able to report that the persistence
failed, and a failure must not leave the in-memory state ahead of the
disk.

## The decision lineage

### ADR-0065 amends ADR-0060 C1 for the cinder trait surface only

ADR-0060 deliberately PRESERVED `TieringStore` byte-identity: it injected
the `FsyncBackend` through an inherent `open_with_fsync_backend`
constructor precisely so the trait surface stayed unchanged. ADR-0065
makes the narrow, justified departure ADR-0060 itself flagged was out of
its scope. It changes the `TieringStore` trait so two of its
state-mutating operations can report a persistence failure.

The amendment is scoped and explicit. ADR-0060 stays Accepted and is NOT
edited (ADRs are immutable). Its C1 byte-identity continues to hold for
every OTHER store trait (`LogStore`, `TraceStore`, `MetricStore`, beacon
`RuleStateStore`) and for cinder's `open_with_fsync_backend` constructor,
which stays an inherent (non-trait) method. The break is the cinder trait
surface and nothing else.

### Reuse, not invention: zero new types

The fix reuses `MigrateError::PersistenceFailed { reason }`
(`cinder/src/store.rs:49`) and `EnqueueError::PersistenceFailed`
(`sluice/src/queue.rs:65`). Both already exist; both are already what
`append_wal` produces. No new error type, no new trait, no new crate, no
new event, no new dashboard. The only additive public surface is the
trait-signature changes (intended, semver-MINOR) and two thin CLI `Error`
variants (`CinderPlace`/`CinderEvaluate`) that sharpen the stderr prefix.

`migrate()` (`file_backed.rs:295-321`) ALREADY did the right thing:
`append_wal(...)?` BEFORE `apply_to_entries`. It was the in-crate model
the fix generalised to `place` and `evaluate_at`. The whole feature is the
`migrate` discipline made uniform across all three state-mutating
operations.

### Write-ahead ordering as the core principle

The load-bearing principle is write-ahead ordering: append to the WAL
FIRST, mutate the in-memory map ONLY on `Ok`. The old `place` attempted
the append first but did NOT gate the memory write on it, which is
backwards for a write-ahead log. A failed WAL append must ABORT the memory
mutation, not be ignored after it. Post-fix, the `?` on `append_wal`
returns before `apply_to_entries`, so on failure the in-memory map is
untouched and memory stays consistent with disk.

## The as-built shape

### Fallible `place` and `evaluate_at`, write-ahead-ordered

```text
fn place(&self, ...) -> Result<(), MigrateError>;        // was -> ()
fn evaluate_at(&self, ...) -> Result<usize, MigrateError>;  // was -> usize
```

`FileBackedTieringStore` appends-before-applies with `?`;
`InMemoryTieringStore` returns `Ok(())`/`Ok(count)` (it never persists, so
it never fails). A failed overwrite preserves the prior durable value by
construction: the prior value is only replaced by `apply_to_entries`,
which never runs on the failing path. A failed fresh placement is never
readable. On success the returned `usize` equals the durable count
exactly.

### The 20+ file caller ripple

The trait change rippled to ~15 files: the one live cinder caller
(`flush`), two CLI lib fns (`place`, `evaluate_policy`), the `InMemory`
impl, the `cinder_crash_target` bin, the integration-suite, and roughly
ten cinder/CLI/self-observe test files. Most were mechanical
`.unwrap()`/`?` additions on the healthy path. The self-observe
`CinderToPulseRecorder`/`CinderToOtlpJsonWriter` bridges were confirmed
NOT callers (they consume the `MetricsRecorder` port, not
`TieringStore::place`/`evaluate_at`), so the ripple did not reach them.
The whole workspace builds clean.

### D2 fail-the-ingest on the live CLI

When `cinder.place(...)` returns `PersistenceFailed` inside `flush()`, the
ingest fails loudly: `flush` propagates via `.map_err(Error::CinderPlace)?`,
the live binary exits non-zero and prints
`cinder place: persistence failed: io: <reason>` to stderr. The failing
batch is never reported durable. This extends ADR-0064's all-or-nothing
posture from the parse axis to the write-failure axis ADR-0064 explicitly
left to this line of work. Cross-store rollback of earlier durable batches
stays out of scope, consistent with ADR-0064.

### D3 fail-whole sweep

`evaluate_at` returns `Err` on the first WAL append failure (the `?`), the
same as `migrate` for a single item. It carries NO count on failure, so
the count never overstates durability. The post-failure invariant is
precise: every migration before the failing one is durable AND applied
(memory == disk); the failing migration is neither on disk nor applied (no
torn half-applied item); every migration after it is untouched in its
prior durable tier. The already-durable prefix survives and re-run is
idempotent.

### D4 sluice unwired uniformity

sluice's `Queue` trait gets the same surfacing through
`EnqueueError::PersistenceFailed`:

```text
fn dequeue(&self, ...) -> Result<Option<Message>, EnqueueError>;  // was -> Option
fn ack(&self, id) -> Result<(), EnqueueError>;                    // was -> ()
fn nack(&self, id) -> Result<(), EnqueueError>;                   // was -> ()
```

`dequeue` nests `Result<Option<_>, _>` because empty-vs-present is
orthogonal to persisted-vs-failed. `FileBackedQueue` is write-ahead
ordered: append FIRST, mutate `pending`/`in_flight`/`total` only on `Ok`.
sluice is UNWIRED (verified by grep of `crates/**/src/**`: only its own
crate, `sluice_crash_target`, and the integration-suite reference it), so
the trait change had ZERO live blast radius. Its value is purely
uniformity: preventing the cinder defect from being re-shipped under the
queue pillar when sluice is eventually wired. It was grouped as the R3
carpaccio cut so the live-value cinder stories were never gated on it.

## The proof

- 100% mutation kill on the modified surface (ADR-0005 Gate 5; CLAUDE.md
  per-feature 100%), via `cargo mutants --in-diff`: cinder 9/9, sluice
  12/12 viable (one test was added to kill two `total -= 1` mutants in the
  rewritten `dequeue`), kaleidoscope-cli 4/4. The existing `--in-diff`
  mutation jobs picked up the diff; no new CI job was needed.
- `cargo test --workspace` green (1480 passed, 0 failed);
  fmt/clippy(`-D warnings`)/deny all clean.
- DISTILL committed the falsifiability tests RED-`#[ignore]`d at `e735fe9`
  and DELIVER un-ignored them one at a time, skeleton-first, in the same
  commit that changed each signature: cinder fresh-place, cinder
  failed-overwrite, cinder failing-sweep, the CLI WS-B subprocess (D2,
  real read-only WAL), sluice failing-dequeue, sluice failing-ack. The
  four healthy-disk negative controls stayed green throughout.

### The falsifiability subtlety (DWD-2): the live-handle discriminator

The load-bearing test lesson of this feature, recorded in the same spirit
as the prior archives' honest-finding sections: the naive "absent on
reopen" framing does NOT discriminate the bug.

Reading `append_wal` (`cinder/src/file_backed.rs:409-419`): it
`write_all`s the record and `wal.flush()`es it to the OS file BEFORE
calling `fsync_file`. So a failing-`fsync` backend fails AFTER the bytes
are already in the OS page cache. An in-process reopen still reads the
record back: the page cache survives a same-host reopen, and only a real
power-cut would lose the un-fsynced bytes (the central ADR-0060 thesis).
Therefore "the placement is absent on reopen" is NOT a reliable
discriminator for a failing-`fsync` substrate.

The honest, falsifiable assertion is write-ahead ordering on the LIVE
HANDLE: when `append_wal` returns `Err`, the fix must leave the in-memory
map untouched, so the live handle's `get_tier` returns the PRIOR value (or
`None`). The overwrite scenario is the cleanest discriminator: a prior
durable `Hot`, then a failing-substrate `place(Cold)`. Today the live
handle returns `Cold` (memory mutated, WRONG); post-fix it returns `Hot`
(memory untouched, CORRECT). No reopen page-cache ambiguity. The substrate
is a test-local `FailingFsyncBackend` whose `fsync_file` returns
`io::Error` and whose `fsync_dir` returns `Ok` (so `open` still succeeds);
`FsyncBackend` is public, so a test crate implements it with no production
change.

### The honest sluice failing-ack test correction

The sluice failing-ack acceptance test carried a latent design bug: it
nacked on the STILL-failing substrate expecting depth 1, which honest
write-ahead ordering correctly PREVENTS (the nack append fails too) and
which the write-before-fsync page cache makes reopen-ambiguous (DWD-2). It
was corrected to the honest, falsifiable live-handle observable: a repeat
ack of the same id still surfaces `PersistenceFailed` iff the message is
still in-flight (the swallow bug returns a no-op `Ok`). The assertion's
intent and falsifiability are preserved, verified RED against a
reintroduced swallow ack. The lesson is the same as the prior archives':
an assertion can be written true and prove nothing, and the work was in
finding the discriminator the substrate actually supports.

## The semver consequence

The `TieringStore` and `Queue` trait changes are breaking changes to
cinder's and sluice's public surfaces. Gate 2 (`cargo public-api`) and
Gate 3 (semver) flag them, which is the correct signal, not a regression:
an operation that persists must be able to fail. cinder and sluice were
bumped `0.1.0` -> `0.2.0` (semver-MINOR, where a pre-1.0 minor bump may
carry breaking changes per Cargo's pre-1.0 semantics), with the
in-workspace dependency pins updated. This is NEVER 1.0.0: 1.0.0 is a
public stability promise, is Andrea's call alone, and is substantively
premature while these APIs churn. DEVOPS corrected the DESIGN/ADR
expectation that Gate 2/3 fire on cinder in CI: cinder and sluice are NOT
enrolled in those gates, so the bump is a manual DELIVER act, not a CI
signal.

## The honest finding

The production change was conceptually small: generalise the `migrate`
write-ahead discipline (append-then-apply with `?`) to `place`,
`evaluate_at`, and sluice's three queue ops, and thread the resulting
`Result` through ~15 callers. No new type, no new component. The
difficulty was not in the fix. It was in the falsifiability: a
write-before-fsync WAL means the obvious "absent on reopen" test passes on
the swallow bug, because the page cache survives a same-host reopen. The
load-bearing realisation was that the only honest discriminator is the
LIVE HANDLE state after a failing op, and that one of the inherited
sluice assertions was written against an observable the substrate cannot
honestly produce. The value of recording this is the same as the prior
archives': the difficulty was not where a first glance put it. It was in
building the live-handle discriminator and in correcting the one test that
asserted a true-looking fact the page cache makes meaningless.

## Known follow-ups (open, carried forward across the project)

These are open across the project and carried forward; this feature
neither introduced nor closed them except where noted.

1. sluice nack-past-cap. sluice's behaviour when a write is nacked past
   its cap needs its own slice. Open.

2. sluice wiring. sluice remains UNWIRED: no gateway/server `src` path
   constructs or drives `FileBackedQueue`. This feature made its `Queue`
   surface fail-loud BEFORE it is wired (zero live blast radius), so the
   operator who eventually wires it inherits a fail-loud queue from day
   one. The wiring itself is a separate, still-open slice. Open.

3. sluice torn-tail migration. sluice still carries the inline
   parse-or-die recovery loop; its migration to the shared
   `replay_wal_tolerating_torn_tail` routine is the tracked ADR-0059 §5
   follow-up, orthogonal to this feature's error-surfacing fix. Open.

4. ingest-dedup-v0. A re-run of a SUCCESSFUL, fully-valid ingest still
   doubles the store, because lumen has no idempotency key. The
   designed extraction (ADR-0064 DD-3): success-case dedup earns its own
   slice. Open.

5. ingest-bounded-memory. The buffer-all-then-flush design (ADR-0064)
   holds the whole input's records in RAM before commit. For very large
   inputs this is a real bound; a future feature lifts it with a temp-WAL
   staging stage or a max-records streaming cap. Open.

6. ADR-0059 Decision 8 layer b, the AST structural check, remains UNWIRED.
   The structural pre-commit check asserting in-scope stores delegate to
   the shared wal-recovery routine (and now that they call
   `append_wal(...)?` before any memory mutation, the swallow-pattern
   check ADR-0065 §Enforcement (b) describes) retains no inline replay
   loop and no `let _ = append_wal` / `if let Err(_e) = append_wal`
   swallow; the tool choice was deferred and remains deferred. It is
   feedback, not a gate, consistent with the pure trunk-based,
   no-required-checks posture; when wired it belongs in the local
   pre-commit stage. Open.

7. aegis unwired. No path authenticates; the aegis surface exists but is
   not on any request path. Open.

8. beacon SLO unreachable (B06). The beacon SLO as specified is not
   reachable by the current implementation; the SLO MWMBR synthesis the
   verifier left for later is still outstanding. Open.

9. OTLP partial_success never populated. The OTLP `partial_success`
   response field is never populated, so partial-accept signalling is not
   surfaced to clients. Open.

10. The two claims-honesty DOCUMENT items remain future features if
    wanted. The actual Prometheus-stepped grid for `query_range` (a
    query-api feature) and real gRPC-prefix honouring for `harness`
    (`Framing::GrpcProtobuf`) were documented as v0 reality rather than
    built; each would retire its respective pin. Open only if wanted.
