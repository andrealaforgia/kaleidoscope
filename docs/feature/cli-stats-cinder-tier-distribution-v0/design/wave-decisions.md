# Wave Decisions — `cli-stats-cinder-tier-distribution-v0` / DESIGN

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-19.

Mode: PROPOSE. The DISCUSS artefacts collapse the design space almost
entirely: DISCUSS D9 already rules out in-place modification of the
existing `stats()` (because the predecessor's locked
`tests/stats_subcommand.rs` calls `ingest(.., None)` which places one
Hot Cinder item per batch, so an in-place extension would break the
predecessor's "exactly 3 non-empty lines" assertion); DISCUSS D10 makes
the no-modification rule on `tests/stats_subcommand.rs` a hard
contract; DISCUSS D-EmptyRender locks Option B (selective emission for
non-zero tiers); DISCUSS D7/D8 lock the keys (`hot`/`warm`/`cold`,
lower-case) and the order (`hot` then `warm` then `cold`). DESIGN's
load-bearing job is to lock the function name (the candidate set is
the two shapes named in DISCUSS D9), confirm the iteration strategy,
confirm the Cinder construction site, and discharge the Reuse Analysis
hard gate.

Scope inherited from DISCUSS (locked, not re-litigated): no new
subcommand, no new flag (D1, D5, D6); no Cinder placement, migration,
or policy evaluation (D2, D3); no per-item dump (D4); selective
emission of non-zero tier lines per Option B (D-EmptyRender); byte-
equivalent stdout for tenants with zero Cinder placements
(D-Backwards-compat / OK4); keys `hot`/`warm`/`cold` lower-case (D7) in
order `hot` → `warm` → `cold` (D8); new acceptance test file
`crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`
(D10) and `tests/stats_subcommand.rs` is NOT modified (D10).

---

## DD1: Function shape — Option A, new `stats_with_tiers` sibling

**Decision**: Add a new free function

```rust
pub fn stats_with_tiers(
    tenant: &TenantId,
    data_dir: &Path,
    mut writer: impl Write,
) -> Result<usize, Error>
```

in `crates/kaleidoscope-cli/src/lib.rs`. The existing `stats()`
function is left untouched. The `main.rs` dispatcher's
`Some("stats") => run_stats(&args)` arm is repointed so that
`run_stats` calls `stats_with_tiers(..)` instead of `stats(..)`. The
returned `usize` continues to be the Lumen record count (mirrors
`read()`'s and `stats()`'s return shape).

**Function name choice**: `stats_with_tiers`, not `stats_with_cinder`
or `stats_full`. Rationale: the public-visible concept is "tier
distribution" (the keys are `hot`/`warm`/`cold`, the user story is
"see the tier distribution"); `cinder` is the substrate, not the
concept. `stats_full` is too vague — it does not say what is added.
`stats_with_tiers` reads in the user's vocabulary and pairs with the
existing `stats` ("the one without tier counts") without leaking the
crate name into the public API.

**Rationale**:

1. **DISCUSS D9 hard-rules out in-place modification.** The
   predecessor's locked `tests/stats_subcommand.rs` calls
   `ingest(&acme, &data, DEFAULT_BATCH_SIZE, .., None)` which goes
   through `flush()` at `crates/kaleidoscope-cli/src/lib.rs:243-244`
   and places one Hot Cinder item per batch via `cinder.place(tenant,
   &item, Tier::Hot, SystemTime::now())`. Every populated-tenant test
   in the locked file would acquire at least one `hot=N` line if
   `stats()` itself were extended, breaking the "exactly 3 non-empty
   lines" assertion at `tests/stats_subcommand.rs:210` and
   `tests/stats_subcommand.rs:328` and `tests/stats_subcommand.rs:416`.
   In-place extension is structurally impossible without breaking the
   locked file, and DISCUSS D10 makes that file's modification a hard
   contract violation.

2. **Option A over Option B (rename + new `stats`).** Option B would
   rename `stats()` to (say) `stats_lumen_only()` and introduce the
   new behaviour under the name `stats`. The rename is a library API
   break for any caller that uses `kaleidoscope_cli::stats` by name
   today. The only callers today are `main.rs::run_stats` (in-tree)
   and `tests/stats_subcommand.rs` (locked by D10 — modifying its
   `use kaleidoscope_cli::{.., stats, ..}` import would be a
   modification of the locked file, forbidden). Option A leaves both
   call sites intact: `tests/stats_subcommand.rs` still imports and
   calls `stats`; only `main.rs` is repointed to `stats_with_tiers`.

3. **The "dead code" objection against Option A is weak.** The
   existing `stats()` becomes a library-only entry point: still
   reachable, still tested by the locked acceptance file (which is
   the most thorough byte-level oracle on its behaviour anywhere in
   the codebase), and still useful to any future library caller that
   genuinely wants Lumen-only stats (e.g. a hypothetical future
   integration test that wants to observe Lumen state without
   touching Cinder). It is dead with respect to the binary, not dead
   with respect to the test surface or the library API. The
   `#[allow(dead_code)]` attribute is NOT needed (the function is
   reachable from the locked test file, which keeps the compiler
   happy).

4. **Self-application note.** If a future feature ever needs to
   delete the old `stats()` (e.g. a v1 cleanup pass), THAT feature
   owns the locked-test contract renegotiation. Today's posture is
   "leave it, it is cheap, it is the byte-level oracle for OK4".

**Rejected alternative — Option B (rename + new `stats`)**: breaks
the import in the locked `tests/stats_subcommand.rs:69`. D10
forbids touching that file. **Rejected.**

**Rejected alternative — Option C (modify the locked test assertions
to expect a new `hot=1` line)**: explicitly banned by DISCUSS D10
("hard rule: DO NOT modify `tests/stats_subcommand.rs`"). The locked
file IS the byte-level oracle for OK4 (backwards-compatibility);
modifying its assertions would destroy that oracle. **Rejected.**

**Rejected alternative — Option D (optional `with_cinder: bool` or
`Option<&FileBackedTieringStore>` parameter on `stats()`)**: Rust has
no overload mechanism and no default parameters. Adding a fourth
parameter changes `stats()`'s arity, which breaks the locked test's
three-arg call site at `tests/stats_subcommand.rs:201`, `:260`,
`:317`, `:404`, `:471`. The only way Option D could be non-breaking
is to introduce a sibling function under a different name, which
collapses into Option A or B. **Rejected.**

---

## DD2: Cinder iteration strategy — three `list_by_tier` calls in `hot, warm, cold` order

**Decision**: Inside `stats_with_tiers()`, after the existing Lumen-
side lines are written, iterate the Cinder tiers in a fixed order and
emit one line per tier whose `list_by_tier(tenant, tier).len()` is
non-zero:

```rust
for tier in [Tier::Hot, Tier::Warm, Tier::Cold] {
    let count = cinder.list_by_tier(tenant, tier).len();
    if count > 0 {
        let key = match tier {
            Tier::Hot => "hot",
            Tier::Warm => "warm",
            Tier::Cold => "cold",
        };
        writeln!(writer, "{key}={count}")?;
    }
}
```

Hardcode the array `[Tier::Hot, Tier::Warm, Tier::Cold]`. Do NOT add
a `Tier::all()` associated function to the `cinder` crate.

**Rationale**:

1. **The order is locked by DISCUSS D8.** The keys must appear in
   `hot` → `warm` → `cold` order when more than one line is emitted.
   A hardcoded array literal expresses the order at its single
   point of truth (the `for` loop above) and is impossible to
   misorder accidentally.

2. **No `Tier::all()` exists today** in `crates/cinder/src/tier.rs`
   (lines 28-44 define the enum, its derives, and `next_forward`;
   no `pub fn all()`). Adding one would expand the `cinder` crate's
   public API surface for a single in-crate use. The Reuse-Choose-
   Author rule favours not creating a public abstraction until at
   least one second caller exists.

3. **Three calls, not one walk.** The `TieringStore` trait at
   `crates/cinder/src/store.rs:101-102` exposes `list_by_tier(tenant,
   tier) -> Vec<ItemId>`. There is no `list_all_tiers(tenant) ->
   HashMap<Tier, Vec<ItemId>>` aggregating call. Three calls is the
   contract; the cost is three `HashMap`-style lookups against the
   in-memory tier index of `FileBackedTieringStore`, which is
   negligible.

4. **The `.len()` reduction discards the `Vec<ItemId>` immediately.**
   `list_by_tier` returns owned `Vec<ItemId>` (per the trait
   signature); `stats_with_tiers()` calls `.len()` and drops the
   vector at the end of the iteration. No per-item allocation
   survives; no item id is rendered to stdout (D4 forbids per-item
   dumps).

5. **The Option B selective-emission contract is enforced by the
   `if count > 0` guard.** A tier with zero items produces no line.
   This is what makes OK4 (backwards-compatibility) achievable: a
   tenant with all-zero tiers emits exactly the predecessor's
   stdout shape.

**Rejected alternative — `Tier::Hot.next_forward()` chain walk**: the
existing `Tier::next_forward` (`crates/cinder/src/tier.rs:38-44`)
walks `Hot → Warm → Cold → None`. A `while let Some(t) = current
{...}` loop would express the same iteration, in the same order,
without the array literal. **Rejected** because (a) the loop body
gains a tier variable rebinding (`current = t.next_forward()`) for
no clarity gain, (b) the array literal is more obviously
exhaustive — adding a hypothetical `Tier::Archive` variant would
fail-compile the `match key` arm but the chain walk would silently
include the new tier without a key mapping.

**Rejected alternative — single iteration over the inner `HashMap`**:
the `FileBackedTieringStore` does not expose its inner map; the only
trait-level enumeration call is `list_by_tier`. Reaching into the
adapter's private state would violate the port-and-adapter boundary
(`stats_with_tiers()` is a driving-side composer; it must go through
the `TieringStore` port). **Rejected.**

---

## DD3: Cinder construction — `FileBackedTieringStore::open(cinder_base(data_dir), Box::new(NoopRecorder))`

**Decision**: `stats_with_tiers()` constructs the Cinder store
identically to how `ingest()`'s no-flag arm does it
(`crates/kaleidoscope-cli/src/lib.rs:173-174` and `:179-180`):

```rust
let cinder = FileBackedTieringStore::open(
    cinder_base(data_dir),
    Box::new(CinderRecorder),
)
.map_err(Error::CinderOpen)?;
```

Where `CinderRecorder` is the existing import alias for
`cinder::NoopRecorder` (already in `lib.rs`'s use list at line 58).
`cinder_base(data_dir)` is the existing private helper at
`crates/kaleidoscope-cli/src/lib.rs:122-124`.

**Rationale**:

1. **Quiescent recorder pattern is the DISCUSS-locked posture.**
   DISCUSS handoff item 1 reads: "the function constructs a
   `cinder::NoopRecorder` for the `FileBackedTieringStore::open(...)`
   call identically to the way `ingest()` does in its no-flag arm
   (`crates/kaleidoscope-cli/src/lib.rs:170-174`); no OTLP file is
   created and no `--observe-otlp` flag is accepted in v0". This DD
   confirms the wiring.

2. **`Error::CinderOpen(MigrateError)` already exists** at
   `crates/kaleidoscope-cli/src/lib.rs:77` and is the canonical
   failure mode for this construction site (already used by
   `ingest()` at line 180). No new variant needed.

3. **No try_clone-of-OTLP-file shenanigans.** `stats_with_tiers()`
   does not accept an `otlp_log_path`; the recorder is the no-flag
   `NoopRecorder`; no file handle is shared between Lumen and Cinder
   sides (the Lumen side keeps its own `LumenToPulseRecorder` —
   see DD4).

---

## DD4: Empty-Cinder rendering — Option B (omit line if count == 0)

**Decision**: Inherit DISCUSS D-EmptyRender Option B verbatim. The
`if count > 0` guard in the loop body of DD2 IS the implementation of
this contract. There is no architectural choice to add here; this DD
exists as a single-point-of-truth pointer back to the DISCUSS
decision and to confirm DESIGN does not deviate.

**Rationale**:

1. DISCUSS D-EmptyRender already documented the rationale at length
   (`docs/feature/cli-stats-cinder-tier-distribution-v0/discuss/wave-decisions.md`
   lines 56-114): Option A would silently hide orphan tier metadata
   for empty-Lumen tenants; Option C would break byte-equivalence for
   populated-Lumen-empty-Cinder tenants (OK4 guardrail).

2. The DESIGN-level confirmation is that the `if count > 0` guard
   lives in the iteration loop (DD2), not in a separate post-render
   pass. One branch, one place.

3. The `\n` line terminator is `writeln!`'s default behaviour. The
   stdout output ends with `\n` (preserves the existing predecessor
   contract).

---

## DD5: Reuse Analysis (RCA F-1 hard gate)

| Existing component | Path | Decision | Rationale |
|---|---|---|---|
| `kaleidoscope_cli::stats` body shape | `crates/kaleidoscope-cli/src/lib.rs:313-332` | **EXTEND THE SHAPE** | `stats_with_tiers()` reuses the entire body of `stats()` verbatim for the Lumen side: quiescent `LumenToPulseRecorder`, `FileBackedLogStore::open`, `lumen.query(tenant, TimeRange::all())`, the `records=N` writeln, and the `(records.first(), records.last())` match emitting the `earliest=` / `latest=` lines. The only addition is the Cinder block after the Lumen block. |
| `lumen_base(data_dir)` private helper | `crates/kaleidoscope-cli/src/lib.rs:118-120` | **REUSE** | Same call site as `stats()` for the Lumen open. |
| `cinder_base(data_dir)` private helper | `crates/kaleidoscope-cli/src/lib.rs:122-124` | **REUSE** | Already used by `ingest()` at line 179. `stats_with_tiers()` joins as the second call site. |
| `LumenToPulseRecorder` quiescent pattern | `crates/kaleidoscope-cli/src/lib.rs:314-316` (current `stats()`) | **REUSE** | Identical construction. No OTLP file is created on the Lumen side. |
| `cinder::NoopRecorder` (alias `CinderRecorder`) | `crates/kaleidoscope-cli/src/lib.rs:58, 173-174` (ingest's no-flag arm) | **REUSE** | Quiescent recorder for the Cinder side. Same construction pattern as `ingest()`'s no-flag arm. |
| `FileBackedLogStore::open` | `crates/kaleidoscope-cli/src/lib.rs:317-318` (current `stats()`) | **REUSE** | Identical to `stats()`. |
| `FileBackedTieringStore::open` | `crates/kaleidoscope-cli/src/lib.rs:179-180` (ingest) | **REUSE** | Same construction as `ingest()`. The only new call site outside `ingest()`. |
| `TieringStore::list_by_tier(tenant, tier)` | `crates/cinder/src/store.rs:101-102` | **REUSE** | Three calls per invocation (one per tier). The `.len()` reduction discharges DISCUSS handoff item 2. |
| `Error::LumenOpen`, `Error::LumenQuery`, `Error::CinderOpen`, `Error::Io` variants | `crates/kaleidoscope-cli/src/lib.rs:73-83` | **REUSE** | All four failure modes for `stats_with_tiers()` are already covered. No new variant. |
| `From<std::io::Error> for Error` | `crates/kaleidoscope-cli/src/lib.rs:104-108` | **REUSE** | Lifts `writeln!(writer, ...)?` and `writer.flush()?` failures. |
| `format_iso8601_utc_nanos` private formatter | `crates/kaleidoscope-cli/src/lib.rs:345-355` | **REUSE** | Called from the inherited Lumen-side `(first, last)` branch. Unchanged. |
| `civil_from_days` private helper | `crates/kaleidoscope-cli/src/lib.rs:361-373` | **REUSE** | Indirectly via the formatter. Unchanged. |
| `Tier::Hot`, `Tier::Warm`, `Tier::Cold` enum variants | `crates/cinder/src/tier.rs:28-32` | **REUSE** | Pattern-matched in DD2's loop body for the key string. |
| `parse_positional` helper | `crates/kaleidoscope-cli/src/main.rs:155-161` | **REUSE** | `run_stats` parses identically; only its body changes (call `stats_with_tiers` instead of `stats`). |
| Existing test harness helpers (`tenant`, `record`, `temp_root`, `cleanup`, `ndjson`) | `crates/kaleidoscope-cli/tests/stats_subcommand.rs:77-118` and four siblings | **DUPLICATE INLINE AT V0** | DISCUSS D10 confirms rule-of-three extraction is deferred. This is the sixth test file in the cluster using the same harness shape. |
| `Tier::all()` associated function | n/a — does not exist | **DO NOT CREATE** | DD2 hardcodes the array literal `[Tier::Hot, Tier::Warm, Tier::Cold]` to avoid expanding the `cinder` public API for a single in-crate caller. |
| New `StatsSummary` or `TierCounts` struct | n/a | **DO NOT CREATE** | The predecessor wave DD2 rejected `StatsSummary` on premature-abstraction grounds (`docs/feature/cli-stats-subcommand-v0/design/wave-decisions.md` lines 178-179). The same argument applies here: zero external library callers consume the data shape; stdout is the only consumer. |

**Verdict**: **EXTEND** (`stats()`'s body shape inside the new
sibling `stats_with_tiers()`) + **REUSE** (fourteen existing
constructs: both base-path helpers, both store opens, both recorder
patterns, four error variants + `From<io::Error>`, the formatter,
`civil_from_days`, the three `Tier` variants, `list_by_tier`, and
`parse_positional`). **No new public type, no new trait, no new
module, no new private helper, no new dependency, no new error
variant.** The change surface is one new function in `lib.rs` plus
one line change in `main.rs::run_stats` plus one new test file plus
one new `[[test]]` block in `Cargo.toml`.

---

## DD6: Out-of-scope confirmations

The DESIGN wave confirms (does not re-litigate) the following DISCUSS
decisions:

1. **No new subcommand, no new flag** (DISCUSS D1, D5, D6). Confirmed.
2. **No Cinder placement, migration, or policy evaluation** (DISCUSS
   D2, D3). Confirmed: `stats_with_tiers()` calls only
   `TieringStore::list_by_tier`; no `place`, no `migrate`, no
   `evaluate_at`.
3. **No per-item dump** (DISCUSS D4). Confirmed: the `.len()`
   reduction is the only consumption of the `Vec<ItemId>`.
4. **No JSON / CSV / `--format=...`** (DISCUSS D5). Confirmed.
5. **No `--cinder-only` / `--no-lumen` mode** (DISCUSS D6).
   Confirmed.
6. **Selective emission of non-zero tier lines** (DISCUSS
   D-EmptyRender Option B). Confirmed in DD4.
7. **Byte-equivalent stdout for zero-Cinder-placement tenants**
   (DISCUSS D-Backwards-compat / OK4). Confirmed: when all three
   `list_by_tier(..).len()` are zero, the `if count > 0` guard
   suppresses all three lines and the output matches the
   predecessor exactly.
8. **No modification of `tests/stats_subcommand.rs`** (DISCUSS D10).
   Confirmed: the new test file is
   `tests/stats_cinder_tier_distribution.rs`; the locked file is
   untouched. Repointing `main.rs::run_stats` from `stats` to
   `stats_with_tiers` is a `main.rs` change, not a
   `tests/stats_subcommand.rs` change. The locked test imports
   `kaleidoscope_cli::stats` directly (line 69) and continues to
   exercise it.
9. **No SSOT journey or `jobs.yaml` modification** (DISCUSS D11).
   Confirmed.
10. **No new ADR**. The new function is a fourth public function in
    `lib.rs` parallel to `ingest`, `read`, and `stats`, following
    the same shape. No new public type, no new trait, no new
    module, no new external dependency. ADR-0001 (public API
    surface) is unchanged by an additive function on an existing
    crate's public surface that follows the established free-
    function pattern. See "Why no ADR change" below.

---

## DEVOPS handoff annotation

Recipient: `nw-platform-architect`. Receives:

- **Inputs**:
  - This `design/wave-decisions.md`.
  - The accompanying `design/application-architecture.md` (C4 L1 + L2;
    L3 explicitly skipped, reification conditions documented).
  - The new subsection appended to
    `docs/product/architecture/brief.md > ## Application Architecture
    — cli-stats-cinder-tier-distribution-v0`.
  - The DISCUSS-wave artefacts under
    `docs/feature/cli-stats-cinder-tier-distribution-v0/discuss/`
    (locked, not modified).
  - Outcome KPIs (`discuss/outcome-kpis.md`): OK1 (principal — tier
    count correctness), OK2 (tenant isolation), OK3 (empty-Lumen
    Option B contract), OK4 (backwards-compatibility for zero-Cinder
    tenants).

- **Development paradigm for DELIVER**: Rust idiomatic per `CLAUDE.md`.
  Data + free functions + traits where genuinely needed. The new
  `stats_with_tiers()` is a free function. No new trait. No new
  struct. No new `dyn` boundary beyond the existing
  `Box<dyn LumenRec + Send + Sync>` and `Box<dyn CinderRec + Send +
  Sync>` at the recorder construction sites (inherited from
  `stats()`'s and `ingest()`'s shapes).

- **External integrations**: **none**. No HTTP client, no webhook,
  no third-party API, no vendor SDK, no subprocess, no network I/O.
  Pure local read over the Lumen + Cinder WAL+snapshot pair. No
  contract-test recommendation applies.

- **External dependency footprint**: **no new external crate**. All
  used types (`Tier`, `ItemId`, `FileBackedTieringStore`,
  `TieringStore`, `NoopRecorder` alias `CinderRecorder`) are
  already in `kaleidoscope-cli`'s use list at
  `crates/kaleidoscope-cli/src/lib.rs:56-59`. `Cargo.lock` churn is
  zero beyond what a recompile produces.

- **CI gates** (ADR-0005): the five existing workspace gates apply
  unchanged. The new acceptance test
  `cargo test --package kaleidoscope-cli --test stats_cinder_tier_distribution`
  exits 0 as the OK1/OK2/OK3/OK4 acceptance probe under Gate 1
  (`cargo test --workspace`). The locked
  `cargo test --package kaleidoscope-cli --test stats_subcommand`
  continues to pass green and serves as the byte-level oracle for
  OK4. **No new gate is added.**

  Specifically on **Gate 5 (mutation testing)**: the existing
  `gate-5-mutants-kaleidoscope-cli` job at
  `.github/workflows/ci.yml:949-1028` is path-filtered on
  `crates/kaleidoscope-cli/**` via `--in-diff`. Any commit touching
  `crates/kaleidoscope-cli/src/lib.rs` or
  `crates/kaleidoscope-cli/src/main.rs` (this feature touches both)
  is automatically mutated by the existing job. The new
  `stats_with_tiers()` body's branches (the `for tier in [...]`
  loop, the `if count > 0` guard, the `match tier` to key string)
  fall inside the same mutation surface. **No new Gate 5 job
  needed.**

- **Workspace changes**: no `Cargo.toml` additions at the workspace
  root. `crates/kaleidoscope-cli/Cargo.toml` gains exactly one new
  `[[test]]` block:

  ```toml
  [[test]]
  name = "stats_cinder_tier_distribution"
  path = "tests/stats_cinder_tier_distribution.rs"
  ```

  No new `[dependencies]` line; no new `[dev-dependencies]` line.
  `aegis`, `cinder`, `lumen`, `self-observe`, and `pulse` are
  already declared.

- **Mutation-testing scope** (per `CLAUDE.md` and ADR-0005 Gate 5):
  scoped to `crates/kaleidoscope-cli/src/lib.rs` and
  `crates/kaleidoscope-cli/src/main.rs` (the two modified source
  files). Run after the DELIVER refactor pass. 100% kill rate. The
  changed code surface is small (the new `stats_with_tiers()`
  function ~25 lines plus the one-line `main.rs::run_stats`
  repoint); mutation-testing budget should be modest and well
  under the 30-minute timeout in the existing job.

- **Architectural-rule enforcement tooling** (Principle 11): no new
  tooling is recommended for this feature. The existing five-gate
  workspace contract already enforces every rule this feature
  touches. The "no `place()` / `migrate()` / `evaluate_at()` calls
  in `stats_with_tiers()`" property is structurally enforced by the
  fact that those methods take `&mut`-style state-mutating
  semantics and the function does not call them; a regression would
  surface as an unjustified diff in a future PR plus the OK4
  byte-equivalent test would break on `place()` mischief.

### Why no ADR change

The new `stats_with_tiers` function introduces **no new public type,
no new abstraction, no new module, no new external dependency**. It
is a fourth public free function on `kaleidoscope-cli`'s library
surface, parallel to `ingest`, `read`, and `stats`, following the
same shape (tenant + data_dir + writer → result with usize count).
The Cinder iteration loop (DD2) is an in-function detail with no
architectural surface. ADR-0001 (kaleidoscope-cli public API surface)
absorbs the addition without amendment because the addition follows
the established free-function pattern documented there.

If a future feature deletes the old `stats()` function (e.g. a v1
cleanup pass after the locked test file is renegotiated), THAT
feature would warrant a new ADR documenting the public-surface
contraction. This feature does not.
