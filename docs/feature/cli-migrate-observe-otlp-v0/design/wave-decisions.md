# Wave Decisions — `cli-migrate-observe-otlp-v0` / DESIGN

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-19.

Mode: PROPOSE — DISCUSS artefacts (`wave-decisions.md`,
`outcome-kpis.md`, `slices/slice-01-migrate-observe-otlp.md`) and the
three pre-shipped precedents (`cinder-to-otlp-json-bridge-v0` for the
writer, `observe-otlp-read-flag-v0` for the `read()` flag-threading
shape, `cli-cinder-otlp-wiring-v0` for the `ingest()` recorder
construction at store-open time) collapse the design space to a
single shape. DESIGN names that shape, enumerates the rejected
variants, and quantifies the mechanical-update list.

Pre-decided (carried in, not re-litigated):

- Feature-type: **backend / application architecture** (no platform
  or domain change). D-RecorderConstruction (DISCUSS): the OTLP
  writer is constructed at `FileBackedTieringStore::open` time inside
  `kaleidoscope_cli::migrate(...)`, NOT at `migrate()` call time and
  NOT passed through the Cinder API surface. Rust idiomatic per
  `CLAUDE.md`. No stress (residuality) analysis.

---

## DD1: `migrate()` signature gains a sixth parameter, `otlp_log_path: Option<&Path>`

**Decision**: extend the existing 5-arg

```text
pub fn migrate(tenant, data_dir, item_id, to_tier_arg, writer) -> Result<(), Error>
```

to the 6-arg

```text
pub fn migrate(tenant, data_dir, item_id, to_tier_arg, writer, otlp_log_path: Option<&Path>) -> Result<(), Error>
```

The new parameter is appended in last position (mirror of the
`read()` and `stats()` flag rollouts on this crate). It is
`Option<&Path>` (not `Option<PathBuf>`) because the caller already
owns the path in `main.rs` via `parse_observe_otlp(args)?` returning
`Option<PathBuf>`; the library borrows the path slice with
`.as_deref()` at the call site, matching the precedent at
`crates/kaleidoscope-cli/src/main.rs:152` (the `ingest` call site)
and `crates/kaleidoscope-cli/src/main.rs:194` (the `read` call site).

**Rationale**:

1. Exact mirror of the `read()` and `ingest()` signatures: the
   library function takes `Option<&Path>` and dispatches internally
   on `match otlp_log_path { Some(path) => ..., None => ... }`. Same
   shape inside the function (DD2 below).
2. No new public type, no new trait, no new struct. The single
   additive parameter slots into the existing free-function shape
   that ADR-0005 paradigm requires.
3. Locked-test mechanical-update cost is bounded: five call sites
   workspace-wide (enumerated in DD5) need `None` appended as the
   sixth argument. No assertions touched; the locked OK1..OK4 of
   `cli-migrate-subcommand-v0` continue to hold byte-for-byte (D8
   inherited).

**Rejected alternatives**:

- **Pass an `Option<Box<dyn cinder::MetricsRecorder + Send + Sync>>`
  instead of `Option<&Path>`**: pushes file-open responsibility up
  into `main.rs` (or further), defeating the
  D-RecorderConstruction contract that fixes the open at
  `FileBackedTieringStore::open` time. **Rejected on contract
  violation.**
- **Introduce a `MigrateConfig` struct and pass it instead of
  growing the positional list**: premature abstraction at 6
  parameters. The four sibling functions on this crate
  (`ingest`, `read`, `stats_with_tiers`, `migrate`) all use
  positional parameters; reaching for a config struct here breaks
  the symmetry and forces a refactor cascade across the cluster
  for zero behavioural gain. **Rejected as out of proportion.**
- **Overload via a sibling `migrate_with_otlp(...)` function**: two
  near-identical entry points where one parameterised function
  suffices. The `read()` / `stats()` precedents both chose the
  parameterised form; consistency wins. **Rejected as redundant
  duplication.**

---

## DD2: Internal `match otlp_log_path { ... }` mirrors the `ingest()` shape

**Decision**: inside `migrate()` the recorder construction becomes
a `match otlp_log_path { Some(path) => ..., None => ... }` that
produces a `Box<dyn cinder::MetricsRecorder + Send + Sync>` passed
to `FileBackedTieringStore::open(cinder_base(data_dir), recorder)`.
The `Some(path)` arm opens the file with
`OpenOptions::new().create(true).append(true).open(path)?` and
wraps it via `CinderToOtlpJsonWriter::new(file)`. The `None` arm
constructs `CinderRecorder` (the `cinder::NoopRecorder` alias)
exactly as today at `crates/kaleidoscope-cli/src/lib.rs:434`.

The match block lives **between** `parse_tier(to_tier_arg)?` (line
431, unchanged — preserves OK4: parse-before-open) and
`FileBackedTieringStore::open(...)` (line 434, changed — receives
the matched box instead of the literal `Box::new(CinderRecorder)`).
Therefore the only file system effect added by the feature is the
single `OpenOptions::open(path)?` inside the `Some` arm, reachable
only when (a) `parse_tier` succeeded AND (b) `otlp_log_path` is
`Some`. OK4 (invalid-tier → no file created) is preserved by
construction.

**Rationale**:

1. **Exact mirror of `ingest()`'s pattern at lines 155-184**: same
   `match`-arm shape, same `OpenOptions` flag set
   (`create(true).append(true)`), same `Box<dyn ... + Send + Sync>`
   coercion idiom. A reader following the `ingest` body sees the
   `migrate` body as the obvious parallel; the only structural
   divergence is that `migrate` opens **one** file (Cinder only —
   Lumen is never touched, D-NoLumenTouch inherited from
   `cli-migrate-subcommand-v0`) versus `ingest`'s **one-file /
   two-handle** via `try_clone()` (DD1 of `cli-cinder-otlp-wiring-v0`).
   No `try_clone()` is needed here: there is only one writer.
2. **No re-design of writer ownership**: each writer owns its own
   `Mutex<W>` per ADR-0039 §1. The `Some` arm constructs a fresh
   `CinderToOtlpJsonWriter<File>` whose lifetime ends when the
   `FileBackedTieringStore` is dropped at end of `migrate()`.
3. **`None` arm is byte-identical to today**: the `Box::new(CinderRecorder)`
   construction at line 434 moves inside the `None` arm verbatim.
   OK2 (no-flag byte-equivalence on stdout + locked
   `migrate_subcommand.rs`) is preserved by construction: the only
   visible change on the `None` path is the indentation depth of
   the box construction inside a match arm. No behavioural change.

**Rejected alternatives**:

- **Construct the recorder unconditionally and have it no-op when
  `otlp_log_path` is `None`**: forces an open of `/dev/null` or
  similar in the no-flag path, breaking OK2 (file would be
  observable on `lsof` output) and OK4 (an open would happen
  before `parse_tier`). **Rejected on contract violation.**
- **Open the file BEFORE `parse_tier`**: would create the sink file
  on invalid-tier invocations, breaking OK4. The DISCUSS contract
  pins parse-before-open. **Rejected on contract violation.**
- **Use `if let Some(path) = otlp_log_path { ... } else { ... }`
  instead of `match`**: stylistically equivalent; `ingest()` uses
  `match`. Pick the same idiom. **Rejected as stylistically
  inconsistent.**

---

## DD3: `main.rs` thread-through and usage-text update

**Decision**:

1. `run_migrate(args)` and `run_migrate_with<O: Write>(args, stdout)`
   call `parse_observe_otlp(args)?` (the existing helper at
   `main.rs:161-175`, unchanged) to obtain `Option<PathBuf>`, then
   pass `otlp_path.as_deref()` as the sixth argument to
   `migrate(...)`. The helper is already invoked by `run_ingest`
   (line 144) and `run_read_with` (line 192); this is the third
   invocation site.
2. The usage text in `write_usage(w)` gains a `[--observe-otlp <path>]`
   suffix to the `migrate` line (currently `main.rs:128-136`) and a
   single explanatory sentence mirroring the wording on the `ingest`
   and `read` paragraphs.

The wording mirrors the `ingest` paragraph exactly:

```text
--observe-otlp appends one `cinder.migrate.count` OTLP-JSON line
per successful invocation to <path>; pointing it at the same file
used by `ingest` gives a single sidecar feed for the full
ingest+place+migrate audit trail.
```

**Rationale**: identical shape to the two existing thread-throughs,
identical wording shape to the existing usage paragraphs, zero new
helper required. The `parse_observe_otlp` helper is the **only**
argv-to-flag-value machinery for `--observe-otlp` across the
binary; reusing it on the `migrate` path is reuse, not
duplication.

**Rejected alternatives**:

- **Introduce a `--migrate-observe-otlp <path>` distinct flag**:
  splits the operator's mental model and forks the sidecar
  configuration. Explicitly forbidden by DISCUSS handoff. **Rejected
  on operator-experience contract.**
- **Move `parse_observe_otlp` into `lib.rs` so the binary doesn't
  need to call it**: introduces a public API surface for argv
  parsing; the binary-private helper is the right scope. **Rejected
  on encapsulation.**

---

## DD4: Reuse Analysis — everything REUSE; no new public type, no new trait, no new module

Hard gate per the Reuse-Choose-Author rule:

| Existing construct | Path | Decision | Rationale |
|---|---|---|---|
| `kaleidoscope_cli::migrate(...)` | `crates/kaleidoscope-cli/src/lib.rs:424-456` | **EXTEND** | Add a sixth `Option<&Path>` parameter (DD1). No structural change to the body except the match insertion at line 434. |
| `CinderToOtlpJsonWriter::new(file)` | `crates/self-observe/src/cinder_otlp_json.rs:175-180` | **REUSE AS-IS** | Public surface locked by ADR-0039 §1. Constructor is `new(W: Write + Send + Sync) -> Self`. Already imported in `lib.rs` (line 65 of the sibling feature's wiring; this feature adds nothing). |
| `cinder::NoopRecorder` (alias `CinderRecorder`) | `crates/kaleidoscope-cli/src/lib.rs:57, 434` | **REUSE IN `None` ARM** | Stays in the `None` arm verbatim. OK2 byte-equivalence on the no-flag path is preserved by construction. |
| `parse_observe_otlp(args)` | `crates/kaleidoscope-cli/src/main.rs:161-175` | **REUSE AS-IS** | Already feeds `run_ingest` and `run_read_with`. This feature is the third invocation site. No change. |
| `OpenOptions::new().create(true).append(true).open(path)` shape | `crates/kaleidoscope-cli/src/lib.rs:158-169` (ingest) and `:276-282` (read) | **REUSE THE SHAPE** | Identical flag set. ADR-0039 §8 names this exact incantation as the load-bearing one for the cross-writer atomicity property; on this path there is only one writer so the cross-writer property degrades to within-writer (already guaranteed by `CinderToOtlpJsonWriter`'s `Mutex<W>`), but the shape choice stays uniform across the three call sites. |
| `From<std::io::Error> for Error` | `crates/kaleidoscope-cli/src/lib.rs:104-108` | **REUSE** | The `OpenOptions::open(path)?` failure path lifts cleanly via `?`. No new error variant. |
| `parse_tier(s)` | `crates/kaleidoscope-cli/src/lib.rs:464-471` | **REUSE AS-IS** | Stays at line 431 (parse-before-open contract); unchanged. |
| `pre-flight get_entry` short-circuit | `crates/kaleidoscope-cli/src/lib.rs:437-442` | **REUSE AS-IS** | Stays after `FileBackedTieringStore::open`; unchanged. OK3 (UnknownItem → no `cinder.migrate.count` line) is automatic because `get_entry` does not call any `MetricsRecorder` method. |
| `Box<dyn cinder::MetricsRecorder + Send + Sync>` coercion | `crates/kaleidoscope-cli/src/lib.rs:155-184` (ingest precedent) | **REUSE THE SHAPE** | Same coercion idiom used in the `ingest` match. No new trait, no new type. |
| Existing test harness (`tenant`, `temp_root`, `cleanup`, `cinder_base`, `place_item`, `read_entry`, `bin` helpers) | `crates/kaleidoscope-cli/tests/migrate_subcommand.rs:35-160` | **DUPLICATE INLINE AT V0** | DISCUSS D5: rule-of-three deferral. The new test file `tests/migrate_observe_otlp_flag.rs` duplicates the helpers inline. This will be test file #11 in the cluster; extraction is a deliberate refactor across all eleven, not this feature. |

**Verdict**: **EXTEND** (the `migrate()` signature) + **REUSE**
(everything else). **No new public type, no new trait, no new
module, no new external crate.** Change surface: two files in
`src/` (`lib.rs`, `main.rs`) plus one new test file
(`tests/migrate_observe_otlp_flag.rs`) plus one new `[[test]]`
block in `Cargo.toml` plus mechanical signature updates on five
call sites (DD5).

---

## DD5: Mechanical signature-match updates on five 5-arg call sites

The `migrate()` signature change from 5-arg to 6-arg propagates to
five call sites workspace-wide. Each gains a literal `None` as the
sixth argument; no assertions are altered. This is the exact mirror
of how the locked `migrate_subcommand.rs` test file was previously
treated by `observe_otlp_read_flag.rs` and
`stats_cinder_tier_distribution.rs` for their respective
`read(...)` / `stats_with_tiers(...)` signature growths
(`TimeRange::all()` and `None` parameter additions).

| # | Location | Current call | Updated call |
|---|---|---|---|
| 1 | `crates/kaleidoscope-cli/src/main.rs:279` (`run_migrate_with`) | `migrate(&tenant, &data_dir, &item_id, &to_tier, stdout)?` | `migrate(&tenant, &data_dir, &item_id, &to_tier, stdout, otlp_path.as_deref())?` |
| 2 | `crates/kaleidoscope-cli/src/lib.rs:843` (inline white-box test `migrate_updates_migrated_at_to_current_clock_above_pre_call_time`) | `migrate(&acme, &data, "acme/forge-item", "cold", &mut buf)` | `migrate(&acme, &data, "acme/forge-item", "cold", &mut buf, None)` |
| 3 | `crates/kaleidoscope-cli/tests/migrate_subcommand.rs:187` (OK1 happy path) | `migrate(&acme, &data, "acme/batch-00042", "cold", &mut buf)` | append `, None` |
| 4 | `crates/kaleidoscope-cli/tests/migrate_subcommand.rs:233` (OK4 idempotent same-tier) | `migrate(&acme, &data, "acme/batch-00007", "hot", &mut buf)` | append `, None` |
| 5 | `crates/kaleidoscope-cli/tests/migrate_subcommand.rs:440` (OK2 cross-tenant isolation) | `migrate(&globex, &data, "acme/batch-00042", "cold", &mut buf)` | append `, None` |
| 6 | `crates/kaleidoscope-cli/tests/migrate_subcommand.rs:513` (UnknownItem stderr probe) | `migrate(&acme, &data, "ghost-item", "warm", &mut buf)` | append `, None` |

Six call sites total (the table reuses `#` for ordering; case 1 is
the production caller in `main.rs`, case 2 is the inline white-box
test in `lib.rs`, cases 3-6 are the locked `migrate_subcommand.rs`
test file). The locked-test-file edits are **mechanical
signature-match** only: zero assertions touched, zero scenarios
removed, zero scenarios renamed. The precedent for treating locked
tests this way is established by the previous two waves on this
crate.

**Precedent verification**: when `read()` grew the `TimeRange`
parameter in `observe-otlp-read-flag-v0`, `ingest_and_read_roundtrip.rs`
gained `TimeRange::all()` on every `read(...)` call without
assertion edits (see lines 109, 150, 182, 183 of that file). When
`stats_with_tiers(...)` grew the same `TimeRange` parameter,
`stats_cinder_tier_distribution.rs` gained the same suffix on every
call site without assertion edits (lines 272, 378, 450, 542, 647).
This DESIGN wave applies the **identical posture** to
`migrate(...)`'s `Option<&Path>` growth: locked tests gain `None`
as the sixth argument; assertions remain byte-equivalent.

---

## Out-of-scope confirmations

1. **No public API change to `CinderToOtlpJsonWriter`** (ADR-0039
   §1, DISCUSS D7). This feature consumes
   `CinderToOtlpJsonWriter::new(file)` unchanged.
2. **No multi-process scope**. The in-scope concurrency is the
   single thread inside one `kaleidoscope-cli migrate` invocation.
3. **No new flag, no new subcommand, no bulk-migrate, no
   `--dry-run`, no JSON-output** (DISCUSS D6, D7).
4. **No `--observe-otlp` on the from-tier read**. The pre-flight
   `get_entry` is read-only and does not call any
   `MetricsRecorder` method; OK3 is automatic and the test file
   asserts it explicitly.
5. **No new external crate dependency**. `self-observe` is already
   a `kaleidoscope-cli` dependency; the import statement at the top
   of `lib.rs` already names `CinderToOtlpJsonWriter` (inherited
   from `cli-cinder-otlp-wiring-v0`). No new line in
   `Cargo.toml`'s `[dependencies]`.
6. **No new ADR**. The wiring change is a direct application of the
   pattern locked by ADR-0039 §1, §2, and §8. Section §8 already
   covers the `OpenOptions` + `O_APPEND` choice; the present feature
   reuses that mechanism on a single-writer path, which is a strict
   simplification of §8's two-writer case. No new ADR-0040.

---

## DEVOPS handoff annotation

Recipient: `@nw-platform-architect`. Receives:

- **Inputs**:
  - This `design/wave-decisions.md`.
  - The accompanying `design/application-architecture.md` (C4 L1 +
    L2 inline; L3 explicitly skipped per the SA principle "L3 only
    for complex subsystems").
  - The DISCUSS-wave artefacts under
    `docs/feature/cli-migrate-observe-otlp-v0/discuss/` (locked).
  - Outcome KPIs (`discuss/outcome-kpis.md`): OK1 (principal — wire
    shape per successful migrate), OK2 (no-flag byte-equivalence
    guardrail), OK3 (UnknownItem → no emission), OK4 (InvalidTier
    → no file created).

- **Development paradigm for DELIVER**: Rust idiomatic per
  `CLAUDE.md`. Data + free functions + traits only where
  polymorphism is genuinely needed. No new trait, no new struct,
  no new `dyn` boundary beyond what already exists in the
  recorder-box coercion (`Box<dyn cinder::MetricsRecorder + Send +
  Sync>`, which is forced by the conditional construction over two
  concrete recorder types, not a design preference).

- **External integrations**: **none**. No new HTTP client, no
  webhook, no third-party API, no vendor SDK, no subprocess. The
  downstream OTLP/HTTP collector is at the operator's deployment
  boundary, not at this feature's boundary; the existing Lumen-side
  and Cinder-side wirings (commits `3af7e82` and the
  `cli-cinder-otlp-wiring-v0` follow-up) have already validated the
  wire-shape acceptability for the collector and the sidecar
  contract. **No contract-test recommendation applies.**

- **CI gates** (ADR-0005): the five existing workspace gates apply
  unchanged. The new acceptance test
  `cargo test --package kaleidoscope-cli --test migrate_observe_otlp_flag`
  exits 0 as the OK1/OK3/OK4 acceptance probe under Gate 1
  (`cargo test --workspace`). The locked-test file
  `tests/migrate_subcommand.rs` is the OK2 byte-equivalence probe
  and continues to pass green under the same Gate 1, with
  mechanical signature-match edits (six call sites, DD5) and **no
  assertion edits**. Gate 5 (`cargo mutants`) scope:
  `crates/kaleidoscope-cli/src/{lib,main}.rs` (modified files) at
  100% kill rate per ADR-0005 Gate 5. No new gate is added.

- **Workspace changes**: no `Cargo.toml` additions at the workspace
  root. `crates/kaleidoscope-cli/Cargo.toml` gains exactly one
  `[[test]]` block:

  ```toml
  [[test]]
  name = "migrate_observe_otlp_flag"
  path = "tests/migrate_observe_otlp_flag.rs"
  ```

  No new `[dependencies]` line; `self-observe` is already there.

- **Architectural-rule enforcement tooling** (Principle 11): no new
  tooling recommended for this feature. The existing five-gate
  workspace contract already enforces every rule this feature
  touches (public-API surface, SemVer compatibility, mutation kill
  rate, `cargo test` outcome). Rust does not have an idiomatic
  ArchUnit equivalent for the "OTLP file is opened only inside the
  `Some(path)` arm" property; the enforcement mechanism is the
  acceptance test's OK4 scenario (invalid-tier → sink file does not
  exist post-call), which fails loudly if any future refactor moves
  the open call site.

- **Earned Trust (Principle 12)**: the dependency at the boundary is
  the POSIX `OpenOptions::create(true).append(true).open(path)` +
  `write(2)` semantics. ADR-0039 §7 already catalogues the
  substrate lies this contract must survive (Docker overlayfs
  `fsync` no-op, WSL2 DrvFs `O_APPEND` semantics on small writes).
  The OK1 acceptance test exercises the contract on a real
  `tempfile`-derived path on the CI substrate (Linux + macOS per
  ADR-0005 matrix). No additional probe is added by this feature —
  the single-writer case is a strict subset of the two-writer case
  already probed by `cli-cinder-otlp-wiring-v0`'s
  `cross_writer_ndjson_validity_under_concurrent_random_pauses`
  acceptance test.
