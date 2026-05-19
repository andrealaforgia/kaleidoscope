# Wave Decisions — `cli-place-subcommand-v0` / DESIGN

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-19.

Mode: PROPOSE. DISCUSS named three decisions for DESIGN to lock —
D-FunctionShape (signature of the new library function), D-StderrWording
(exact bytes the OK3 fail-fast emits) and D-RecorderFactor (whether to
extract the `Some(path)` / `None` recorder-construction match into a
shared helper). The remaining surface is heavily constrained by
precedent: `migrate()` at `crates/kaleidoscope-cli/src/lib.rs:424-467`
(post `cli-migrate-observe-otlp-v0`) is the byte-for-byte template —
six positional parameters with `otlp_log_path: Option<&Path>` trailing,
`parse_tier(s)` short-circuit before any store open, recorder match
selecting `CinderToOtlpJsonWriter` vs `CinderRecorder`,
`FileBackedTieringStore::open(cinder_base(data_dir), recorder)`, then
the substantive call followed by one literal stdout line.

Scope inherited from DISCUSS (locked, not re-litigated): one new
subcommand `place`; one new free function in `lib.rs`; one modified
`main.rs` dispatcher (new arm + `run_place` / `run_place_with` helpers
+ usage paragraph); one new acceptance test file
`tests/place_subcommand.rs`; lower-case-only tier argument
(D-LowerCase); `SystemTime::now()` at the call site (D-Timestamp);
Cinder-only open, no Lumen touch (D-NoLumenTouch); faithful to the
underlying overwrite-semantics API (D-Overwrite); `--observe-otlp
<path>` as the ONLY optional flag (D-ObserveOtlp); locked test files
NOT modified (D-LockedTests); no SSOT journey or `jobs.yaml`
modification (D-NoSSOT).

---

## DD1: Function shape — mirror `migrate()` byte-for-byte, simpler body

**Decision**: Add a new free function

```rust
pub fn place(
    tenant: &TenantId,
    data_dir: &Path,
    item_id: &str,
    tier_arg: &str,
    mut writer: impl Write,
    otlp_log_path: Option<&Path>,
) -> Result<(), Error>
```

in `crates/kaleidoscope-cli/src/lib.rs`. The function:

1. Parses `tier_arg` via the existing private helper
   [`parse_tier`](../../../../crates/kaleidoscope-cli/src/lib.rs#L505-L512).
   On parse failure returns `Err(Error::InvalidTier { value })`
   carrying the verbatim invalid input, BEFORE opening any store.
2. Constructs the Cinder recorder via the existing `match
   otlp_log_path { Some(path) => Box::new(CinderToOtlpJsonWriter::new(
   OpenOptions::new().create(true).append(true).open(path)?)), None
   => Box::new(CinderRecorder) }` pattern, byte-for-byte mirror of
   `migrate()` lines 435-444.
3. Opens `FileBackedTieringStore::open(cinder_base(data_dir),
   recorder)`, mapping open failure to `Err(Error::CinderOpen(_))`.
4. Calls `cinder.place(tenant, &ItemId::new(item_id.to_string()),
   tier, SystemTime::now())`. The trait method at
   `crates/cinder/src/store.rs:78-81` returns `()` — overwrite-
   semantics, no failure modes at the trait surface. NO `Result`
   to unwrap. NO `.map_err(...)`.
5. Writes one literal line to `writer`: `placed tenant=<tenant>
   item=<item_id> tier=<tier>\n`, using `tier_lowercase(tier)` for
   the `<tier>` token.
6. Returns `Ok(())`.

The Lumen store is NEVER opened (D-NoLumenTouch). NO pre-flight
`get_entry` (overwrite-semantics by design; D-Overwrite +
D-OutOfScope-ExistsCheck). NO `get_entry` post-call read (the
acceptance test owns that oracle on its own
`FileBackedTieringStore` instance).

**Function name choice**: `place`, parallel to `ingest`, `read`,
`stats`, `stats_with_tiers`, `migrate`, `list_items`. The free
function is the seventh sibling on the library's public surface.

**Rationale**:

1. **Six-parameter signature matches `migrate()` exactly.** Both
   functions take `(tenant, data_dir, item_id, tier_arg, writer,
   otlp_log_path)`. Adopting the same order at the same arity
   removes one degree of freedom the operator's muscle memory has
   to track. The acceptance test harness's call shape is the same
   modulo the function name. Departing from this order (e.g.
   reordering `otlp_log_path` to position 3 to match `ingest()`'s
   shape, where it sits after `batch_size`) would needlessly
   complicate DISTILL's translation of the slice's five `#[test]`
   scenarios.

2. **Simpler body than `migrate()`** because `TieringStore::place`
   has overwrite-semantics and a `()` return. Three differences
   from `migrate()`:
   - No `get_entry` pre-flight (no need to discover a `from`
     tier; there is no `from`).
   - No `.map_err(Error::CinderMigrate)?` lift (the trait method
     cannot fail).
   - The stdout line names ONE tier (`tier=<x>`), not two
     (`from=<f> to=<t>`).

3. **Returning `Result<(), Error>` rather than a typed
   `PlaceReport` struct**. The stdout bytes are the only consumer
   of the tier information. No library caller asks for the report
   structure; no test destructures it. A typed `PlaceReport`
   would be a premature abstraction — same argument as `migrate`'s
   DD1 rationale 2 and `cli-stats-cinder-tier-distribution-v0`'s
   DD5. **The writer parameter is the contract.**

4. **`item_id: &str` and `tier_arg: &str` rather than typed
   `ItemId` and `Tier`**. The function sits at the parse boundary;
   its job INCLUDES validating the raw argument string for tier.
   Passing typed `ItemId` and `Tier` would shift parse
   responsibility to the caller (`main.rs`), splitting it across
   the binary/library boundary. Same argument as `migrate`'s DD1
   rationale 3.

**Rejected alternative — Option B (no `otlp_log_path` parameter;
hard-wire `CinderRecorder`)**: contradicts D-ObserveOtlp
(`--observe-otlp` is mandated by DISCUSS and the task brief).
**Rejected.**

**Rejected alternative — return a typed `PlaceReport { tier:
Tier }` struct**: premature abstraction (zero library callers
consume the structure; stdout is the only consumer). **Rejected.**

**Rejected alternative — accept typed `ItemId` and `Tier`
arguments**: shifts parse responsibility to `main.rs`, splits
parse logic across the library / binary boundary. **Rejected.**

**Rejected alternative — Option C (pre-flight `get_entry` to log a
"will-overwrite" warning)**: contradicts D-Overwrite (CLI faithfully
reflects the API; no guard; no warning). **Rejected.**

---

## DD2: Recorder construction — byte-for-byte mirror of `migrate()`

**Decision**: The recorder construction inside `place()` is a
literal copy of `migrate()` lines 435-444:

```rust
let recorder: Box<dyn CinderRec + Send + Sync> = match otlp_log_path {
    Some(path) => {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Box::new(CinderToOtlpJsonWriter::new(file))
    }
    None => Box::new(CinderRecorder),
};
```

No helper extraction. The match block lives between the
`parse_tier(tier_arg)?` short-circuit and the
`FileBackedTieringStore::open(...)` line.

**Rationale**:

1. **Mechanical mirror.** Copying the existing nine-line pattern
   verbatim is the lowest-risk way to honour the ADR-0039 §8
   incantation (`create(true).append(true)`, single open, no
   truncate). Any divergence introduces a new failure mode the
   acceptance test does NOT assert on.

2. **D-RecorderFactor (rule-of-three deferred).** The match
   pattern now appears at four sites: `ingest()` (lines 158-184,
   with `try_clone` for the dual-writer Lumen + Cinder shape),
   `migrate()` (lines 435-444), and now `place()` (new). The
   `ingest()` shape diverges (it opens TWO writers via
   `try_clone`); only `migrate()` and `place()` share the
   single-writer single-file shape. Two sites is NOT the rule of
   three. Extraction is deferred to a follow-up refactoring wave
   when a fourth single-writer site materialises.

3. **Short-circuit ordering preserved.** `parse_tier(tier_arg)?`
   runs FIRST. Invalid-tier inputs never reach the
   `OpenOptions::create(true).append(true).open(path)?` line, so
   the OK3 invariant ("no file created on invalid-tier failure")
   is preserved by construction — same posture as
   `cli-migrate-observe-otlp-v0`'s DD2.

**Rejected alternative — extract a `cinder_recorder(otlp_log_path:
Option<&Path>) -> Result<Box<dyn CinderRec + Send + Sync>,
std::io::Error>` helper now**: premature abstraction at two
sites; the cost of refactoring two call sites later when the
third single-writer site appears is lower than the cost of
designing the helper's API today without three concrete
consumers. **Rejected.**

**Rejected alternative — open the OTLP file INSIDE
`FileBackedTieringStore::open` via a recorder-construction
closure**: breaks the layering (Cinder doesn't know about
`std::fs::OpenOptions`). **Rejected.**

---

## DD3: No new `Error` variant

**Decision**: The existing
[`Error`](../../../../crates/kaleidoscope-cli/src/lib.rs#L72-L88)
enum is sufficient. No new variant is added.

| Failure path | Existing variant |
|---|---|
| Invalid `tier_arg` (parse short-circuit) | `Error::InvalidTier { value }` (lib.rs:79-81) |
| `FileBackedTieringStore::open` failure | `Error::CinderOpen(MigrateError)` (lib.rs:77) |
| `OpenOptions::open(path)?` failure on `--observe-otlp` | `Error::Io(std::io::Error)` (lib.rs:82) via the existing `From<std::io::Error>` impl (lib.rs:112-116) |
| `writeln!(writer, ...)?` failure | `Error::Io(std::io::Error)` via the same `From` impl |

`TieringStore::place(tenant, item, tier, placed_at)` at
`crates/cinder/src/store.rs:78-81` returns `()`. There is NO
`Result` to lift, no `PlaceError` to wrap, no new variant to
introduce. The `record_place` recorder call inside the
`FileBackedTieringStore::place` adapter is fire-and-forget on
this code path (same posture as `record_migrate` inside
`migrate()`).

**Rationale**:

1. **The trait surface has no place-side failure.** Adding a
   variant for a failure mode that the upstream API cannot signal
   would be speculative typing. If a future Cinder revision
   introduces `TieringStore::place(...) -> Result<(),
   PlaceError>`, that wave introduces the variant. Today's wave
   does not.

2. **Mirrors `cli-cinder-otlp-wiring-v0`'s posture.** That
   feature also did not add a new variant for the recorder-
   construction file-open failure (it relied on `Error::Io` via
   `From`).

3. **`Error::InvalidTier { value }` already carries the verbatim
   invalid input** (used by `migrate` at lines 432-434 and
   `list_items` at lines 486-488). The third call site is `place`
   on the parse failure path. The variant's `Display` impl at
   lines 98-100 renders `invalid tier "<value>": expected one of
   hot, warm, cold` — the OK3 substring invariant ("stderr
   contains the verbatim invalid value") is satisfied verbatim.

**Rejected alternative — introduce `Error::CinderPlace(...)`
mirroring `Error::CinderMigrate`**: there is no `MigrateError`-
equivalent to wrap (the trait method returns `()`). A synthetic
wrapper would have to invent a payload type with no semantic
content. **Rejected.**

---

## DD4: Reuse Analysis (RCA F-1 hard gate) — 100% reuse

| Existing component | Path | Decision | Rationale |
|---|---|---|---|
| `cinder_base(data_dir)` private helper | `crates/kaleidoscope-cli/src/lib.rs:130-132` | **REUSE** | Used by `ingest()`, `migrate()`, `list_items()`, `stats_with_tiers()`. The fifth call site. |
| `FileBackedTieringStore::open` | `crates/kaleidoscope-cli/src/lib.rs:187-188` (ingest), `:445-446` (migrate), `:489-490` (list_items) | **REUSE** | Same construction pattern. The fourth call site. |
| `cinder::NoopRecorder` (alias `CinderRecorder`) | `crates/kaleidoscope-cli/src/lib.rs:58, 443` | **REUSE** | Quiescent recorder for the `None` arm. Same construction pattern as `migrate()`'s no-flag arm. |
| `self_observe::CinderToOtlpJsonWriter` | `crates/kaleidoscope-cli/src/lib.rs:65, 173-174, 441` | **REUSE** | OTLP-JSON sink for the `Some(path)` arm. Third call site. |
| `std::fs::OpenOptions::new().create(true).append(true).open(path)?` (ADR-0039 §8) | `crates/kaleidoscope-cli/src/lib.rs:166-169, 437-440` | **REUSE** | Single open with append semantics. Third call site. |
| `TieringStore::place(tenant, item, tier, placed_at)` | `crates/cinder/src/store.rs:78-81` | **REUSE** | The mutation API. One call per `place` invocation. Overwrite-semantics; returns `()`. |
| `cinder::ItemId` + `ItemId::new(s)` | `crates/cinder/src/tier.rs` | **REUSE** | Wraps the raw `item_id: &str` argument for the Cinder call. |
| `cinder::Tier` enum | `crates/cinder/src/tier.rs` | **REUSE** | The `Tier::Hot` / `Tier::Warm` / `Tier::Cold` variants are pattern targets of `parse_tier` and `tier_lowercase`. |
| `parse_tier(s: &str) -> Result<Tier, ()>` private helper | `crates/kaleidoscope-cli/src/lib.rs:505-512` | **REUSE** | The fourth call site (after `migrate`, `list_items`, and the freshly-shipped `cli-list-items-subcommand-v0`'s call site). No visibility promotion needed — `place()` is in the same module. |
| `tier_lowercase(tier) -> &'static str` private helper | `crates/kaleidoscope-cli/src/lib.rs:519-525` | **REUSE** | Renders the `<tier>` token of the stdout report. Same-module visibility. |
| `Error::InvalidTier { value: String }` variant + Display | `crates/kaleidoscope-cli/src/lib.rs:79-81, 98-100` | **REUSE** | Materialised by the `parse_tier(tier_arg).map_err(|_| Error::InvalidTier { value: tier_arg.to_string() })?` short-circuit. The fourth call site. |
| `Error::CinderOpen(MigrateError)` variant | `crates/kaleidoscope-cli/src/lib.rs:77, 96` | **REUSE** | Covers `FileBackedTieringStore::open` failure. |
| `Error::Io(std::io::Error)` variant + `From<std::io::Error> for Error` impl | `crates/kaleidoscope-cli/src/lib.rs:82, 112-116` | **REUSE** | Lifts `OpenOptions::open(path)?` and `writeln!(writer, ...)?` failures. |
| `parse_positional` helper | `crates/kaleidoscope-cli/src/main.rs` (existing) | **REUSE** | New `run_place` parses `args[2]` (tenant) and `args[3]` (data_dir) identically. The new positional `args[4]` (item_id) and `args[5]` (tier) parse inline in `run_place`. |
| `parse_observe_otlp` helper | `crates/kaleidoscope-cli/src/main.rs` (existing, shared with `ingest`, `read`, `migrate`) | **REUSE** | Fourth call site. Detects `--observe-otlp <path>` in argv. |
| `aegis::TenantId` | reused as in every sibling | **REUSE** | The tenant argument. |
| `cinder::MetricsRecorder as CinderRec` trait + `Box<dyn CinderRec + Send + Sync>` coercion idiom | `crates/kaleidoscope-cli/src/lib.rs:57` | **REUSE** | Same `dyn` boundary as `migrate()`. No new trait, no new boundary. |
| Inline test-harness helpers (`tenant`, `record`, `temp_root`, `cleanup`, `ndjson`) | `crates/kaleidoscope-cli/tests/migrate_observe_otlp_flag.rs` and twelve siblings | **DUPLICATE INLINE AT V0** | DISCUSS D-NewTestFile confirms rule-of-three extraction deferred. This is the 13th test file in the cluster. |
| `pub fn place(...)` library function | n/a — new | **CREATE** | DD1. ~20 lines (simpler body than `migrate()`'s ~25). The seventh public free function on the library surface. |
| `run_place` / `run_place_with` binary helpers + dispatch arm + usage paragraph | n/a — new in `main.rs` | **CREATE** | Mirrors `run_migrate` / `run_migrate_with` byte-for-byte modulo the function name and the `tier_arg` parameter (no `to_tier_arg` renaming). ~25 lines total including the usage paragraph. |
| New `PlaceReport` / `TierPlacement` struct | n/a | **DO NOT CREATE** | DD1 rationale 3: stdout is the only consumer; premature abstraction. |
| New `Error::CinderPlace(_)` variant | n/a | **DO NOT CREATE** | DD3: the trait method returns `()`; no payload to wrap. |
| New `TieringStore::place_if_absent(...)` trait method | n/a | **DO NOT CREATE** | DISCUSS D-Overwrite + D-OutOfScope-ExistsCheck explicit. Speculative addition; deferred to a follow-up feature if a no-overwrite mode is ever needed. |

**Verdict**: **100% REUSE on the production substrate.** Seventeen
existing constructs (`cinder_base`, `FileBackedTieringStore::open`,
`NoopRecorder`, `CinderToOtlpJsonWriter`, `OpenOptions`, `place`,
`ItemId`, `Tier`, `parse_tier`, `tier_lowercase`, `Error::InvalidTier`,
`Error::CinderOpen`, `Error::Io` + `From<io::Error>`,
`parse_positional`, `parse_observe_otlp`, `TenantId`, the `Box<dyn
CinderRec + Send + Sync>` coercion idiom). **CREATE NEW**: one
public free function (`place`), and the binary-side dispatch arm +
`run_place` / `run_place_with` helpers + usage paragraph. **No new
public type, no new trait, no new module, no new external crate, no
new `Error` variant.** Change surface: two files in `src/`
(`lib.rs`, `main.rs`) plus one new test file
(`tests/place_subcommand.rs`) plus one new `[[test]]` block in
`Cargo.toml`.

---

## DD5: Out-of-scope confirmations

The DESIGN wave confirms (does not re-litigate) the following
DISCUSS decisions:

1. **No `--placed-at` flag** (D-Timestamp / D-OutOfScope-PlacedAt).
   `SystemTime::now()` at the call site.
2. **No bulk placement** (D-OutOfScope-Bulk). One item per
   invocation.
3. **No "already placed" verification** (D-Overwrite /
   D-OutOfScope-ExistsCheck). The CLI faithfully reflects the
   overwrite-semantics of `TieringStore::place`.
4. **No Lumen mutation** (D-NoLumenTouch / D-OutOfScope-LumenMutation).
   `lumen_base(data_dir)` is NOT called.
5. **Lower-case tier argument only** (D-LowerCase). `parse_tier`
   matches three literal strings.
6. **No `--dry-run` / `--json` / `--csv` / `--format=...`**
   (inherited from cluster posture). Plain-text stdout only.
7. **Locked test files NOT modified** (D-LockedTests). The twelve
   existing test files continue to pass green UNMODIFIED.
8. **No SSOT journey or `jobs.yaml` modification** (D-NoSSOT).
9. **No new ADR**. Same reasoning as `cli-migrate-subcommand-v0`'s
   DD7.11 and `cli-list-items-subcommand-v0`. The new `place`
   function is the seventh public free function on
   `kaleidoscope-cli`'s library surface, following the established
   shape. No new public type, no new abstraction, no new module,
   no new external dependency, no new `Error` variant. ADR-0001
   absorbs the addition without amendment.
10. **D-StderrWording**: the inherited `Error::InvalidTier`
    Display impl at lines 98-100 (`invalid tier "<value>":
    expected one of hot, warm, cold`) is accepted unchanged. No
    `place`-specific phrasing is introduced. The OK3 substring
    invariant is satisfied verbatim.

---

## DEVOPS handoff annotation

Recipient: `@nw-platform-architect`. Receives:

- **Inputs**:
  - This `design/wave-decisions.md`.
  - The accompanying `design/application-architecture.md` (C4 L1
    and L2; L3 explicitly skipped, reification conditions
    documented).
  - The new subsection appended to
    `docs/product/architecture/brief.md > ## Application
    Architecture — cli-place-subcommand-v0`.
  - The DISCUSS-wave artefacts under
    `docs/feature/cli-place-subcommand-v0/discuss/` (locked, not
    modified).
  - Outcome KPIs (`discuss/outcome-kpis.md`): OK1-CLI-place-
    success (principal — placement correctness on stdout +
    post-call `get_entry().tier == tier`), OK2-CLI-place-overwrite-
    semantics (guardrail — second call overwrites the first),
    OK3-CLI-place-invalid-tier-fail-fast (stderr names verbatim
    invalid value), OK4-CLI-place-observe-otlp-emission (one
    `cinder.place.count` line appended per call when
    `--observe-otlp` is set).

- **Development paradigm for DELIVER**: Rust idiomatic per
  `CLAUDE.md`. Data + free functions + traits where genuinely
  needed. The new `place()` is a free function. No new trait. No
  new struct. No new `dyn` boundary beyond the existing `Box<dyn
  CinderRec + Send + Sync>` at the recorder construction site
  (inherited from `migrate()`'s shape).

- **External integrations**: **none**. No HTTP client, no
  webhook, no third-party API, no vendor SDK, no subprocess, no
  network I/O. Pure local mutation of the Cinder WAL+snapshot via
  the existing `TieringStore::place` trait method, plus optional
  local append to an operator-supplied OTLP-JSON sidecar file via
  the already-shipped `CinderToOtlpJsonWriter`. The Lumen store
  is not opened. No contract-test recommendation applies.

- **External dependency footprint**: **no new external crate**.
  All used types (`TenantId`, `Tier`, `ItemId`,
  `FileBackedTieringStore`, `TieringStore`, `NoopRecorder` alias
  `CinderRecorder`, `CinderToOtlpJsonWriter`) are already in
  `kaleidoscope-cli`'s use list at
  `crates/kaleidoscope-cli/src/lib.rs:55-65`. `Cargo.lock`
  churn is zero beyond what a recompile produces.

- **CI gates** (ADR-0005): the five existing workspace gates
  apply unchanged. The new acceptance test `cargo test --package
  kaleidoscope-cli --test place_subcommand` exits 0 as the
  OK1/OK2/OK3/OK4 acceptance probe under Gate 1 (`cargo test
  --workspace`). The twelve locked test files continue to pass
  green and collectively serve as the no-regression oracle for
  every shipped CLI feature. **No new gate is added.**

  Specifically on **Gate 5 (mutation testing)**: the existing
  `gate-5-mutants-kaleidoscope-cli` job is path-filtered on
  `crates/kaleidoscope-cli/**` via `--in-diff`. Any commit
  touching `crates/kaleidoscope-cli/src/lib.rs` or
  `crates/kaleidoscope-cli/src/main.rs` (this feature touches
  both) is automatically mutated by the existing job. The new
  `place()` body's branches (the `parse_tier` match arm via
  `map_err`, the recorder construction match, the `writeln!`
  line), the new `run_place` / `run_place_with` dispatch path,
  and the updated usage paragraph all fall inside the same
  mutation surface. **No new Gate 5 job needed.**

- **Workspace changes**: no `Cargo.toml` additions at the
  workspace root. `crates/kaleidoscope-cli/Cargo.toml` gains
  exactly one new `[[test]]` block:

  ```toml
  [[test]]
  name = "place_subcommand"
  path = "tests/place_subcommand.rs"
  ```

  No new `[dependencies]` line; no new `[dev-dependencies]` line.

- **Mutation-testing scope** (per `CLAUDE.md` and ADR-0005 Gate
  5): scoped to `crates/kaleidoscope-cli/src/lib.rs` and
  `crates/kaleidoscope-cli/src/main.rs` (the two modified source
  files). Run after the DELIVER refactor pass. 100% kill rate.
  The changed code surface is small (the new `place()` function
  ~20 lines plus the `main.rs` delta ~25 lines = ~45 lines of
  new production source). Mutation-testing budget should be
  modest.

- **Architectural-rule enforcement tooling** (Principle 11): no
  new tooling is recommended for this feature. The existing
  five-gate workspace contract already enforces every rule this
  feature touches. The "no `migrate()` / `evaluate_at()` / Lumen
  open in `place()`" property is structurally enforced by the
  acceptance test (D-NoLumenTouch byte-equivalence assertion on
  the `<data_dir>/lumen.*` directory before and after the call,
  inherited from the cluster's harness shape).

### Why no ADR change

The new `place` function introduces **no new public type, no new
abstraction, no new module, no new external dependency, no new
`Error` variant**. It is the seventh public free function on
`kaleidoscope-cli`'s library surface, parallel to `ingest`,
`read`, `stats`, `stats_with_tiers`, `migrate`, `list_items`,
following the same shape (tenant + data_dir + positional args +
writer + optional otlp_log_path → result). The change is captured
within the established free-function-plus-typed-error pattern.
ADR-0001 absorbs the addition without amendment.
