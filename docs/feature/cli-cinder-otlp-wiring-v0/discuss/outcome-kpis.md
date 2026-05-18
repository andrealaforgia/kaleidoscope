# Outcome KPIs — `cli-cinder-otlp-wiring-v0`

## Feature

`cli-cinder-otlp-wiring-v0` — extend the `kaleidoscope-cli ingest`
subcommand so that the existing `--observe-otlp <path>` flag also
routes Cinder's tier-management events into the same NDJSON sink that
already carries the Lumen events. Today the flag wires
`LumenToOtlpJsonWriter` against the file
(`crates/kaleidoscope-cli/src/lib.rs:153`) but Cinder is constructed
with `cinder::NoopRecorder` (`crates/kaleidoscope-cli/src/lib.rs:163`),
so every `cinder.place(...)` call inside the ingest loop produces
exactly zero bytes in the operator's stream.

## Objective

A single `kaleidoscope-cli ingest acme /tmp/data --observe-otlp
/tmp/foo.ndjson < records.json` invocation leaves a file at
`/tmp/foo.ndjson` whose contents interleave `lumen.batches.ingested.count`
lines from the existing Lumen writer with `cinder.place.count` lines
from the newly-wired Cinder writer, in the order the underlying events
fire from the ingest loop. The operator's sidecar tails the file and
forwards lines to the existing OTLP/HTTP collector without any sidecar
or collector change; the dashboard gains a `kaleidoscope.cinder` row
the next time it refreshes.

## Note on KPI granularity

This feature is operator-visible at the CLI surface. The principal
guarantee is the **cross-writer NDJSON-validity invariant** that
ADR-0039 §7 mandated when the Cinder OTLP-JSON writer landed: a single
file receiving lines from two distinct writers' `Mutex<W>` critical
sections, each over its own clone of the same underlying file handle,
must produce a byte stream where every line is independently parseable
JSON and the stream ends with `\n`, even under concurrent emission.
OK6 is therefore the first-class KPI; OK7 and OK8 are the
per-writer presence and the non-regression guarantees that frame it.

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| OK6-CLI-cross-writer-ndjson | Priya the platform operator, observed at the byte level on her configured `--observe-otlp <path>` sink | Sees a file in which 100% of captured NDJSON lines are independently parseable as JSON AND the stream ends with `\n`, even when the in-process Lumen and Cinder writers emit concurrently to the same `File` opened via `OpenOptions::new().create(true).append(true)` | 100% of lines parseable; stream ends with `\n`; zero observed interleaving of bytes belonging to two distinct logical lines under a concurrent-random-pause scenario | n/a (today only Lumen writes, so the cross-writer invariant has no exercise yet) | New acceptance test `crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs` — a "concurrent random pause" scenario spawns Lumen-driving and Cinder-driving threads against a single real `File` and asserts post-run line-by-line JSON validity and trailing newline | Leading (operator-visible guardrail; principal KPI inherited from ADR-0039 §7) |
| OK7-CLI-cinder-events-present | Priya the platform operator, observed at the byte level on her configured `--observe-otlp <path>` sink | Sees exactly one new line with metric name `cinder.place.count`, scope `kaleidoscope.cinder`, and the per-tenant resource attribute, per `cinder.place(...)` call that the ingest loop executes (one per batch flush, per `crates/kaleidoscope-cli/src/lib.rs:228`) | 100% of `cinder.place` calls produce exactly one line; 0% of ingest invocations leave the file without a `cinder.place.count` line when the input produces at least one batch | 0% (CLI's Cinder recorder is `cinder::NoopRecorder` today — emits nothing) | New acceptance test `crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs` — happy path with 6 records / batch_size 3 produces a file containing 2 `cinder.place.count` lines plus the existing 2 `lumen.batches.ingested.count` lines | Leading (operator-visible behaviour) |
| OK8-CLI-no-regression | Priya the platform operator, observed at the byte level on her existing `--observe-otlp <path>` sink behaviour | Sees Lumen-side output byte-equivalent to the pre-feature behaviour: the same number of `lumen.batches.ingested.count` lines, with the same metric name, scope, resource attribute, and `asInt` per line | 100% pass of every existing assertion in `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` (the 3 tests there continue to pass with no edit to their assertions) | n/a (baseline = current shipped behaviour at commit `3af7e82`) | Existing test file `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` continues to pass green under `cargo test --package kaleidoscope-cli --test observe_otlp_flag` after this feature ships, with no edits to the assertions in that file | Guardrail (non-regression on the prior `--observe-otlp` wiring feature) |

## Metric Hierarchy

- **North Star**: **OK6-CLI-cross-writer-ndjson** — the cross-writer
  guarantee inherited from ADR-0039 §7. It is the single invariant that
  earned this feature its existence: without it, naively pointing both
  writers at the same file path is unsafe and the operator's stream
  cannot be trusted by a sidecar.
- **Leading Indicators**: OK7 (Cinder events are present in the stream
  at all) — without OK7, OK6 has no Cinder lines to validate against;
  with OK7 alone but OK6 failing, the stream is corrupted and the
  Cinder lines are useless.
- **Guardrail Metrics**: OK8 (Lumen-side behaviour unchanged) — the
  existing acceptance test file is the binary-equivalence probe.

## Cross-bridge alignment

OK7 is the CLI surface for what was OK1 in
`docs/feature/cinder-to-otlp-json-bridge-v0/discuss/outcome-kpis.md` at
the library contract level. The library feature proved that
`CinderToOtlpJsonWriter::record_place` produces one line per call
against an in-memory `SharedBuf`; this feature proves that the same
guarantee survives the move to a real `File` opened with `O_APPEND` and
shared (via two distinct `Mutex<File>` guards over two `File::try_clone`
handles, or whatever the DESIGN wave picks) with the Lumen writer.

OK6 is the CLI surface for what was OK5 in the library feature, lifted
from the within-writer NDJSON-validity guarantee against `Vec<u8>` to
the **cross-writer** guarantee against a real shared `File`. ADR-0039
§7 made this lift explicit and assigned it to this feature.

| KPI | Library precedent | This feature |
|-----|-------------------|--------------|
| OK7 | OK1 in `cinder-to-otlp-json-bridge-v0` (one parseable line per `place` against `SharedBuf`) | one parseable line per CLI-driven `place` against the operator's real file |
| OK6 | OK5 in `cinder-to-otlp-json-bridge-v0` (NDJSON validity within one writer) | NDJSON validity across both writers concurrently against one shared file |
| OK8 | OK1 from the prior `--observe-otlp` CLI feature (Lumen side) | unchanged: the Lumen side continues to pass byte-equivalently |

## Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| OK6-CLI-cross-writer-ndjson | `crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs` — concurrent-random-pause scenario | `cargo test --package kaleidoscope-cli --test observe_otlp_cinder_wiring` exit code; the test reads back the file after the threads join and asserts (a) every non-empty line parses as `serde_json::Value`, (b) the file content ends with `\n`, (c) no line is empty, (d) the per-writer-expected line counts match | At every commit touching the CLI ingest path or either of the two writers | `kaleidoscope-cli` maintainer (CI feedback per ADR-0005) |
| OK7-CLI-cinder-events-present | Same test file — happy-path scenario | Same `cargo test` invocation. The test feeds 6 records at batch_size 3, asserts exactly 2 lines with metric name `cinder.place.count`, and asserts that the resource-attribute `tenant_id` value equals the tenant passed to `ingest` | Same | Same |
| OK8-CLI-no-regression | `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` (unchanged) | `cargo test --package kaleidoscope-cli --test observe_otlp_flag` exit code | Same | Same |

## Hypothesis

We believe that **constructing `CinderToOtlpJsonWriter::new` against a
clone of the same `File` handle the existing Lumen writer is already
constructed against, and substituting it for `cinder::NoopRecorder` in
the `--observe-otlp <path>` arm of `kaleidoscope-cli::ingest`** for the
**platform operator (Priya)** will achieve **a single NDJSON sink that
interleaves Lumen and Cinder lines while remaining valid line-by-line
JSON, observable on her existing sidecar + collector + dashboard chain
with zero configuration change**.

We will know this is true when:

- The new acceptance test's concurrent-random-pause scenario passes
  green, asserting cross-writer NDJSON validity under scheduling jitter
  (OK6).
- The new acceptance test's happy-path scenario passes green, asserting
  that a 6-record / batch_size-3 ingest produces exactly 2
  `cinder.place.count` lines alongside the 2
  `lumen.batches.ingested.count` lines the Lumen feature already
  produces (OK7).
- The existing `observe_otlp_flag.rs` test file passes byte-equivalently
  with no edits to its assertions (OK8).

## Handoff to DESIGN

The DESIGN wave (`nw-solution-architect`) should preserve:

1. **The cross-writer NDJSON-validity invariant as the principal
   constraint**: any chosen substrate for sharing the `File` between
   the two writers (`File::try_clone`, `Arc<File>`, two separate
   `OpenOptions` opens of the same path with `append(true)`, …) must
   produce a byte stream that passes the concurrent-random-pause
   scenario in the new acceptance test.
2. **The cross-bridge metric-name contract from ADR-0039 §2**: the
   Cinder side of the wiring must produce lines whose `metric.name`
   is exactly `cinder.place.count`. No string transformation, no
   scope-prefix injection.
3. **The Lumen side untouched**: the chosen wiring must not alter the
   shape, count, or contents of the lines the Lumen writer already
   produces. The existing `observe_otlp_flag.rs` test is the
   byte-equivalence probe.
4. **Single-process scope**: per `wave-decisions.md` D7, multi-process
   scenarios (two CLI processes writing to the same path) are out of
   scope. The in-scope concurrency is two in-process threads inside one
   `kaleidoscope-cli ingest` invocation.

The DESIGN wave should NOT introduce a new CLI flag, a new subcommand,
or a new `--observe-cinder-otlp` split-stream variant. The existing
flag must do the right thing; that is the entire point of this
feature.

## DEVOPS instrumentation needs

No new collection infrastructure. The writer appends to the same NDJSON
file the existing sidecar already tails; the operator's existing
dashboards extend by adding `kaleidoscope.cinder`-scoped panels (which
the library feature's KPI table already anticipated as `OK1-CLI` /
`OK2-CLI` / `OK3-CLI` in
`docs/feature/cinder-to-otlp-json-bridge-v0/discuss/outcome-kpis.md`,
post-v0 row). The CI gate is the new acceptance test's exit code, per
ADR-0005 Gate 1 (the workspace already runs `cargo test` on every
commit).
