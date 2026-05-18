# Wave Decisions — `cli-read-observe-otlp-v0` / DESIGN

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-19.

Mode: PROPOSE. The DISCUSS artefacts and ADR-0039 §8 jointly collapse
the design space to a near-singleton: only one writer participates in
`read` (the Lumen writer), so the `try_clone` machinery introduced for
the two-writer ingest scenario is not required here. DESIGN's load-
bearing job is to confirm the single-handle open mechanism, decide
whether to extract a tiny file-open helper, lock the `read()` signature
extension, and verify that no new CI gate is needed.

Scope inherited from DISCUSS (locked, not re-litigated): wiring change
inside `kaleidoscope_cli::read`
(`crates/kaleidoscope-cli/src/lib.rs:252-269`); a parallel
`parse_observe_otlp` call inside `run_read`
(`crates/kaleidoscope-cli/src/main.rs:121-128`); a one-line
`print_usage` update; one new acceptance test file. No writer public
API change (ADR-0039 §1, DISCUSS D3). No new flag, no new subcommand
(DISCUSS D1). No Cinder participation on the read path (DISCUSS D2).
No multi-process scope (DISCUSS D4). Principal KPI is OK1 — Lumen
query events present on the operator's `--observe-otlp` sink per
`read` invocation.

---

## DD1: Open mechanism — single `OpenOptions::append`, no `try_clone`

**Decision**: In the `Some(path) => { … }` arm of the new
`otlp_log_path` match inside `kaleidoscope_cli::read`, open the
operator-supplied path **exactly once** with
`std::fs::OpenOptions::new().create(true).append(true).open(path)`
and pass the resulting `File` directly into
`LumenToOtlpJsonWriter::new(file)`. **Do not** call `file.try_clone()`.

**Rationale**:

1. **Only one writer participates in `read`.** Unlike `ingest`, where
   both Lumen and Cinder writers needed independent `File` handles
   against a shared file description (ADR-0039 §8), the `read` function
   instantiates only the Lumen recorder
   (`crates/kaleidoscope-cli/src/lib.rs:253-255` today; the Cinder
   store is not constructed at all per DISCUSS D2). The second clone
   has nowhere to go and would be immediately discarded. `try_clone`
   exists in the std-lib precisely to solve "two owners over one file
   description"; with one owner the call is unjustified noise.

2. **Inherits the cross-`ingest`/`read` symmetry through the kernel,
   not through the wiring.** OK3 (ingest-then-read symmetry) is the
   shell-session scenario in which a previous `ingest` invocation has
   already populated the same `--observe-otlp` path with
   `lumen.ingest.count` and `cinder.place.count` lines. When `read`
   runs against the same path with `create(true).append(true)`, POSIX
   `O_APPEND` semantics seek to end-of-file on every `write(2)` — the
   new `lumen.query.count` line lands after the existing content
   without truncation, regardless of which process opened the file
   first. This is exactly the property the existing test
   `observe_otlp_file_is_appended_to_across_multiple_ingest_calls`
   (`crates/kaleidoscope-cli/tests/observe_otlp_flag.rs:150-191`) already
   probes for the ingest-only case; the same kernel guarantee covers
   the cross-subcommand case for free, because the two processes do
   not overlap in time (DISCUSS D5).

3. **Flag set identical to the `ingest` path.** `create(true)` is
   load-bearing for the first-invocation scenario (file does not exist
   yet); `append(true)` is load-bearing for the cross-invocation
   append safety; the absence of `truncate(true)` is load-bearing for
   the existing-file scenario. The exact flag combination has been in
   production since commit `3af7e82` and validated by the ingest-side
   acceptance suite; no flag deviation is warranted here.

4. **Code change footprint**: approximately +6 lines inside the
   `Some(path) => …` arm — one `OpenOptions::open` call, the
   `LumenToOtlpJsonWriter::new(file)` construction, and the
   `Box<dyn LumenRec + Send + Sync>` cast. Mirrors lines 158-164 of
   `ingest()` minus the `try_clone` line and minus the Cinder writer
   construction.

**Rejected alternative — Reuse the exact `open + try_clone` idiom
from `ingest()`**: the cloned handle would be bound to a `_` and
dropped immediately. Cost: one wasted `dup(2)` syscall per `read`
invocation, plus a misleading code shape that suggests two writers
exist when only one does. Pure deficit; nothing gained for the
maintenance reader.

**Rejected alternative — `Arc<Mutex<File>>` adapter**: same arguments
as ADR-0039 §8 Alternative 2. Defeats `O_APPEND` atomicity by
serialising at userspace, doubles mutex acquisitions per emission,
introduces a new public-ish type. Already rejected at the more
demanding two-writer site; trivially rejected at the one-writer site.

---

## DD2: Helper extraction — **no**, inline both call sites

**Decision**: Do NOT extract the four-line file-open block into a
shared helper such as
`fn open_observe_otlp_file(path: &Path) -> io::Result<File>`. Leave
both `ingest()` and `read()` with their own inlined
`OpenOptions::new().create(true).append(true).open(path)?` calls.

**Rationale (rule of three)**:

1. **Two call sites is N=2, not N=3.** The Kaleidoscope codebase
   already established the rule-of-three deferral idiom in
   `cli-cinder-otlp-wiring-v0` (test-harness extraction deferred to a
   third test file) and in ADR-0039 §5 (OTLP-JSON serde-struct
   duplication deferred to a third writer). Extracting at N=2 would
   break the project's own convention; extraction is warranted when
   the third site lands and confirms that "the same idiom" really is
   the same.

2. **The block is four lines of standard library calls, not a
   non-trivial abstraction.** The body is
   `OpenOptions::new().create(true).append(true).open(path)`. There
   is no name to invent that is shorter or clearer than the literal;
   `open_observe_otlp_file` is longer than the body it abstracts. The
   helper buys nothing for the reader and costs an indirection that
   future maintainers must navigate to confirm "yes, those flags".

3. **The flag combination IS the semantic contract.** A reader of
   `read()` who has not yet read `ingest()` can confirm the
   `O_APPEND` posture from the call site directly; with a helper, the
   reader must hop to a definition (potentially in a different file)
   and confirm the helper's flag set has not drifted. The literal
   form is the more honest shape.

4. **Extraction is cheap when justified.** When a third writer-wired
   subcommand lands (or when a future operator-facing file-open
   primitive needs additional posture — `O_CLOEXEC` hardening, a
   tracing hook, an `OpenOptionsExt` parameter), the extraction
   becomes a one-commit refactor. Pre-paying that refactor now is
   premature.

**Rejected alternative — Extract `open_observe_otlp_file(path)`
now**: pays the abstraction cost at N=2 against the project's own
rule-of-three convention. Saves three lines of source across two call
sites at the cost of an indirection that obscures the kernel-level
flag posture. Net cost in maintainability.

**Self-application note**: if `cli-read-observe-otlp-v0` were the
THIRD wiring feature in sequence, this decision would flip. It is
the second (after `cli-cinder-otlp-wiring-v0` already wired `ingest`).
The rule-of-three trigger arrives with the NEXT writer-wired
subcommand or sink-target, not with this one.

---

## DD3: `read()` signature extension — append `otlp_log_path: Option<&Path>`

**Decision**: Extend `kaleidoscope_cli::read`'s signature from

```rust
pub fn read(tenant: &TenantId, data_dir: &Path, mut writer: impl Write)
    -> Result<usize, Error>
```

to

```rust
pub fn read(
    tenant: &TenantId,
    data_dir: &Path,
    mut writer: impl Write,
    otlp_log_path: Option<&Path>,
) -> Result<usize, Error>
```

The new parameter is positional, fourth in the parameter list, and
**not** defaulted (Rust has no default arguments). All in-tree callers
that pass `None` express the no-flag behaviour explicitly. The
recorder construction inside the function body becomes a match on
`otlp_log_path`:

```rust
let recorder: Box<dyn LumenRec + Send + Sync> = match otlp_log_path {
    Some(path) => {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Box::new(LumenToOtlpJsonWriter::new(file))
    }
    None => {
        let pulse: Arc<dyn MetricStore + Send + Sync> =
            Arc::new(InMemoryMetricStore::new(Box::new(PulseRecorder)));
        Box::new(LumenToPulseRecorder::new(pulse))
    }
};
```

The `None` arm is structurally byte-equivalent to today's body
(lines 253-255) — same `Pulse` construction, same `LumenToPulseRecorder`
wrapping, same `Box<dyn LumenRec + Send + Sync>` shape implied by the
recorder slot in `FileBackedLogStore::open`. The `Some` arm mirrors
`ingest()`'s Lumen-side construction at lines 158-164 minus the
`try_clone` line and minus the Cinder writer.

**Rationale**:

1. **Mirrors `ingest()`'s already-shipped fifth parameter.** Operator
   and crafter mental model is "one optional `--observe-otlp`
   parameter per subcommand, same place, same type, same semantics".
   The DISCUSS handoff explicitly called out this idiom
   (`outcome-kpis.md > Handoff to DESIGN > item 2`).

2. **`Option<&Path>` matches the existing CLI dispatcher's call shape.**
   `parse_observe_otlp` in `main.rs:105-119` returns
   `Result<Option<PathBuf>, _>`, and `run_ingest` already forwards it
   as `otlp_path.as_deref()`. Adding `parse_observe_otlp` to
   `run_read` and forwarding identically is the line-for-line parallel
   of the existing dispatch.

3. **Public-surface change visible to Gate 2 (`cargo public-api`).**
   The change adds a parameter to a public function in a `pub`
   library crate. SemVer: this is a breaking change for any external
   consumer of `kaleidoscope_cli::read`. The crate is `publish =
   false` (`crates/kaleidoscope-cli/Cargo.toml:9`), so no published
   consumers exist; in-tree callers (the binary at
   `main.rs:121-128` and the existing acceptance test
   `ingest_and_read_roundtrip.rs`) are updated in the same commit.
   Gate 3 (`cargo semver-checks`) will surface the breaking change at
   CI time; the change is intentional and is recorded here as the
   audit trail.

**Rejected alternative — overload via a `ReadOpts` builder struct**:
introduces a new public type for a single optional parameter; breaks
the symmetry with `ingest()` (which uses a positional `Option<&Path>`,
not a builder); over-abstraction at N=1 optional parameter.
**Rejected.**

**Rejected alternative — separate `read_with_otlp(...)` function**:
two functions diverging only in one parameter's presence; doubles the
public surface; doubles the test surface; breaks the symmetry with
`ingest()`. **Rejected.**

---

## DD4: Reuse Analysis (RCA F-1 hard gate)

Hard gate per the Reuse-Choose-Author rule.

| Existing component | Path | Decision | Rationale |
|---|---|---|---|
| `LumenToOtlpJsonWriter::new(file)` construction site | `crates/kaleidoscope-cli/src/lib.rs:158-164` (inside `ingest`) | **EXTEND THE SHAPE** | The `Some(path)` arm of the new match in `read()` mirrors the Lumen-side fragment of the ingest wiring (open with `OpenOptions::create(true).append(true)`, pass into `LumenToOtlpJsonWriter::new(file)`, box as `Box<dyn LumenRec + Send + Sync>`). Minus the `try_clone` line, minus the Cinder writer construction. |
| `LumenToOtlpJsonWriter` | `crates/self-observe/src/lumen_otlp_json.rs` | **REUSE AS-IS** | Public surface locked by ADR-0039 §1 (and by the already-in-production Lumen writer surface since commit `c6b336c`). Constructor is `new(W: Write + Send + Sync) -> Self`; takes ownership by value. No change required. |
| `LumenToPulseRecorder` | `crates/self-observe/src/lumen_bridge.rs` | **REUSE IN `None` ARM** | The `None` arm of the new match preserves today's behaviour byte-equivalently: fresh `InMemoryMetricStore` over `PulseRecorder`, wrapped in `LumenToPulseRecorder`. Source bytes unchanged from current `read()` body. |
| `From<std::io::Error> for Error` | `crates/kaleidoscope-cli/src/lib.rs:104-108` | **REUSE** | `OpenOptions::open(path)?` inside the `Some` arm lifts a `std::io::Error` through `?` into `Error::Io`. Identical posture to `ingest()`'s line 161. No new error variant. |
| `parse_observe_otlp(args)` | `crates/kaleidoscope-cli/src/main.rs:105-119` | **REUSE** | Already exists, returns `Result<Option<PathBuf>, _>`, parses the same flag with the same semantics. `run_read` gains one call to it, identical to `run_ingest`'s call at line 88. |
| `fn open_observe_otlp_file(path) -> io::Result<File>` (hypothetical helper) | n/a — does not exist | **DO NOT CREATE** | Rule of three: N=2 call sites is one short of the extraction trigger (DD2 above). Pre-paying the abstraction cost at N=2 breaks the project's own convention established by ADR-0039 §5 and `cli-cinder-otlp-wiring-v0` DISCUSS D4. |
| `try_clone` machinery from ADR-0039 §8 | `crates/kaleidoscope-cli/src/lib.rs:162` | **DO NOT REUSE** | Specifically motivated by the two-writer ingest case (Lumen + Cinder over one shared file description). `read()` has one writer; the second clone has no consumer. DD1 above. |
| Existing test harness (`tenant`, `record`, `temp_root`, `cleanup`, `ndjson` helpers) | `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs:35-76` | **DUPLICATE INLINE AT V0** | DISCUSS D6 (this feature's `discuss/wave-decisions.md`) defers extraction to a `tests/common.rs` module to a follow-up. After this feature ships, the crate has three test files using the same harness pattern; the rule-of-three trigger arrives WITH this feature but the extraction itself is a separate refactoring task. |

**Verdict**: **EXTEND** (the existing ingest-side Lumen wiring shape)
+ **REUSE** (`LumenToOtlpJsonWriter`, `LumenToPulseRecorder`,
`From<io::Error>`, `parse_observe_otlp`). **No new abstraction is
created.** The "no helper extracted" finding (DD2) is the correct
output of the rule-of-three discipline at N=2.

---

## DD5: Out-of-scope confirmations

The DESIGN wave confirms (does not re-litigate) the following DISCUSS
decisions and ADR-0039 locks:

1. **No writer public API change** (DISCUSS D3, ADR-0039 §1). The
   wiring consumes `LumenToOtlpJsonWriter::new(file)` unchanged. No
   constructor variant, no `new_shared`, no `with_scope`.

2. **No Cinder participation on the read path** (DISCUSS D2). The
   `read()` function does not construct a Cinder store at all today;
   this feature does not change that. No `cinder.*` lines will appear
   in the file as a consequence of THIS feature (lines from a prior
   `ingest` invocation under the OK3 scenario are not produced by
   this feature; they are produced by `ingest()` and merely
   pre-exist).

3. **No multi-process scope** (DISCUSS D4). The in-process concurrency
   footprint is one thread inside one `kaleidoscope-cli read`
   invocation — `read()` calls `lumen.query(tenant, TimeRange::all())`
   exactly once, which drives exactly one `record_query` event, which
   produces exactly one OTLP-JSON line. There is no within-process
   concurrency to defend against here. Multi-process scenarios (two
   CLI processes writing to the same `--observe-otlp` path
   simultaneously) remain out of scope per ADR-0039 §7 and inherit
   the `O_APPEND` substrate posture if they ever land.

4. **No new flag, no new subcommand, no `--observe-read-otlp` split**
   (DISCUSS D1). The `--observe-otlp <path>` flag is the only
   operator surface and is shared between `ingest` and `read`
   subcommands with byte-equivalent semantics on the wire and on the
   file. The `print_usage` text in `main.rs:68-84` gains one mention
   of the flag on the `read` subcommand line; no new help section.

5. **No change to either writer's source file** (ADR-0039 §1).
   Edits stay confined to `crates/kaleidoscope-cli/src/lib.rs`
   (the `read` body and signature), `crates/kaleidoscope-cli/src/main.rs`
   (the `run_read` dispatcher and `print_usage`), the new test file
   `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs`, and the
   manifest `crates/kaleidoscope-cli/Cargo.toml` (one new `[[test]]`
   block).

6. **No new external crate dependency**. `self-observe` is already
   imported (`lib.rs:65: use self_observe::{CinderToOtlpJsonWriter,
   LumenToOtlpJsonWriter, LumenToPulseRecorder};`); the import list
   is unchanged. `serde_json` is already a dev-dependency. No
   `Cargo.lock` churn beyond what a recompile produces.

7. **No new ADR**. ADR-0039 §8 already documents the file-sharing
   mechanism for the multi-writer case; the single-writer case is
   the same mechanism with the second clone elided. No §9 extension
   is warranted — the helper extraction was rejected (DD2), so no
   architectural decision is reified beyond what §8 already records.
   See "Why no ADR change" below.

---

## DEVOPS handoff annotation

Recipient: `nw-platform-architect`. Receives:

- **Inputs**:
  - This `design/wave-decisions.md`.
  - The accompanying `design/application-architecture.md` (C4 L1 + L2
    with prose narrative; L3 explicitly skipped).
  - The new subsection appended to
    `docs/product/architecture/brief.md > ## Application Architecture
    — cli-read-observe-otlp-v0`.
  - The DISCUSS-wave artefacts under
    `docs/feature/cli-read-observe-otlp-v0/discuss/` (locked, not
    modified).
  - Outcome KPIs (`discuss/outcome-kpis.md`): OK1 (principal —
    `lumen.query.count` line per `read` invocation), OK2 (no-flag
    non-regression guardrail), OK3 (cross-subcommand symmetry).

- **Development paradigm for DELIVER**: Rust idiomatic per `CLAUDE.md`.
  Data + free functions + traits only where polymorphism is genuinely
  needed. The wiring change introduces no new trait, no new struct,
  no new `dyn` boundary beyond what already exists at the recorder
  construction site (`Box<dyn LumenRec + Send + Sync>`, forced by the
  conditional construction over two concrete recorder types, not a
  design preference). `OpenOptions::open` is invoked directly as a
  std-lib method; no wrapper.

- **External integrations**: **none**. No new HTTP client, no
  webhook, no third-party API, no vendor SDK, no subprocess. The
  downstream OTLP/HTTP collector that an operator's sidecar will
  eventually forward to is at the operator's deployment boundary,
  not at this feature's boundary; the existing Lumen-side wiring
  (commit `3af7e82`) and Cinder-side wiring
  (`cli-cinder-otlp-wiring-v0`) have already validated wire-shape
  acceptability for the collector and the sidecar contract. No
  contract-test recommendation applies.

- **CI gates** (ADR-0005): the five existing workspace gates apply
  unchanged. The new acceptance test
  `cargo test --package kaleidoscope-cli --test observe_otlp_read_flag`
  exits 0 as the OK1/OK2/OK3 acceptance probe under Gate 1
  (`cargo test --workspace`). **No new gate is added.**

  Specifically on **Gate 5 (mutation testing)**: the existing
  `gate-5-mutants-kaleidoscope-cli` job at
  `.github/workflows/ci.yml:949-1028` is path-filtered on
  `crates/kaleidoscope-cli/**` via `--in-diff` against the
  `origin/main → HEAD~1 → full` baseline cascade. Any commit
  touching `crates/kaleidoscope-cli/src/lib.rs` or
  `crates/kaleidoscope-cli/src/main.rs` (this feature touches both)
  is automatically mutated by the existing job; no per-file job
  fan-out is required. The DEVOPS A1/A3 deferral recorded in the
  `cli-cinder-otlp-wiring-v0` wave's `ci.yml` comment is the explicit
  precedent for this auto-coverage posture. **No new Gate 5 job needed.**

- **Workspace changes**: no `Cargo.toml` additions at the workspace
  root. `crates/kaleidoscope-cli/Cargo.toml` gains exactly one
  `[[test]]` block:

  ```toml
  [[test]]
  name = "observe_otlp_read_flag"
  path = "tests/observe_otlp_read_flag.rs"
  ```

  No new `[dependencies]` line; `self-observe` and `serde_json` are
  already there. No `[dev-dependencies]` change.

- **Mutation-testing scope** (per `CLAUDE.md` and ADR-0005 Gate 5):
  scoped to `crates/kaleidoscope-cli/src/lib.rs` and
  `crates/kaleidoscope-cli/src/main.rs` (the two modified source
  files). Run after the DELIVER refactor pass. 100% kill rate. The
  changed code surface is small (the `read()` signature extension and
  match body, plus `run_read`'s `parse_observe_otlp` call and the
  `print_usage` line); mutation-testing budget should be modest and
  well under the 30-minute timeout in the existing job.

- **Architectural-rule enforcement tooling** (Principle 11): no new
  tooling is recommended for this feature. The existing five-gate
  workspace contract already enforces every rule this feature
  touches. Rust does not have an idiomatic ArchUnit equivalent for
  the "single-writer open uses `OpenOptions::append`, not a wrapper
  type" property; the enforcement mechanism is the acceptance test
  suite, which fails loudly if any future refactor switches to a
  substrate that breaks the append posture.

### Why no ADR change

The chosen mechanism (`OpenOptions::create(true).append(true).open` →
`LumenToOtlpJsonWriter::new(file)`) introduces **no new public type,
no new abstraction, no new module**. It is a single-writer instance
of the same wiring shape already locked by ADR-0039 §8 for the
multi-writer case — the `try_clone` step is simply elided because no
second writer exists. The §8 text already records the file-sharing
mechanism at the level of formality this feature needs. A §9
extension was considered for the helper extraction (DD2); the
extraction was rejected on rule-of-three grounds; with no helper
type to reify, no §9 extension is warranted.

If a future feature reaches the third writer-wired subcommand or
sink-target and extracts the `open_observe_otlp_file` helper, THAT
feature would warrant a new §9 extension (or, if the helper becomes a
public-surface type, a new ADR-0040). This feature does not.
