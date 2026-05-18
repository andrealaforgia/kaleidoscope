# Wave Decisions — `cli-cinder-otlp-wiring-v0` / DISCUSS

Decisions taken with Andrea before this wave opened, plus decisions
recorded during DISCUSS.

## Pre-wave decisions (carried in, not re-litigated)

| Decision | Value | Rationale |
|----------|-------|-----------|
| `feature_type` | `backend` | CLI plumbing inside `kaleidoscope_cli::ingest`. No new UI, no new flag, no new subcommand. The visible surface is the byte sequence in the operator's `--observe-otlp <path>` file. |
| `walking_skeleton` | `no` | The CLI already has `--observe-otlp` wired for Lumen (commit `3af7e82`); the Cinder writer is already shipped as a library (`cinder-to-otlp-json-bridge-v0`). This feature is the wire that connects them. Nothing to span end-to-end that does not exist already. |
| `research_depth` | `lightweight` | Single operator persona (Priya, inherited from the two reference features), single decision enabled ("did `acme` actually place anything in Hot during the last ingest run?"). The library precedent (`crates/self-observe/src/cinder_otlp_json.rs`) and the wiring precedent (`crates/kaleidoscope-cli/src/lib.rs:147-160`) collapse the design space. |
| `jtbd_analysis` | `no` | The job is obvious and singular: observe Cinder transitions through the OTLP collector. Persona and forces are identical to the prior `--observe-otlp` Lumen wiring feature (already validated by ship). DIVERGE artifacts absent; absence recorded as a risk below. |

## Discovery shortcuts taken (recorded as risk)

| Shortcut | Risk | Mitigation |
|----------|------|------------|
| No DIVERGE wave artefacts (`recommendation.md`, `job-analysis.md`) | LOW. Job statement implicit and singular: operator wants `cinder.place.count` lines in the same NDJSON file that already carries `lumen.ingest.count` lines. | DIVERGE skipped by Andrea's explicit instruction. The wiring has exactly one shape that compiles (`Box<dyn cinder::MetricsRecorder + Send + Sync>` constructed from `self_observe::CinderToOtlpJsonWriter::new(file)` instead of `cinder::NoopRecorder`); design space is collapsed by the two pre-shipped precedents. |
| No formal JTBD workshop | LOW. Persona, push, pull, anxiety, habit mirror-image of the Lumen OTLP-JSON wiring feature (already shipped at `3af7e82`). | Persona + emotional-arc inherited from `cinder-to-otlp-json-bridge-v0/discuss/journey-observe-cinder-via-otlp-json-visual.md` and the prior CLI wiring feature. |
| No standalone Three Amigos session | LOW. Reviewer pass at handoff time replaces the workshop. The shape is doubly constrained: by the existing `otlp_log_path: Option<&Path>` parameter on `ingest`, and by ADR-0039 §1 (`CinderToOtlpJsonWriter`'s public surface) which is locked. | Peer review against `nw-po-review-dimensions` skill before handoff. |

## In-wave decisions

### D1: Scope is the `place` event only

The CLI's ingest loop invokes `cinder.place(...)` exactly once per
batch flush (`crates/kaleidoscope-cli/src/lib.rs:228`). It does NOT
invoke `cinder.migrate` or `cinder.evaluate` — those would require a
separate CLI subcommand that does not currently exist (a hypothetical
`kaleidoscope-cli tier evaluate` or similar). Wiring the writer's
`record_migrate` / `record_evaluate` methods would therefore have no
exercise from the CLI today.

In the absence of a call site, the writer is constructed once with all
three methods present (because `cinder::MetricsRecorder` is one trait
with three methods, ADR-0039 §1), but only `record_place` is exercised
by the CLI surface. The unexercised methods remain available for
future CLI subcommands; this feature does not need a separate decision
about them and does not add tests for them at the CLI level (the
library feature's tests in
`crates/self-observe/tests/cinder_to_otlp_json.rs` cover them).

The Cinder side of `kaleidoscope_cli::ingest` therefore remains
`NoopRecorder` for the `evaluate` and `migrate` methods in effect
(unreachable code), and `CinderToOtlpJsonWriter` for the `place`
method that the ingest loop actually drives.

### D2: Use the existing `--observe-otlp <path>` flag; no new flag

The operator already knows `--observe-otlp <path>`. Adding a separate
`--observe-cinder-otlp <path>` would force the operator to remember
two flags for one logical concept ("show me the OTLP stream"). Worse,
splitting Lumen and Cinder onto two files would break the ADR-0039 §7
cross-writer NDJSON-validity invariant by trivialising it — the cross-
writer guarantee is interesting precisely because the two writers
share one file, which is the entire point of the feature.

The change is therefore confined to the `Some(path) => { … }` arm of
the `otlp_log_path` match inside `kaleidoscope_cli::ingest` (currently
`crates/kaleidoscope-cli/src/lib.rs:147-160`). The Cinder recorder
construction at line 163 becomes conditional on the same
`otlp_log_path` value the Lumen recorder construction at lines 147-160
is already conditional on.

### D3: ADR-0039 §7 is the contract — OK6 is the principal KPI

ADR-0039 §7 ("Post-v0 Cross-Writer NDJSON-Validity Handoff") was
written during the DEVOPS wave of `cinder-to-otlp-json-bridge-v0` and
explicitly mandates that THIS feature must:

1. Define a new outcome KPI `OK6-CLI-cross-writer-ndjson` (item 1 of
   §7).
2. Measure it via acceptance tests that spawn Lumen and Cinder record
   threads simultaneously against a real `File` opened with
   `OpenOptions::new().create(true).append(true)` (item 2 of §7).
3. Include a "concurrent random pause" scenario forcing scheduling
   variations capable of exposing interleaving bugs (item 3 of §7).

All three mandates are honoured: OK6 is the principal KPI in
`outcome-kpis.md` (item 1); the new acceptance test
`crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs` uses a
real temp file via the same `temp_root` helper pattern as
`observe_otlp_flag.rs` and exercises it through two threads (item 2);
the concurrent-random-pause scenario is named explicitly in the slice
brief's AC list (item 3).

### D4: A new acceptance test file mirrors `observe_otlp_flag.rs`

New file: `crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs`.
The harness pattern (`tenant`, `record`, `temp_root`, `cleanup`,
`ndjson` helpers) is duplicated inline at v0, mirroring the rule-of-
three deferral that
`docs/feature/cinder-to-otlp-json-bridge-v0/discuss/wave-decisions.md`
D7 made for the OTLP-JSON serde structs. After this feature ships the
`kaleidoscope-cli` crate has two test files using the same harness
(`observe_otlp_flag.rs` and `observe_otlp_cinder_wiring.rs`); the
extraction trigger is the third such test file, not this one.

A separate decision (DESIGN-owned, not DISCUSS-owned) is which of the
two threads in the concurrent-random-pause scenario invokes the
writers directly versus indirectly through `kaleidoscope_cli::ingest`.
The DISCUSS posture is: the happy-path test invokes `ingest` (operator-
realistic); the concurrent-random-pause test invokes the writers
directly because spawning two `ingest` calls would require two
independent data dirs and would conflate "what the CLI does" with
"what the writers do under concurrency". The cross-writer guarantee is
a property of the two writers sharing one file, not a property of two
`ingest` calls; isolating the test scope to the two writers makes the
failure-mode signal cleaner. DESIGN may revisit this if a different
substrate choice changes the picture.

### D5: Out of scope — `--observe-otlp` on the `read` subcommand

The `read` subcommand
(`crates/kaleidoscope-cli/src/lib.rs:236-253`) queries Lumen and writes
NDJSON to stdout. It does not currently accept `--observe-otlp` and is
not changed by this feature. The decision to extend `read` with an
observability flag is left to a future feature with its own
operator-visible motivation; the present feature's job statement is
about ingest-time tier placement, not read-time queries.

### D6: Out of scope — changing either writer's public API

ADR-0039 §1 locks `CinderToOtlpJsonWriter`'s public surface
(constructor `new(W) -> Self`, the three trait-method dispatches).
This feature consumes that surface unchanged. The DESIGN wave for THIS
feature MUST NOT propose any change to `crates/self-observe/src/`.

Likewise, the Lumen writer's public surface is unchanged from the
prior `--observe-otlp` wiring feature; that feature is in production
at commit `3af7e82`.

### D7: Out of scope — multi-process scenarios

The in-scope concurrency is two threads inside one
`kaleidoscope-cli ingest` invocation. Multi-process scenarios (two CLI
processes writing to the same `--observe-otlp` path simultaneously)
are explicitly out of scope. ADR-0039 §7 documents this distinction:
within-process atomicity is each writer's `Mutex<W>` over its `Write`
handle plus the `write_all + b"\n" + flush` triple inside the critical
section; cross-process atomicity for sub-`PIPE_BUF` writes relies on
POSIX `O_APPEND` (which the existing
`crates/kaleidoscope-cli/src/lib.rs:148-152` already requests). The
two regimes have different probes; this feature ships only the
within-process probe.

If a future production scenario surfaces multi-process interleaving
needs, that is a separate feature with its own KPI and its own
acceptance harness.

### D8: SSOT journey and `jobs.yaml` are NOT modified in this wave

Same posture as `cinder-to-otlp-json-bridge-v0` D10. The SSOT
operator-incident-response journey is incident-time focused; this
wiring feature serves the orthogonal "operator gets cross-process
observability of tier placement" journey, which is operationally
nice-to-have but does not rise to the level of an SSOT journey
modification. The feature-local artefacts produced in this wave
(user-stories.md, story-map.md, outcome-kpis.md, slice brief,
wave-decisions.md, dor-validation.md) are NOT promoted to
`docs/product/journeys/` or `docs/product/jobs.yaml`.

## Scope assessment (Elephant Carpaccio gate)

Right-sized. 1 story, 1 bounded context (`kaleidoscope-cli` crate), 1
modified file (`src/lib.rs`), 1 new test file
(`tests/observe_otlp_cinder_wiring.rs`), 1 manifest line-level change
(`Cargo.toml`). Estimated effort: well under 1 day. PASSES the
right-sized gate. Smaller than both reference features
(`cinder-to-pulse-bridge-v0` and `cinder-to-otlp-json-bridge-v0`).

## Handoff

Next wave: DESIGN (nw-solution-architect). Inputs delivered:

- `user-stories.md`
- `outcome-kpis.md`
- `story-map.md`
- `slices/slice-01-cinder-events-also-land-in-observe-otlp-file.md`
- `dor-validation.md`
- `wave-decisions.md`

DESIGN wave's principal decision (per `outcome-kpis.md` "Handoff to
DESIGN" section): pick the file-sharing mechanism between the two
writers (e.g. `File::try_clone`, two separate
`OpenOptions::new().create(true).append(true).open(path)` calls, a
single `Arc<Mutex<File>>` shared adapter, …) that satisfies the
concurrent-random-pause scenario for OK6. Any choice that passes the
acceptance test is acceptable.
