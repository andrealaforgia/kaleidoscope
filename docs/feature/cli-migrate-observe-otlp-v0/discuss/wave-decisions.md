# Wave Decisions — `cli-migrate-observe-otlp-v0` / DISCUSS

Decisions taken with Andrea before this wave opened, plus decisions
recorded during DISCUSS.

## Pre-wave decisions (carried in, not re-litigated)

| Decision | Value | Rationale |
|----------|-------|-----------|
| `feature_type` | `backend` (D1) | CLI plumbing inside `kaleidoscope_cli::migrate` plus a 3-line thread-through at the binary boundary. No new UI, no new subcommand. The visible surface is the byte sequence appended to the operator's `--observe-otlp <path>` file and the unchanged stdout line. |
| `walking_skeleton` | `no` (D2) | The CLI already has the `migrate` subcommand (`cli-migrate-subcommand-v0`), the `--observe-otlp` flag parsing (commit `3af7e82`; `parse_observe_otlp` helper at `crates/kaleidoscope-cli/src/main.rs:161-175`), and the `CinderToOtlpJsonWriter::record_migrate` implementation (`cinder-to-otlp-json-bridge-v0`, locked at ADR-0039 §2). This feature is the wire that connects them. Nothing to span end-to-end that does not exist already. |
| `research_depth` | `lightweight` (D3) | Single operator persona (Priya, inherited from the three sibling features), single decision enabled ("who moved this item yesterday?" / "how many manual migrations during the incident window?"). The library precedent (`crates/self-observe/src/cinder_otlp_json.rs:263-285`) and the wiring precedent (`crates/kaleidoscope-cli/src/lib.rs:155-184` for `ingest`) collapse the design space. |
| `jtbd_analysis` | `no` (D4) | The job is obvious and singular: capture state-mutating operator actions in the same OTLP audit stream the operator already runs for ingest activity. Persona and forces mirror the sibling features. DIVERGE artifacts absent; absence recorded as a risk below. |

## Discovery shortcuts taken (recorded as risk)

| Shortcut | Risk | Mitigation |
|----------|------|------------|
| No DIVERGE wave artefacts (`recommendation.md`, `job-analysis.md`) | LOW. Job statement implicit and singular: operator wants one `cinder.migrate.count` line in the same NDJSON file shape the existing `--observe-otlp` flag already produces on the `ingest` path for `cinder.place.count`. | DIVERGE skipped by Andrea's explicit instruction. The wiring has exactly one shape that compiles (`Box<dyn cinder::MetricsRecorder + Send + Sync>` constructed from `self_observe::CinderToOtlpJsonWriter::new(file)` instead of `cinder::NoopRecorder`); design space is collapsed by the three pre-shipped precedents. |
| No formal JTBD workshop | LOW. Persona, push, pull, anxiety, habit mirror-image of the sibling `cli-cinder-otlp-wiring-v0` feature. Push: "manual migrations leave no audit trail outside shell history". Pull: "audit the incident window's tier transitions from the existing collector". Anxiety: "the new flag must not change the existing stdout contract". Habit: "the operator already types `--observe-otlp` for `ingest`". | Persona + emotional-arc inherited from `cli-cinder-otlp-wiring-v0`. |
| No standalone Three Amigos session | LOW. The shape is doubly constrained: by the existing `migrate` subcommand signature (`crates/kaleidoscope-cli/src/lib.rs:424-456`) and by ADR-0039 §1+§2 (the writer's public surface and the per-event wire contract, both locked). | Peer review against `nw-po-review-dimensions` skill before handoff. |

## In-wave decisions

### D-RecorderConstruction: the OTLP writer is constructed at `FileBackedTieringStore::open` time

The `CinderToOtlpJsonWriter` is constructed at the same site where the
existing `cinder::NoopRecorder` is currently passed to
`FileBackedTieringStore::open(...)` inside `kaleidoscope_cli::migrate`
(`crates/kaleidoscope-cli/src/lib.rs:434`). It is NOT constructed at
`migrate()` call time, nor passed through the Cinder API surface from
the caller.

This mirrors the `ingest` path's pattern at
`crates/kaleidoscope-cli/src/lib.rs:155-184`, which opens the OTLP file
once with `OpenOptions::new().create(true).append(true).open(path)`,
wraps it in a `CinderToOtlpJsonWriter`, and passes the resulting box as
the `cinder::MetricsRecorder` to `FileBackedTieringStore::open`.

Rationale:

1. ADR-0039 §1 locks `CinderToOtlpJsonWriter`'s public surface
   (constructor `new(W) -> Self`, the three trait-method dispatches).
   The writer is constructed from a `Write` handle. The `Write` handle
   is owned by the file open; the file open is the natural place to
   construct the writer.
2. The Cinder API `FileBackedTieringStore::open(base, recorder)` takes
   the recorder at open time and stores it. Any other call site for
   constructing the writer would have to either (a) replace the
   recorder mid-flight (no API for that), or (b) construct a recorder
   stub before the file is open and patch it later (no API for that).
   Store-open time is the only seam.
3. The `OpenOptions::new().create(true).append(true).open(path)` shape
   is identical to the `ingest` pattern. The same shape on the
   `migrate` path keeps the codebase's mental model uniform and lets
   the locked tests' file-creation expectations carry over verbatim
   (no file created on no-flag invocations because no `OpenOptions`
   call is reached; file created on `--observe-otlp` invocations
   because the open is the first action in the `Some(path)` arm).

The exact `OpenOptions` shape, the exact lifetime/box wrapping, and
the exact pattern-match arm structure are DESIGN-owned. The DISCUSS
contract is: the writer is constructed at store-open time, not at
`migrate()` call time.

### D1: Backend feature (no new UI or subcommand)

Confirmed by the task brief's pre-decided D1. The visible surface is
the byte sequence appended to the operator's `--observe-otlp <path>`
file (one new line per successful migrate) and the existing stdout
line (unchanged).

### D2: No walking skeleton needed

Confirmed by the task brief's pre-decided D2. All three substrates
already exist:

- `migrate` subcommand (feature `cli-migrate-subcommand-v0`).
- `--observe-otlp` flag parsing (commit `3af7e82`; `parse_observe_otlp`
  helper in `main.rs:161-175`, already invoked by `run_ingest` and
  `run_read`).
- `CinderToOtlpJsonWriter::record_migrate` wire contract
  (`cinder-to-otlp-json-bridge-v0`; library tests at
  `crates/self-observe/tests/cinder_to_otlp_json.rs`).

The thinnest valuable change is to thread the flag through and replace
one `Box::new(CinderRecorder)` with a conditional construction. Slice
01 ships exactly that.

### D3: Lightweight research depth

Confirmed by the task brief's pre-decided D3. Single persona, single
decision enabled, three pre-shipped precedents collapsing the design
space.

### D4: No JTBD workshop

Confirmed by the task brief's pre-decided D4. The job is obvious and
mirrors the sibling features.

### D5: A new acceptance test file mirrors `migrate_subcommand.rs`

New file: `crates/kaleidoscope-cli/tests/migrate_observe_otlp_flag.rs`.
The harness pattern (`tenant`, `temp_root`, `cleanup`, `cinder_base`,
`place_item`, `read_entry`, `bin` helpers) is duplicated inline at v0,
mirroring the rule-of-three deferral that the sibling features
established. After this feature ships the `kaleidoscope-cli` crate has
eleven test files using broadly the same harness shape; the extraction
trigger is a deliberate refactor across the cluster, not this feature.

The new test file does NOT modify
`crates/kaleidoscope-cli/tests/migrate_subcommand.rs` (locked per the
task brief's hard constraint). Its assertions are independent.

### D6: Out of scope — bulk migrate

The subcommand still takes exactly one `<item_id>`. Bulk-migrate (one
invocation mutating many items) is a separate feature with its own
KPI shape (per-item line vs. one aggregate line, batch atomicity,
partial-failure reporting). The present feature does not extend the
single-item shape and emits exactly one `cinder.migrate.count` line per
successful invocation.

### D7: Out of scope — `--dry-run`, JSON output, observe-otlp on the from-tier read

- `--dry-run`: not introduced. The subcommand always performs the
  migrate when arguments parse.
- JSON output of the stdout line: not introduced. The stdout report
  stays in its existing `key=value` shape
  (`migrated tenant=<t> item=<i> from=<f> to=<t>\n`).
- `--observe-otlp` on the from-tier read: not wired. The pre-flight
  `get_entry` (`crates/kaleidoscope-cli/src/lib.rs:437-442`) is a
  read-only pre-flight; only the actual `cinder.migrate(...)` call
  emits a `cinder.migrate.count` line. This is a property of the
  `CinderToOtlpJsonWriter` trait dispatch (only `record_migrate` emits
  `cinder.migrate.count`; `get_entry` does not call any
  `MetricsRecorder` method) and is therefore automatic; the test file
  asserts it explicitly via the unknown-item scenario (OK3).

### D8: SSOT journey and `jobs.yaml` are NOT modified in this wave

Same posture as `cli-cinder-otlp-wiring-v0` D8. The SSOT
operator-incident-response journey is incident-time focused; this
wiring feature serves the orthogonal "operator gets cross-process
audit of manual tier migrations" journey, which is operationally
nice-to-have but does not rise to the level of an SSOT journey
modification. The feature-local artefacts produced in this wave
(`user-stories.md`, `story-map.md`, `outcome-kpis.md`,
`slices/slice-01-migrate-observe-otlp.md`, `wave-decisions.md`,
`dor-validation.md`) are NOT promoted to `docs/product/journeys/` or
`docs/product/jobs.yaml`.

## Scope assessment (Elephant Carpaccio gate)

Right-sized. 1 story, 1 bounded context (`kaleidoscope-cli` crate), 1
modified library file (`src/lib.rs`), 1 file touched at the binary
(`src/main.rs` — thread-through + usage text update), 1 new test file
(`tests/migrate_observe_otlp_flag.rs`), 1 manifest line-level change
(`Cargo.toml`). Estimated effort: well under 1 day. PASSES the
right-sized gate. Strictly smaller than `cli-cinder-otlp-wiring-v0`
(no cross-writer NDJSON-validity invariant to solve, because the
migrate path drives only the Cinder writer; Lumen is never opened on
this path per the contract in `lib.rs:413-414`).

## Handoff

Next wave: DESIGN (`nw-solution-architect`). Inputs delivered:

- `user-stories.md`
- `outcome-kpis.md`
- `story-map.md`
- `slices/slice-01-migrate-observe-otlp.md`
- `dor-validation.md`
- `wave-decisions.md`

DESIGN wave's principal decisions:

1. The exact signature of `kaleidoscope_cli::migrate(...)` after adding
   `otlp_log_path: Option<&Path>` (parameter position, `Option<&Path>`
   vs. `Option<PathBuf>`, the box type for the recorder).
2. The exact pattern-match shape inside `migrate(...)` for the
   conditional recorder construction (mirroring or diverging from the
   `ingest`-side `match otlp_log_path { ... }` block at
   `lib.rs:155-184`).
3. The exact `main.rs` thread-through for `run_migrate_with(...)`
   (parameter forwarding, usage text update wording).

Any choice that passes the new acceptance test's four scenarios AND
leaves the locked `migrate_subcommand.rs` test file green is acceptable.
