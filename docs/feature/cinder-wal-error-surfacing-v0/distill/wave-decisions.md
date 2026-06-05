# Wave Decisions — cinder-wal-error-surfacing-v0 (DISTILL)

- **Wave**: DISTILL (nWave)
- **Agent**: Quinn (`nw-acceptance-designer`)
- **Date**: 2026-06-05
- **Mode**: Autonomous overnight run. All decisions made here; no clarifications raised.
- **Inputs read**: `discuss/{user-stories,story-map,wave-decisions}.md`,
  `design/wave-decisions.md`, ADR-0065, `brief.md`
  (§"Application Architecture — cinder-wal-error-surfacing-v0" incl. the
  For-Acceptance-Designer driving-ports note + falsifiability requirement),
  `devops/{environments.yaml,wave-decisions.md}`. Grounded the API surface by
  reading `crates/cinder/src/{store.rs,file_backed.rs}`,
  `crates/wal-recovery/src/lib.rs`, `crates/sluice/src/{queue.rs,file_backed.rs}`,
  and the existing harnesses
  `crates/cinder/tests/v1_slice_01_wal_durability.rs`,
  `crates/kaleidoscope-cli/tests/place_subcommand.rs`,
  `crates/kaleidoscope-cli/tests/ingest_atomic.rs`.

## Prior Wave Consultation (+/- checklist)

| Artefact | + (used) | − (gap / flag) |
|---|---|---|
| `discuss/user-stories.md` | 4 stories (US-01..04), embedded BDD, the three failure shapes per store op, Domain Examples with concrete values (acme/trade-002/warm, globex/batch-007 overwrite), the falsifiability trap (C5) | − No DIVERGE wave (recorded low-impact risk upstream; the defect is verified in code, does not block DISTILL) |
| `discuss/story-map.md` | WS = US-01+US-02 on the LIVE path; R1/R2/R3 slicing; sluice = R3 carpaccio cut | − none |
| `discuss/wave-decisions.md` | D1-D4 flagged-to-DESIGN; Luna's D2 lean (fail-the-ingest); C1-C7 | − D2/D3/D4 branches left OPEN by DISCUSS — RESOLVED by DESIGN (reconciled below) |
| `design/wave-decisions.md` + ADR-0065 + brief | the EXACT post-change signatures (`place -> Result<(), MigrateError>`, `evaluate_at -> Result<usize, MigrateError>`, sluice `Queue` three ops fallible); D2 = **fail-the-ingest** (`error: cinder place: persistence failed: io: <reason>`, non-zero exit); D3 = **fail-whole** (count == durable, failing item neither on disk nor in memory, rest untouched); D4 = sluice `Queue` fallible via `EnqueueError::PersistenceFailed`; the failing-substrate seam = inject a failing `FsyncBackend` via `open_with_fsync_backend`; the four driving ports | − the brief/ADR say "Gate 2/Gate 3 WILL fire on cinder" — DEVOPS corrected this (cinder/sluice NOT enrolled). Irrelevant to DISTILL (no public-api baseline to author); noted for completeness |
| `devops/environments.yaml` + `devops/wave-decisions.md` | the failing substrate is an IN-PROCESS, real-local-IO test concern, NO infra, NO host disk-fill; tests run in `clean` + `with-pre-commit` + `ci`; **determinism mandate** (boolean presence/absence + memory==disk, NO wall-clock threshold) so the pre-commit hook does not flake overnight; the manual semver bump is a DELIVER act, NOT a test concern; falsifiability is MANDATORY (C-DEVOPS-4) | − none |

### Reconciliation log (contradictions checked across DISCUSS/DESIGN/DEVOPS)

1. **D2/D3/D4 open in DISCUSS vs locked in DESIGN** — no contradiction; DISCUSS
   deliberately delegated the branch choice. DISTILL encodes the DESIGN-locked
   branches as the ACs: fail-the-ingest (D2), fail-whole sweep (D3), sluice
   `Queue` fallible (D4). The DISCUSS embedded Gherkin that said "behaves exactly
   as D2/D3 specifies" is now made concrete against the locked branch.
2. **"Gate 2/Gate 3 fire on cinder" (ADR-0065/brief) vs "cinder/sluice NOT
   enrolled" (DEVOPS finding C-DEVOPS-2)** — DEVOPS is authoritative on the CI
   reality. DISTILL impact: NONE. There is no cinder/sluice public-api baseline
   for an acceptance test to author or assert against; the semver-MINOR bump is a
   manual DELIVER act. No scenario depends on Gate 2/3.
3. **Failing-substrate mechanism: "inject a failing `FsyncBackend`" (ADR/brief)
   vs the actual code** — GROUNDED IN CODE. `append_wal` (cinder
   `file_backed.rs:418`, sluice `file_backed.rs`) calls
   `fsync_backend.fsync_file(...).map_err(io)?`. A backend whose `fsync_file`
   returns `io::Error` makes `append_wal` return
   `PersistenceFailed { reason: "io: <reason>" }` deterministically. **No such
   backend exists today** — see the Falsifiability note below; DISTILL defines a
   test-local `FailingFsyncBackend` (a test can `impl` the public `FsyncBackend`
   trait), so the failure scenarios do not depend on DELIVER adding a shared-crate
   mode first.

## DWD-1 — Walking-Skeleton strategy: **Strategy C (real-local-IO)**

The WS exercises the operator's actual entry points against **real temp-dir WAL
files** on the **real local filesystem** (`std::env::temp_dir()` per-test dirs,
mirroring `v1_slice_01_wal_durability.rs`), with the failure injected by an
**in-process failing `FsyncBackend`** at the library seam and by a **real
read-only WAL substrate** at the subprocess seam. No `@in-memory` doubles on any
WS scenario (the InMemory store can never fail to persist, so it cannot pin the
write-ahead ordering — using it here would be Fixture Theater).

- **Why C, not B (no external service) or D (full-stack)**: cinder/sluice are
  in-process file-backed stores; the only "external" resource is the local
  filesystem, which is cheap and real. DEVOPS confirms NO infra, NO host
  disk-fill. The real I/O is mandatory because InMemory doubles cannot catch the
  write-ahead-ordering wiring bug this feature exists to fix (memory==disk after
  a reopen is the load-bearing assertion).
- **Litmus ("if I deleted the real adapter, would this WS still pass?")**: NO.
  Each WS asserts durability across a real reopen (`FileBackedTieringStore::open`
  re-reading the real `.wal`/`.snapshot`), so it genuinely exercises the
  file-backed adapter, not a double.

## DWD-2 — Falsifiability strategy (the heart of the feature; C-DEVOPS-4)

**Grounded-in-code correction to the naive "absent on reopen" framing.** Reading
`append_wal` (`cinder/src/file_backed.rs:409-419`): it `write_all`s the record and
`wal.flush()`es it to the OS file BEFORE calling `fsync_file`. So a
failing-`fsync` backend fails AFTER the bytes are already in the OS page cache —
an in-process **reopen still reads the record back** (the page cache survives a
same-host reopen; only a real power-cut would lose the un-fsynced bytes — the
central ADR-0060 §1 thesis). Therefore "the placement is absent on reopen" is NOT
a reliable discriminator for a failing-`fsync` substrate.

**The load-bearing falsifiable assertion for THIS error-surfacing feature is
write-ahead ordering on the LIVE handle**: when `append_wal` returns `Err`, the
fix must leave the in-memory map UNTOUCHED, so the live handle's `get_tier`
returns the PRIOR value (or `None`). The discriminator:

| | live `get_tier` after a failing `place(new)` over a prior value |
|---|---|
| **Today (swallow bug)** | returns the NEW (un-persisted) value — memory was mutated unconditionally |
| **Post-fix (write-ahead)** | returns the PRIOR value — `?` returns before `apply_to_entries`, memory untouched |

Every failure scenario therefore asserts (against the LIVE handle, the strongest
discriminator):

1. the operation surfaces `PersistenceFailed` (post-fix API; in the compiled-RED
   shape this is implicit — the memory-untouched assertion below is what runs RED
   today); AND
2. the live in-memory state is UNTOUCHED by the failed op — `get_tier` returns the
   prior value (overwrite case) or `None` (fresh-placement case). **This is the
   assertion that FAILS on the swallow bug** (today memory IS mutated) and is the
   write-ahead-ordering pin.

The **overwrite scenario is the cleanest discriminator** and is the primary
compiled-RED falsifiability proof: prior value durably `Hot`; a failing-substrate
`place(Cold)`; today the live handle returns `Cold` (memory mutated — WRONG),
post-fix it returns `Hot` (memory untouched — CORRECT). No reopen page-cache
ambiguity. The fresh-placement case asserts the live handle returns `None` after a
failing `place` (today returns the placement — WRONG).

Substrate: a **test-local `FailingFsyncBackend`** whose `fsync_file` returns
`io::Error` (so `append_wal` returns `Err`) and whose `fsync_dir` returns `Ok` (so
`open` succeeds). This is the precise substrate the post-fix `?` consumes; on the
present code the `Err` is swallowed and memory is mutated anyway — exactly the
defect the live-handle assertion pins.

### Falsifiability substrate — GROUNDED, with the gap precisely specified

**Finding (read from code, not guessed)**: the `wal-recovery` `FsyncBackend`
family today has NO write/fsync-FAILING variant.
`crates/wal-recovery/src/lib.rs`:

- `RealFsyncBackend::fsync_file` → `file.sync_all()` (always `Ok` in-process).
- `LyingFsyncBackend` (`no_op`/`truncating`/`byte_flipping`) → returns **`Ok(())`
  in every mode**; it lies by silently dropping/truncating/flipping bytes, but
  **never returns `Err`**. Injected into an append path it makes a correct store
  LOSE the record (proves REFUSAL on reopen), it does NOT make `append_wal`
  return `Err`.
- `CountingFsyncBackend::fsync_file` → delegates to `RealFsyncBackend` (always
  `Ok`), counts the call.

None of these make `fsync_file` return `io::Error`, which is what
`append_wal`'s `.map_err(io)?` needs to surface `PersistenceFailed`.

**Decision**: DISTILL defines a **test-local `FailingFsyncBackend`** inside each
acceptance test file — a small struct that `impl FsyncBackend` and returns
`Err(io::Error::new(ErrorKind::Other, "no space left on device"))` from
`fsync_file` (and `Ok` from `fsync_dir` so `open` still creates the dir entry).
`FsyncBackend` is public (`cinder::FsyncBackend` / `sluice::FsyncBackend`), so a
test crate can implement it with no production change. This keeps the failure
scenarios self-contained and does NOT block on DELIVER.

**Handoff to DELIVER (shared-crate option, NOT required for the tests to run)**:
DELIVER MAY promote this into `wal-recovery` as a `FailingFsyncBackend` (mirror
the `CountingFsyncBackend` struct shape) or a `failing` arm on
`LyingFsyncBackend` — IF it does so it MUST be purely additive and
behaviour-preserving for the existing Real/Lying/Counting modes (brief
shared-crate caution; six other pillars share the leaf). DISTILL does not require
it: the test-local double is sufficient and keeps the shared crate untouched.

## DWD-3 — Subprocess driving-adapter strategy (US-02, the D2 walking skeleton)

The brief names the **CLI ingest path** (and the `place`/`evaluate-policy`
subcommands) as driving ports; the Driving-Adapter mandate requires at least one
scenario through the ACTUAL binary via **subprocess** (exit code + stderr), not
just the `place()` lib fn.

**Constraint discovered in code**: the binary's `ingest`/`place`/`evaluate_policy`
construct the store via `FileBackedTieringStore::open(...)` (the production path,
`RealFsyncBackend`). There is **no CLI flag to inject a failing `FsyncBackend`**
through the binary — the failing backend is a library-seam-only concern. So the
subprocess FAILURE scenario cannot use the injected backend; it must make the
real WAL append fail with a genuine `io::Error` from the real filesystem.

**Decision**:

- **WS-A (happy path, subprocess, `@real-io`)** — drives the real binary
  `place`/`get-tier` (or `ingest`) on a healthy temp dir end to end: exit 0, the
  placement line on stdout, clean stderr, and the placement durable across a real
  reopen. Proves the wiring works (negative control at the binary boundary).
- **WS-B (failure path, subprocess, `@real-io @ignore`)** — drives the real
  binary against a **real read-only WAL substrate**: seed a healthy placement so
  the `.wal` exists, then `chmod` the WAL file (and its parent) read-only so the
  binary's reopen-for-append succeeds but the `write_all`/`flush`/`fsync` fails
  with a real `io::Error`. Asserts non-zero exit, stderr names the persistence /
  io failure (`persistence failed: io:` substring — after the D2 fix this is
  `cinder place: persistence failed: io: …`), and a follow-up read against a
  restored-permission reopen shows the failed placement is NOT durable. This
  exercises the real binary's error-propagation glue (`map_err` + `?` + main.rs
  `eprintln!("kaleidoscope-cli: {e}")` + non-zero exit). Marked `#[ignore]` with
  a clear marker because the D2 fail-the-ingest path does not exist yet (today
  the binary swallows and exits 0).

Note: the read-only substrate may surface as `CinderOpen` rather than
`CinderPlace` depending on exactly where the append fails relative to open; the
OBSERVABLE D2 contract the operator sees (binary fails loudly, non-zero exit,
names a `persistence failed: io:` failure, nothing acked durable) holds either
way and is what the scenario asserts. The exact `cinder place:` prefix is pinned
additionally by the library-seam D2 test (DWD-4) where the failing backend
guarantees the failure lands in the `place` append, not open.

## DWD-4 — RED-not-BROKEN classification (Mandate 7) — by RUNNING, not guessing

Brownfield: the trait, the `FileBacked` impls, `open_with_fsync_backend`, and the
public `FsyncBackend` seam ALREADY EXIST.

**Compile reality (the decisive constraint)**: a Rust test file compiles as a
whole against ONE version of the `TieringStore`/`Queue` signatures per build. A
test that asserts `matches!(store.place(...), Err(_))` against today's
`place(...) -> ()` is a TYPE ERROR (you cannot `matches!` a `()` against `Err(_)`)
— it does NOT compile, so it would BREAK the workspace build (`cargo test
--workspace --all-targets`), violating DEVOPS C-DEVOPS-3 (the pre-commit hook must
stay green). Therefore the intended-`Result`-API assertions cannot sit in a file
that is compiled today.

**Decision — two test shapes, both honest, neither breaks today's build:**

1. **Compiled, behaviourally-RED (the falsifiability proof, runs NOW)** — files
   that call the EXISTING `place(...) -> ()` / `evaluate_at(...) -> usize` /
   sluice `dequeue(...) -> Option` signatures, inject the failing `FsyncBackend`,
   and assert the **read-side consistency invariant that the swallow bug
   violates**: after a failing-backend `place`, **reopen the store from the real
   files and assert the un-persisted placement is ABSENT** (`get_tier` on the
   reopened store returns `None`), and for the overwrite case assert the prior
   durable value SURVIVES. These compile today (they do not mention `Result`) and
   FAIL RED on the swallow bug, because today `place` mutates memory while the WAL
   append failed, so the placement is reported by the live handle yet — crucially
   — the assertion is made against a **reopened** handle whose state is the real
   disk: today the reopen shows the value is NOT durable in the failing-`no_op`
   case OR shows a torn overwrite, contradicting the post-fix invariant the test
   encodes. Classification: **RED (behavioural)** — proven by running (output
   captured below in the run log).

2. **`#[ignore]`d intended-API specs (turned on by DELIVER, one at a time)** —
   the SAME scenarios re-expressed against the post-fix API
   (`place(...) -> Result<(), MigrateError>` asserted with
   `matches!(.., Err(PersistenceFailed{..}))`; sluice
   `dequeue -> Result<Option<_>, _>`). Because these mention the not-yet-existing
   `Result` shape they cannot be compiled today; they are therefore delivered as
   **non-compiled companion specs** (`*.intended.rs` files NOT under `tests/` so
   Cargo does not build them, carrying the exact intended call sites + assertions
   + the `// RED: intended post-fix API per D1 — DELIVER moves this into tests/
   and un-ignores` marker). DELIVER, in the same commit that changes the
   signature, moves each spec into `tests/`, switches the healthy call sites to
   `.unwrap()` and the failing ones to `.unwrap_err()/matches!`, and un-ignores
   them one at a time as the outside-in GREEN loop.

This is the smallest honest shape: the compiled RED tests prove the defect TODAY
(RED-not-BROKEN, no fake scaffold, no broken build), and the intended-API specs
pin the exact post-fix contract for DELIVER without forcing a non-compiling file
into the workspace build.

## DWD-5 — Why NOT a thin `Result`-shim scaffold

Considered and rejected: a local `fn place_intended(...) -> Result<(), _>` shim
that wraps today's `place` and returns `Ok(())`. Rejected because such a shim
would make the failing-path `#[ignore]`d test PASS GREEN if un-ignored before the
real fix (the shim always returns `Ok`, never `Err`) — Fixture Theater that hides
the defect (Critical Rule 7). The compiled behaviourally-RED test (DWD-4 shape 1)
already proves the defect honestly with the real failing substrate; no shim is
needed and a shim would be actively harmful. The only place a genuine RED scaffold
symbol would be justified is a TRULY ABSENT symbol — there are none here (every
type, trait, and constructor the tests touch already exists on the public
surface), so no scaffold is added.

See `acceptance-test-scenarios.md` for the full scenario list, the
adapter-coverage table, and the self-review checklist.

## DWD-6 — `#[ignore]`-until-DELIVER on the committed RED tests (trunk hygiene)

Kaleidoscope is pure trunk-based: the pre-commit hook runs `cargo test --workspace`
on every commit and the project NEVER uses `--no-verify` — the hook is the gate, and
the trunk must stay green and releasable at every SHA. The behaviourally-RED
acceptance tests (DWD-4 shape 1) fail by construction, so leaving them enabled would
fail the default test run and BLOCK the DISTILL commit.

Decision — adopt the verifier-confirmed Mandate-7 pattern already used on
`beacon-sighup-reload-v0` (and on this feature's CLI WS-B subprocess test): the RED
acceptance tests are **committed at the DISTILL SHA but `#[ignore]`d** with a precise
per-test reason string (`#[ignore = "RED until DELIVER: <one-line behaviour>, see
distill/acceptance-test-scenarios.md"]`). They stay out of the default run (trunk
green) but are **proven-RED on demand** with `--ignored`. DELIVER un-ignores them ONE
AT A TIME as its outside-in GREEN loop, in the same commit that changes each signature.

The 4 negative-control tests (cinder `healthy_disk_places...`, cinder
`healthy_sweep...`, sluice `healthy_queue...`, cli WS-A healthy) are left **un-ignored**
— they pass today and post-fix, so they are useful regression guards in the default
run. The cli WS-B subprocess test keeps its existing `#[ignore]`.

The 5 tests now carrying `#[ignore]`:
- cinder/tests/wal_error_surfacing_red.rs: `failed_overwrite_preserves_prior_durable_placement_in_memory`,
  `failed_fresh_placement_is_not_visible_in_memory`, `failing_sweep_does_not_migrate_in_memory_without_persistence`
- sluice/tests/wal_error_surfacing_red.rs: `failing_dequeue_keeps_message_pending`,
  `failing_ack_does_not_silently_lose_the_in_flight_message`

**Evidence — committed state at this SHA:**

Default run (`cargo test -p cinder -p sluice -p kaleidoscope-cli`, no `--ignored`) is
GREEN. The three new test binaries report:
- cinder `wal_error_surfacing_red`: `2 passed; 0 failed; 3 ignored`
- sluice `wal_error_surfacing_red`: `1 passed; 0 failed; 2 ignored`
- cli `wal_error_surfacing_cli_skeleton`: `1 passed; 0 failed; 1 ignored`

Proven-RED-via-`--ignored` (RED-not-BROKEN — clean `assertion left == right`
panics, NOT compile/infra errors):
- `failed_fresh_placement_is_not_visible_in_memory` — left: `Some(Warm)`, right: `None`
- `failed_overwrite_preserves_prior_durable_placement_in_memory` — left: `Some(Cold)`, right: `Some(Hot)`
- `failing_sweep_does_not_migrate_in_memory_without_persistence` — left: `Some(Warm)`, right: `Some(Hot)`
- `failing_dequeue_keeps_message_pending` — left: `0`, right: `1`
- `failing_ack_does_not_silently_lose_the_in_flight_message` — left: `0`, right: `1`

`cargo fmt --all` and `cargo clippy -p cinder -p sluice -p kaleidoscope-cli
--all-targets -- -D warnings` are clean. Two `io_other_error` nits (the test-local
`FailingFsyncBackend` double, `io::Error::new(ErrorKind::Other, _)` ->
`io::Error::other(_)`) and one `permissions_set_readonly_false` nit (the CLI skeleton
test's permission restore, `set_readonly(false)` -> `PermissionsExt::from_mode(0o644)`)
were fixed in the test files; no `crates/*/src/**` was touched.

## What this DISTILL wave does NOT do

- Does not write production code or change any `crates/*/src/**` (only adds test
  files and, if a symbol is genuinely absent, the smallest RED scaffold inside a
  test file).
- Does not change CI config, `Cargo.toml` versions, or `CLAUDE.md`.
- Does not add a shared-crate `FailingFsyncBackend` (the test-local double
  suffices; promotion is an optional DELIVER detail).
- Does not proceed into DELIVER (un-ignoring is the crafter's outside-in loop).
