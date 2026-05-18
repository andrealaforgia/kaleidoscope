# Wave Decisions — `cli-read-observe-otlp-v0` / DISCUSS

Decisions taken with Andrea before this wave opened, plus decisions
recorded during DISCUSS.

## Pre-wave decisions (carried in, not re-litigated)

| Decision | Value | Rationale |
|----------|-------|-----------|
| `feature_type` | `backend` | CLI plumbing inside `kaleidoscope_cli::read`. No new UI; reuses the existing `--observe-otlp <path>` flag idiom shipped for `ingest` at commit `3af7e82`. |
| `walking_skeleton` | `no` | The CLI already exists, the `read` subcommand already exists, the OTLP-JSON Lumen writer already exists (`crates/self-observe/src/lumen_otlp_json.rs`), and the binary already knows how to parse `--observe-otlp <path>` for `ingest`. This feature extends one subcommand. There is no end-to-end to span that does not exist already. |
| `research_depth` | `lightweight` | Single operator persona (Priya, inherited from the two prior `--observe-otlp` features). Single decision enabled (per-tenant Lumen query latency / throughput / expensive-tenant detection). The wiring shape is collapsed by precedent: the Lumen writer is `LumenToOtlpJsonWriter`, which already implements `record_query` (`crates/self-observe/src/lumen_otlp_json.rs:205-207`); it just needs to be constructed in the `read` path the same way it is constructed in the `ingest` path. |
| `jtbd_analysis` | `no` | The job is obvious and singular: observe Lumen query activity through the same OTLP collector chain that already receives ingest-side metrics. Persona and forces are direct mirror-images of the `ingest --observe-otlp` feature (already validated by ship at commit `3af7e82`). DIVERGE artifacts absent; absence recorded as a risk below. |

## Discovery shortcuts taken (recorded as risk)

| Shortcut | Risk | Mitigation |
|----------|------|------------|
| No DIVERGE wave artefacts (`recommendation.md`, `job-analysis.md`) | LOW. Job statement implicit and singular: operator wants `lumen.query.count` lines in the same NDJSON file that already carries `lumen.ingest.count` lines (when the same path is supplied to both subcommands across a single shell session). | DIVERGE skipped by Andrea's explicit instruction. The wiring has exactly one shape that compiles (`Box<dyn lumen::MetricsRecorder + Send + Sync>` constructed from `self_observe::LumenToOtlpJsonWriter::new(file)` instead of `self_observe::LumenToPulseRecorder::new(pulse)` in the `read` function); design space is collapsed by the pre-shipped precedent in `ingest`. |
| No formal JTBD workshop | LOW. Persona, push, pull, anxiety, habit mirror-image of the `--observe-otlp` ingest wiring feature (already shipped at `3af7e82`) and the cross-bridge `cli-cinder-otlp-wiring-v0` feature. | Persona + emotional-arc inherited from `docs/feature/cli-cinder-otlp-wiring-v0/discuss/`. |
| No standalone Three Amigos session | LOW. Reviewer pass at handoff time replaces the workshop. The shape is doubly constrained: by the existing `Option<&Path>` parameter idiom on `ingest` (which `read` will adopt), and by ADR-0039 §1 (`LumenToOtlpJsonWriter`'s public surface) which is locked. | Peer review against `nw-po-review-dimensions` skill before handoff. |

## In-wave decisions

### D1: Scope is the `read` subcommand only

The change is confined to `kaleidoscope_cli::read`
(`crates/kaleidoscope-cli/src/lib.rs:252-269`). The `ingest` subcommand
is NOT touched. This is the symmetric counterpart of
`cli-cinder-otlp-wiring-v0` D5 ("Out of scope — `--observe-otlp` on the
`read` subcommand"); that feature deferred `read` to this one, and
this feature accepts the deferral as its sole reason for existing.

Concretely: `read()`'s signature gains an `otlp_log_path: Option<&Path>`
parameter (mirroring `ingest()`'s already-shipped fifth parameter), and
the recorder construction inside `read()` becomes conditional on that
value the same way `ingest()`'s already is. The `main.rs` `run_read`
dispatcher gains the same `parse_observe_otlp` call that `run_ingest`
already makes.

### D2: Out of scope — Cinder events from `read`

The `read()` function does not construct a Cinder store at all
(`crates/kaleidoscope-cli/src/lib.rs:252-269`); Cinder is only touched
during ingest, when the ingest loop calls `cinder.place(...)` once per
batch flush. There is no `cinder.query` or `cinder.read` event in the
Cinder API. Therefore there is no Cinder writer to wire on the read
path, and no `cinder.*` lines will appear in the file as a consequence
of this feature. This makes the present feature strictly thinner than
its immediate predecessor `cli-cinder-otlp-wiring-v0`, which had to
juggle two writers against one file; here only the Lumen writer
participates.

### D3: Out of scope — changes to either writer's public API

ADR-0039 §1 locks `CinderToOtlpJsonWriter`'s public surface. The
analogous (and unwritten) lock for `LumenToOtlpJsonWriter` is in
practice the same — public surface is `new(W) -> Self` plus the two
`MetricsRecorder` trait-method dispatches `record_ingest` and
`record_query` (`crates/self-observe/src/lumen_otlp_json.rs:200-208`).
The DESIGN wave for THIS feature MUST NOT propose any change to
`crates/self-observe/src/`. The §2 correction box (the single
`write_all(line_with_trailing_newline)` pattern) is now standing for
both writers and is not re-litigated.

### D4: Out of scope — multi-process scenarios

Same posture as `cli-cinder-otlp-wiring-v0` D7. The in-scope
concurrency is the single thread inside one `kaleidoscope-cli read`
invocation — `read()` does exactly one `lumen.query(...)` call, which
emits exactly one `record_query(...)` event, which produces exactly
one OTLP-JSON line. There is no within-process concurrency to defend
against here (unlike the ingest+cinder-wiring feature, which had two
in-process writers on one file). Multi-process scenarios (two CLI
processes writing to the same `--observe-otlp` path simultaneously)
remain out of scope per ADR-0039 §7.

### D5: OK3 ingest-symmetry KPI is a same-session sequential append

The intended cross-subcommand demo is one shell session in which the
operator runs `kaleidoscope-cli ingest acme /tmp/data --observe-otlp
/tmp/foo.ndjson < records.json` and then, in the same session, runs
`kaleidoscope-cli read acme /tmp/data --observe-otlp /tmp/foo.ndjson
> /dev/null`. After both commands exit, the single file at
`/tmp/foo.ndjson` contains both `lumen.ingest.count` lines (from the
first invocation) and `lumen.query.count` lines (from the second
invocation), plus the `cinder.place.count` lines that
`cli-cinder-otlp-wiring-v0` already wired for the ingest side. The two
processes do NOT overlap in time — the operator runs them
sequentially. This is the natural shell-session shape the OTLP file is
designed for: an append-only NDJSON log that survives between
invocations (the `O_APPEND` open mode is already inherited from the
existing `ingest` wiring, and the existing test
`observe_otlp_file_is_appended_to_across_multiple_ingest_calls` in
`crates/kaleidoscope-cli/tests/observe_otlp_flag.rs:150-191` proves
this property for the ingest side).

The OK3 acceptance test in
`crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` exercises
exactly this shape: one `ingest()` call followed by one `read()` call,
both against the same `otlp_log_path`, with assertions that BOTH
metric types appear in the resulting file.

### D6: A new acceptance test file mirrors `observe_otlp_flag.rs`

New file: `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs`.
The harness pattern (`tenant`, `record`, `temp_root`, `cleanup`,
`ndjson` helpers) is duplicated inline at v0, mirroring the
rule-of-three deferral from
`cli-cinder-otlp-wiring-v0/discuss/wave-decisions.md` D4. After this
feature ships the `kaleidoscope-cli` crate has three test files using
the same harness (`observe_otlp_flag.rs`, the predecessor's
`observe_otlp_cinder_wiring.rs`, and this feature's
`observe_otlp_read_flag.rs`). The rule-of-three extraction trigger
arrives WITH this feature, but the extraction itself is a separate
refactoring task and is NOT a deliverable of this DISCUSS wave —
extracting a shared module is a DESIGN/DELIVER concern and the
extraction may be done in a follow-up fix-forward commit rather than
inside this feature's slice. The slice ships its three tests with
duplicated helpers.

### D7: SSOT journey and `jobs.yaml` are NOT modified in this wave

Same posture as `cli-cinder-otlp-wiring-v0` D8. The SSOT
operator-incident-response journey is incident-time focused; this
wiring feature serves the orthogonal "operator gets cross-process
observability of query activity" journey, which is operationally
nice-to-have but does not rise to the level of an SSOT journey
modification. The feature-local artefacts produced in this wave
(user-stories.md, story-map.md, outcome-kpis.md, slice brief,
wave-decisions.md, dor-validation.md) are NOT promoted to
`docs/product/journeys/` or `docs/product/jobs.yaml`.

## Scope assessment (Elephant Carpaccio gate)

Right-sized. 1 story, 1 bounded context (`kaleidoscope-cli` crate), 1
modified file (`src/lib.rs`), 1 modified file (`src/main.rs` — for the
new `parse_observe_otlp` call inside `run_read` and an updated
`print_usage` string), 1 new test file
(`tests/observe_otlp_read_flag.rs`), 1 manifest line-level change
(`Cargo.toml`). Estimated effort: well under 1 day. PASSES the
right-sized gate. Strictly smaller than `cli-cinder-otlp-wiring-v0`
because only ONE writer participates here (no cross-writer concurrency
probe is needed; the OK6-equivalent KPI from the predecessor has no
counterpart in this feature).

## Handoff

Next wave: DESIGN (nw-solution-architect). Inputs delivered:

- `user-stories.md`
- `outcome-kpis.md`
- `story-map.md`
- `slices/slice-01-read-emits-otlp-json-on-flag.md`
- `dor-validation.md`
- `wave-decisions.md`

DESIGN wave's principal decision: confirm the parameter shape on
`kaleidoscope_cli::read` (most-likely-correct: append a
`otlp_log_path: Option<&Path>` parameter mirroring `ingest`'s fifth
parameter). Any signature that lets the `main.rs` dispatcher call
`parse_observe_otlp` and forward the result is acceptable as long as
the resulting behaviour satisfies OK1, OK2, OK3.
