# Story Map: `cli-cinder-otlp-wiring-v0`

## User: Priya the platform operator

## Goal

When Priya runs
`kaleidoscope-cli ingest acme /tmp/data --observe-otlp /tmp/foo.ndjson < records.json`,
she sees `cinder.place.count` lines interleaved with the
`lumen.batches.ingested.count` lines that the existing `--observe-otlp`
wiring already produces, in the same file, with the stream remaining
valid line-by-line JSON and ending with `\n` even under concurrent
emission. Her existing sidecar tails the file and forwards both
writers' lines to the existing OTLP/HTTP collector without
configuration change; the dashboard gains a `kaleidoscope.cinder` row
next refresh.

## Backbone

The journey has exactly one activity: the operator sees Cinder
transitions in the OTLP stream she is already configured to consume.
The activity is a thin end-to-end slice: a single
`kaleidoscope-cli ingest ... --observe-otlp ...` invocation produces
the interleaved stream, which a sidecar reads, which a collector
ingests, which a dashboard displays. The thinness is the point: the
prior `--observe-otlp` feature already shipped the file mechanism and
the sidecar contract; the prior `cinder-to-otlp-json-bridge-v0` feature
already shipped the Cinder writer; this feature is the wire that
connects them at one site inside `kaleidoscope_cli::ingest`.

| Activity 1: operator sees Cinder transitions in OTLP stream |
|---|
| CLI's ingest path constructs `CinderToOtlpJsonWriter` against the operator-supplied file path (instead of `cinder::NoopRecorder`) when `--observe-otlp` is set, leaving Lumen's existing wiring untouched. The file contains both writers' lines, interleaved per batch flush; the cross-writer stream remains valid NDJSON under concurrent emission. |

## Walking Skeleton

Per `wave-decisions.md` D2, the walking-skeleton concept is N/A: the
CLI already has `--observe-otlp` wired for Lumen (commit `3af7e82`),
and the Cinder writer is already shipped as a library
(`cinder-to-otlp-json-bridge-v0`). This feature is a thin extension
that routes one already-existing writer to a place that previously
held a `NoopRecorder`. There is no UI backbone to span; there is one
construction site to change.

Equivalent statement: **the smallest valuable change is to replace the
Cinder `NoopRecorder` with a `CinderToOtlpJsonWriter` constructed
against the same file the Lumen writer already targets, in the
`Some(path)` arm of the existing `otlp_log_path` match in
`crates/kaleidoscope-cli/src/lib.rs:147-160`.** Slice 01 ships exactly
that.

## Release Slices

### Slice 01 — Cinder events also land in the `--observe-otlp` file

- **Outcome**: A sidecar reading the operator's
  `--observe-otlp <path>` file sees one `cinder.place.count` line per
  CLI-driven `cinder.place(...)` call, interleaved with the existing
  `lumen.batches.ingested.count` lines, with the cross-writer NDJSON
  stream remaining valid line-by-line JSON terminated by `\n` even
  under concurrent emission (OK6).
- **Stories**: `US-01` (single slice; all DoR-validated AC inside).
- **Learning hypothesis**: disproves the assumption that cross-writer
  NDJSON validity is automatic if we naively pass the same `File` path
  to both writers. If both writers were constructed against
  independent `File::open(path)` handles with `O_APPEND`, the kernel's
  per-write atomicity for sub-`PIPE_BUF` writes is the only thing
  defending against interleaving. The acceptance test's concurrent
  random-pause scenario is the empirical probe that this assumption
  holds under our writers' actual line sizes; if it fails, the slice's
  observed failure mode tells DESIGN what shape the file-sharing
  mechanism actually needs to take (`File::try_clone`, an `Arc<File>`
  adapter, a single shared `Mutex<File>` across both writers, etc.).
- **Production-data-equivalent AC**: an end-to-end test invokes
  `kaleidoscope_cli::ingest` (the actual library function the binary
  calls — same entry point) with `otlp_log_path = Some(...)` against a
  real `tempfile::NamedTempFile`-style temp path, feeds 6 records at
  batch_size 3, and reads back the file to assert exactly 2
  `cinder.place.count` lines plus 2 `lumen.batches.ingested.count`
  lines, all parseable as JSON, file ending in `\n`. This is the same
  data path the operator's `kaleidoscope-cli ingest acme /tmp/data
  --observe-otlp /tmp/foo.ndjson` invocation will exercise.
- **Dogfood moment**: After the slice ships, Andrea opens a terminal,
  runs `cargo run --bin kaleidoscope-cli -- ingest acme /tmp/kdata
  --observe-otlp /tmp/kobs.ndjson < some_records.ndjson` in one pane,
  and `tail -f /tmp/kobs.ndjson | jq .` in another. The second pane
  shows both `kaleidoscope.lumen` and `kaleidoscope.cinder` lines
  streaming in as the ingest loop runs, every line parsed by `jq`
  without error. The two-pane demo is the dogfood gate for the slice.
- **Effort**: well under 1 day. The change inside
  `kaleidoscope_cli::ingest` is structurally a copy of the Lumen-side
  wiring already at lines 147-160 of `crates/kaleidoscope-cli/src/lib.rs`
  (open file in append mode, wrap in writer, box-up as `dyn
  cinder::MetricsRecorder + Send + Sync`); the new acceptance test
  mirrors the existing `observe_otlp_flag.rs` harness. The
  concurrent-random-pause scenario is one additional test function
  alongside the happy-path test.

## Priority Rationale

There is one slice and it is the only slice. The reference-class
sizing (this is the third small feature in a row after
`cinder-to-pulse-bridge-v0` and `cinder-to-otlp-json-bridge-v0`, and
smaller than both) means there is no benefit from further splitting:

- Slice 01 carries the wiring change, the happy-path acceptance test
  (OK7), the concurrent-random-pause acceptance test (OK6), and the
  byte-equivalence guarantee on the existing `observe_otlp_flag.rs`
  test (OK8) all together. Splitting OK6 and OK7 into separate slices
  would force two PRs for one wiring change — net negative for the
  reviewer.
- The principal KPI (OK6) is the cross-writer invariant; OK7 (Cinder
  lines present) is its prerequisite; OK8 (Lumen non-regression) is
  its guardrail. Shipping any of the three without the others is
  meaningless: OK6 cannot be measured without Cinder lines to validate
  against (OK7); OK7 by itself is corrupted output if OK6 fails; OK8
  by itself is "we did nothing useful, but we didn't break anything",
  which is not a shippable outcome.

If schedule pressure ever forces a partial ship, **the slice is
already as thin as it can be**: the wiring change is one match-arm
substitution in `crates/kaleidoscope-cli/src/lib.rs:163`. There is no
sub-slice worth shipping in isolation.

## Cross-bridge alignment

This story-map intentionally mirrors the operator-facing post-v0 row
in
`docs/feature/cinder-to-otlp-json-bridge-v0/discuss/outcome-kpis.md`
("Post-v0 outcome KPIs deferred to the CLI follow-up feature").
OK1-CLI / OK2-CLI / OK3-CLI in that document map onto OK7 in this one
(restricted to `place` because the CLI's ingest loop only triggers
`place` — see `wave-decisions.md` D1). The OK6 cross-writer guarantee
was explicitly mandated by ADR-0039 §7 as the CLI follow-up feature's
principal KPI, and this story-map honours that mandate by promoting it
to the principal KPI and the slice's learning hypothesis.

## Scope Assessment: PASS — 1 story, 1 bounded context, estimated < 1 day

- 1 story (US-01).
- 1 bounded context (`kaleidoscope-cli` crate; the wiring change is at
  one match-arm and the new acceptance test lives in one new file).
- 1 modified file (`crates/kaleidoscope-cli/src/lib.rs`), 1 new file
  (`crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs`), 1
  line-level modification (`crates/kaleidoscope-cli/Cargo.toml` for the
  new `[[test]]` entry).
- 1 integration point (the existing `Some(path) => { … }` arm of the
  `otlp_log_path` match in `kaleidoscope_cli::ingest`, already
  exercised by the prior feature's `observe_otlp_flag.rs` tests).
- Estimated effort: well under 1 day for the crafter. The acceptance
  test's concurrent-random-pause scenario is the highest-information-
  density piece, and even that is one `std::thread::spawn` pair plus a
  read-back-and-assert sequence.

The feature is right-sized. No splitting required, no thinning
possible.
