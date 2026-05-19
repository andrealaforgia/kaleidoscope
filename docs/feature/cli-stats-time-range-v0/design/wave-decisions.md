# Wave Decisions — `cli-stats-time-range-v0` / DESIGN

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-19.

Mode: PROPOSE. The DISCUSS artefacts collapse nearly the entire
design space: `discuss/wave-decisions.md` already locks the half-open
`[since, until)` semantics (D-Interval, inherited from
`lumen::TimeRange`), the `--since` / `--until` shape and the
`u64::MAX` / `0` half-bounded defaults (D-Optional), the Cinder
state-snapshot invariance (D-CinderScope), the empty-window contract
(D-EmptyWindow), the no-new-error posture (D-NoNewError), and the
locked-test mechanical-update precedent (D-Locked-tests). The
predecessor feature `cli-read-time-range-v0` shipped the
`parse_iso8601_utc_nanos` library parser, the binary-side
`parse_time_range(args)` and `parse_flag_iso(args, flag)` helpers,
and the public `IsoParseError` type — all reused here unchanged.

DESIGN's load-bearing job is to lock the exact `stats_with_tiers`
signature evolution, to confirm the D-CinderScope implementation
shape, to confirm the D-EmptyWindow rendering, to pin which locked
test files actually require the mechanical signature-match update,
and to discharge the Reuse Analysis hard gate. **No new ADR**: the
signature extension is the same decision class as DD1 of the
predecessor feature (additive parameter on an existing public
function, no new public type or trait).

Confirmed environment state (Earned Trust probe — Principle 12):
`crates/kaleidoscope-cli/src/lib.rs:528-647` ships
`parse_iso8601_utc_nanos` already; `crates/kaleidoscope-cli/src/main.rs:188-214`
ships `parse_time_range` and `parse_flag_iso`. A workspace grep for
`use chrono|use jiff|use time::` returns zero matches; the
no-time-crate posture inherited from `cli-stats-subcommand-v0` is
still in force.

---

## DD1: `stats_with_tiers()` signature evolution — Option A, explicit `range: TimeRange` as 4th parameter

**Decision**: extend `kaleidoscope_cli::stats_with_tiers` from its
current 3-arg shape

```rust
pub fn stats_with_tiers(
    tenant: &TenantId,
    data_dir: &Path,
    mut writer: impl Write,
) -> Result<usize, Error>
```

to a 4-arg shape

```rust
pub fn stats_with_tiers(
    tenant: &TenantId,
    data_dir: &Path,
    mut writer: impl Write,
    range: TimeRange,
) -> Result<usize, Error>
```

with `range` appended at the end of the argument list. The call-site
change inside `stats_with_tiers()` is a single token swap:
`lumen.query(tenant, TimeRange::all())` at
`crates/kaleidoscope-cli/src/lib.rs:359-361` becomes
`lumen.query(tenant, range)`. Nothing else in the function body
changes.

At the binary, `run_stats_with` at
`crates/kaleidoscope-cli/src/main.rs:226-235` gains a single new
line `let range = parse_time_range(args)?;` and threads `range` into
the `stats_with_tiers(...)` call. `parse_time_range` is the existing
helper from the predecessor — REUSED unchanged.

**Rationale**:

1. **Option A over Option B (`Option<TimeRange>` with `None` → all)**.
   `Option<TimeRange>` introduces a second null-state semantic on top
   of `TimeRange::all()` itself. The library port already has a
   "give me everything" sentinel (`TimeRange::all()`); adding `None`
   as a second one violates "make illegal states unrepresentable".
   Callers should construct the `TimeRange` they want;
   `TimeRange::all()` is the no-flag idiom and is one line at the
   call site. **Rejected.**
2. **Option A over Option C (parallel `stats_with_tiers_range()`
   sibling)**. A parallel sibling was the right shape in
   `cli-stats-cinder-tier-distribution-v0` DD1 because the locked
   `stats_subcommand.rs` referenced `kaleidoscope_cli::stats` by
   name — a rename would have broken that test. Here the equivalent
   structural force does not apply: only one locked file
   (`stats_cinder_tier_distribution.rs`) references
   `stats_with_tiers` by name, and the precedent (DD1 of
   `cli-read-time-range-v0`) chose the additive 5th-parameter shape
   on `read()` for the same reason. Mirroring the `read()` precedent
   here keeps the public surface coherent across the two
   sister-features. **Rejected.**
3. **Option A is the precedent shape**. DD1 of `cli-read-time-range-v0`
   chose exactly the same shape (append `range: TimeRange` to the
   parameter list of an existing public library function). Reusing
   that shape on `stats_with_tiers` makes the two cluster features
   present a single coherent extension pattern.

**Rejected alternative — builder pattern (`StatsOptions::new().with_range(..)`)**:
one optional knob does not earn a builder. A positional parameter
is clearer at the call site (`stats_with_tiers(&tenant, &dir, sink, range)`)
than a builder chain. **Rejected.**

**Rejected alternative — extend the legacy `stats()` (3-arg) too**:
explicitly out-of-scope per `discuss/wave-decisions.md` D-OutOfScope
item 5. `stats()` is the byte-level OK4 oracle for the original
`cli-stats-subcommand-v0` feature; it remains untouched.

---

## DD2: D-CinderScope implementation — Option (a), range parameter threaded but Cinder branch ignores it

**Decision**: `stats_with_tiers` accepts the 4th `range: TimeRange`
parameter and uses it ONLY at the Lumen call site
(`crates/kaleidoscope-cli/src/lib.rs:359-361`). The Cinder loop at
`crates/kaleidoscope-cli/src/lib.rs:375-380` is structurally
identical to today; it does NOT reference `range`. The parameter is
effectively dropped on the Cinder side of the function body.

The two options the DISCUSS handoff named (option (a): single
function, range parameter unused by Cinder branch; option (b):
extract the Cinder-emitting code into a separate free function
called after the Lumen-emitting code) are FUNCTIONALLY equivalent
on the observable surface — both honour D-CinderScope. Option (a)
is the simpler implementation: one function, one signature, one
call site to update at the binary. Option (b) would split a 30-line
function into two for no observable-behaviour reason and create a
gratuitous internal API boundary.

**Option (a) is also semantically honest at the source level**: the
function name `stats_with_tiers` already signals that the Cinder
side is part of the same emission unit as the Lumen side; the
`range` parameter applying to Lumen but not Cinder is precisely
what D-CinderScope says, and the natural source-level encoding of
that decision is a parameter that is consulted on one branch and
not on the other. The Rust doc-comment on `stats_with_tiers`
already says: *"the supplied `range` filters the Lumen query only;
the Cinder per-tier counts are state-snapshot at call time"*.

**Rationale**:

1. **Smaller diff, easier review**. Option (a) is a 2-line code
   diff in `stats_with_tiers` plus a 1-line diff in `run_stats_with`.
   Option (b) would refactor the body into two free functions plus
   an orchestrator.
2. **No change to the function's public boundary semantics**.
   Callers see one function returning the same `usize` (the Lumen
   record count). Option (b)'s split would tempt future callers to
   call only one half, which is not the current contract.
3. **Mutation surface is concentrated**. Gate 5 covers
   `crates/kaleidoscope-cli/src/lib.rs` already; the new branch
   (one token swap) sits in a single function and is covered by the
   new acceptance tests' OK1, OK2, OK3, OK4 assertions plus the
   pre-existing mutation tests for the unchanged Cinder loop.
4. **The DISCUSS handoff explicitly named option (a) as the simpler
   implementation** and asked DESIGN to choose; this DD confirms
   that selection.

---

## DD3: D-EmptyWindow rendering — confirmed, mirrors predecessor's empty-tenant contract

**Decision**: when `lumen.query(tenant, range)` returns zero records,
`stats_with_tiers` writes exactly one Lumen line `records=0\n` and
NO `earliest=` / `latest=` lines, then the unchanged Cinder
state-snapshot lines (selective Option B emission for non-zero tiers
only).

This is achieved by the existing `if let (Some(first), Some(last)) =
(records.first(), records.last())` arm at
`crates/kaleidoscope-cli/src/lib.rs:364-369`. The arm only fires
when `records` is non-empty; under the bounded-query extension, the
same arm fires on the windowed result instead of the
`TimeRange::all()` result. The empty-tenant contract from
`cli-stats-subcommand-v0` carries over to the empty-window case
automatically because the code path is the same. **No new code
needed for D-EmptyWindow**.

**Rejected alternative — emit `earliest=null` / `latest=null` or
`earliest=<since>` / `latest=<until>` for the empty-window case**:
would diverge from the predecessor's empty-tenant contract on which
the locked OK4 oracle tests depend. Byte-equivalence under the
no-flag default requires the empty-tenant contract to be preserved
unchanged; the simplest way to ensure that is to carry the same
contract through to the empty-window case. The signal-to-noise
ratio is also strictly worse for the operator (two distracting
lines vs none). **Rejected** in `discuss/wave-decisions.md`
D-EmptyWindow; DD3 confirms.

---

## DD4: Locked test mechanical update — scoped to `stats_cinder_tier_distribution.rs` ONLY

**Decision**: under the 4-arg signature extension chosen in DD1,
the locked test file
`crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`
gets a mechanical signature-match update at each of its five
`stats_with_tiers(&acme, &data, &mut stdout)` call sites
(verified by grep — lines 271, 376, 447, 538, 642), each gaining
`TimeRange::all()` as the new 4th argument:

```rust
let count = stats_with_tiers(&acme, &data, &mut stdout, TimeRange::all()).expect("stats_with_tiers");
```

The file's `use` line at `tests/stats_cinder_tier_distribution.rs:95`
gains `TimeRange` from `lumen::`:

```rust
use lumen::{LogRecord, SeverityNumber, TimeRange};  // (TimeRange added)
use kaleidoscope_cli::{ingest, stats_with_tiers, DEFAULT_BATCH_SIZE};
```

(The exact import shape is DELIVER's choice — `lumen::TimeRange` is
already in scope for other test files; the import goes wherever the
crafter's `cargo fmt` lands it. Verified that `TimeRange::all()` is
the existing public sentinel at `crates/lumen/src/record.rs:111-114`.)

**NO assertion text is edited**. This is the SAME precedent as
`observe_otlp_read_flag.rs`'s mechanical update in
`cli-read-time-range-v0` and `tests/ingest_and_read_roundtrip.rs`'s
mechanical update in `cli-read-observe-otlp-v0`: only call-site
arguments change, never assertion text. OK4 holds.

**`stats_subcommand.rs` requires NO update.** Verified by grep:
the file calls only `stats(&acme, &data, &mut stdout)` (the legacy
3-arg function at `crates/kaleidoscope-cli/src/lib.rs:312-331`)
across all its call sites (lines 201, 260, 317, 404, 471). The
legacy `stats()` is explicitly out-of-scope for this feature per
DISCUSS D-OutOfScope item 5, and its signature is not extended.
The DISCUSS handoff anticipated this answer ("the likely answer is
NO mechanical update needed for `stats_subcommand.rs`"); DD4
confirms.

**`observe_otlp_*` tests** require no update (they do not call
`stats_with_tiers` — they call `ingest` and `read` via the library
or via subprocess). **`read_time_range.rs`** requires no update
(same reason; it calls `read`). **`ingest_and_read_roundtrip.rs`**
already passes `TimeRange::all()` to `read` (5-arg) per the
predecessor's mechanical update; nothing further.

Summary of locked-test edits this feature triggers:

| File | Edit |
|---|---|
| `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs` | Mechanical 4th-arg `TimeRange::all()` at five `stats_with_tiers(...)` call sites. Import line gains `TimeRange`. No assertion text edited. |
| `crates/kaleidoscope-cli/tests/stats_subcommand.rs` | None. File exercises only the legacy `stats()` (3-arg) which is not modified. |
| `crates/kaleidoscope-cli/tests/observe_otlp_*.rs` | None. None of these reference `stats_with_tiers`. |
| `crates/kaleidoscope-cli/tests/read_time_range.rs` | None. Calls `read`, not `stats_with_tiers`. |
| `crates/kaleidoscope-cli/tests/ingest_and_read_roundtrip.rs` | None. Calls `read`, not `stats_with_tiers`. |

---

## DD5: Reuse Analysis (RCA F-1 hard gate)

| Existing component | Path | Decision | Rationale |
|---|---|---|---|
| `parse_iso8601_utc_nanos` library parser | `crates/kaleidoscope-cli/src/lib.rs:528-647` | **REUSE unchanged.** | Shipped by `cli-read-time-range-v0`. The `stats` side inherits it via the shared `parse_time_range` helper. |
| `parse_time_range(args)` binary helper | `crates/kaleidoscope-cli/src/main.rs:188-195` | **REUSE unchanged.** | Subcommand-neutral — scans from `args.iter().skip(2)`, works identically for `read` and `stats`. |
| `parse_flag_iso(args, flag)` binary helper | `crates/kaleidoscope-cli/src/main.rs:197-214` | **REUSE unchanged.** | Same subcommand-neutral scan; produces stderr message naming the offending flag. |
| `IsoParseError` typed error | `crates/kaleidoscope-cli/src/lib.rs:464-510` | **REUSE unchanged.** | No new variant (D-NoNewError). |
| `lumen::TimeRange` value type | `crates/lumen/src/record.rs:97-120` | **REUSE.** | Already in `kaleidoscope_cli`'s use list. The new 4th parameter on `stats_with_tiers` uses this exact type. **DO NOT modify** the lumen crate. |
| `TimeRange::all()` sentinel | `crates/lumen/src/record.rs:111-114` | **REUSE.** | No-flag CLI default constructs this; locked OK4 tests pass this explicitly under the new 4-arg signature. |
| `format_iso8601_utc_nanos` formatter | `crates/kaleidoscope-cli/src/lib.rs:409-419` | **REUSE unchanged.** | Renders `earliest=` / `latest=` lines from the windowed query result; nothing about the formatter changes. |
| `FileBackedLogStore::open` + `query` | `crates/lumen/src/*` | **REUSE unchanged.** | The bounded query is the existing `lumen.query(tenant, TimeRange)` call — only the constructed `TimeRange` changes. |
| `FileBackedTieringStore::open` + `list_by_tier` | `crates/cinder/src/*` | **REUSE unchanged.** | D-CinderScope: the Cinder loop at lines 375-380 is not touched. State-snapshot semantics preserved. |
| `stats_with_tiers` body (lines 349-383) | `crates/kaleidoscope-cli/src/lib.rs` | **EXTEND.** | One new parameter; one token swap at line 360. No restructuring. |
| `run_stats_with` body | `crates/kaleidoscope-cli/src/main.rs:226-235` | **EXTEND.** | One new line: `let range = parse_time_range(args)?;`; one new positional argument at the `stats_with_tiers(..)` call site. |
| `write_usage` text | `crates/kaleidoscope-cli/src/main.rs:81-119` | **EXTEND.** | The `stats` subcommand block gains `[--since <ISO 8601 UTC>] [--until <ISO 8601 UTC>]`, half-open `[since, until)` note, D-CinderScope note (range applies to Lumen lines only), D-EmptyWindow note (empty window emits `records=0\n` with no `earliest=` / `latest=`). |
| Legacy `stats()` 3-arg function | `crates/kaleidoscope-cli/src/lib.rs:312-331` | **DO NOT MODIFY.** | Byte-level OK4 oracle for `cli-stats-subcommand-v0`. Out-of-scope per DISCUSS D-OutOfScope item 5. |
| Locked test file `stats_cinder_tier_distribution.rs` | `crates/kaleidoscope-cli/tests/` | **MECHANICAL 4th-arg UPDATE ONLY** (DD4). | Five call-site edits, no assertion edits. Same precedent as `observe_otlp_read_flag.rs` in the predecessor feature. |
| Locked test files `stats_subcommand.rs`, `observe_otlp_*.rs`, `read_time_range.rs`, `ingest_and_read_roundtrip.rs` | `crates/kaleidoscope-cli/tests/` | **DO NOT MODIFY** (DD4). | None reference `stats_with_tiers` by signature. |
| Test harness helpers (`tenant`, `record`, `temp_root`, `cleanup`, `ndjson`, `cinder_base`, `lumen_base`, `seed_cinder`, `cinder_count`) | `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs` | **DUPLICATE INLINE** at v0 per DISCUSS D-Test-file. | Seventh test file in the cluster; rule-of-three extraction is a separate refactoring task and NOT a deliverable of this feature. |
| `chrono`, `time`, `jiff` external crates | n/a — not in workspace | **DO NOT INTRODUCE.** | Verified by grep at design time. No-time-crate posture preserved. |

**Verdict**: **EXTEND** (`stats_with_tiers`'s signature;
`run_stats_with`'s body; `write_usage`'s text) + **REUSE** (everything
else — twelve existing constructs). **CREATE NEW**: zero new
library functions, zero new helpers, zero new types, zero new
private items, zero new crates. The ONLY new entity in production
source is the additional parameter on `stats_with_tiers`'s public
signature.

**No new public type, no new trait, no new module, no new external
dependency.** This is the thinnest extension shape in the cluster
to date — strictly thinner than `cli-read-time-range-v0`, which
introduced the parser and the `IsoParseError` type. This feature
introduces nothing; it only consumes what the predecessor shipped.

---

## DD6: Out-of-scope confirmations

The DESIGN wave confirms (does not re-litigate) the following
DISCUSS decisions:

1. **Half-open `[since, until)` semantics** (D-Interval). Confirmed:
   the CLI surface passes values straight into `TimeRange::new(s, e)`
   and inherits the contract from `crates/lumen/src/record.rs:116-119`.
2. **Both flags optional with half-bounded support** (D-Optional).
   Confirmed: `parse_time_range` defaults absent `--since` to `0`
   and absent `--until` to `u64::MAX`. No-flag default is
   `TimeRange::all()` byte-equivalent.
3. **Cinder lines are state-snapshot** (D-CinderScope). Confirmed
   by DD2 (option (a) shape). Cinder loop at lines 375-380 untouched.
4. **Empty-window mirrors empty-tenant contract** (D-EmptyWindow).
   Confirmed by DD3. The existing `if let (Some, Some)` arm at lines
   364-369 handles the empty-window case automatically.
5. **No new error code** (D-NoNewError). Confirmed: `IsoParseError`
   reused unchanged; `parse_time_range` produces the same stderr
   message format as the `read` feature.
6. **Locked tests get mechanical signature-match update only**
   (D-Locked-tests). Confirmed by DD4, scoped to
   `stats_cinder_tier_distribution.rs` only.
7. **Test harness rule-of-three extraction deferred** (D-Test-file).
   Confirmed: the new `tests/stats_time_range.rs` duplicates harness
   helpers inline; extraction is a separate refactoring task.
8. **No SSOT journey or `jobs.yaml` modification** (D-OutOfScope
   item 7). Confirmed.
9. **No new ADR.** The signature extension is the same decision
   class as DD1 of `cli-read-time-range-v0` (additive 5th-parameter
   on `read()`), itself the same class as the 4th-parameter
   addition on `read()` shipped in `cli-read-observe-otlp-v0`.
   ADR-0001 (`kaleidoscope-cli` public API surface) absorbs this
   as another additive parameter change that does NOT introduce a
   new public type, trait, or module.

---

## DEVOPS handoff annotation

Recipient: `nw-platform-architect`. Receives:

- **Inputs**:
  - This `design/wave-decisions.md`.
  - The accompanying `design/application-architecture.md` (C4 L1 + L2;
    L3 explicitly skipped — reification conditions noted feature-side).
  - The new subsection appended to
    `docs/product/architecture/brief.md > ## Application Architecture
    — cli-stats-time-range-v0`.
  - The DISCUSS-wave artefacts under
    `docs/feature/cli-stats-time-range-v0/discuss/` (locked).
  - Outcome KPIs (`discuss/outcome-kpis.md`): OK1 (principal —
    bounded-window record count), OK2 (windowed earliest/latest),
    OK3 (Cinder lines unchanged — pins D-CinderScope), OK4 (no-flag
    byte equivalence).

- **Development paradigm for DELIVER**: Rust idiomatic per
  `CLAUDE.md`. Data + free functions + traits where genuinely
  needed. **No new free function. No new trait. No new `dyn`
  boundary. No new module. No new typed error variant.** The
  feature is a 4th-positional-argument addition on a public
  library function, a one-line addition in a private binary
  function, a five-call-site mechanical update in a locked test
  file, and a six-test-function new acceptance test file.

- **External integrations**: **none**. No HTTP client, no webhook,
  no third-party API, no vendor SDK, no subprocess, no network I/O.
  The change is local to the `kaleidoscope-cli` crate; Lumen and
  Cinder remain the same in-process Rust crates. **No contract-test
  recommendation applies.**

- **External dependency footprint**: **no new external crate**.
  `lumen::TimeRange`, `aegis::TenantId`, std I/O traits are already
  in `kaleidoscope-cli`'s use list. `Cargo.lock` churn is zero.

- **CI gates** (ADR-0005): the five existing workspace gates apply
  unchanged. The new acceptance test
  `cargo test --package kaleidoscope-cli --test stats_time_range`
  exits 0 as the OK1/OK2/OK3/OK4 acceptance probe under Gate 1
  (`cargo test --workspace`). The locked
  `cargo test --package kaleidoscope-cli --test stats_subcommand`
  and `cargo test --package kaleidoscope-cli --test stats_cinder_tier_distribution`
  continue to pass green and serve as the byte-level oracles for
  OK4. **No new gate is added.**

  Specifically on **Gate 5 (mutation testing)**: the existing
  `gate-5-mutants-kaleidoscope-cli` job is path-filtered on
  `crates/kaleidoscope-cli/**` via `--in-diff`. Any commit
  touching `crates/kaleidoscope-cli/src/lib.rs` or
  `crates/kaleidoscope-cli/src/main.rs` (this feature touches
  both, minimally) is automatically mutated. The new surface is
  small (one parameter; one token swap at line 360; one new line
  in `run_stats_with`); the kill rate is discharged by the new
  acceptance test's OK1, OK2, OK3 assertions plus the existing
  inline test `run_stats_with_writes_summary_to_stdout_and_records_line_to_stderr`
  (which the crafter will update to pass `TimeRange::all()` as
  the new 4th arg in addition to the locked test update). **No
  new Gate 5 job needed.**

- **Workspace changes**: no `Cargo.toml` additions at the workspace
  root. `crates/kaleidoscope-cli/Cargo.toml` gains exactly one
  new `[[test]]` block:

  ```toml
  [[test]]
  name = "stats_time_range"
  path = "tests/stats_time_range.rs"
  ```

  No new `[dependencies]` line; no new `[dev-dependencies]` line.

- **Mutation-testing scope** (per `CLAUDE.md` and ADR-0005 Gate 5):
  scoped to `crates/kaleidoscope-cli/src/lib.rs` and
  `crates/kaleidoscope-cli/src/main.rs`. Run after the DELIVER
  refactor pass. 100% kill rate. The changed code surface is
  TINY (~3 production source lines: one parameter declaration, one
  token swap, one new helper-call line at the binary; plus
  `write_usage` text additions). Mutation-testing budget is
  trivial and well under the 30-minute timeout.

- **Architectural-rule enforcement tooling** (Principle 11): no
  new tooling. The "no `chrono`/`time`/`jiff` dependency" property
  is enforced structurally; the "Cinder loop untouched" property
  is enforced by OK3 byte-identity assertion in the new acceptance
  test (the test invokes `stats_with_tiers` TWICE with two
  different `TimeRange` values and asserts the Cinder lines are
  byte-identical between the two stdouts).

- **Earned Trust posture** (Principle 12): no new substrate, no
  new vendor SDK, no new filesystem path. The parser and the
  `TimeRange` semantics are pre-shipped, pre-tested, pre-mutated by
  the predecessor feature. Earned Trust for this feature is
  discharged at the acceptance-test level: OK3's byte-identity
  guardrail probes the D-CinderScope contract empirically (the
  test exercises BOTH branches — Lumen filtered, Cinder unfiltered
  — in the same invocation and asserts both outcomes on the same
  captured stdout). OK1 / OK2 probe the half-open `[since, until)`
  boundary with witness records at exactly `since_ns` and exactly
  `until_ns`.

### Why no ADR change

The signature extension is the same architectural decision class as
`cli-read-time-range-v0` DD1 (which extended `read()` from 4 args to
5 by appending `range: TimeRange`). That decision shipped without a
new ADR — it was framed as an additive parameter change on an
existing public function. The same framing applies here: extending
`stats_with_tiers` from 3 args to 4 by appending `range: TimeRange`
introduces no new public type, no new trait, no new module, and no
new external dependency. ADR-0001 (`kaleidoscope-cli` public API
surface) already governs the public surface and absorbs this change
under the same additive-parameter rubric.

If a future feature ever extends the parser to accept timezone
offsets other than `Z` (the forward-compat hook documented in
predecessor DD3), OR introduces a `--cinder-at <ISO>` flag for
time-bound Cinder snapshots (the future candidate documented in
DISCUSS D-CinderScope), THAT feature would warrant a new ADR. This
feature does not.
