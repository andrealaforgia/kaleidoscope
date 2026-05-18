<!-- markdownlint-disable MD024 -->

# User Stories — `cli-read-observe-otlp-v0`

## System Constraints (apply to every story)

- Rust idiomatic per `CLAUDE.md`: data + free functions + traits where
  polymorphism is genuinely needed. The trait-port boundary
  (`lumen::MetricsRecorder`) is already in place; this feature replaces
  one runtime construction site (the recorder slot inside
  `kaleidoscope_cli::read`), not a trait.
- License: AGPL-3.0-or-later, matching the rest of the workspace.
- The acceptance idiom for this project is Rust `#[test]` functions with
  `// Given / // When / // Then` comment blocks, not Gherkin `.feature`
  files. The Given/When/Then text in the UAT Scenarios sections below is
  the specification; DISTILL translates it into `#[test]` functions in
  `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` mirroring
  the pattern already in `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs`.
- Metric-name contract: the Lumen-query metric name on the wire MUST
  be exactly `lumen.query.count`. This is what
  `LumenToOtlpJsonWriter::record_query` already emits
  (`crates/self-observe/src/lumen_otlp_json.rs:205-207`). Drift between
  this feature's observed wire output and the library precedent is a
  review failure.
- Scope name contract: the scope MUST be exactly `kaleidoscope.lumen`
  (already inherited from the writer's hardcoded constructor at
  `crates/self-observe/src/lumen_otlp_json.rs:138`).
- File-open contract: when `--observe-otlp` is set, the operator-
  supplied path is opened with
  `OpenOptions::new().create(true).append(true).open(path)`, the same
  way `ingest` does at `crates/kaleidoscope-cli/src/lib.rs:158-161`.
  This is what makes the OK3 ingest-then-read symmetry scenario work
  on one shared file across two function calls in one shell session.
- No-flag non-regression: every assertion describing the existing
  behaviour of `kaleidoscope_cli::read` (stdout NDJSON, returned
  `count`, no side channel) MUST continue to pass when
  `otlp_log_path = None`. This is OK2.
- Single-process scope: multi-process scenarios (two CLI processes
  writing to the same path) are out of scope (`wave-decisions.md` D4).
  In-scope concurrency: a single thread inside one
  `kaleidoscope-cli read` invocation.
- Scope is the `read` subcommand only. The `ingest` subcommand was
  wired by the prior two features (commit `3af7e82` and
  `cli-cinder-otlp-wiring-v0`) and is NOT touched here. Cinder events
  from `read` are out of scope (`wave-decisions.md` D2) — `read()`
  does not construct a Cinder store.

---

## US-01: Lumen query events are visible on the operator's existing `--observe-otlp` stream

### Elevator Pitch

- **Before**: Priya runs
  `kaleidoscope-cli read acme /tmp/data --observe-otlp /tmp/foo.ndjson > /dev/null`.
  The CLI rejects the flag because `read` does not accept
  `--observe-otlp` (the binary's `run_read` dispatcher at
  `crates/kaleidoscope-cli/src/main.rs:121-128` never calls
  `parse_observe_otlp` and the library's `read()` function at
  `crates/kaleidoscope-cli/src/lib.rs:252-269` has no parameter for
  it). Even if she invokes the library directly with the right hand-
  rolled wiring, the function constructs `LumenToPulseRecorder` over
  an in-process Pulse store that dies at end of call
  (`crates/kaleidoscope-cli/src/lib.rs:253-255`), so the `record_query`
  call inside Lumen produces zero bytes anywhere she can inspect. Her
  collector dashboard shows nothing about query activity — no per-
  tenant query latency, no per-tenant query throughput, no detection
  of tenants generating expensive queries. She cannot answer "what is
  our query latency distribution across tenants right now?" from her
  standard tooling.
- **After**: Priya runs the same command:
  `kaleidoscope-cli read acme /tmp/data --observe-otlp /tmp/foo.ndjson > /dev/null`.
  She now sees one new `lumen.query.count` line appended to
  `/tmp/foo.ndjson`, with `scope = "kaleidoscope.lumen"`, `tenant_id =
  "acme"` as the resource attribute, and `sum.dataPoints[0].asInt`
  equal to the number of records the query matched. Her sidecar
  forwards the line without any sidecar change; the collector ingests
  it; the dashboard gains query-side panels next refresh. In the same
  shell session, if she had earlier run `kaleidoscope-cli ingest acme
  /tmp/data --observe-otlp /tmp/foo.ndjson < records.ndjson`, the
  single file at `/tmp/foo.ndjson` now contains `lumen.ingest.count`
  lines (from the earlier ingest) AND `cinder.place.count` lines (from
  the earlier ingest, per the prior `cli-cinder-otlp-wiring-v0`
  feature) AND `lumen.query.count` lines (from this `read`
  invocation) — the full Lumen lifecycle visible on one
  configuration.
- **Decision enabled**: Priya can decide "what is `acme`'s query
  latency and throughput distribution right now, and which tenants
  are generating the most expensive queries?" from the same
  cross-process dashboard she already uses for ingest activity,
  without patching Lumen source, without adding any new flag to
  remember, and without changing her sidecar or collector
  configuration.

### Problem

Priya the platform operator runs a multi-tenant Kaleidoscope
deployment. The CLI's `--observe-otlp <path>` flag, shipped in commit
`3af7e82` for `ingest` and extended in `cli-cinder-otlp-wiring-v0` to
also carry Cinder events, plumbs `LumenToOtlpJsonWriter` and
`CinderToOtlpJsonWriter` into the ingest path — but `read` is
unwired. The `read()` library function constructs
`LumenToPulseRecorder` over a fresh in-process Pulse store
(`crates/kaleidoscope-cli/src/lib.rs:253-255`); that recorder dies at
end of call and produces zero bytes anywhere the operator can
inspect. The binary's `run_read` dispatcher does not even accept the
`--observe-otlp` flag (`crates/kaleidoscope-cli/src/main.rs:121-128`).

Priya finds it operationally hostile to answer "what is `acme`'s
query latency and throughput right now?" or "which tenants are
generating the most expensive queries?" from her cross-process
collector, because no data flows from `read` to that collector. Her
only workarounds today are (a) writing a separate Rust binary that
constructs Lumen with `LumenToOtlpJsonWriter` (no production option
for routine use), or (b) instrumenting the dashboard via
fundamentally different out-of-band tooling that bypasses her
existing OTLP collector configuration (defeats the purpose of the
collector).

### Who

Priya the platform operator | runs a multi-tenant Kaleidoscope
deployment for a fintech | already uses `--observe-otlp` for
ingest-side observability via an existing sidecar + OTLP/HTTP
collector + dashboard chain | already has `lumen.ingest.count` and
`cinder.place.count` panels on her dashboard | wants the same chain
to surface Lumen query events with zero infrastructure additions and
zero new flags to remember.

### Solution

Add an `otlp_log_path: Option<&Path>` parameter to
`kaleidoscope_cli::read` (structurally identical to `ingest`'s fifth
parameter at `crates/kaleidoscope-cli/src/lib.rs:144`). Inside
`read()`, branch on that parameter: the `Some(path)` arm constructs
`LumenToOtlpJsonWriter::new(file)` against an
`OpenOptions::new().create(true).append(true).open(path)` handle
(exact mirror of `crates/kaleidoscope-cli/src/lib.rs:158-164`); the
`None` arm preserves the existing `LumenToPulseRecorder::new(pulse)`
construction (no behaviour change).

In `crates/kaleidoscope-cli/src/main.rs`, `run_read` (currently lines
121-128) gains a `parse_observe_otlp(args)?` call mirroring
`run_ingest`'s call at line 88; the parsed `Option<PathBuf>` is
forwarded into the new `read()` parameter as `otlp_path.as_deref()`.
`print_usage` is updated to mention that `read` also accepts
`--observe-otlp <path>` with the same semantics as `ingest`.

When `--observe-otlp` is absent, `read` continues to construct
`LumenToPulseRecorder` exactly as today; no file is created; stdout
behaviour and return value are byte-equivalent to today (OK2).

### Domain Examples

#### 1. Happy path — Priya sees one `lumen.query.count` line per read invocation

Priya has previously run:

```text
kaleidoscope-cli ingest acme /tmp/k-data \
  --observe-otlp /tmp/k-observe.ndjson \
  < records.ndjson
```

where `records.ndjson` contains 6 `lumen::LogRecord` entries (for
tenant `acme`, varied bodies) and the configured batch size is 3 (the
test invokes the library directly so it can set `batch_size = 3`).
She then runs:

```text
kaleidoscope-cli read acme /tmp/k-data \
  --observe-otlp /tmp/k-observe.ndjson \
  > /dev/null
```

After the command exits, `/tmp/k-observe.ndjson` contains 5
non-empty lines: 2 `lumen.ingest.count` lines (one per batch flush
during the prior ingest) + 2 `cinder.place.count` lines (one per
batch flush during the prior ingest) + 1 new `lumen.query.count`
line from this `read` invocation. The new line looks like (formatted
for readability; on the wire it is one line):

```json
{
  "resource": {
    "attributes": [
      {"key": "tenant_id", "value": {"stringValue": "acme"}}
    ]
  },
  "scopeMetrics": [{
    "scope": {"name": "kaleidoscope.lumen"},
    "metrics": [{
      "name": "lumen.query.count",
      "sum": {
        "aggregationTemporality": 2,
        "isMonotonic": true,
        "dataPoints": [{
          "attributes": [
            {"key": "tenant_id", "value": {"stringValue": "acme"}}
          ],
          "timeUnixNano": "...",
          "asInt": "6"
        }]
      }
    }]
  }]
}
```

`asInt == "6"` because the query matched all 6 ingested records under
`TimeRange::all()`. The file ends with `\n`. Every line parses as a
JSON object under `serde_json::Value`. Priya's sidecar reads the file,
forwards every line (Lumen-ingest, Cinder-place, and Lumen-query) to
the OTLP collector, and her existing dashboard gains a new
`kaleidoscope.lumen / lumen.query.count` row for `acme`.

#### 2. No-flag quiescence — stdout behaviour byte-equivalent to today

Priya runs:

```text
kaleidoscope-cli read acme /tmp/k-data
```

with no `--observe-otlp` flag. The CLI takes the (new) `None` arm of
the recorder match: Lumen gets `LumenToPulseRecorder` (existing
behaviour). Stdout contains exactly the matched records as NDJSON,
one per line, terminated by `\n`. The function's return value
(`count: usize`) equals the number of matched records. No file is
created at any path. The behaviour is byte-equivalent to the
pre-feature `read` function: an operator who never uses
`--observe-otlp` sees no change.

#### 3. Ingest-then-read symmetry — one file, full lifecycle

Priya runs, in one shell session:

```text
kaleidoscope-cli ingest acme /tmp/k-data --observe-otlp /tmp/k-observe.ndjson < records.ndjson
kaleidoscope-cli read   acme /tmp/k-data --observe-otlp /tmp/k-observe.ndjson > /dev/null
```

with 6 records at batch_size 3 for the `ingest` call. After both
commands exit, `/tmp/k-observe.ndjson` contains 5 non-empty lines:

- 2 lines with `metrics[0].name == "lumen.ingest.count"` (from
  ingest, batch flushes 1 and 2)
- 2 lines with `metrics[0].name == "cinder.place.count"` (from
  ingest, one per batch flush)
- 1 line with `metrics[0].name == "lumen.query.count"` (from read,
  one per invocation)

The file ends with `\n`. Every line parses as `serde_json::Value`.
The operator sees the full Lumen + Cinder lifecycle on one sidecar
configuration. No new flag to learn; the `--observe-otlp` muscle
memory she already has now extends across both subcommands.

### UAT Scenarios (BDD)

#### Scenario: Read with `--observe-otlp` emits one `lumen.query.count` line per invocation

```text
Given Priya has pre-ingested 6 records for tenant acme into /tmp/k-data
And the OTLP file at /tmp/k-observe.ndjson does not exist yet
When Priya invokes `kaleidoscope_cli::read` with `otlp_log_path = Some(/tmp/k-observe.ndjson)` and the same tenant and data_dir
And the call returns Ok
Then exactly 1 non-empty line exists in /tmp/k-observe.ndjson
And that line has `scopeMetrics[0].metrics[0].name == "lumen.query.count"`
And that line has `scopeMetrics[0].scope.name == "kaleidoscope.lumen"`
And that line has `resource.attributes[0].value.stringValue == "acme"`
And that line has `sum.dataPoints[0].asInt == "6"`
And the file ends with `\n`
```

#### Scenario: Read with `--observe-otlp` preserves stdout NDJSON output

```text
Given Priya has pre-ingested N records for tenant acme into /tmp/k-data
When Priya invokes `kaleidoscope_cli::read` with `otlp_log_path = Some(/tmp/k-observe.ndjson)` and a captured stdout sink
And the call returns Ok with `count == N`
Then the captured stdout bytes equal the N pre-ingested records re-serialised as NDJSON, one per line, terminated by `\n`
And the OTLP file additionally contains exactly 1 `lumen.query.count` line (the OTLP file is a side channel, not a replacement for stdout)
```

#### Scenario: Read without `--observe-otlp` creates no file and preserves existing behaviour

```text
Given Priya has pre-ingested N records for tenant acme into /tmp/k-data
When Priya invokes `kaleidoscope_cli::read` with `otlp_log_path = None` and a captured stdout sink
And the call returns Ok with `count == N`
Then the captured stdout bytes equal the N pre-ingested records re-serialised as NDJSON, one per line, terminated by `\n`
And no file is created at any path the test specifies for the would-be flag
And the Lumen recorder constructed inside `read()` is `LumenToPulseRecorder` (existing behaviour)
```

#### Scenario: Ingest then read in one session share one `--observe-otlp` file

```text
Given Priya invokes `kaleidoscope_cli::ingest` with 6 records for tenant acme, batch_size 3, and `otlp_log_path = Some(/tmp/k-observe.ndjson)`
And that call returns Ok
When Priya then invokes `kaleidoscope_cli::read` with the same tenant, the same data_dir, and `otlp_log_path = Some(/tmp/k-observe.ndjson)`
And that call also returns Ok
Then /tmp/k-observe.ndjson contains exactly 5 non-empty lines
And exactly 2 of those lines have `metrics[0].name == "lumen.ingest.count"` with `asInt == "3"`
And exactly 2 of those lines have `metrics[0].name == "cinder.place.count"` with `asInt == "1"` and `tier == "hot"`
And exactly 1 of those lines has `metrics[0].name == "lumen.query.count"` with `asInt == "6"`
And every non-empty line parses as `serde_json::Value`
And the file ends with `\n`
```

#### Scenario: Existing `observe_otlp_flag.rs` tests continue to pass byte-equivalently

```text
Given the existing test file `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` is unmodified
And the existing test file `crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs` (from `cli-cinder-otlp-wiring-v0`) is unmodified
When `cargo test --package kaleidoscope-cli` runs after this feature ships
Then all tests in both files pass green
And no assertion in either file is edited
```

### Acceptance Criteria

- [ ] `kaleidoscope_cli::read` gains an `otlp_log_path: Option<&Path>` parameter (structurally identical to `ingest`'s fifth parameter).
- [ ] When `kaleidoscope_cli::read` is invoked with `otlp_log_path = Some(path)`, the Lumen query call inside the function appends exactly one non-empty line to the file at `path` whose `scopeMetrics[0].metrics[0].name` equals `lumen.query.count`.
- [ ] That line has `resource.attributes[0].value.stringValue` equal to the tenant id passed to `read`.
- [ ] That line has `scopeMetrics[0].scope.name` equal to `kaleidoscope.lumen`.
- [ ] That line has `sum.dataPoints[0].asInt` equal to the stringified count of records the query matched (equal to the number of records previously ingested for the tenant under `TimeRange::all()`).
- [ ] When `kaleidoscope_cli::read` is invoked with `otlp_log_path = Some(path)`, the captured stdout bytes equal the matched records re-serialised as NDJSON, one record per line, terminated by `\n` (stdout behaviour unchanged from today).
- [ ] When `kaleidoscope_cli::read` is invoked with `otlp_log_path = None`, no file is created at any path; the captured stdout bytes equal the matched records re-serialised as NDJSON; the returned `count` equals the number of matched records; the constructed recorder is `LumenToPulseRecorder` (existing behaviour, byte-equivalent to today).
- [ ] After one `kaleidoscope_cli::ingest` call (6 records, batch_size 3, `otlp_log_path = Some(path)`) followed by one `kaleidoscope_cli::read` call against the same `path`, the file contains exactly 5 non-empty lines: 2 with `metrics[0].name == "lumen.ingest.count"`, 2 with `metrics[0].name == "cinder.place.count"`, 1 with `metrics[0].name == "lumen.query.count"`; every line parses as `serde_json::Value`; the file ends with `\n`.
- [ ] The binary's `run_read` dispatcher in `crates/kaleidoscope-cli/src/main.rs` parses `--observe-otlp <path>` via the existing `parse_observe_otlp` helper (line 105-119) and forwards the result into the new `read()` parameter.
- [ ] `print_usage` in `crates/kaleidoscope-cli/src/main.rs` documents `--observe-otlp <path>` on the `read` subcommand with the same semantics as the `ingest` subcommand.
- [ ] The existing test files `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` and `crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs` continue to pass green under `cargo test --package kaleidoscope-cli` with no edits to their assertions.

### Outcome KPIs

- **Who**: platform operator (Priya), observed at the byte level on
  her configured `--observe-otlp <path>` sink
- **Does what**: sees `lumen.query.count` lines per `read` invocation
  on the same file that already carries `lumen.ingest.count` and
  `cinder.place.count` lines from earlier `ingest` invocations in the
  same shell session, with stdout NDJSON behaviour preserved
- **By how much**: 100% of `read()` invocations with
  `otlp_log_path = Some(path)` produce exactly one
  `lumen.query.count` line (OK1); 100% byte equivalence of stdout
  and 0% file creation when flag absent (OK2); both ingest-side and
  read-side metric types present in one file after one sequential
  shell session (OK3)
- **Measured by**: new test
  `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` (OK1 +
  OK2 + OK3) and existing tests
  `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` +
  `observe_otlp_cinder_wiring.rs` (non-regression)
- **Baseline**: 0% Lumen query lines today (in-process Pulse sink
  that emits to no file); existing `read` stdout behaviour as
  shipped at the current commit

Maps to OK1-CLI-read-lumen-query-events-present (principal),
OK2-CLI-read-no-side-channel, and OK3-CLI-read-ingest-symmetry in
`outcome-kpis.md`.

### Technical Notes

- Modified file: `crates/kaleidoscope-cli/src/lib.rs` — the `read`
  function gains a fifth parameter `otlp_log_path: Option<&Path>` and
  a `match` on that value selecting the Lumen recorder
  construction. The `Some(path)` arm is a near-copy of the
  ingest-side Lumen wiring at lines 158-164 (open file with
  `OpenOptions::new().create(true).append(true).open(path)`, wrap in
  `LumenToOtlpJsonWriter::new(file)`, box as `Box<dyn lumen::MetricsRecorder + Send + Sync>`).
- Modified file: `crates/kaleidoscope-cli/src/main.rs` — `run_read`
  (lines 121-128) gains a `parse_observe_otlp(args)?` call mirroring
  `run_ingest`'s at line 88; the parsed `Option<PathBuf>` is
  forwarded into `read()` as `otlp_path.as_deref()`. `print_usage`
  (lines 68-84) gains a one-line note about `--observe-otlp` on the
  `read` subcommand.
- New test file:
  `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs`. Mirrors
  the harness pattern from `observe_otlp_flag.rs` (the `tenant`,
  `record`, `temp_root`, `cleanup`, `ndjson` helpers can be either
  duplicated inline at v0 or extracted to a shared module —
  `wave-decisions.md` D6 defers the extraction to a follow-up; the
  rule-of-three trigger arrives with this feature but the extraction
  is a separate refactoring task).
- Manifest: `crates/kaleidoscope-cli/Cargo.toml` gains a new
  `[[test]]` entry `name = "observe_otlp_read_flag", path =
  "tests/observe_otlp_read_flag.rs"`. The `self-observe` dependency
  in that manifest already re-exports `LumenToOtlpJsonWriter`; no new
  dependency required.
- Concurrency model: single thread per `read()` call. The `read()`
  function calls `lumen.query(tenant, TimeRange::all())` exactly once
  (`crates/kaleidoscope-cli/src/lib.rs:258-260`), which drives
  exactly one `record_query` event, which produces exactly one
  OTLP-JSON line. No within-process concurrency to defend against;
  no cross-writer concurrency to defend against (only one writer
  participates).
- The OK3 ingest-then-read scenario writes to the same file across
  two sequential function calls in one test process — there is no
  concurrent access between the two calls. The file's `O_APPEND`
  semantics (inherited from the existing `ingest` wiring) ensure
  that the `read`-side line appends after the `ingest`-side lines
  without truncating anything.
- Slice tag: not `@infrastructure` — this story directly enables an
  operator-visible decision on a real CLI surface
  (`kaleidoscope-cli read ... --observe-otlp ...`).

### Dependencies

- Prior `--observe-otlp` Lumen wiring shipped (commit `3af7e82`).
- `cli-cinder-otlp-wiring-v0` shipped (the Cinder side of the file is
  already populated for the OK3 scenario).
- `LumenToOtlpJsonWriter` publicly re-exported from `self-observe`
  (already a `kaleidoscope-cli` dependency at runtime).
- `aegis` (already a `kaleidoscope-cli` dependency).
- `serde_json` (used by the existing `observe_otlp_flag.rs` test;
  available as a dev-dependency on `kaleidoscope-cli`).
- No new external crates required.

### Slice

`slices/slice-01-read-emits-otlp-json-on-flag.md`
