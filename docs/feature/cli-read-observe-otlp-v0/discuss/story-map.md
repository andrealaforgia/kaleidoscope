# Story Map: `cli-read-observe-otlp-v0`

## User: Priya the platform operator

## Goal

When Priya runs
`kaleidoscope-cli read acme /tmp/data --observe-otlp /tmp/foo.ndjson > /dev/null`,
she sees `lumen.query.count` lines appended to `/tmp/foo.ndjson` — one
per Lumen query call — alongside the `lumen.ingest.count` lines and
`cinder.place.count` lines that earlier `ingest --observe-otlp
/tmp/foo.ndjson` invocations in the same shell session already
produced. Her existing sidecar tails the file and forwards both
ingest-side and read-side lines to the existing OTLP/HTTP collector
without configuration change; the dashboard gains
`kaleidoscope.lumen / lumen.query.count` panels next refresh.

## Backbone

The journey has exactly one activity: the operator sees Lumen query
events in the same OTLP stream she is already configured to consume
for ingest. The activity is a thin end-to-end slice: a single
`kaleidoscope-cli read ... --observe-otlp ...` invocation produces the
new line(s), which a sidecar reads, which a collector ingests, which a
dashboard displays. The thinness is the point: the file mechanism,
sidecar contract, collector wiring, and dashboard layout already exist
from the prior two `--observe-otlp` features (the original ingest
wiring at commit `3af7e82`, then `cli-cinder-otlp-wiring-v0`); this
feature is the wire that lets the `read` subcommand participate in the
same chain.

| Activity 1: operator sees Lumen query events in OTLP stream |
|---|
| CLI's `read` path constructs `LumenToOtlpJsonWriter` against the operator-supplied file path (instead of the in-process `LumenToPulseRecorder`) when `--observe-otlp` is set. The file contains `lumen.query.count` lines per `read()` invocation; the existing `read` stdout behaviour is unchanged; when the same path was used by a prior `ingest --observe-otlp` invocation in the same session, the file ends up containing both ingest-side and read-side metric types. |

## Walking Skeleton

Per `wave-decisions.md` D? (no explicit decision needed), the
walking-skeleton concept is N/A: the CLI already has `--observe-otlp`
wired for `ingest` (commit `3af7e82`), the Lumen writer already
implements both `record_ingest` and `record_query`
(`crates/self-observe/src/lumen_otlp_json.rs:200-208`), and the
binary's argument parser already knows how to parse `--observe-otlp
<path>` (`crates/kaleidoscope-cli/src/main.rs:105-119`). This feature
is a thin extension that uses one already-shipped writer's
already-shipped second method (`record_query`) on a code path that
previously constructed an in-process Pulse sink that nobody could
observe.

Equivalent statement: **the smallest valuable change is to add an
`otlp_log_path: Option<&Path>` parameter to `kaleidoscope_cli::read`,
add a `Some(path) => { … }` arm constructing
`LumenToOtlpJsonWriter::new(file)` exactly the way `ingest` does
(`crates/kaleidoscope-cli/src/lib.rs:147-160`), and call
`parse_observe_otlp` inside `run_read` in `main.rs` exactly the way
`run_ingest` does (`crates/kaleidoscope-cli/src/main.rs:87`).** Slice
01 ships exactly that.

## Release Slices

### Slice 01 — `read` emits OTLP-JSON on `--observe-otlp` flag

- **Outcome**: A sidecar reading the operator's `--observe-otlp <path>`
  file sees one `lumen.query.count` line per CLI-driven `read()` call,
  with the same scope (`kaleidoscope.lumen`), tenant resource
  attribute, and OTLP-JSON shape that `lumen.ingest.count` lines from
  the prior feature already use (OK1). The no-flag path is byte-
  equivalent to today (OK2). A single shell session that runs `ingest
  --observe-otlp foo.ndjson` then `read --observe-otlp foo.ndjson`
  produces a file with both ingest-side and read-side metric types
  (OK3).
- **Stories**: `US-01` (single slice; all DoR-validated AC inside).
- **Learning hypothesis**: disproves the assumption that Lumen's
  `record_query` path needs anything beyond the ingest-side wiring
  template. The Lumen writer already implements `record_query` — the
  prior wiring feature just never exercised that method because the
  CLI's `read` path constructed a different recorder
  (`LumenToPulseRecorder`). If the assumption holds (highly likely
  given the symmetry of the two `MetricsRecorder` trait methods at
  `crates/self-observe/src/lumen_otlp_json.rs:200-208`), the OK1
  acceptance test passes green with a copy-paste of the ingest-side
  wiring substituted into `read`. If it fails, the failure mode tells
  DESIGN what the unobvious difference between the two methods is at
  the call site (e.g. unusual ownership constraints in `LogStore`'s
  query path, surprising `MetricsRecorder` trait bound interactions
  with the existing `FileBackedLogStore::open` recorder slot, etc.).
- **Production-data-equivalent AC**: an end-to-end test invokes
  `kaleidoscope_cli::read` (the actual library function the binary
  calls — same entry point) with `otlp_log_path = Some(...)` against a
  real temp path, against a Lumen store that was pre-populated by a
  prior `ingest()` setup call, and reads back the OTLP file to assert
  exactly one `lumen.query.count` line with the correct shape. This is
  the same data path the operator's `kaleidoscope-cli read acme
  /tmp/data --observe-otlp /tmp/foo.ndjson > /dev/null` invocation
  will exercise.
- **Dogfood moment**: After the slice ships, Andrea opens a terminal,
  runs `cargo run --bin kaleidoscope-cli -- ingest acme /tmp/kdata
  --observe-otlp /tmp/kobs.ndjson < some_records.ndjson`, then `cargo
  run --bin kaleidoscope-cli -- read acme /tmp/kdata --observe-otlp
  /tmp/kobs.ndjson > /dev/null`, then `cat /tmp/kobs.ndjson | jq
  '.scopeMetrics[0].metrics[0].name' | sort | uniq -c`. The output
  shows nonzero counts for `lumen.ingest.count`, `cinder.place.count`,
  and `lumen.query.count`. The three-metric-name demo is the dogfood
  gate for the slice.
- **Effort**: well under 1 day. The change inside
  `kaleidoscope_cli::read` is structurally a slimmer copy of the
  Lumen-side wiring already at lines 147-160 of
  `crates/kaleidoscope-cli/src/lib.rs` (open file in append mode, wrap
  in `LumenToOtlpJsonWriter`, box-up as `dyn lumen::MetricsRecorder +
  Send + Sync`); the new acceptance test mirrors the existing
  `observe_otlp_flag.rs` harness. No second writer to coordinate, no
  concurrency probe to spawn — strictly thinner than the predecessor
  feature.

## Priority Rationale

There is one slice and it is the only slice. The reference-class
sizing (this is the fourth consecutive small feature in the
`self-observe` / `kaleidoscope-cli` cluster, after
`cinder-to-pulse-bridge-v0`, `cinder-to-otlp-json-bridge-v0`, and
`cli-cinder-otlp-wiring-v0`, and strictly smaller than all three
because no cross-writer concurrency probe is required) means there is
no benefit from further splitting:

- Slice 01 carries the wiring change (one new `Option<&Path>`
  parameter on `read`, one new match arm, one new `parse_observe_otlp`
  call in `main.rs`), the OK1 happy-path test, the OK2 no-flag test,
  and the OK3 ingest-then-read symmetry test all together. Splitting
  any one of the three KPIs into a separate slice would force a second
  PR for trivially the same wiring — net negative for the reviewer.
- The principal KPI (OK1) is the presence KPI; OK2 is its
  no-flag-quiescence guardrail; OK3 is the cross-subcommand symmetry
  KPI. Shipping any without the others is meaningless: OK1 alone gives
  the operator query lines but leaves the no-flag path unverified
  (regression risk); OK2 alone is "we did nothing useful, but we
  didn't break anything"; OK3 alone is structurally impossible without
  OK1 because OK3 asserts the presence of OK1's metric type.

If schedule pressure ever forces a partial ship, **the slice is
already as thin as it can be**: the wiring change is one new match arm
plus one new `parse_observe_otlp` call. There is no sub-slice worth
shipping in isolation.

## Cross-feature alignment

This story-map intentionally mirrors the operator-facing posture of
`cli-cinder-otlp-wiring-v0/discuss/story-map.md` and inherits its
persona (Priya), file substrate (the shared `--observe-otlp` NDJSON
file with `O_APPEND` semantics), and sidecar/collector chain
assumptions. The principal contractual difference is that this feature
does NOT need a cross-writer concurrency probe: `read()` involves
exactly one writer (the Lumen writer), one query call per invocation,
and no in-process concurrency. The cross-writer NDJSON-validity
invariant from ADR-0039 §7 was discharged by the predecessor feature;
the present feature inherits the file substrate's correctness and adds
only a new emission source on the same file.

## Scope Assessment: PASS — 1 story, 1 bounded context, estimated < 1 day

- 1 story (US-01).
- 1 bounded context (`kaleidoscope-cli` crate; the wiring change is at
  one match arm in `lib.rs`, one parse call in `main.rs`, and the new
  acceptance test lives in one new file).
- 2 modified files (`crates/kaleidoscope-cli/src/lib.rs`,
  `crates/kaleidoscope-cli/src/main.rs`), 1 new file
  (`crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs`), 1
  line-level modification (`crates/kaleidoscope-cli/Cargo.toml` for
  the new `[[test]]` entry).
- 1 integration point (the new `Some(path) => { … }` arm inside
  `kaleidoscope_cli::read`, structurally a copy of the existing
  arm at `crates/kaleidoscope-cli/src/lib.rs:147-160`).
- Estimated effort: well under 1 day for the crafter. No concurrency
  test to write, no shared-handle ownership puzzle to solve (the
  ingest-symmetry scenario writes to the file sequentially across two
  function calls, not in parallel from two threads).

The feature is right-sized. No splitting required, no thinning
possible.
