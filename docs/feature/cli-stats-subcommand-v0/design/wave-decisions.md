# Wave Decisions — `cli-stats-subcommand-v0` / DESIGN

Author: `@nw-solution-architect` (Morgan), DESIGN wave, 2026-05-19.

Mode: PROPOSE. The DISCUSS artefacts and the existing `kaleidoscope-cli`
shape collapse the design space to a near-singleton: the `read()`
function at `crates/kaleidoscope-cli/src/lib.rs:261-294` is the
load-bearing template, and `lumen::LogStore::query(tenant,
TimeRange::all())` already returns a `Vec<LogRecord>` sorted ascending
by `observed_time_unix_nano` (`crates/lumen/src/store.rs:69-70, 136`).
DESIGN's load-bearing job is to lock the timestamp formatter, the
function signature, the iteration strategy, and to confirm no new
external dependency is warranted.

Scope inherited from DISCUSS (locked, not re-litigated): new free
function `kaleidoscope_cli::stats` in
`crates/kaleidoscope-cli/src/lib.rs`; new `Some("stats") =>
run_stats(&args)` arm in `main.rs` plus a new `run_stats` helper plus a
new `print_usage` block; new acceptance test file
`crates/kaleidoscope-cli/tests/stats_subcommand.rs`; one new `[[test]]`
entry in `Cargo.toml`. No Cinder (D2); no `--observe-otlp` flag (D3);
no JSON or CSV (D4); empty-tenant emits exactly `records=0\n` with no
timestamp lines (D5); ISO 8601 UTC with `Z` and target nanosecond
precision (D6); no filtering or multi-tenant aggregate (D7); function
named `kaleidoscope_cli::stats(tenant, data_dir, writer)` (D8); test
harness duplicated inline at v0 (D9).

---

## DD1: ISO 8601 timestamp formatter — hand-rolled, zero new dependencies

**Decision**: Format `observed_time_unix_nano: u64` into the ISO 8601
UTC string `YYYY-MM-DDTHH:MM:SS.NNNNNNNNNZ` with a hand-rolled formatter
in a private function inside `crates/kaleidoscope-cli/src/lib.rs`. Do
NOT add `chrono`, `time`, `jiff`, or any other datetime crate to
`crates/kaleidoscope-cli/Cargo.toml`.

**Rationale**:

1. **No datetime crate exists in the workspace.** A workspace-wide
   `grep` for `^chrono`, `^time =`, `^jiff` across every `Cargo.toml`
   returns zero matches. Adding the first datetime crate to the entire
   Kaleidoscope workspace for a 30-line CLI extension is a substantial
   dependency footprint expansion for a problem the standard library
   solves directly. The DISCUSS D6 preference ("an existing workspace
   dependency is preferred") collapses to "hand-roll" because no
   existing workspace dependency provides the formatting.

2. **The formatting problem is trivially closed-form.** The conversion
   from nanoseconds-since-epoch (a `u64`) to a Gregorian
   `(year, month, day, hour, minute, second, nanos)` tuple is a
   constant-time arithmetic computation: extract `nanos = ns % 10^9`
   and `secs = ns / 10^9`; convert `secs` to `(year, month, day)` via
   the standard "days since 1970-01-01" algorithm (Howard Hinnant's
   civil_from_days, public-domain, ~20 lines of integer arithmetic);
   extract `(hour, minute, second)` from `secs % 86400`. Output via
   `write!` to the supplied `writer` with the format string
   `"{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:09}Z"`. Total source
   addition: ~30 lines including doc-comment.

3. **Mutation-killing surface is tractable.** The hand-rolled
   function's behaviour is exercised by the acceptance test (which
   seeds known nanos and asserts the rendered string is exactly the
   expected ISO 8601 form). Boundary cases (`ns = 0` →
   `1970-01-01T00:00:00.000000000Z`; `ns` exactly on a year boundary;
   leap year February day boundaries) are covered by deterministic
   seed values in the acceptance test. The existing
   `gate-5-mutants-kaleidoscope-cli` mutation job at 100% kill rate
   gives the same coverage guarantee a third-party crate would.

4. **Aligns with the existing CLI's hand-rolled posture.** `main.rs`
   already justifies a hand-rolled argument parser over `clap` ("a
   two-subcommand positional CLI does not earn it",
   `crates/kaleidoscope-cli/src/main.rs:19-21`). The same posture
   applies here: a single output formatter does not earn a 100kLOC
   datetime crate.

5. **Nanosecond precision is preserved.** `chrono`'s default
   `to_rfc3339` emits microsecond precision; reaching nanosecond
   precision requires opting into a specific format string anyway.
   The hand-rolled formatter emits nanoseconds natively because the
   underlying field is nanoseconds; no precision downgrade is
   required (the D6 downgrade clause does not need to be invoked).

**Rejected alternative — `chrono` crate**: ~100kLOC, brings in
`iana-time-zone` and `js-sys` transitively unless features are pared
back, requires manual configuration to emit nanosecond ISO 8601 (the
`%Y-%m-%dT%H:%M:%S%.9fZ` format string). License: MIT/Apache-2.0
(compatible). **Rejected**: too large a dependency for a 30-line
formatter; not already in the workspace; the workspace-wide rule is
"prefer existing dependencies", and there is no existing one to
prefer.

**Rejected alternative — `time` crate**: smaller than `chrono`
(~30kLOC), idiomatic modern Rust, license MIT/Apache-2.0. Has
`OffsetDateTime::format(&Rfc3339)` with nanosecond precision out of
the box. **Rejected**: same workspace-debut argument as `chrono`; the
arithmetic is the same closed-form computation either way; the only
thing the crate buys is "we did not write civil_from_days ourselves",
which is one ~20-line function with a 50-year track record.

**Rejected alternative — `jiff` crate** (Burnt Sushi, 2024): modern,
nanosecond-native, smaller again than `time`. Same rejection: no
workspace precedent; the formatter remains a ~30-line hand-roll
either way.

---

## DD2: Function shape — Option B, write inside the function, return count

**Decision**: The library function signature is

```rust
pub fn stats(
    tenant: &TenantId,
    data_dir: &Path,
    mut writer: impl Write,
) -> Result<usize, Error>
```

The function writes the `records=N\n` line (always) and, when `N > 0`,
the `earliest=<ISO 8601>\n` and `latest=<ISO 8601>\n` lines, directly
into the supplied `writer`. The returned `usize` is the record count
(same shape as `read()`'s return).

**Rationale**:

1. **Mirrors `read()`'s already-shipped shape exactly.** `read()` at
   `crates/kaleidoscope-cli/src/lib.rs:261-294` takes `(tenant,
   data_dir, writer, otlp_log_path)` and returns `Result<usize,
   Error>` while writing NDJSON lines into the writer. The new
   `stats()` is the same shape minus the `otlp_log_path` parameter
   (DISCUSS D3 forbids it) plus a different in-function rendering
   loop. Operator mental model is "stats is like read, but emits
   summary lines instead of records". The crafter mental model is
   "open store, query, write to writer, return count".

2. **The caller's only job is to forward `stdout`.** `main.rs`'s
   `run_stats` helper becomes a tight three-line body: parse
   positional args; call `stats(&tenant, &data_dir,
   io::stdout().lock())`; map any error. No post-processing of a
   returned tuple; no separate rendering step the caller must own.
   This matches `run_read`'s shape (`crates/kaleidoscope-cli/src/main.rs:134-153`).

3. **The returned `usize` enables the inline mutation-killing test
   precedent.** The existing inline test
   `run_read_with_writes_records_to_stdout_and_summary_to_stderr` in
   `main.rs:208-265` exists to discharge `cargo mutants` on the
   binary-only seams. The same posture applies here: a
   `run_stats_with` inner helper can assert on the bytes written to
   stdout and on the returned count, killing both
   `replace stats -> Ok(0)` and `delete writer.write_all(...)`
   mutants.

4. **Empty case is naturally branched in one place.** The function
   body has one match on `records.first()` / `records.last()`:
   `(Some(first), Some(last))` writes the three lines;
   `(None, None)` writes only the count line. The branch lives next
   to the formatting code, not in the caller. The empty-case AC
   (D5) is testable directly through the function's writer.

5. **No `StatsSummary` struct is needed at v0.** Option C
   (`Result<StatsSummary, Error>`) introduces a new public type for
   data the caller does not consume in any way other than printing
   it to stdout. The crate is `publish = false`
   (`crates/kaleidoscope-cli/Cargo.toml:9`); no external consumer
   exists. Premature abstraction.

**Rejected alternative — Option A** (`Result<(usize,
Option<(SystemTime, SystemTime)>), Error>`, caller renders): caller
must own the formatting code, which then has to live somewhere —
either in `main.rs` (clutters the dispatcher), or in another
library-private helper (which the function would have to call anyway,
so the caller-renders separation buys nothing). Doubles the test
surface (one library test + one binary-side rendering test) for no
gain. **Rejected.**

**Rejected alternative — Option C** (`Result<StatsSummary, Error>`,
new public struct): see point 5 above. **Rejected.**

**Self-application note**: if a future feature needs to consume the
stats data programmatically (e.g. a `--json` flag), THAT feature can
refactor by introducing the struct then. Today there is one consumer
(stdout) and one output shape (key=value lines); one path through the
function is enough.

---

## DD3: Iteration strategy — Option C, `records.first()` / `records.last()`

**Decision**: Compute the time-range bounds by indexing the
`Vec<LogRecord>` returned by `lumen.query(tenant, TimeRange::all())`:

```rust
let earliest = records.first().map(|r| r.observed_time_unix_nano);
let latest = records.last().map(|r| r.observed_time_unix_nano);
```

Do NOT iterate the vector with `.min_by_key`, `.max_by_key`, or a
manual fold.

**Rationale (sortedness verified)**:

1. **`LogStore` trait contracts ascending order.** The trait doc at
   `crates/lumen/src/store.rs:67-75` reads:

   > **Observed-time ordering.** `query` returns records in ascending
   > `observed_time_unix_nano` order within a tenant.

   This is a binding semantic guarantee of the `LogStore` port, not
   an implementation accident.

2. **Both adapters honour the contract.**
   - `InMemoryLogStore::ingest` at `crates/lumen/src/store.rs:131-139`
     calls `bucket.sort_by_key(|r| r.observed_time_unix_nano)` after
     every ingest, and `query` returns the bucket in its sorted
     order.
   - `FileBackedLogStore` (the production adapter constructed by
     `read()` and to be constructed by `stats()`) honours the same
     contract per the trait's documented invariant; its
     ingest-then-query roundtrip is already exercised by the existing
     `ingest_and_read_roundtrip` test.

3. **`first()` / `last()` is O(1); a fold is O(N).** For Priya's
   stated tenant size (10 M records), the difference is theoretical
   but the simpler shape is also strictly faster and strictly
   shorter source. The fold gains nothing.

4. **The single-record degenerate case falls out naturally.**
   `records.first() == records.last()` when `records.len() == 1`,
   which the third AC explicitly probes
   (`stats_single_record_tenant_emits_identical_earliest_and_latest`,
   slice line 126-133). No special-case branch needed.

5. **Self-healing under contract drift.** If a future Lumen adapter
   relaxes the sort invariant (a real bug, not an evolution path —
   the trait doc would have to change first), the acceptance test
   for the populated case (which seeds 7 records with deterministic
   nanos and asserts `earliest` < `latest`) will fail on the
   relaxation. The choice between fold and `first/last` does not
   affect test sensitivity; both surface the bug.

**Rejected alternative — Option B** (`.min_by_key` / `.max_by_key`):
defensively re-derives a property the trait already contracts. Two
linear passes over the vector for a property the kernel of the trait
already pins. Pure deficit. **Rejected.**

**Rejected alternative — Sort then take** (Option A): the vector is
already sorted; re-sorting is pure waste. **Rejected** (and
mathematically equivalent to Option C anyway because the sort is a
no-op on already-sorted input).

---

## DD4: Reuse Analysis (RCA F-1 hard gate)

Hard gate per the Reuse-Choose-Author rule.

| Existing component | Path | Decision | Rationale |
|---|---|---|---|
| `kaleidoscope_cli::read` body shape | `crates/kaleidoscope-cli/src/lib.rs:261-294` | **EXTEND THE SHAPE** | The new `stats()` body mirrors `read()`'s structure: construct quiescent `LumenToPulseRecorder` over fresh `InMemoryMetricStore`; open `FileBackedLogStore` via `lumen_base(data_dir)`; call `lumen.query(tenant, TimeRange::all())`; write to the supplied writer; return `usize`. The difference is the rendering loop (three key=value lines vs N NDJSON lines) and the absence of the `otlp_log_path` match (D3 forbids it). |
| `lumen_base(data_dir)` helper | `crates/kaleidoscope-cli/src/lib.rs:118-120` | **REUSE** | Already private; already used by both `ingest()` and `read()` for the Lumen base path. `stats()` joins them as the third call site. |
| `LumenToPulseRecorder` quiescent pattern | `crates/kaleidoscope-cli/src/lib.rs:275-279` (read's no-flag arm) | **REUSE** | Identical construction: `Arc::new(InMemoryMetricStore::new(Box::new(PulseRecorder)))` wrapped in `LumenToPulseRecorder`. No OTLP file is created; no observable side effect other than the bytes written to the supplied writer. Matches the DISCUSS handoff item 1 ("the quiescent recorder pattern"). |
| `FileBackedLogStore::open` | `crates/kaleidoscope-cli/src/lib.rs:281-282` (read) | **REUSE** | Same construction as `read()`. The recorder is the quiescent one above. No new failure mode; the existing `Error::LumenOpen(LogStoreError)` and `Error::LumenQuery(LogStoreError)` variants (lines 73-83) cover the two failure points. |
| `Error::LumenOpen` / `Error::LumenQuery` variants | `crates/kaleidoscope-cli/src/lib.rs:73-83` | **REUSE** | The only two failure modes inside `stats()`: store open failure and query failure. Both already wired. No new variant. The `Error::Io` variant covers writer failures (the `From<std::io::Error> for Error` at lines 104-108 lifts via `?`). |
| `From<std::io::Error> for Error` | `crates/kaleidoscope-cli/src/lib.rs:104-108` | **REUSE** | Lifts `writer.write_all(...)?` and `writer.flush()?` failures into `Error::Io`. Identical to `read()`'s writer-failure posture. |
| `parse_positional` helper | `crates/kaleidoscope-cli/src/main.rs:155-161` | **REUSE** | Already extracts `(TenantId, PathBuf)` from `args[2]` / `args[3]`. `run_stats` calls it identically to `run_ingest` and `run_read`. |
| `parse_observe_otlp` helper | `crates/kaleidoscope-cli/src/main.rs:118-132` | **DO NOT CALL** | D3 forbids the `--observe-otlp` flag on the `stats` subcommand. `run_stats` does not call this helper. |
| `write_usage` text block | `crates/kaleidoscope-cli/src/main.rs:77-97` | **EXTEND** | Add a `stats` subcommand block alongside the `ingest` and `read` blocks: positional args, one-line description of the stdout key=value output shape. Also update the trailing footer line that currently says "Stats are emitted to stderr after `ingest` completes" so it does not contradict the new subcommand (where stats go to stdout by design). |
| Hand-rolled ISO 8601 formatter | n/a — does not exist | **CREATE NEW** (private to `lib.rs`) | The only new private function this feature introduces. DD1 records the rationale (zero workspace datetime crate; closed-form arithmetic; ~30 lines). |
| `kaleidoscope_cli::IngestStats` struct | `crates/kaleidoscope-cli/src/lib.rs:111-116` | **DO NOT REUSE** | Carries a different concept (records ingested + batches flushed + tier items placed). The stats subcommand's return is a single count; DD2 rejects an analogous `StatsSummary` struct. |
| `StatsSummary` (hypothetical new public struct) | n/a — does not exist | **DO NOT CREATE** | DD2 rejects on premature-abstraction grounds. The function returns `Result<usize, Error>` and writes the rendered lines directly. |
| Existing test harness helpers (`tenant`, `record`, `temp_root`, `cleanup`, `ndjson`) | `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` (and three siblings) | **DUPLICATE INLINE AT V0** | DISCUSS D9 explicitly defers the rule-of-three extraction to a separate refactoring task. After this feature ships, the crate has four test files using the same harness pattern; the extraction is overdue, but the explicit decision is to keep this feature's test surface single-purpose. |

**Verdict**: **EXTEND** (the existing `read()` body shape) +
**REUSE** (six existing constructs: `lumen_base`, the quiescent
recorder pattern, `FileBackedLogStore::open`, the two error variants,
`From<io::Error>`, `parse_positional`, the existing `write_usage`
shape) + **CREATE NEW** (one private formatter function inside
`lib.rs`). **No new public type, no new trait, no new module.** The
EXTEND vs CREATE NEW table answers the slice's mandatory reuse
question: only one new internal artefact (the formatter) and zero new
public surface beyond the `stats` function itself.

---

## DD5: Out-of-scope confirmations

The DESIGN wave confirms (does not re-litigate) the following DISCUSS
decisions:

1. **Empty-tenant rendering is `records=0\n` and no timestamp lines**
   (DISCUSS D5). Confirmed: DD2's function body branches on
   `records.first()` / `records.last()` and writes the three lines
   only when both are `Some`. No sentinel string in any branch. The
   AC `stats_empty_tenant_emits_records_zero_and_no_timestamps`
   pins the wire shape; the function body cannot drift away from it
   without failing the test.

2. **No Cinder participation** (DISCUSS D2). Confirmed: `stats()`
   constructs only `FileBackedLogStore`; no `FileBackedTieringStore`
   is opened; no `cinder_base(data_dir)` is called; no
   `cinder.place(...)` is invoked; no `Tier::Hot` / `Tier::Warm` is
   referenced. The Cinder-side state of `data_dir` is invisible to
   this subcommand.

3. **No `--observe-otlp` flag on `stats`** (DISCUSS D3). Confirmed:
   `run_stats` does not call `parse_observe_otlp`; `stats()` does
   not take an `otlp_log_path` parameter; the quiescent
   `LumenToPulseRecorder` is the only recorder constructed; no
   `LumenToOtlpJsonWriter` import is added to `lib.rs`'s use list
   for this feature (the existing import for `read`'s use is
   unchanged). If a future feature wires `--observe-otlp` to
   `stats`, that feature owns the wiring change.

4. **No JSON / CSV / `--format=...`** (DISCUSS D4). Confirmed: the
   only output shape is the three keys (`records`, `earliest`,
   `latest`) on key=value lines terminated by `\n`. No `serde_json`
   path in `stats()`'s body. No format flag in `run_stats` or
   `print_usage`.

5. **No filtering, sorting, or multi-tenant aggregate** (DISCUSS D7).
   Confirmed: `stats()` takes exactly two positional arguments
   (tenant, data_dir) and no optional flags; `TimeRange::all()` is
   the only range constant used; no `--since`, no
   `--severity-min=`, no all-tenants form, no histogram.

6. **No SSOT journey or `jobs.yaml` modification** (DISCUSS D10).
   Confirmed: this DESIGN wave produces feature-local artefacts
   under `docs/feature/cli-stats-subcommand-v0/design/` plus one new
   subsection in `docs/product/architecture/brief.md`. No
   `docs/product/journeys/*.yaml` change; no `docs/product/jobs.yaml`
   change.

7. **No new ADR**. The timestamp formatter choice (DD1) is a
   one-function implementation choice contained inside
   `crates/kaleidoscope-cli/src/lib.rs`; it introduces no new public
   type, no new module, no new architectural boundary. ADR-0039 §1
   (writer public API) is untouched (no writer is constructed); the
   existing CLI shape ADR governing the binary's structure is
   extended only by a third subcommand arm parallel to the two
   already there. See "Why no ADR change" below.

---

## DEVOPS handoff annotation

Recipient: `nw-platform-architect`. Receives:

- **Inputs**:
  - This `design/wave-decisions.md`.
  - The accompanying `design/application-architecture.md` (C4 L1 + L2
    with prose narrative; L3 explicitly skipped, reification
    conditions documented).
  - The new subsection appended to
    `docs/product/architecture/brief.md > ## Application Architecture
    — cli-stats-subcommand-v0`.
  - The DISCUSS-wave artefacts under
    `docs/feature/cli-stats-subcommand-v0/discuss/` (locked, not
    modified).
  - Outcome KPIs (`discuss/outcome-kpis.md`): OK1 (principal — record
    count correctness, consistency with `read`), OK2 (time-range
    correctness — earliest/latest match min/max
    `observed_time_unix_nano`), OK3 (empty-tenant unambiguity).

- **Development paradigm for DELIVER**: Rust idiomatic per `CLAUDE.md`.
  Data + free functions + traits only where polymorphism is genuinely
  needed. The new `stats()` is a free function on the existing
  `LogStore` port; no new trait is introduced; no new struct is
  introduced (DD2 rejects `StatsSummary`); no new `dyn` boundary is
  introduced beyond the existing `Box<dyn LumenRec + Send + Sync>` at
  the recorder construction site (inherited from `read()`'s shape).
  The hand-rolled ISO 8601 formatter is a private free function inside
  `lib.rs`.

- **External integrations**: **none**. No new HTTP client, no
  webhook, no third-party API, no vendor SDK, no subprocess, no
  network I/O of any kind. The subcommand is a pure local read over
  the Lumen WAL+snapshot. No contract-test recommendation applies.

- **External dependency footprint**: **no new external crate**.
  Workspace-wide grep confirms no existing `chrono`, `time`, or
  `jiff` dependency. DD1 rejects adding any datetime crate; the
  formatter is hand-rolled (~30 lines of arithmetic + format
  string). `Cargo.lock` churn for this feature is zero beyond what a
  recompile produces.

- **CI gates** (ADR-0005): the five existing workspace gates apply
  unchanged. The new acceptance test
  `cargo test --package kaleidoscope-cli --test stats_subcommand`
  exits 0 as the OK1/OK2/OK3 acceptance probe under Gate 1
  (`cargo test --workspace`). **No new gate is added.**

  Specifically on **Gate 5 (mutation testing)**: the existing
  `gate-5-mutants-kaleidoscope-cli` job at
  `.github/workflows/ci.yml:949-1028` is path-filtered on
  `crates/kaleidoscope-cli/**` via `--in-diff`. Any commit touching
  `crates/kaleidoscope-cli/src/lib.rs` or
  `crates/kaleidoscope-cli/src/main.rs` (this feature touches both)
  is automatically mutated by the existing job. The hand-rolled
  formatter's branches (year/month/day arithmetic; the
  `Some(first)/Some(last)` vs `None/None` branch) fall inside the
  same mutation surface. **No new Gate 5 job needed.**

- **Workspace changes**: no `Cargo.toml` additions at the workspace
  root. `crates/kaleidoscope-cli/Cargo.toml` gains exactly one
  `[[test]]` block:

  ```toml
  [[test]]
  name = "stats_subcommand"
  path = "tests/stats_subcommand.rs"
  ```

  No new `[dependencies]` line; no new `[dev-dependencies]` line.
  `aegis`, `lumen`, `self-observe`, and `pulse` are already declared.

- **Mutation-testing scope** (per `CLAUDE.md` and ADR-0005 Gate 5):
  scoped to `crates/kaleidoscope-cli/src/lib.rs` and
  `crates/kaleidoscope-cli/src/main.rs` (the two modified source
  files). Run after the DELIVER refactor pass. 100% kill rate. The
  changed code surface is small (the new `stats()` function, the new
  `run_stats` helper, the `print_usage` extension); mutation-testing
  budget should be modest and well under the 30-minute timeout in
  the existing job.

- **Architectural-rule enforcement tooling** (Principle 11): no new
  tooling is recommended for this feature. The existing five-gate
  workspace contract already enforces every rule this feature
  touches. The "no `chrono` / `time` / `jiff` in
  `kaleidoscope-cli`'s dependency closure" property is naturally
  enforced by the absence of those lines in
  `crates/kaleidoscope-cli/Cargo.toml`; a regression would surface
  as an unjustified diff in a future PR. Rust does not have an
  idiomatic ArchUnit equivalent for "no datetime crate added", but
  `cargo deny` (already in the workspace per ADR-0005) can be
  configured with a `deny` list if the project ever wants to harden
  this; left as a follow-up if the workspace grows enough datetime
  use to warrant a global posture.

### Why no ADR change

The new `stats` function introduces **no new public type, no new
abstraction, no new module, no new external dependency**. It is a
third subcommand arm parallel to the two already wired (`ingest`,
`read`), following the same shape (binary dispatcher → positional
arg parse → library function call → write to supplied writer →
return count). The hand-rolled ISO 8601 formatter (DD1) is a
private free function with no architectural surface; it is an
implementation detail of `stats()` confined to `lib.rs`. No ADR
extension is warranted.

If a future feature adds a `--json` flag (DISCUSS D4 rejection
reversal) and consequently introduces a public `StatsSummary` struct
(DD2 rejection reversal), THAT feature would warrant a new ADR
documenting the public-surface expansion. This feature does not.
