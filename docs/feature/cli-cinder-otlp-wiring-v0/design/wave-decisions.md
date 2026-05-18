# Wave Decisions — `cli-cinder-otlp-wiring-v0` / DESIGN

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-18.

Mode: PROPOSE — DISCUSS artefacts and ADR-0039 §7 named the failure
mode this feature must close; DESIGN enumerates the file-sharing
candidate mechanisms, evaluates each against OK6, idiomatic Rust
posture, and code change footprint, then picks one. The full propose-
mode walkthrough lives in `design/application-architecture.md`.

Scope inherited from DISCUSS (locked, not re-litigated): wiring change
inside the `Some(path) => { … }` arm of the `otlp_log_path` match in
`crates/kaleidoscope-cli/src/lib.rs:147-163`, plus one new acceptance
test file. No writer public-API change (ADR-0039 §1 + DISCUSS D6). No
new flag, no `read` subcommand wiring, no multi-process scope
(DISCUSS D5, D7). The principal KPI is OK6 — cross-writer NDJSON
validity under concurrent emission — mandated by ADR-0039 §7.

---

## DD1: File-sharing mechanism — `File::try_clone`

**Decision**: Open the operator-supplied path **exactly once** with
`std::fs::OpenOptions::new().create(true).append(true).open(path)`,
then call `file.try_clone()` to obtain a second `File` handle pointing
at the same underlying OS file description. Pass the original `File`
into `LumenToOtlpJsonWriter::new(file)` and the cloned `File` into
`CinderToOtlpJsonWriter::new(file_clone)`. Each writer continues to
own its own `Mutex<File>` per ADR-0039 §1 (locked, not changed).
Cross-writer atomicity is guaranteed by POSIX `O_APPEND` semantics:
the kernel atomically appends each `write(2)` call up to `PIPE_BUF`
bytes (4096 on Linux and macOS), which exceeds the size of any single
OTLP-JSON line either writer emits in practice (worst case is the
Cinder migrate line at roughly 540 bytes).

**Rationale**:

1. **OK6**: `File::try_clone` produces two `File` handles whose
   underlying file description carries the `O_APPEND` flag set at
   `open` time. POSIX guarantees that each `write(2)` against an
   `O_APPEND` descriptor seeks to end-of-file and writes atomically
   relative to other `O_APPEND` writes on the same file (up to
   `PIPE_BUF` = 4096 bytes on Linux and macOS). Each writer's internal
   `Mutex<File>` already serialises the
   `write_all(body) + write_all(b"\n") + flush` triple within that
   writer (ADR-0039 §2 / DISCUSS D6); the kernel's `O_APPEND` atomicity
   then composes those triples across the two writers without
   interleaving at the byte level. Worst-case line size:
   `cinder.migrate.count` carries three point attributes plus the
   resource attribute and is approximately 540 bytes serialised; well
   under `PIPE_BUF`. The Slice 01 concurrent-random-pause acceptance
   test mandated by ADR-0039 §7 item 3 is the empirical probe that
   confirms the composition holds.

2. **Idiomatic Rust posture (CLAUDE.md)**: `File::try_clone` is the
   `std` library's exact answer to "I have two structs that each want
   to own a `Write` over the same file". Documented at
   `https://doc.rust-lang.org/std/fs/struct.File.html#method.try_clone`
   as "creates a new independently owned handle to the underlying file".
   No `dyn Trait` indirection is introduced (CLAUDE.md: "no `dyn
   Trait` indirection where direct generic monomorphisation suffices");
   no new trait, no new public type. Each writer's monomorphisation
   `XxxToOtlpJsonWriter<File>` is the natural shape.

3. **Code change footprint**: Approximately +5 lines inside the
   existing `Some(path) => { … }` arm of the `otlp_log_path` match.
   The Lumen-side construction at `crates/kaleidoscope-cli/src/lib.rs:148-153`
   stays structurally identical; what changes is that the file handle
   is bound to a `let file = ...;` line, then cloned via
   `file.try_clone()?` for the Cinder writer, then the original passed
   into `LumenToOtlpJsonWriter::new(file)`. The Cinder recorder match
   at line 163 becomes a parallel `match` over `otlp_log_path` that
   constructs `CinderToOtlpJsonWriter::new(file_clone)` in the `Some`
   arm and keeps `CinderRecorder` (the `NoopRecorder` alias) in the
   `None` arm.

4. **Resource hygiene**: `File::try_clone` returns `io::Result<File>`;
   failure is rare in practice (it is a `dup(2)` syscall under the
   hood, whose only realistic failure modes are `EMFILE` (per-process
   FD table full) and `ENFILE` (system-wide FD table full)). Both are
   represented identically to the original `open` failure, so the
   error path lifts cleanly into `Error::Io` (DD3 below). No
   double-close hazard: each `File` is `Drop`-closed independently
   when its owning writer goes out of scope at end of `ingest`, and
   the OS reference-counts the underlying file description.

5. **Forward compatibility**: `O_APPEND` semantics are the same
   guarantee POSIX gives for multi-process appenders. If a future
   feature lifts the DISCUSS D7 multi-process out-of-scope decision,
   the `File::try_clone` design extends transparently: each process
   opens the path with `OpenOptions::create(true).append(true).open(path)`
   and the kernel's `O_APPEND` atomicity composes across processes
   exactly as it does across the in-process two-writer case here.
   `File::try_clone` does NOT paint future-Andrea into a corner.

**Rejected alternatives** (full evaluation in
`design/application-architecture.md`; recorded as
"Considered Alternatives" in the ADR-0039 §8 extension):

- **Two separate `OpenOptions::new().create(true).append(true).open(path)`
  calls** — same OS-level atomicity guarantee as `try_clone`. Marginally
  more error-prone (second `open` failure after the first succeeded
  leaves a half-constructed state to unwind) and marginally less
  idiomatic — once you have an open, validated descriptor, `try_clone`
  is what the std-lib documents you to do. Code footprint +1 line
  versus `try_clone`. **Rejected on idiomatic posture.** Acceptable as
  a fallback if a portability surprise ever made `try_clone` unusable.

- **`Arc<Mutex<File>>` shared via a `SharedFile(Arc<Mutex<File>>)`
  adapter implementing `Write` by locking the inner mutex on each
  `write_all`** — introduces a new public-ish type in
  `kaleidoscope-cli`, two mutex acquisitions per emission (writer's
  outer `Mutex<W>` then adapter's inner `Mutex<File>`), and **defeats
  the OS `O_APPEND` atomicity by serialising at userspace**. The
  userspace serialisation is genuinely a single point of cross-writer
  ordering, but it is not what we need: `O_APPEND` already provides
  the cross-writer guarantee we are after, at zero abstraction cost
  and zero second-mutex contention. The adapter would also paint
  future-Andrea into a corner: when multi-process scenarios surface
  post-v0, the userspace mutex protects nothing (it is per-process),
  and the adapter would have to be torn out and the design re-done
  around `O_APPEND` anyway. Code footprint: +25-35 lines in `lib.rs`
  for the new `SharedFile` type and its `Write` impl, plus two
  `Arc::clone()` calls at the construction site. **Rejected on
  abstraction cost, double-mutex contention, multi-process
  forward-incompatibility, and ADR scope (this would force a new
  ADR-0040 rather than a §8 extension).**

- **`fs::write` via a shared buffer drained periodically through a
  `parking_lot::Mutex<File>` outside both writers** — defeats per-line
  atomicity by coalescing writes across lines into a single buffer
  flush; introduces a "did the buffer drain before the process exited?"
  failure mode requiring explicit shutdown logic. **Rejected as the
  task brief anticipated**; the wrong shape for an NDJSON sink where
  per-line atomicity is the contract.

- **`OwnedFd::try_clone_to_owned()` then re-wrap as `File`** —
  equivalent to `File::try_clone` on Unix; less portable (Windows
  needs `as_handle().try_clone_to_owned()` shimming). `File::try_clone`
  already wraps the platform-appropriate primitive internally per
  `std/src/fs.rs`. **Rejected as redundant indirection.**

**Quality-attribute alignment (ISO 25010)**:

- **Reliability — Fault tolerance**: `O_APPEND` is a hard kernel
  guarantee on Linux and macOS for writes ≤ `PIPE_BUF`. The mechanism
  is robust against process-internal thread scheduling jitter (which
  is the OK6 probe scenario) without any userspace lock contention
  between the two writers.
- **Performance Efficiency — Resource utilisation**: two FDs (one per
  writer) for the lifetime of the `ingest` call; each `Mutex<File>`
  acquisition is independent so there is no cross-writer lock
  contention; one `write(2)` syscall per OTLP-JSON line. Identical
  cost profile to the existing Lumen-side wiring at line 153, simply
  doubled.
- **Maintainability — Analysability**: the change adds five lines and
  references one std-lib method (`try_clone`); a reader following the
  Lumen-side wiring at line 153 sees the Cinder-side wiring as the
  obvious parallel.
- **Portability**: `File::try_clone` is cross-platform. The
  `O_APPEND` atomicity guarantee holds on Linux and macOS (the CI
  matrix per ADR-0005); Windows behaves equivalently because
  `FILE_APPEND_DATA` performs an atomic seek-to-end-and-write under
  the hood. The `kaleidoscope-cli` binary's deployment target is
  POSIX (Docker Linux per the recent `Dockerfile` work in commit
  `0c5d91c`); cross-platform behaviour is a bonus.

---

## DD2: `OpenOptions` flags — `create(true).append(true)`

**Decision**: Use exactly the existing flag set already in production
at `crates/kaleidoscope-cli/src/lib.rs:149-152`:

```rust
let file = std::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(path)?;
```

That is, `O_CREAT | O_WRONLY | O_APPEND` (the kernel translation of
the `create(true).append(true)` combination; `append(true)` implies
`O_WRONLY` and `O_APPEND`; the absence of `truncate(true)` means
existing content is preserved).

**Rationale**: the flag set is the load-bearing one for OK6. `O_APPEND`
is what gives the kernel-level cross-writer atomicity guarantee that
DD1 depends on. `O_CREAT` means the first `ingest` invocation creates
the file; subsequent invocations append. **No** `O_TRUNC`: the existing
`observe_otlp_file_is_appended_to_across_multiple_ingest_calls` test
in `tests/observe_otlp_flag.rs:139-170` is the byte-equivalence probe
for that decision and is OK8's principal assertion. No `O_EXCL`: we
want the file to be created if absent but appended-to if present, not
"create-fail-if-exists".

This is the **same flag set** the Lumen-side wiring already uses; this
decision is essentially "do not change what already works". The
decision exists in the wave log only because the architectural
question forces us to articulate why these flags (and not others)
discharge the cross-writer guarantee.

---

## DD3: Error handling — `File::try_clone` failure propagates via `Error::Io`

**Decision**: If `file.try_clone()` returns `Err(e)`, the error
propagates up through `ingest`'s `Result<IngestStats, Error>` return
type as `Error::Io(e)` via the existing `From<std::io::Error> for
Error` impl at `crates/kaleidoscope-cli/src/lib.rs:104-108`. No retry,
no fallback to a different mechanism, no silent degradation.

**Rationale**:

1. The `File::try_clone` failure modes (EMFILE, ENFILE) are exactly
   the same shape as the `OpenOptions::open` failure modes that
   already propagate via `Error::Io` at line 152. Behavioural
   consistency: a "cannot get a file descriptor" condition fails the
   `ingest` call cleanly with the same error variant whether it
   surfaces during the original `open` or during the `try_clone`. No
   new error variant needed.

2. The "best-effort emission" posture in ADR-0039 §2 / DISCUSS D5
   applies to **per-event** emission inside the writer (serialisation
   failure, write failure, mutex poisoning at runtime) — NOT to the
   one-time **construction** of the writer at the top of `ingest`.
   Construction-time failure is a setup error; it must surface
   loudly. The wiring change preserves this distinction: the
   `try_clone()?` call uses `?` to propagate (loud, setup-time);
   `let _ = writer.write_all(...)` swallows (silent, runtime per
   ADR-0039 §2).

3. The acceptance test `flag_absent_creates_no_file_and_does_not_change_recorders`
   in the slice brief's AC list (and the existing
   `no_observe_otlp_means_no_otlp_file_created` test at
   `tests/observe_otlp_flag.rs:117-137`) probes the `None` arm where
   `try_clone` is never called; the happy-path test
   `cinder_place_lines_appear_in_observe_otlp_file_one_per_batch_flush`
   probes the `Some(path)` arm where `try_clone` succeeds. The
   `try_clone` failure path is not probed by an acceptance test — it
   is a single line whose error path is the existing `From<io::Error>`
   shape that the Lumen-side wiring's `open(path)?` already exercises.
   This is acceptable per the substrate-lie posture in ADR-0039 §7:
   `try_clone` is a std-lib primitive whose failure modes are well-
   characterised by the kernel's `dup(2)` semantics; adding an
   acceptance test for a `dup` failure would require mocking the
   kernel.

**Confirmation**: yes, propagating via `Error::Io` is the right
default. No new error variant.

---

## DD4: Reuse Analysis — no MultiWriter precedent exists; `try_clone` makes one unnecessary

Hard gate per the Reuse-Choose-Author rule. The Reuse Analysis table:

| Existing component | Path | Decision | Rationale |
|---|---|---|---|
| `LumenToOtlpJsonWriter::new(file)` construction site | `crates/kaleidoscope-cli/src/lib.rs:148-153` | **EXTEND THE SHAPE (not the type)** | The Cinder-side wiring is structurally a parallel match arm. The `file` binding from the `OpenOptions::open(path)?` call is reused (via `try_clone`); the writer construction `XxxToOtlpJsonWriter::new(handle)` is the same idiom locked by ADR-0039 §1. |
| `CinderToOtlpJsonWriter` | `crates/self-observe/src/cinder_otlp_json.rs` | **REUSE AS-IS** | Public surface locked by ADR-0039 §1 and DISCUSS D6. Constructor is `new(W: Write + Send + Sync) -> Self`; takes ownership by value. No change required; the wiring just passes the `try_clone`d `File` into it. |
| `cinder::NoopRecorder` (alias `CinderRecorder` at line 57) | `crates/kaleidoscope-cli/src/lib.rs:57, 163` | **REUSE IN `None` ARM** | The wiring change is conditional on `otlp_log_path`: when absent, `NoopRecorder` stays as today (DISCUSS D5-by-implication, since `--observe-otlp` is the only trigger); when present, `CinderToOtlpJsonWriter` replaces it. Same construction-site shape as the Lumen-side match (lines 147-160). |
| `From<std::io::Error> for Error` | `crates/kaleidoscope-cli/src/lib.rs:104-108` | **REUSE** | `file.try_clone()?` lifts a `std::io::Error` through the `?` operator into `Error::Io`. No new error variant needed (DD3). |
| Any `Tee` / `MultiWriter` / `SharedFile` / `Arc<Mutex<File>>` adapter | searched: `crates/self-observe/src/`, `crates/cinder/src/`, `crates/lumen/src/`, workspace-wide grep for `try_clone\|MultiWriter\|TeeWriter\|tee_writer\|fan.?out\|Arc<Mutex<File>>\|SharedFile\|shared.?file` | **DOES NOT EXIST IN WORKSPACE** | No precedent for any multi-writer-to-one-sink fanout pattern. The `self-observe` crate's four writer files (`lumen_bridge.rs`, `lumen_otlp_json.rs`, `cinder_bridge.rs`, `cinder_otlp_json.rs`) each implement a single source-side `MetricsRecorder` trait and dispatch to a single sink; none of them combine recorders. The `cinder/recorder.rs` and `lumen/recorder.rs` paths host the upstream `MetricsRecorder` traits but no fanout machinery. The `Write` impls in the workspace are exclusively the `SharedBuf` test substrates at `crates/self-observe/tests/{lumen_to_otlp_json,cinder_to_otlp_json}.rs:54-64`. |
| New `MultiWriter` / `Tee` / `SharedFile` type | — | **DO NOT CREATE** | DD1's `File::try_clone` choice obviates the need for any such adapter. Creating a `SharedFile(Arc<Mutex<File>>)` type would be **CREATE NEW** with no precedent to extend, and the design analysis in DD1 shows it would be a strict regression on idiomatic posture, lock contention, abstraction cost, and forward compatibility. The "no MultiWriter exists" finding therefore validates the `try_clone` choice rather than motivating a new abstraction. |
| Existing test harness (`tenant`, `record`, `temp_root`, `cleanup`, `ndjson` helpers) | `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs:35-76` | **DUPLICATE INLINE AT V0** | DISCUSS D4 (in this feature's `discuss/wave-decisions.md`) explicitly defers extraction to a `tests/common.rs` module until a third test file lands (rule-of-three). This feature ships test file #2; extraction trigger is file #3. |

**Verdict**: **EXTEND** (the existing CLI construction-site shape) +
**REUSE** (`CinderToOtlpJsonWriter`, `NoopRecorder`, `From<io::Error>`).
**No new abstraction** is created. The Reuse Analysis output is in
strict alignment with DD1: the absence of a `MultiWriter` precedent
in the workspace is exactly the right signal here — the OS provides
the multi-writer-to-one-sink atomicity natively via `O_APPEND`, so
no userspace abstraction is needed.

---

## DD5: Out-of-scope confirmations

The DESIGN wave confirms (does not re-litigate) the following DISCUSS
decisions:

1. **No public API change to either writer** (DISCUSS D6, ADR-0039
   §1). The wiring change consumes
   `CinderToOtlpJsonWriter::new(file_clone)` and
   `LumenToOtlpJsonWriter::new(file)` unchanged. No `new_shared`,
   no `new_with_arc`, no constructor variant.

2. **No `read` subcommand wiring** (DISCUSS D5). The `read` function
   at `crates/kaleidoscope-cli/src/lib.rs:236-253` does not gain an
   `otlp_log_path` parameter. The CLI's `main.rs` `parse_observe_otlp`
   helper continues to be called only from the `ingest` subcommand
   path.

3. **No multi-process scope** (DISCUSS D7). The in-scope concurrency
   is two threads inside one `kaleidoscope-cli ingest` invocation.
   The `File::try_clone` design does not preclude multi-process
   correctness post-v0 (DD1 forward-compatibility note), but the
   acceptance test exercises the in-process case only.

4. **No new flag, no new subcommand, no `--observe-cinder-otlp` split**
   (DISCUSS D2). The `--observe-otlp <path>` flag is the only operator
   surface; both writers feed its target.

5. **No change to either writer's source file** (ADR-0039 §1, DISCUSS
   D6). Edits stay confined to `crates/kaleidoscope-cli/src/lib.rs`,
   the new test file `crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs`,
   and the manifest `crates/kaleidoscope-cli/Cargo.toml` (one new
   `[[test]]` block).

6. **No change to `self-observe`'s public surface** (ADR-0039 §1).
   `CinderToOtlpJsonWriter` is consumed unchanged through its
   existing re-export from `self-observe::CinderToOtlpJsonWriter`.

7. **No new external crate dependency**. `self-observe` is already a
   `kaleidoscope-cli` dependency (line 65: `use self_observe::{LumenToOtlpJsonWriter, LumenToPulseRecorder};`);
   the import gains a third name (`CinderToOtlpJsonWriter`). `serde_json`
   is already a dev-dependency for the existing test file. No
   `Cargo.lock` churn beyond what a recompile produces.

---

## DEVOPS handoff annotation

Recipient: `nw-platform-architect`. Receives:

- **Inputs**:
  - This `design/wave-decisions.md`.
  - The accompanying `design/application-architecture.md` (C4 L1 + L2
    with prose narrative).
  - The §8 extension to ADR-0039 documenting DD1 (the file-sharing
    mechanism choice). No new ADR-0040: the chosen mechanism
    (`File::try_clone`) introduces no new public type and no new
    abstraction, so a section extension to the locked ADR is the
    correct level of formalism. See "Why §8 not ADR-0040" below.
  - The DISCUSS-wave artefacts under
    `docs/feature/cli-cinder-otlp-wiring-v0/discuss/` (locked).
  - Outcome KPIs (`discuss/outcome-kpis.md`): OK6 (principal — cross-
    writer NDJSON validity under concurrent emission), OK7 (Cinder
    events present at one line per `place` call), OK8 (Lumen-side
    non-regression).

- **Development paradigm for DELIVER**: Rust idiomatic per `CLAUDE.md`.
  Data + free functions + traits only where polymorphism is genuinely
  needed. The wiring change introduces no new trait, no new struct, no
  new `dyn` boundary beyond what already exists at line 163's
  `Box<dyn cinder::MetricsRecorder + Send + Sync>` (which is forced by
  the conditional construction over two concrete recorder types, not a
  design preference). `File::try_clone` is invoked directly as a
  `std::fs::File` method; no wrapper.

- **External integrations**: **none**. No new HTTP client, no
  webhook, no third-party API, no vendor SDK, no subprocess. The
  downstream OTLP/HTTP collector that an operator's sidecar will
  eventually forward to is at the operator's deployment boundary, not
  at this feature's boundary; the existing Lumen-side wiring (commit
  `3af7e82`) has already validated the wire-shape acceptability for
  the collector and the sidecar contract. No contract-test
  recommendation applies.

- **CI gates** (ADR-0005): the five existing workspace gates apply
  unchanged. The new acceptance test
  `cargo test --package kaleidoscope-cli --test observe_otlp_cinder_wiring`
  exits 0 as the OK6/OK7 acceptance probe under Gate 1
  (`cargo test --workspace`). No new gate is added. **Discussion of a
  self-observe-conditional gate**: the existing five gates already
  cover this feature. Gate 2 (`cargo public-api`) catches any
  inadvertent public-API change to either writer (which DISCUSS D6
  forbids). Gate 3 (`cargo semver-checks`) catches breaking-change
  regressions to `kaleidoscope-cli`'s `ingest` signature (which this
  feature does NOT change). Gate 5 (`cargo mutants`) covers the
  mutation-testing posture for the changed source file. There is
  **no need** for a self-observe-conditional gate: the cross-writer
  contract is property of the `kaleidoscope-cli` test surface, not of
  `self-observe` (whose tests use `SharedBuf` in-memory substrates).
  **Recommendation**: inherit ADR-0005 unchanged.

- **Workspace changes**: no `Cargo.toml` additions at the workspace
  root. `crates/kaleidoscope-cli/Cargo.toml` gains exactly one
  `[[test]]` block:

  ```toml
  [[test]]
  name = "observe_otlp_cinder_wiring"
  path = "tests/observe_otlp_cinder_wiring.rs"
  ```

  No new `[dependencies]` line; `self-observe` is already there.

- **Mutation-testing scope** (per `CLAUDE.md` and ADR-0005 Gate 5):
  scoped to `crates/kaleidoscope-cli/src/lib.rs` (modified file).
  Run after the DELIVER refactor pass. 100% kill rate. The changed
  code surface is small (the wiring change inside the `Some(path)`
  arm of the `otlp_log_path` match plus the parallel Cinder match);
  mutation-testing budget should be modest.

- **Architectural-rule enforcement tooling** (Principle 11): no new
  tooling is recommended for this feature. The existing five-gate
  workspace contract already enforces every rule this feature touches
  (public-API surface, SemVer compatibility, mutation kill rate,
  `cargo test` outcome). Rust does not have an idiomatic ArchUnit
  equivalent for the "writers compose via `try_clone`" property; the
  enforcement mechanism is the acceptance test
  `cross_writer_ndjson_validity_under_concurrent_random_pauses`, which
  fails loudly if any future refactor switches to a substrate that
  defeats the cross-writer guarantee.

### Why §8 not ADR-0040

The chosen mechanism (`File::try_clone`) introduces **no new public
type, no new abstraction, no new module**. It is a five-line
extension to the existing `Some(path) => { … }` arm of the
`otlp_log_path` match in `crates/kaleidoscope-cli/src/lib.rs`. The
shape of the writers' public surfaces (locked by ADR-0039 §1) is
unchanged; only one additional call site for an already-locked
constructor is added. ADR-0039 §7 explicitly mandated this feature
(`The CLI follow-up feature's DEVOPS wave MUST … Measure this KPI
via acceptance tests that spawn Lumen and Cinder record threads
simultaneously against a real File`); the §8 extension is the
correct place to record how that mandate was discharged.

If a future feature introduces a new public type for sink fanout
(e.g. a `MultiWriter` or `SharedFile` adapter), THAT feature would
warrant a new ADR-0040 (or higher). This feature does not.
