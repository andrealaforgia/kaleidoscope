<!-- markdownlint-disable MD024 -->

# User Stories — `cli-cinder-otlp-wiring-v0`

## System Constraints (apply to every story)

- Rust idiomatic per `CLAUDE.md`: data + free functions + traits where
  polymorphism is genuinely needed. The two trait-port boundaries
  (`lumen::MetricsRecorder` on the Lumen edge, `cinder::MetricsRecorder`
  on the Cinder edge) are already in place from the prior `--observe-otlp`
  feature and from `cinder-to-otlp-json-bridge-v0`; this feature replaces
  one runtime construction site, not a trait.
- License: AGPL-3.0-or-later, matching the rest of the workspace.
- The acceptance idiom for this project is Rust `#[test]` functions with
  `// Given / // When / // Then` comment blocks, not Gherkin `.feature`
  files. The Given/When/Then text in the UAT Scenarios sections below is
  the specification; DISTILL translates it into `#[test]` functions in
  `crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs` mirroring
  the pattern already in `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs`.
- Cross-bridge contract (locked in ADR-0039 §2 and inherited from
  `docs/feature/cinder-to-otlp-json-bridge-v0`): the Cinder metric name
  on the wire MUST be exactly `cinder.place.count`. Drift between this
  feature's observed wire output and the library precedent is a review
  failure.
- Cross-writer NDJSON-validity invariant (mandated by ADR-0039 §7): when
  both `LumenToOtlpJsonWriter` and `CinderToOtlpJsonWriter` are
  constructed against the same operator-supplied file path (the entire
  point of `--observe-otlp <path>` being a single path argument), the
  resulting NDJSON stream MUST remain valid line-by-line JSON with a
  trailing `\n`, even under concurrent emission. This is OK6 and is the
  principal KPI this feature exists to satisfy.
- Lumen-side non-regression: every assertion in
  `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` MUST continue to
  pass after this feature ships, with no edits to that file's
  assertions. This is OK8.
- Single-process scope: multi-process scenarios (two CLI processes
  writing to the same path) are out of scope (`wave-decisions.md` D7).
  In-scope concurrency is two threads inside one `kaleidoscope-cli
  ingest` invocation — the natural concurrency of an ingest loop that
  drives both a Lumen ingest and a Cinder place per batch flush.
- Scope is the `place` event only. The CLI's ingest loop only ever
  invokes `cinder.place(...)` (`crates/kaleidoscope-cli/src/lib.rs:228`);
  `migrate` and `evaluate` are not reached from this code path and are
  therefore out of scope (`wave-decisions.md` D1).
- Scope is the `ingest` subcommand only. Adding `--observe-otlp` to the
  `read` subcommand is out of scope (`wave-decisions.md` D5).

---

## US-01: Cinder placements are visible on the operator's existing `--observe-otlp` stream

### Elevator Pitch

- **Before**: Priya runs
  `kaleidoscope-cli ingest acme /tmp/data --observe-otlp /tmp/foo.ndjson < records.json`.
  Her sidecar is already tailing `/tmp/foo.ndjson` and forwarding lines
  to her org's OTLP collector; the collector dashboard shows
  `kaleidoscope.lumen` rows for `acme` per batch flush. The same CLI run
  executed `cinder.place(...)` once per batch
  (`crates/kaleidoscope-cli/src/lib.rs:228`), but those calls produced
  zero bytes in the file because the CLI constructs Cinder with
  `cinder::NoopRecorder` (`crates/kaleidoscope-cli/src/lib.rs:163`).
  Her dashboard shows zero `kaleidoscope.cinder` rows. She cannot
  answer "did `acme` actually place anything in Hot during the last
  ingest run?" from her standard tooling.
- **After**: Priya runs the same command:
  `kaleidoscope-cli ingest acme /tmp/data --observe-otlp /tmp/foo.ndjson < records.json`.
  She now sees `cinder.place.count` lines interleaved with
  `lumen.ingest.count` lines in `/tmp/foo.ndjson` — one of
  each per batch flush, in the order they were emitted. A `tail -f` on
  the file shows pairs streaming in as the ingest loop runs; a
  line-by-line `jq` over the file parses every line; the file ends with
  `\n`. The sidecar forwards both writers' lines without any sidecar
  change; the collector ingests both; the dashboard gains a
  `kaleidoscope.cinder / cinder.place.count` row for `acme` next refresh.
- **Decision enabled**: Priya can decide "did `acme`'s last ingest run
  actually land batches in the Hot tier, and at what rate?" from the
  same cross-process dashboard she already uses for Lumen, without
  patching Cinder source, without adding any new flag, and without
  changing her sidecar or collector configuration.

### Problem

Priya the platform operator runs a multi-tenant Kaleidoscope deployment.
The CLI's `--observe-otlp <path>` flag, shipped in commit `3af7e82`,
plumbs `LumenToOtlpJsonWriter` into the Lumen store — but it leaves the
Cinder store with `cinder::NoopRecorder`
(`crates/kaleidoscope-cli/src/lib.rs:163`). The cross-process bridge for
Cinder shipped as a library in `cinder-to-otlp-json-bridge-v0` (commits
landing `CinderToOtlpJsonWriter` under `crates/self-observe/src/`), but
the CLI does not construct it yet, so every `cinder.place` call during
ingest produces zero lines in the file Priya's sidecar is tailing.

Priya finds it operationally hostile to answer "did tenant `acme` just
place an item in Hot during the last ingest run?" from her cross-process
collector, because the data does not exist there. Her only workarounds
today are (a) writing a separate Rust binary that constructs Cinder with
`CinderToOtlpJsonWriter` (no production option for routine use), or (b)
giving up and reading the Cinder snapshot file directly (defeats the
purpose of running a cross-process collector).

### Who

Priya the platform operator | runs a multi-tenant Kaleidoscope
deployment for a fintech | already uses `--observe-otlp` for Lumen
observability via an existing sidecar + OTLP/HTTP collector + dashboard
chain | already has `kaleidoscope.lumen` panels on her dashboard | wants
the same chain to surface Cinder events with zero infrastructure
additions and zero new flags to remember.

### Solution

Inside `kaleidoscope_cli::ingest`, in the `Some(path) => { … }` arm of
the `otlp_log_path` match (currently `crates/kaleidoscope-cli/src/lib.rs`
lines 147-160), construct a `CinderToOtlpJsonWriter` against the same
operator-supplied file path that the Lumen writer is already wired
against, and pass it as Cinder's recorder instead of
`cinder::NoopRecorder`. The DESIGN wave picks the file-sharing
mechanism (e.g. `File::try_clone`, two separate `OpenOptions::new().create(true).append(true).open(path)`
calls, an `Arc<File>` wrapped in an adapter, etc.); any choice is
acceptable as long as the resulting cross-writer behaviour satisfies
OK6 (NDJSON validity under concurrent emission) and OK7 (Cinder lines
present per `place` call).

When `--observe-otlp` is absent (`None` arm of the same match), Cinder
continues to be constructed with `cinder::NoopRecorder`, unchanged.
Nothing else in the CLI changes.

### Domain Examples

#### 1. Happy path — Priya sees pairs of lines per batch flush

Priya runs:

```text
kaleidoscope-cli ingest acme /tmp/k-data \
  --observe-otlp /tmp/k-observe.ndjson \
  < records.ndjson
```

where `records.ndjson` contains 6 `lumen::LogRecord` entries (for
tenant `acme`, varied bodies) and the configured batch size is 3 (the
test invokes the library directly so it can set `batch_size = 3`; the
binary uses `DEFAULT_BATCH_SIZE = 100` by default). The CLI flushes
twice. After the command exits, `/tmp/k-observe.ndjson` contains exactly
4 non-empty lines:

```text
{"resource":{"attributes":[{"key":"tenant_id","value":{"stringValue":"acme"}}]},"scopeMetrics":[{"scope":{"name":"kaleidoscope.lumen"},"metrics":[{"name":"lumen.ingest.count","sum":{"aggregationTemporality":2,"isMonotonic":true,"dataPoints":[{"attributes":[{"key":"tenant_id","value":{"stringValue":"acme"}}],"timeUnixNano":"...","asInt":"3"}]}}]}]}
{"resource":{"attributes":[{"key":"tenant_id","value":{"stringValue":"acme"}}]},"scopeMetrics":[{"scope":{"name":"kaleidoscope.cinder"},"metrics":[{"name":"cinder.place.count","sum":{"aggregationTemporality":2,"isMonotonic":true,"dataPoints":[{"attributes":[{"key":"tenant_id","value":{"stringValue":"acme"}},{"key":"tier","value":{"stringValue":"hot"}}],"timeUnixNano":"...","asInt":"1"}]}}]}]}
{"resource":{"attributes":[{"key":"tenant_id","value":{"stringValue":"acme"}}]},"scopeMetrics":[{"scope":{"name":"kaleidoscope.lumen"},"metrics":[{"name":"lumen.ingest.count","sum":{...,"dataPoints":[{...,"asInt":"3"}]}}]}]}
{"resource":{"attributes":[{"key":"tenant_id","value":{"stringValue":"acme"}}]},"scopeMetrics":[{"scope":{"name":"kaleidoscope.cinder"},"metrics":[{"name":"cinder.place.count","sum":{...,"dataPoints":[{...,"asInt":"1"}]}}]}]}
```

Two `lumen.ingest.count` lines (one per batch flush, each
with `asInt="3"`) interleaved with two `cinder.place.count` lines (one
per batch flush, each with `asInt="1"` and `tier="hot"`). The file ends
with `\n`. Every line parses as a JSON object under `serde_json::Value`.
Priya's sidecar reads the file, forwards both writers' lines to the
OTLP collector, and her existing dashboard gains a new
`kaleidoscope.cinder / cinder.place.count` row for `acme`.

#### 2. Cross-writer NDJSON validity under concurrent random pauses

Priya's CI worker runs an acceptance test that spawns two threads:
thread A repeatedly invokes the Lumen writer's
`record_ingest(tenant_id="acme", count=3)` (driving
`lumen.ingest.count` lines), thread B repeatedly invokes the
Cinder writer's `record_place(tenant_id="acme", tier=Tier::Hot)`
(driving `cinder.place.count` lines). Each thread sleeps a random
`Duration` between 0 and 5 ms between calls; total run length is 100
emissions per thread (200 lines total). The test reads back the file
after both threads join.

Every non-empty line parses as `serde_json::Value` without error. The
file content ends with `\n`. No line is empty. The set of metric names
across all lines is exactly `{"lumen.ingest.count",
"cinder.place.count"}`. There are 100 of each. No line is split across
two writers' bytes (every line is either entirely Lumen-shaped or
entirely Cinder-shaped).

#### 3. Flag absent — Lumen-side behaviour byte-equivalent to pre-feature

Priya runs:

```text
kaleidoscope-cli ingest acme /tmp/k-data < records.ndjson
```

with no `--observe-otlp` flag. The CLI takes the existing `None` arm
of the recorder match: Lumen gets `LumenToPulseRecorder`; Cinder gets
`cinder::NoopRecorder`. No file is created at any path. The existing
test `no_observe_otlp_means_no_otlp_file_created` in
`crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` continues to pass
byte-equivalently. Likewise the other two existing tests in that file
(`observe_otlp_writes_one_line_per_batch_flush` and
`observe_otlp_file_is_appended_to_across_multiple_ingest_calls`)
continue to pass without any edit, with the Lumen-side assertions still
green — the file just additionally contains `cinder.place.count` lines
the existing tests do not assert against.

### UAT Scenarios (BDD)

#### Scenario: Ingest with `--observe-otlp` emits one `cinder.place.count` line per batch flush

```text
Given Priya invokes `kaleidoscope_cli::ingest` with `otlp_log_path = Some(/tmp/x.ndjson)`
And the input contains 6 records for tenant acme
And the batch size is 3
When the call returns Ok
Then exactly 2 non-empty lines in /tmp/x.ndjson have `scopeMetrics[0].metrics[0].name == "cinder.place.count"`
And every such line has `resource.attributes[0].value.stringValue == "acme"`
And every such line has `scopeMetrics[0].scope.name == "kaleidoscope.cinder"`
And every such line has `sum.dataPoints[0].asInt == "1"`
And every such line has a `tier=hot` point attribute
```

#### Scenario: Ingest with `--observe-otlp` continues to emit the existing Lumen lines

```text
Given Priya invokes `kaleidoscope_cli::ingest` with `otlp_log_path = Some(/tmp/x.ndjson)`
And the input contains 6 records for tenant acme
And the batch size is 3
When the call returns Ok
Then exactly 2 non-empty lines in /tmp/x.ndjson have `scopeMetrics[0].metrics[0].name == "lumen.ingest.count"`
And every such line has `resource.attributes[0].value.stringValue == "acme"`
And every such line has `sum.dataPoints[0].asInt == "3"`
And the total non-empty line count is 4 (2 Lumen + 2 Cinder)
```

#### Scenario: Cross-writer NDJSON validity under concurrent random pauses

```text
Given Priya's test spawns one thread invoking the Lumen writer 100 times and one thread invoking the Cinder writer 100 times
And both writers are constructed against handles onto the same operator-supplied file path
And each thread sleeps a random duration in [0, 5] ms between calls
When both threads join
Then the file content ends with `\n`
And every non-empty line in the file parses as a `serde_json::Value`
And exactly 100 lines have metric name "lumen.ingest.count"
And exactly 100 lines have metric name "cinder.place.count"
And no line is empty
```

#### Scenario: Flag absent leaves no OTLP file and no behaviour change

```text
Given Priya invokes `kaleidoscope_cli::ingest` with `otlp_log_path = None`
And the input contains records for tenant acme
When the call returns Ok
Then no file is created at any path the test could have specified
And Lumen recorder is `LumenToPulseRecorder` (existing behaviour, in-process Pulse sink)
And Cinder recorder is `cinder::NoopRecorder` (existing behaviour)
```

#### Scenario: Existing `observe_otlp_flag.rs` tests pass byte-equivalently

```text
Given the existing test file `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` is unmodified
When `cargo test --package kaleidoscope-cli --test observe_otlp_flag` runs after this feature ships
Then all 3 tests in that file pass green
And no assertion in that file is edited
```

### Acceptance Criteria

- [ ] When `kaleidoscope_cli::ingest` is invoked with `otlp_log_path = Some(path)`, every `cinder.place(...)` call inside the ingest loop appends exactly one non-empty line to the file at `path` whose `scopeMetrics[0].metrics[0].name` equals `cinder.place.count`.
- [ ] Each such line has `resource.attributes[0].value.stringValue` equal to the tenant id passed to `ingest`.
- [ ] Each such line has `scopeMetrics[0].scope.name` equal to `kaleidoscope.cinder`.
- [ ] Each such line has `sum.dataPoints[0].asInt` equal to `"1"`.
- [ ] Each such line has a point attribute with key `tier` and `stringValue` equal to `"hot"` (because the ingest loop always places under Hot — `crates/kaleidoscope-cli/src/lib.rs:228`).
- [ ] When `kaleidoscope_cli::ingest` is invoked with `otlp_log_path = Some(path)`, every batch flush also appends exactly one line whose `scopeMetrics[0].metrics[0].name` equals `lumen.ingest.count` (unchanged from the prior feature).
- [ ] The total non-empty line count in the file after an N-batch ingest equals 2N (one Lumen line plus one Cinder line per batch flush).
- [ ] After a concurrent scenario in which Lumen and Cinder writers each emit 100 lines against handles onto the same file, with random pauses between calls, every non-empty line in the file parses as `serde_json::Value`, the file ends with `\n`, exactly 100 lines have metric name `lumen.ingest.count`, and exactly 100 lines have metric name `cinder.place.count`.
- [ ] When `kaleidoscope_cli::ingest` is invoked with `otlp_log_path = None`, no file is created at any path and the recorders are constructed as before (Lumen = `LumenToPulseRecorder`, Cinder = `cinder::NoopRecorder`).
- [ ] The existing test file `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` continues to pass green under `cargo test --package kaleidoscope-cli --test observe_otlp_flag` with no edits to its assertions.

### Outcome KPIs

- **Who**: platform operator (Priya), observed at the byte level on her configured `--observe-otlp <path>` sink
- **Does what**: sees `cinder.place.count` lines interleaved with `lumen.ingest.count` lines in the same NDJSON file, with the cross-writer stream remaining valid line-by-line JSON terminated by `\n`
- **By how much**: 100% of `cinder.place` calls produce exactly one line (OK7); 100% of lines parse as JSON and stream ends with `\n` under concurrent emission (OK6); existing Lumen-side assertions continue to pass byte-equivalently (OK8)
- **Measured by**: new test `crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs` (OK6 + OK7) and existing `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` (OK8)
- **Baseline**: 0% Cinder lines today (NoopRecorder); existing Lumen lines as shipped at commit `3af7e82`

Maps to OK6-CLI-cross-writer-ndjson (principal), OK7-CLI-cinder-events-present, and OK8-CLI-no-regression in `outcome-kpis.md`.

### Technical Notes

- Modified file: `crates/kaleidoscope-cli/src/lib.rs` — the `Some(path) => { … }` arm of the `otlp_log_path` match currently spans lines 147-160. DESIGN picks how to share the file handle between the two writers.
- New test file: `crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs`. Mirrors the harness pattern from the existing `observe_otlp_flag.rs` (the `tenant`, `record`, `temp_root`, `cleanup`, `ndjson` helpers can be either duplicated inline at v0 or extracted to a shared module — DESIGN decides; the rule-of-three trigger is one additional test file, which this feature already adds).
- Manifest: `crates/kaleidoscope-cli/Cargo.toml` gains a new `[[test]]` entry `name = "observe_otlp_cinder_wiring", path = "tests/observe_otlp_cinder_wiring.rs"`. The `self-observe` dependency in that manifest already re-exports `CinderToOtlpJsonWriter` after `cinder-to-otlp-json-bridge-v0` shipped; no new dependency required.
- Concurrency model: the in-process concurrency is two threads inside one `kaleidoscope-cli ingest` invocation. The acceptance test for OK6 explicitly spawns Lumen-driving and Cinder-driving threads with random pauses (per ADR-0039 §7 item 3). The implementation does not need to introduce explicit threading inside `ingest` — the ingest loop is naturally sequential at one batch flush per iteration — but the writers MUST be constructed so that the OS-level shared file behaves correctly under the test's threaded scenario.
- The Cinder side of the wiring currently uses `Box::new(CinderRecorder)` (`crates/kaleidoscope-cli/src/lib.rs:163`, where `CinderRecorder` is an alias for `cinder::NoopRecorder` imported at line 57). The change is structurally analogous to the existing `recorder` match for the Lumen side at lines 147-160: a `Box<dyn cinder::MetricsRecorder + Send + Sync>` constructed conditionally on `otlp_log_path`.
- Slice tag: not `@infrastructure` — this story directly enables an operator-visible decision on a real CLI surface.

### Dependencies

- `cinder-to-otlp-json-bridge-v0` shipped (`CinderToOtlpJsonWriter` re-exported from `self-observe`).
- The existing `--observe-otlp` Lumen wiring shipped (commit `3af7e82`).
- `aegis` (already a `kaleidoscope-cli` dependency).
- `serde_json` (used by the existing `observe_otlp_flag.rs` test; available as a dev-dependency on `kaleidoscope-cli`).
- No new external crates required.

### Slice

`slices/slice-01-cinder-events-also-land-in-observe-otlp-file.md`
