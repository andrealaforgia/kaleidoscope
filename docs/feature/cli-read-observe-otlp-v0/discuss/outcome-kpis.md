# Outcome KPIs — `cli-read-observe-otlp-v0`

## Feature

`cli-read-observe-otlp-v0` — extend the `kaleidoscope-cli read`
subcommand so that the existing `--observe-otlp <path>` flag idiom
(already shipped for `ingest` at commit `3af7e82`) also routes Lumen
query events into the same NDJSON sink. Today the `read` subcommand
constructs Lumen with `LumenToPulseRecorder` over an in-process Pulse
store (`crates/kaleidoscope-cli/src/lib.rs:253-255`); that recorder
dies at end of call and produces zero bytes anywhere the operator can
inspect. After this feature, an operator running
`kaleidoscope-cli read acme /tmp/data --observe-otlp /tmp/foo.ndjson
> /dev/null` sees one `lumen.query.count` line appended to
`/tmp/foo.ndjson` per query call.

## Objective

A single `kaleidoscope-cli read acme /tmp/data --observe-otlp
/tmp/foo.ndjson > /dev/null` invocation leaves one new OTLP-JSON line
in `/tmp/foo.ndjson` per Lumen query call (today: exactly one query
per invocation), with the line's `scopeMetrics[0].metrics[0].name` set
to `lumen.query.count`, its `resource.attributes[0].value.stringValue`
set to the tenant id passed to `read`, and the file remaining valid
NDJSON terminated by `\n`. The operator's sidecar tails the same file
the `ingest` side already writes to and forwards both ingest-side and
read-side lines to the existing OTLP/HTTP collector without any
sidecar or collector change; the dashboard gains query-latency /
query-throughput / per-tenant-query-rate panels next refresh.

## Note on KPI granularity

This feature is operator-visible at the CLI surface. Unlike its
immediate predecessor `cli-cinder-otlp-wiring-v0`, there is no
cross-writer concurrency probe to mandate as the principal KPI — only
ONE writer participates in `read` (the Lumen writer). The principal
KPI is therefore the presence KPI (OK1); OK2 is the no-flag
non-regression guardrail; OK3 is the cross-subcommand symmetry KPI
that proves a single shell session can drive both ingest-side and
read-side metrics into one file.

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| OK1-CLI-read-lumen-query-events-present | Priya the platform operator, observed at the byte level on her configured `--observe-otlp <path>` sink | Sees exactly one new line with metric name `lumen.query.count`, scope `kaleidoscope.lumen`, and the per-tenant resource attribute, per Lumen query call that `kaleidoscope_cli::read` executes (one per invocation, because `read()` calls `lumen.query(tenant, TimeRange::all())` exactly once at `crates/kaleidoscope-cli/src/lib.rs:258-260`) | 100% of `read()` invocations with `otlp_log_path = Some(path)` produce exactly one line with `metrics[0].name == "lumen.query.count"`; 0% of such invocations leave the file without that line | 0% (the CLI's `read` path constructs `LumenToPulseRecorder` over an in-process Pulse sink today — emits nothing observable to any file) | New acceptance test `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` — happy path with N records pre-ingested (via a setup call) and one `read()` call produces a file containing exactly 1 `lumen.query.count` line with the correct tenant, scope, and shape | Leading (operator-visible behaviour; principal KPI for this feature) |
| OK2-CLI-read-no-side-channel | Priya the platform operator, observed at the byte level | Sees behaviour byte-equivalent to today when the `--observe-otlp` flag is absent on `read`: the function returns the same NDJSON stdout, the same return value (`count: usize`), and no file is created at any path | 100% byte equivalence of stdout records, 100% equality of returned record count, 0% file creation when flag absent | n/a (baseline = current shipped behaviour of `kaleidoscope_cli::read` at the commit this DISCUSS wave is written against) | New acceptance test `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` — no-flag scenario asserts (a) returned count matches the number of pre-ingested records, (b) stdout bytes equal the pre-ingested records re-serialised as NDJSON, (c) no file is created at any path the test specifies | Guardrail (non-regression on the existing `read` subcommand behaviour) |
| OK3-CLI-read-ingest-symmetry | Priya the platform operator, observed at the byte level in a single shell session | Sees a file at `--observe-otlp <path>` that contains BOTH ingest-side metric names (`lumen.ingest.count` from the prior feature, `cinder.place.count` from `cli-cinder-otlp-wiring-v0`) AND read-side metric names (`lumen.query.count` from this feature) after running `ingest --observe-otlp <path>` followed by `read --observe-otlp <path>` against the same path in the same session | After a 6-record-ingest (batch_size 3) followed by one `read()` call against the same `--observe-otlp` path, the file contains at least 2 `lumen.ingest.count` lines, at least 2 `cinder.place.count` lines, and at least 1 `lumen.query.count` line; every non-empty line parses as `serde_json::Value` and the file ends with `\n` | n/a (this scenario cannot exist today because `read` cannot emit OTLP at all) | Same new test file — sequential `ingest()` + `read()` scenario against one shared `otlp_log_path`; assertion is the union of metric-name sets across all parsed lines | Leading (cross-subcommand symmetry — the principal evidence that the operator's single sidecar configuration captures the full Lumen lifecycle) |

## Metric Hierarchy

- **North Star**: **OK1-CLI-read-lumen-query-events-present** — the
  presence KPI. Without it, the feature does not exist; with it alone
  the operator already sees query metrics on her existing collector.
- **Leading Indicators**: OK3 (ingest-symmetry) — proves the symmetric
  contract between the two subcommands holds end-to-end on one file in
  one shell session, which is the operator's actual workflow.
- **Guardrail Metrics**: OK2 (no-flag non-regression) — when the
  operator omits the flag, `read` continues to behave exactly as it
  does today (stdout NDJSON of all matched records, return value
  equals matched count, no side channel).

## Cross-feature alignment

OK1 in this feature is the read-side mirror of OK1 in the original
`--observe-otlp` ingest wiring feature (commit `3af7e82`). OK3 in this
feature is the first KPI to span subcommands; it is enabled by, but
not duplicative of, the cross-writer NDJSON-validity invariant
(OK6-CLI-cross-writer-ndjson) inherited from
`cli-cinder-otlp-wiring-v0`. OK3 explicitly does not assert
concurrent-writer behaviour — the two subcommands run sequentially in
the operator's shell session (`wave-decisions.md` D5).

| KPI | Cross-feature precedent | This feature |
|-----|-------------------------|--------------|
| OK1 | OK1 in the original `--observe-otlp` ingest wiring (Lumen ingest events present per batch flush) | Lumen query events present per `read()` invocation |
| OK2 | (n/a — no read-side guardrail existed before because `read` did not have the flag) | non-regression on `read` when flag absent |
| OK3 | (new) | cross-subcommand symmetry: both ingest-side and read-side metric types in one file from one shell session |

## Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| OK1-CLI-read-lumen-query-events-present | `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` — happy-path scenario | `cargo test --package kaleidoscope-cli --test observe_otlp_read_flag` exit code. The test pre-ingests records via a setup `ingest()` call (without `--observe-otlp`, so the OTLP file is not polluted), then invokes `read()` with `otlp_log_path = Some(path)`, then asserts (a) the file contains exactly 1 non-empty line, (b) that line's `scopeMetrics[0].metrics[0].name == "lumen.query.count"`, (c) `scopeMetrics[0].scope.name == "kaleidoscope.lumen"`, (d) `resource.attributes[0].value.stringValue == "acme"`, (e) `sum.dataPoints[0].asInt` equals the number of records matched by the query (stringified) | At every commit touching the CLI read path or the Lumen writer | `kaleidoscope-cli` maintainer (CI feedback per ADR-0005) |
| OK2-CLI-read-no-side-channel | Same test file — no-flag scenario | Same `cargo test` invocation. The test calls `read()` with `otlp_log_path = None` and asserts (a) returned `count` equals the number of pre-ingested records, (b) the captured stdout bytes equal the pre-ingested records re-serialised as NDJSON, (c) no file exists at the path the test would have specified for the flag-set case | Same | Same |
| OK3-CLI-read-ingest-symmetry | Same test file — ingest-then-read scenario | Same `cargo test` invocation. The test runs `ingest()` with 6 records / batch_size 3 / `otlp_log_path = Some(path)`, then runs `read()` with `otlp_log_path = Some(path)` (same path), then reads back the file and asserts the union of metric names across all parsed lines is the superset `{ "lumen.ingest.count", "cinder.place.count", "lumen.query.count" }` (counts: 2, 2, 1 respectively), every line parses as JSON, file ends with `\n` | Same | Same |

## Hypothesis

We believe that **constructing `LumenToOtlpJsonWriter::new(file)`
against an `OpenOptions::new().create(true).append(true).open(path)`
handle in the `Some(path)` arm of a new `otlp_log_path` match inside
`kaleidoscope_cli::read`, and threading that path through the
`main.rs` `run_read` dispatcher via the existing `parse_observe_otlp`
helper** for the **platform operator (Priya)** will achieve **a single
NDJSON sink that captures query activity on the same operator
configuration that already captures ingest activity, observable on her
existing sidecar + collector + dashboard chain with zero configuration
change**.

We will know this is true when:

- The new acceptance test's happy-path scenario passes green,
  asserting that one `read()` call with `otlp_log_path = Some(path)`
  produces exactly one `lumen.query.count` line with the correct
  tenant, scope, and `asInt` (OK1).
- The new acceptance test's no-flag scenario passes green, asserting
  byte equivalence of stdout and zero side-channel file creation
  (OK2).
- The new acceptance test's ingest-then-read scenario passes green,
  asserting both ingest-side metric types and the new read-side metric
  type all land in the same file from one sequential shell session
  (OK3).

## Handoff to DESIGN

The DESIGN wave (`nw-solution-architect`) should preserve:

1. **The single-write_all per-event pattern locked in ADR-0039 §2's
   correction box**: the Lumen writer already implements this pattern
   (`crates/self-observe/src/lumen_otlp_json.rs:182-196`); DESIGN
   MUST NOT propose changes to the writer.
2. **The `Option<&Path>` parameter idiom** already established by
   `ingest`'s fifth parameter (`crates/kaleidoscope-cli/src/lib.rs:144`).
   `read`'s new signature should be the straight-line mirror.
3. **Append-mode file open**: same as `ingest` (`OpenOptions::new()
   .create(true).append(true).open(path)`,
   `crates/kaleidoscope-cli/src/lib.rs:158-161`). The OK3 ingest-symmetry
   scenario explicitly depends on `read` appending to a file `ingest`
   already opened earlier in the same session.
4. **Single-process scope**: per `wave-decisions.md` D4, multi-process
   scenarios remain out of scope. The in-process concurrency footprint
   is one thread (the single query call inside `read()`).

The DESIGN wave should NOT introduce a new CLI flag, a new subcommand,
or any change to the Lumen writer's public API.

## DEVOPS instrumentation needs

No new collection infrastructure. The writer appends to the same NDJSON
file the existing sidecar already tails after the two prior
`--observe-otlp` features shipped; the operator's existing dashboards
extend by adding `lumen.query.count`-specific panels (per-tenant query
latency, per-tenant query throughput, expensive-tenant detection). The
CI gate is the new acceptance test's exit code, per ADR-0005 Gate 1
(the workspace already runs `cargo test` on every commit).
