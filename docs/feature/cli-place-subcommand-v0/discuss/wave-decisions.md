# Wave Decisions — `cli-place-subcommand-v0` / DISCUSS

Decisions taken with Andrea before this wave opened, plus decisions
recorded during DISCUSS.

## Pre-wave decisions (carried in, not re-litigated)

| Decision | Value | Rationale |
|----------|-------|-----------|
| D1 `feature_type` | `backend` (CLI subcommand) | New positional subcommand on the existing `kaleidoscope-cli` binary: `place <tenant> <data_dir> <item_id> <tier> [--observe-otlp <path>]`. Adds one new dispatch arm to `main.rs`'s match block at lines 54-69, one new free function in `lib.rs`, and one new acceptance test file. No new persona, no new crate, no new external dependency. |
| D2 `walking_skeleton` | `no` | The CLI exists; four subcommands (`ingest`, `read`, `stats`, `migrate`, `list-items`) work; the `TieringStore` trait already exposes `place(tenant, item, tier, placed_at)` (`crates/cinder/src/store.rs:81`). The OTLP-JSON sink for Cinder events is already wired by `cli-cinder-otlp-wiring-v0` via `CinderToOtlpJsonWriter` (already imported at `crates/kaleidoscope-cli/src/lib.rs:65`). This feature is a thin extension on existing substrate. |
| D3 `research_depth` | `lightweight` | Single operator persona (Priya, inherited from the seven predecessor features in the cluster). Single decision-set: trigger one Cinder `place` per CLI invocation, optionally observed via OTLP-JSON. Output shape collapsed by precedent: one literal stdout line `placed tenant=<t> item=<id> tier=<x>\n`, mirroring the `migrated tenant=... item=... from=... to=...` line that `cli-migrate-subcommand-v0` shipped. |
| D4 `jtbd_analysis` | `no` | The job is obvious and singular: manual Cinder item placement (CRUD's missing "C"). Three operationally distinct decisions (bootstrap items outside the ingest flow / set up controlled test scenarios / recover from a Cinder snapshot corruption against a manifest) all collapse to the same primitive ("place item X for tenant Y in tier T, now"). Persona inherited from the seven predecessor features; forces are direct mirror-images of the operator's daily `ingest` / `stats` / `migrate` workflow. |

## Discovery shortcuts taken (recorded as risk)

| Shortcut | Risk | Mitigation |
|----------|------|------------|
| No DIVERGE wave artefacts (`recommendation.md`, `job-analysis.md`) | LOW. The job statement is implicit and singular: an operator wants `place <tenant> <data_dir> <item_id> <tier>` to call `TieringStore::place(...)` and report the placement on stdout. The optional `--observe-otlp` flag mirrors the established pattern from `cli-cinder-otlp-wiring-v0` / `cli-migrate-observe-otlp-v0`. | DIVERGE skipped by Andrea's explicit instruction. The output shape (one literal stdout line on success; one OTLP-JSON line per place call when `--observe-otlp` is set) has exactly one reasonable shape that operators can `grep` / `tail -f`. |
| No formal JTBD workshop | LOW. Persona, push (operator can't bootstrap items today without writing Rust), pull (one-shot CLI placement with verifiable stdout + post-call `get_entry().tier == tier`), anxiety (overwrite-semantics could quietly clobber an existing entry — documented openly in the user story; no surprise), habit (operator already runs `kaleidoscope-cli ingest` / `migrate` / `list-items` daily). | Persona + emotional-arc inherited from the seven reference features under `docs/feature/cli-*/discuss/`. |
| No standalone Three Amigos session | LOW. Reviewer pass at handoff time replaces the workshop. The shape is doubly constrained: by the existing `TieringStore::place(tenant, item, tier, placed_at)` API surface and by the established CLI conventions (positional arguments, lower-case tier rendering, plain-text stdout, optional `--observe-otlp` wiring). | Peer review against `nw-po-review-dimensions` skill before handoff. |

## In-wave decisions

### D-LowerCase: Tier arguments accepted in lower-case only

The `<tier>` positional argument is accepted ONLY in lower-case as
one of `hot` / `warm` / `cold`. Upper-case (`HOT`, `WARM`, `COLD`),
mixed-case (`Hot`, `Warm`, `Cold`), or any other spelling (typo
`lukewarm`, empty string, leading/trailing whitespace) is REJECTED
with a non-zero exit and a single stderr line containing the
verbatim invalid value.

Rationale:

- Mirrors `cli-migrate-subcommand-v0` D-LowerCase exactly. The
  parse-side contract should be byte-identical across `migrate` and
  `place` so the operator's muscle memory is one rule, not two.
- The existing `parse_tier(s: &str) -> Result<Tier, ()>` private
  helper at `crates/kaleidoscope-cli/src/lib.rs:505-512` is the
  natural reuse point. DESIGN locks whether to promote it to
  `pub(crate)` or duplicate the four-line match inline.
- The `Error::InvalidTier { value }` variant at
  `crates/kaleidoscope-cli/src/lib.rs:79-81` is the natural reuse
  point for the parse error; no new variant required.

### D-Timestamp: `SystemTime::now()` at call site, no `--placed-at` flag

The `placed_at: SystemTime` argument passed to
`TieringStore::place` is `SystemTime::now()` evaluated at the call
site inside the new library `place` function. There is NO
`--placed-at <timestamp>` / `--at <timestamp>` flag in v0
(explicit out-of-scope item in the task brief).

Rationale:

- Same posture as `cli-migrate-subcommand-v0` D-Timestamp. The
  operator's natural use case is "I want to record THIS placement
  as happening NOW".
- Deterministic-time testing remains available at the trait level
  via `cinder::InMemoryTieringStore` which accepts arbitrary
  `SystemTime` per `crates/cinder/src/store.rs:140`. The CLI
  surface does NOT need a `--at` flag to support trait-level unit
  tests.
- The acceptance test asserts WIRE-OBSERVABLE invariants only: the
  stdout report content (which does not include the `placed_at`
  value), and the post-call `get_entry(tenant, item).tier == tier`.
  The exact `entry.placed_at` value recorded in the Cinder store is
  NOT part of the wire-observable contract — DESIGN MUST NOT
  introduce a `--placed-at` flag for testing.

### D-Overwrite: Place is overwrite-semantics — DOCUMENT, do not guard

`TieringStore::place(tenant, item, tier, placed_at)` is defined as
overwrite-semantics per the trait contract at
`crates/cinder/src/store.rs:78-81`:

> Record `(tenant, item)` as living in `tier` at `placed_at`.
> Overwrites any prior placement for the same key.

The `InMemoryTieringStore::place` body at
`crates/cinder/src/store.rs:140-152` confirms this by
unconditionally `state.entries.insert(key, TierEntry { ... })`.

The CLI subcommand MUST NOT verify that `item_id` doesn't already
exist (explicit out-of-scope item in the task brief). The CLI MUST
NOT introduce a special case (e.g. an "AlreadyPlaced" branch that
rejects the call). The CLI faithfully reports the placement and
exits 0 regardless of whether the call was a first-time placement
or an overwrite of an existing entry.

The user story's Domain Example 2 documents this behaviour openly
so the operator understands the contract before invocation.
Mirrors `cli-migrate-subcommand-v0` D-Idempotent in spirit:
faithful to underlying API, no CLI-side guard.

### D-NoLumenTouch: `place` opens ONLY the Cinder store

The new library function opens ONLY
`FileBackedTieringStore::open(cinder_base(data_dir), recorder)`.
It does NOT open
`FileBackedLogStore::open(lumen_base(data_dir), recorder)`.

The Lumen WAL+snapshot under `<data_dir>/lumen.*` is BYTE-
EQUIVALENT before and after every `place` subcommand invocation
(success, invalid-tier failure, observe-otlp success, observe-otlp
failure).

Rationale:

- The job is purely a Cinder-side placement. The Lumen side has no
  role (no records are written, no records are read). Explicit
  out-of-scope item in the task brief ("Lumen mutation").
- Opening the Lumen store would do nothing observable but would
  add cost (WAL replay, snapshot deserialisation) AND require the
  Lumen directory to exist. An operator bootstrapping a new
  Cinder catalog from a manifest should NOT be forced to have a
  Lumen directory.
- Verifiable by the acceptance test: assert the
  presence/absence/byte-content of `lumen_base(data_dir)` is
  unchanged across the place call.
- Mirrors `cli-migrate-subcommand-v0` D-NoLumenTouch exactly.

### D-ObserveOtlp: `--observe-otlp <path>` is the ONLY optional flag

The `place` subcommand accepts EXACTLY ONE optional flag:
`--observe-otlp <path>`. When set, the Cinder recorder is wired to
`CinderToOtlpJsonWriter::new(file)` exactly as
`ingest()`'s `Some(path)` arm does at
`crates/kaleidoscope-cli/src/lib.rs:160-184` and `migrate()`'s
arm does at `crates/kaleidoscope-cli/src/lib.rs:435-444`. The
file is opened ONCE with `OpenOptions::create(true).append(true)`
per ADR-0039 §8.

Per place-call invocation with `--observe-otlp` set, the Cinder
recorder emits EXACTLY ONE `cinder.place.count` OTLP-JSON line to
the path (the underlying `InMemoryTieringStore::place` calls
`self.recorder.record_place(tenant, tier)` exactly once at
`crates/cinder/src/store.rs:151`; the file-backed adapter mirrors
this contract).

When `--observe-otlp` is NOT supplied, the Cinder recorder is the
quiescent `CinderRecorder` (the `None` arm pattern from
`migrate()` at `crates/kaleidoscope-cli/src/lib.rs:443`). No file
is created. The `cinder_base(data_dir)/...` Cinder snapshot is
the only on-disk artefact affected.

Rationale:

- Task brief mandates the flag: "Optional --observe-otlp wiring
  mirrors migrate/ingest patterns".
- The wire format is byte-identical to the lines `ingest` and
  `migrate` already emit on the Cinder side: tenant id resource
  attribute, single `cinder.place.count` metric with `tier` point
  attribute. Same path can be `tail -f`ed alongside an `ingest` or
  `migrate` session.

### D-FunctionShape: Library function shape — NAMED but NOT designed

DESIGN owns the exact signature. The task brief and the
predecessor wave precedent (`cli-migrate-subcommand-v0`
D-FunctionShape) suggest:

- A new free function in `crates/kaleidoscope-cli/src/lib.rs`,
  likely
  `pub fn place(tenant: &TenantId, data_dir: &Path, item_id: &str, tier_arg: &str, writer: impl Write, otlp_log_path: Option<&Path>) -> Result<(), Error>`.
  The function does the parse + open + place + writeln, returning
  `Ok(())` on success and an `Error` variant on parse failure.

The exact internal layout (whether to lift the recorder
construction into a shared helper with `migrate()`, whether to
return a typed `PlaceReport` struct vs unit) is DESIGN's call.
Either way the stdout bytes are the same.

### D-ErrorVariant: No new Error variant required

`Error::InvalidTier { value: String }` at
`crates/kaleidoscope-cli/src/lib.rs:79-81` is already the right
variant for the parse-side fail-fast and is already used by both
`migrate` (`crates/kaleidoscope-cli/src/lib.rs:432-434`) and
`list_items` (`crates/kaleidoscope-cli/src/lib.rs:486-488`).
`Error::CinderOpen(MigrateError)` at
`crates/kaleidoscope-cli/src/lib.rs:77` covers any
`FileBackedTieringStore::open` failure.

No new `Error` variant is needed for `place` because the
underlying `TieringStore::place` returns `()` (not
`Result<(), _>`) per the trait at
`crates/cinder/src/store.rs:81` — it has NO failure modes at the
trait level beyond the (already-handled) store-open failure. The
`Error::Io(std::io::Error)` variant at
`crates/kaleidoscope-cli/src/lib.rs:82` covers any `--observe-otlp`
file-open failure exactly as it does for `migrate`.

### D-StderrWording: Exact stderr wording is DESIGN's call

The OK3 (invalid-tier) contract requires:

- Non-zero exit code.
- Empty stdout.
- A single stderr line that CONTAINS the verbatim offending tier
  value.

The exact byte-level wording (prefix, suffix, punctuation, the
`kaleidoscope-cli: ` binary-name prefix already emitted by
`main.rs:73-77`) is DESIGN's call. The natural inherited wording
via the existing `Error::InvalidTier` Display impl at
`crates/kaleidoscope-cli/src/lib.rs:98-100` is:

```text
kaleidoscope-cli: invalid tier "HOT": expected one of hot, warm, cold
```

— already covers the substring contract verbatim. No new wording
required.

### D-NewTestFile: New acceptance test file — DO NOT modify the locked test files

New file:
`crates/kaleidoscope-cli/tests/place_subcommand.rs`.
Mirrors the harness pattern in
`crates/kaleidoscope-cli/tests/migrate_subcommand.rs` (the
predecessor's shipped file) and
`crates/kaleidoscope-cli/tests/migrate_observe_otlp_flag.rs` (the
shipped --observe-otlp sibling). The `tenant`, `record`,
`temp_root`, `cleanup`, `ndjson` helpers are duplicated inline at
v0 — the rule-of-three extraction was deferred in the predecessor
waves and remains a separate refactoring task. This is the
EIGHTH `tests/*.rs` file in the cluster using the same harness
shape.

### D-LockedTests: Do NOT modify any locked test file

The existing locked acceptance test files MUST NOT be modified:

- `crates/kaleidoscope-cli/tests/stats_subcommand.rs`
- `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`
- `crates/kaleidoscope-cli/tests/stats_time_range.rs`
- `crates/kaleidoscope-cli/tests/read_time_range.rs`
- `crates/kaleidoscope-cli/tests/migrate_subcommand.rs`
- `crates/kaleidoscope-cli/tests/migrate_observe_otlp_flag.rs`
- `crates/kaleidoscope-cli/tests/list_items_subcommand.rs`
- `crates/kaleidoscope-cli/tests/ingest_and_read_roundtrip.rs`
- `crates/kaleidoscope-cli/tests/cli_binary_smoke.rs`
- `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs`
- `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs`
- `crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs`

Each of these locks the byte-level contract for an earlier shipped
feature. The new `place` subcommand is a pure ADDITION to the
binary's dispatch (new arm, new helper, new library function); it
does NOT modify the behaviour of any existing subcommand.
Therefore every locked test file MUST continue to pass green
UNMODIFIED under `cargo test --package kaleidoscope-cli` after this
feature ships.

This is the supplementary oracle for the wave's "no regression"
invariant. Hard constraint restated from the task brief.

### D-OutOfScope-PlacedAt: No `--placed-at` flag

Explicit task-brief out-of-scope item. The `placed_at: SystemTime`
argument is hard-wired to `SystemTime::now()` at the call site.
See D-Timestamp above for the rationale.

### D-OutOfScope-Bulk: No bulk placement

Explicit task-brief out-of-scope item. The CLI takes exactly one
`<item_id>` per invocation. There is NO multi-item shape (`place
<tenant> <data_dir> <item_ids_file> <tier>`, comma-separated id
list, etc.).

Rationale: v0 ships the single-item primitive. Bulk placement is
a reasonable v1 once the single-item contract is validated; the
operator's natural workaround today is a shell loop:

```bash
for item in acme/batch-00001 acme/batch-00002 acme/batch-00003; do
  kaleidoscope-cli place acme /tmp/data "$item" hot || break
done
```

### D-OutOfScope-ExistsCheck: No "already placed" verification

Explicit task-brief out-of-scope item. Per D-Overwrite above, the
underlying API is overwrite-semantics. The CLI MUST NOT verify
that `item_id` doesn't already exist before issuing the `place`
call. The operator who wants to check first uses
`kaleidoscope-cli list-items <tenant> <data_dir> <tier>` for each
tier (the predecessor wave's shipped surface).

### D-OutOfScope-LumenMutation: No Lumen mutation

Explicit task-brief out-of-scope item. Restatement of
D-NoLumenTouch above for symmetry with the task-brief language.

### D-NoSSOT: SSOT journey and `jobs.yaml` are NOT modified in this wave

Same posture as the seven predecessor features in the cluster.
The SSOT operator-incident-response journey is incident-time
focused; this manual item-placement extension serves the orthogonal
"operator manually bootstraps a Cinder item" workflow, which is
operationally useful but does not rise to the level of an SSOT
journey modification. The feature-local artefacts produced in this
wave are NOT promoted to `docs/product/journeys/` or
`docs/product/jobs.yaml`.

## Scope assessment (Elephant Carpaccio gate)

Right-sized. 1 story, 1 bounded context (`kaleidoscope-cli`
crate), 2 modified files in `src/` (`lib.rs` for the new
function; `main.rs` for the new dispatch arm + new `run_place`
helper + `print_usage` update), 1 new test file
(`tests/place_subcommand.rs`), 1 manifest line-level change
(`Cargo.toml` for the new `[[test]]` entry). Estimated effort:
well under 1 day. PASSES the right-sized gate. Comparable in size
to the predecessor (`cli-migrate-subcommand-v0`); the structural
surface area is the same (one new dispatch arm + one new library
function + one new acceptance test + one new `[[test]]` manifest
entry). The substantive difference is that `place` is one-step
(one `place` call) whereas `migrate` is two-step (`get_entry` +
`migrate`).

## Handoff

Next wave: DESIGN (`nw-solution-architect`). Inputs delivered:

- `user-stories.md`
- `outcome-kpis.md`
- `story-map.md`
- `slices/slice-01-place-subcommand-bootstraps-item.md`
- `dor-validation.md`
- `wave-decisions.md`

DESIGN wave's principal decisions to lock:

- **D-FunctionShape**: the exact signature of the new
  `place(...)` library function in `lib.rs` (return `Result<(),
  Error>` with the writer as a parameter; whether to factor a
  shared recorder-construction helper with `migrate()`'s arm).
- **D-StderrWording**: confirm the inherited `Error::InvalidTier`
  Display wording is acceptable, or override with a `place`-
  specific phrasing.
- **D-RecorderFactor**: whether to extract the `match otlp_log_path
  { Some(p) => OtlpJsonWriter, None => Quiescent }` pattern (used
  by `ingest`, `migrate`, `read`, and now `place`) into a single
  helper. Not mandated by DISCUSS; pure code-hygiene call.
