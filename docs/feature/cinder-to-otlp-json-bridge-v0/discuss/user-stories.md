<!-- markdownlint-disable MD024 -->

# User Stories — `cinder-to-otlp-json-bridge-v0`

## System Constraints (apply to every story)

- Rust idiomatic per `CLAUDE.md`: data + free functions + traits where
  polymorphism is genuinely needed. No `dyn Trait` where direct
  monomorphisation suffices, except at the existing trait-port boundaries
  (`cinder::MetricsRecorder` on the in-edge and `std::io::Write` on the
  out-edge) which the writer must honour by definition.
- License: AGPL-3.0-or-later, matching the rest of the workspace.
- The writer MUST be `Send + Sync` for every `W: Write + Send + Sync` so
  that `Box<dyn cinder::MetricsRecorder + Send + Sync>` accepts it.
- Best-effort emission posture: `let _ = writer.write_all(...)` matches
  `lumen_otlp_json.rs:182-189`. Serialisation or write failures are
  silently dropped. `cinder::MetricsRecorder` returns `()` from every
  method anyway — no channel for errors exists.
- Cross-bridge contract (locked in `wave-decisions.md` D1): metric names
  MUST be exactly `cinder.place.count`, `cinder.migrate.count`,
  `cinder.evaluate.migrated.count`. The Pulse-sink sibling
  (`crates/self-observe/src/cinder_bridge.rs`) emits the same three
  names. Drift between the two is a review failure.
- Scope name MUST be exactly `kaleidoscope.cinder` (parallel to
  `kaleidoscope.lumen` in `lumen_otlp_json.rs:138`).
- Tier MUST serialise as lowercase string (`"hot"`/`"warm"`/`"cold"`),
  matching the Pulse-sink sibling.
- `tenant_id` MUST appear in BOTH the resource attribute slot AND every
  point attribute slot (mirroring `lumen_otlp_json.rs:39-43`).
- Each Cinder event MUST produce exactly one OTLP-JSON `ResourceMetrics`
  line. Lines MUST be terminated by `\n`. Lines MUST be independently
  parseable JSON. No batching, no combining multiple metrics into a
  single `scopeMetrics[].metrics[]` array.
- `Mutex<W>` guards the inner writer; the critical section holds across
  `write_all(body) + write_all(b"\n") + flush` to keep each NDJSON line
  atomic for the in-process case. POSIX `O_APPEND` handles the cross-
  process case at the CLI follow-up's call site (see
  `wave-decisions.md` D6).
- File layout: writer in `crates/self-observe/src/cinder_otlp_json.rs`;
  re-export from `crates/self-observe/src/lib.rs`; acceptance tests in
  `crates/self-observe/tests/cinder_to_otlp_json.rs`;
  `crates/self-observe/Cargo.toml` declares the new `[[test]]` entry
  (the `cinder` dependency was already added by the Pulse-sink sibling).

## Note on the `@infrastructure` slice rule

Per `nw-po-review-dimensions` Dimension 0 item 5: if every story in a
slice is `@infrastructure`, the slice has no release value and is
BLOCKED at slice level.

**This feature is library-only at v0.** Andrea decided (recorded in the
task brief and in `wave-decisions.md` D9) that the operator-visible CLI
surface is deferred to a separate follow-up feature. The slices here are
intentionally library-substrate slices.

The Elevator Pitch for each story therefore references the **Rust public
API entry point** (`self_observe::CinderToOtlpJsonWriter::new`) used by
the CLI follow-up feature, and the **NDJSON sink contents** (parsed by
the operator's existing sidecar and forwarded to the existing collector)
as the operator-observable surface. The "After" output for each story
shows the concrete OTLP-JSON line shape a sidecar will parse — that is
real, observable, byte-level output, not internal state.

The cross-process surface (operator's collector dashboard) is the
ultimate observability target, but it sits two integration layers beyond
this library: the CLI follow-up feature wires the writer in, and the
sidecar+collector chain (already deployed, already validated by the
Lumen OTLP-JSON feature) forwards the lines. The library's job is to
produce the lines; the lines are themselves observable.

The reviewer's blocking rule on all-infrastructure slices is acknowledged
and deliberately overridden by feature scope. Same posture as the
Pulse-sink sibling (`cinder-to-pulse-bridge-v0/discuss/user-stories.md`).
The downstream CLI feature will carry the operator-visible Elevator
Pitches.

---

## US-01: Cinder `place` events emit one OTLP-JSON ResourceMetrics line per call

### Elevator Pitch

- **Before**: Priya runs the CLI with `--observe-otlp <path>`. Her
  sidecar reads the NDJSON file and forwards lines to her org's OTLP
  collector. The collector dashboard shows `kaleidoscope.lumen` rows
  per tenant but ZERO `kaleidoscope.cinder` rows, even though the same
  CLI run executed `cinder.place(...)` for every batch
  (`kaleidoscope-cli/src/lib.rs:228`). Her only diagnostic is reading
  Cinder source.
- **After**: The CLI follow-up will swap `NoopRecorder` for
  `self_observe::CinderToOtlpJsonWriter::new(file)`. Each
  `cinder.place(&acme, &item, Tier::Hot, t)` now appends a line like
  `{"resource":{"attributes":[{"key":"tenant_id","value":{"stringValue":"acme"}}]},"scopeMetrics":[{"scope":{"name":"kaleidoscope.cinder"},"metrics":[{"name":"cinder.place.count","sum":{"aggregationTemporality":2,"isMonotonic":true,"dataPoints":[{"attributes":[{"key":"tier","value":{"stringValue":"hot"}}],"timeUnixNano":"...","asInt":"1"}]}}]}]}`
  to the same file. The sidecar parses it as JSON without modification;
  the collector ingests it as a `ResourceMetrics`; the dashboard shows
  the new row.
- **Decision enabled**: Priya can decide "is `acme`'s Hot-tier
  placement rate normal today?" from the same cross-process dashboard
  she already uses for Lumen, without modifying Cinder source and
  without adding new tooling.

### Problem

Priya the platform operator runs a multi-tenant Kaleidoscope deployment.
The CLI's `--observe-otlp <path>` flag, shipped in commit `3af7e82`,
plumbs `LumenToOtlpJsonWriter` into the Lumen store. But it leaves the
Cinder store with `NoopRecorder` (`kaleidoscope-cli/src/lib.rs:163`),
so every `cinder.place` call during ingest produces ZERO lines in the
file Priya's sidecar is tailing.

Priya finds it operationally hostile to answer "did tenant `acme` just
place an item in Hot?" from her cross-process collector, because the
question cannot be answered: the data does not exist there. Her only
workarounds are (a) patching Cinder source with `println!` and
rebuilding (no production option), or (b) running the in-process
Pulse-sink sibling — which gives her a queryable Pulse store but not a
collector-visible time series.

### Who

Priya the platform operator | runs a multi-tenant Kaleidoscope
deployment for a fintech | already uses the `--observe-otlp` flag for
Lumen observability via an existing sidecar + OTLP/HTTP collector +
dashboard chain | wants the same chain to show Cinder events with zero
infrastructure additions.

### Solution

A `CinderToOtlpJsonWriter<W: Write + Send + Sync>` struct in
`crates/self-observe/src/cinder_otlp_json.rs` that implements
`cinder::MetricsRecorder`. The `record_place` method emits one OTLP-JSON
`ResourceMetrics` line per call: scope `kaleidoscope.cinder`, metric
`cinder.place.count` (Sum kind, `aggregationTemporality = 2`,
`isMonotonic = true`), one data point with `asInt = "1"`, `timeUnixNano`
as a uint64 string, point attribute `tier = lowercase(input_tier)`,
resource attribute `tenant_id = tenant`, point attribute `tenant_id =
tenant` (mirroring the Lumen writer's double-emission of `tenant_id`).
The line ends with `\n`. The inner `W` is wrapped in `Mutex<W>` for
thread-safe in-process writes.

### Domain Examples

#### 1. Happy path — Priya sees fresh placements on her dashboard

Priya runs `kaleidoscope-cli ingest --observe-otlp /var/log/k/observe.ndjson`
with input that triggers
`cinder.place(&TenantId("acme".into()), &ItemId::new("trade-2026-05-18-001"), Tier::Hot, SystemTime::now())`.
Her sidecar reads the new line from the file:

```json
{"resource":{"attributes":[{"key":"tenant_id","value":{"stringValue":"acme"}}]},"scopeMetrics":[{"scope":{"name":"kaleidoscope.cinder"},"metrics":[{"name":"cinder.place.count","sum":{"aggregationTemporality":2,"isMonotonic":true,"dataPoints":[{"attributes":[{"key":"tenant_id","value":{"stringValue":"acme"}},{"key":"tier","value":{"stringValue":"hot"}}],"timeUnixNano":"1747569600123456789","asInt":"1"}]}}]}]}
```

The sidecar forwards. The collector ingests. The dashboard shows a new
`kaleidoscope.cinder / cinder.place.count` row for `acme` with `tier=hot`,
value `1`.

#### 2. Multi-tier — Priya verifies tier serialisation across all three tiers

Priya feeds input that triggers three placements for tenant `acme`:
`trade-001` in Hot, `trade-002` in Warm, `trade-003` in Cold. She greps
the NDJSON file for `cinder.place.count` and gets exactly 3 lines. The
set of `tier` point-attribute values across the three lines is exactly
`{"hot", "warm", "cold"}`. Her dashboard now shows three rows for `acme`,
one per tier.

#### 3. Two-tenant isolation — Priya verifies `acme` and `globex` do not bleed

Priya feeds input that triggers one placement for `acme` (Hot) and two
placements for `globex` (both Hot). The NDJSON file gains 3 new lines.
Exactly 1 line has `resource.attributes[0].value.stringValue = "acme"`;
exactly 2 have `... = "globex"`. The downstream collector groups them
correctly; the per-tenant dashboard panels show 1 row for `acme` and 2
rows for `globex`.

### UAT Scenarios (BDD)

#### Scenario: Place under a tenant produces one OTLP-JSON line under that tenant

```gherkin
Given Priya has constructed a CinderToOtlpJsonWriter around a Write sink
And the sink is empty
And tenant acme has no prior tier metadata
When cinder.place(&acme, &item("trade-2026-05-18-001"), Tier::Hot, t0) is called
Then the sink contains exactly one non-empty line
And that line parses as a complete JSON object
And the JSON's resource.attributes[0].key equals "tenant_id"
And the JSON's resource.attributes[0].value.stringValue equals "acme"
And the JSON's scopeMetrics[0].scope.name equals "kaleidoscope.cinder"
And the JSON's scopeMetrics[0].metrics[0].name equals "cinder.place.count"
And the JSON's scopeMetrics[0].metrics[0].sum.aggregationTemporality equals 2
And the JSON's scopeMetrics[0].metrics[0].sum.isMonotonic equals true
And the JSON's scopeMetrics[0].metrics[0].sum.dataPoints[0].asInt equals "1"
And that dataPoint's attributes contain {key: "tier", value: {stringValue: "hot"}}
And the sink ends with a \n byte
```

#### Scenario: Tier attribute reflects each of the three tier values

```gherkin
Given Priya has wired the writer
When cinder.place is called for tenant acme with Tier::Hot, Tier::Warm, Tier::Cold (three distinct items)
Then the sink contains exactly 3 non-empty lines
And every line has metric name "cinder.place.count"
And the set of "tier" attribute values across the three lines is exactly {"hot", "warm", "cold"}
```

#### Scenario: Per-tenant isolation under simultaneous placements

```gherkin
Given Priya has wired the writer
When cinder.place is called once for tenant acme (Tier::Hot) and twice for tenant globex (both Tier::Hot)
Then the sink contains exactly 3 non-empty lines
And exactly 1 line has resource.attributes[0].value.stringValue equal to "acme"
And exactly 2 lines have resource.attributes[0].value.stringValue equal to "globex"
```

#### Scenario: No place call means zero bytes in the sink

```gherkin
Given Priya has wired the writer but called nothing on Cinder
When the sink is inspected
Then the sink contains zero bytes
```

### Acceptance Criteria

- [ ] `self_observe::CinderToOtlpJsonWriter` exists and is publicly exported from `crates/self-observe/src/lib.rs`.
- [ ] `CinderToOtlpJsonWriter<W: Write + Send + Sync>` implements `cinder::MetricsRecorder`.
- [ ] On every `record_place(tenant, tier)` call, exactly one line is appended to the inner `W`, ending with `\n`, parseable as a JSON object.
- [ ] That JSON object has `resource.attributes[0] = {key: "tenant_id", value: {stringValue: <tenant>}}`.
- [ ] That JSON object has `scopeMetrics[0].scope.name = "kaleidoscope.cinder"`.
- [ ] That JSON object has `scopeMetrics[0].metrics[0].name = "cinder.place.count"`.
- [ ] That JSON object has `scopeMetrics[0].metrics[0].sum.aggregationTemporality = 2` and `sum.isMonotonic = true`.
- [ ] That JSON object has `scopeMetrics[0].metrics[0].sum.dataPoints[0].asInt = "1"`.
- [ ] That dataPoint's `attributes` contain an entry `{key: "tier", value: {stringValue: lowercase(tier)}}` with `Tier::Hot -> "hot"`, `Tier::Warm -> "warm"`, `Tier::Cold -> "cold"`.
- [ ] That dataPoint's `timeUnixNano` is a JSON string that parses as `u64`.
- [ ] Two-tenant isolation test passes: 3 events (acme x1, globex x2) produce 3 lines with the correct `resource.attributes[0].value.stringValue` counts.
- [ ] An unused writer produces zero bytes in the sink.
- [ ] The writer is `Send + Sync` (compile-time `assert_send_sync` test against `CinderToOtlpJsonWriter<Vec<u8>>`).

### Outcome KPIs

- **Who**: platform operator (Priya), measured at the library contract level via the acceptance harness
- **Does what**: receives a parseable OTLP-JSON line with metric `cinder.place.count`, scope `kaleidoscope.cinder`, `tenant_id` resource attribute, `tier` point attribute per `place` call
- **By how much**: 100% of `place` calls produce exactly one valid line (no drops, no duplicates, no shape deviation)
- **Measured by**: green tests in `crates/self-observe/tests/cinder_to_otlp_json.rs` (Slice 01 block)
- **Baseline**: 0% (CLI's Cinder recorder is `NoopRecorder` — emits nothing today)

Maps to OK1 in `outcome-kpis.md`.

### Technical Notes

- File: `crates/self-observe/src/cinder_otlp_json.rs` (new).
- Re-export: `pub use cinder_otlp_json::CinderToOtlpJsonWriter;` in `crates/self-observe/src/lib.rs`.
- Test file: `crates/self-observe/tests/cinder_to_otlp_json.rs` (new). Add a `[[test]] name = "cinder_to_otlp_json", path = "tests/cinder_to_otlp_json.rs"` entry in `Cargo.toml`.
- Hand-rolled OTLP-JSON serde structs duplicated from `lumen_otlp_json.rs` per `wave-decisions.md` D7 (rule of three not reached).
- Timestamp source: `SystemTime::now()` -> nanos since Unix epoch as `u64` -> `String`, mirroring `lumen_otlp_json.rs:142-146`.
- Test harness substrate: `SharedBuf(Arc<Mutex<Vec<u8>>>)` with a `Write` impl that locks and writes to the inner `Vec<u8>` — copy from `tests/lumen_to_otlp_json.rs:54-64`.
- Slice tag: `@infrastructure` (library-level; user-visible CLI dashboard wiring is a post-v0 feature).
- The `cinder` dependency in `crates/self-observe/Cargo.toml` was added by the Pulse-sink sibling; no new dep needed here.

### Dependencies

- `cinder` crate v0.1.0 (already shipped; `MetricsRecorder` trait stable).
- `aegis` crate (already a self-observe dependency; provides `TenantId`).
- `serde`, `serde_json` (already in `crates/self-observe/Cargo.toml` for the Lumen writer).
- No new external crates required.

### Slice

`slices/slice-01-place-events-emit-otlp-json-lines.md`

---

## US-02: Cinder `migrate` events emit one OTLP-JSON line per call with direction attributes

### Elevator Pitch

- **Before**: Priya needs to know from her cross-process dashboard how
  many Hot->Warm migrations tenant `acme` saw in the last hour. The CLI
  with `--observe-otlp <path>` produces zero `cinder.migrate.count`
  lines today (NoopRecorder). Even her in-process Pulse-sink sibling
  doesn't reach the cross-process collector. The question is
  unanswerable from her standard tooling.
- **After**: After the CLI follow-up wires the writer, every successful
  `cinder.migrate(&acme, &item, Tier::Warm, t)` (which carries
  `from=hot, to=warm` because Cinder knows the item's current tier)
  appends a line like
  `{"resource":...,"scopeMetrics":[{"scope":{"name":"kaleidoscope.cinder"},"metrics":[{"name":"cinder.migrate.count","sum":{...,"dataPoints":[{"attributes":[{"key":"from","value":{"stringValue":"hot"}},{"key":"to","value":{"stringValue":"warm"}}],...,"asInt":"1"}]}}]}]}`
  to the file. The sidecar forwards, the collector ingests, the
  dashboard counts the matching attribute combinations.
- **Decision enabled**: Priya can decide "is `acme`'s Hot->Warm
  migration rate consistent with the configured tier policy?" from her
  existing cross-process dashboard, without modifying Cinder source.

### Problem

Priya needs to see the **direction** of every tier migration per tenant
on her cross-process collector. A successful
`cinder.migrate(&acme, &item, Tier::Warm, t)` (called when the item was
Hot) carries the information `from=hot, to=warm` — but with
`NoopRecorder` that information dies. A failed migrate (the item was
never placed, returns `MigrateError::UnknownItem`) MUST NOT produce a
spurious line — otherwise the operator's collector cannot distinguish
real migrations from bookkeeping errors.

### Who

Priya the platform operator | wants direction-resolved migration counts
per tenant on her cross-process collector | wants failed migrations to
leave no trace in the NDJSON stream (`UnknownItem` is a caller bug,
not a tier event).

### Solution

The writer's `record_migrate(tenant, from, to)` method emits one
OTLP-JSON `ResourceMetrics` line per call: scope `kaleidoscope.cinder`,
metric `cinder.migrate.count` (Sum kind, `aggregationTemporality = 2`,
`isMonotonic = true`), one data point with `asInt = "1"`, point
attributes `from = lowercase(from)` AND `to = lowercase(to)`. Because
`cinder::InMemoryTieringStore::migrate` (and the FileBacked equivalent)
only calls `record_migrate` on success (see `crates/cinder/src/store.rs`
lines 174-188), a failed migrate naturally produces no writer call and
therefore no line.

### Domain Examples

#### 1. Happy path — Priya tracks a Hot->Warm migration on `acme`

Priya feeds input that triggers a place of `trade-2026-05-18-001` for
`acme` in Hot, then a migrate of the same item to Warm. The NDJSON file
gains two lines (one place, one migrate). The migrate line has metric
name `cinder.migrate.count`, `tenant_id="acme"` resource attribute, and
point attributes `{from: "hot", to: "warm"}`.

#### 2. Quiescence — failed migrate emits nothing

Priya feeds input that triggers `cinder.migrate(&acme, &item("ghost-item"), Tier::Warm, t)`
without first placing `ghost-item`. Cinder returns
`Err(MigrateError::UnknownItem)`. The NDJSON file gains ZERO new
`cinder.migrate.count` lines. The cross-process collector shows no
phantom migration.

#### 3. Per-tenant isolation under simultaneous opposite-direction migrations

Priya feeds input that places `a1` for `acme` in Hot and `g1` for
`globex` in Hot, then migrates `a1` to Warm and `g1` to Cold. The NDJSON
file gains two migrate lines: one with `tenant_id="acme"` and
`{from: hot, to: warm}`, one with `tenant_id="globex"` and
`{from: hot, to: cold}`. The wrong-tenant attributes do not bleed.

### UAT Scenarios (BDD)

#### Scenario: Successful migrate emits one line with direction attributes

```gherkin
Given Priya has wired the writer
And tenant acme has placed item("trade-2026-05-18-001") in Tier::Hot
When cinder.migrate(&acme, &item("trade-2026-05-18-001"), Tier::Warm, t1) returns Ok(())
Then the most recent line in the sink has metric name "cinder.migrate.count"
And that line's resource.attributes[0].value.stringValue equals "acme"
And that line's dataPoints[0].asInt equals "1"
And that line's dataPoints[0].attributes contains {key: "from", value: {stringValue: "hot"}}
And that line's dataPoints[0].attributes contains {key: "to",   value: {stringValue: "warm"}}
```

#### Scenario: Failed migrate (UnknownItem) emits no line

```gherkin
Given Priya has wired the writer
And tenant acme has placed nothing
When cinder.migrate(&acme, &item("ghost-item"), Tier::Warm, t1) returns Err(UnknownItem)
Then no line in the sink has metric name "cinder.migrate.count"
```

#### Scenario: Per-tenant isolation under simultaneous migrations

```gherkin
Given Priya has wired the writer
And tenant acme has placed item("a1") in Tier::Hot
And tenant globex has placed item("g1") in Tier::Hot
When cinder.migrate(&acme, &item("a1"), Tier::Warm, t) and cinder.migrate(&globex, &item("g1"), Tier::Cold, t) both succeed
Then exactly one line has resource.tenant_id="acme"  with metric "cinder.migrate.count" and attrs {from: hot, to: warm}
And  exactly one line has resource.tenant_id="globex" with metric "cinder.migrate.count" and attrs {from: hot, to: cold}
```

### Acceptance Criteria

- [ ] On every successful `record_migrate(tenant, from, to)` call, exactly one line is appended to the inner `W`, ending with `\n`, parseable as JSON.
- [ ] That JSON has `scopeMetrics[0].metrics[0].name = "cinder.migrate.count"`.
- [ ] That JSON has `scopeMetrics[0].metrics[0].sum.aggregationTemporality = 2` and `sum.isMonotonic = true`.
- [ ] That JSON has `scopeMetrics[0].metrics[0].sum.dataPoints[0].asInt = "1"`.
- [ ] That dataPoint's `attributes` contain `{key: "from", value: {stringValue: lowercase(from)}}` AND `{key: "to", value: {stringValue: lowercase(to)}}`.
- [ ] A failed migrate (Cinder returns `Err(UnknownItem)`) leaves zero new lines in the sink (Cinder does not invoke `record_migrate` on failure, which the writer inherits).
- [ ] Two-tenant isolation test passes: acme's and globex's simultaneous migrations produce two lines with distinct resource attributes and distinct direction attributes.

### Outcome KPIs

- **Who**: platform operator (Priya), measured at the library contract level
- **Does what**: receives a parseable OTLP-JSON line with metric `cinder.migrate.count`, `from`/`to` point attributes, per successful `migrate` call
- **By how much**: 100% of successful migrate calls produce one correct line; 0% of failed migrates produce any line
- **Measured by**: green tests in `crates/self-observe/tests/cinder_to_otlp_json.rs` (Slice 02 block)
- **Baseline**: 0% (NoopRecorder)

Maps to OK2 in `outcome-kpis.md`.

### Technical Notes

- Adds one method body to `CinderToOtlpJsonWriter` (the struct + serde structs are introduced in US-01).
- No new file, no new dependency beyond US-01.
- The lowercase-tier helper from US-01 is reused for both `from` and `to` serialisation.
- The point-attribute array in US-01 has a single entry (`tier`); the migrate path needs TWO entries (`from`, `to`) — DESIGN wave decides whether to parameterise the array size or introduce a per-metric serde struct. Either choice is acceptable as long as the output JSON matches the AC.
- Slice tag: `@infrastructure`.

### Dependencies

- US-01 (the writer struct + the lowercase-tier helper + the Mutex<W> pattern + the line-emission function).

### Slice

`slices/slice-02-migrate-events-emit-otlp-json-lines-with-direction.md`

---

## US-03: Cinder `evaluate` events emit one OTLP-JSON line per (tenant, evaluate-call) with per-tenant migrated count

### Elevator Pitch

- **Before**: Priya runs a periodic `cinder.evaluate_at(now, &policy)`
  in her tier-management loop (in the CLI's batched ingest path, this
  is implicit when items age out). With `NoopRecorder` she has no way
  to answer "did the last evaluate run actually migrate anything for
  `acme`?" from her cross-process collector, except by counting
  individual `cinder.migrate.count` lines and grouping them — which
  is a dashboard exercise, not a metric.
- **After**: After the CLI follow-up wires the writer, every
  `evaluate_at` call that migrates at least one item for tenant `acme`
  appends a line with metric `cinder.evaluate.migrated.count`,
  `tenant_id="acme"`, `asInt = "<count>"` to the NDJSON file. The
  collector exposes the aggregate directly; the dashboard shows one
  number per evaluate run per tenant. The dual emission (per-item
  migrate lines AND per-tenant evaluate line) remains visible so the
  operator can cross-check totals.
- **Decision enabled**: Priya can decide "did the last hourly evaluate
  run produce the expected migration volume for `acme`?" via a single
  collector query, without manually aggregating per-item migrate lines.

### Problem

The `evaluate_at` operation is the periodic policy-driven migration
trigger. It is fundamentally **per-tenant aggregate**: one call sweeps
the store and may migrate dozens of items across multiple tenants. The
metric Priya needs is **per (tenant, evaluate-call)**, not per item.
AND the per-item migrations themselves need to remain visible as
`cinder.migrate.count` lines (US-02), so the writer must not
deduplicate or merge.

Additionally, `cinder::InMemoryTieringStore::evaluate_at` does NOT call
`record_evaluate` for tenants with zero migrations (see `store.rs` lines
218-230 — only tenants present in the `per_tenant` map after migration
get a `record_evaluate` call). This is upstream behaviour; the writer
inherits it without modification, so zero-migration tenants produce
zero `cinder.evaluate.migrated.count` lines in the NDJSON stream.

### Who

Priya the platform operator | wants per-tenant per-evaluate aggregated
migration counts visible on her cross-process collector | wants the dual
emission (per-item migrate + per-tenant evaluate) to remain visible and
unsurprising for cross-check | does NOT want ghost evaluate lines for
tenants that had nothing eligible to migrate.

### Solution

The writer's `record_evaluate(tenant, migrated)` method emits one
OTLP-JSON `ResourceMetrics` line per call: scope `kaleidoscope.cinder`,
metric `cinder.evaluate.migrated.count` (Sum kind,
`aggregationTemporality = 2`, `isMonotonic = true`), one data point with
`asInt = migrated.to_string()`. The point's `attributes` contain only
the `tenant_id` (no other point attribute — the value carries the
count, not an attribute). The per-item `record_migrate` calls that
Cinder also makes from `evaluate_at` are handled by the US-02 path and
produce normal `cinder.migrate.count` lines in the SAME NDJSON sink.

### Domain Examples

#### 1. Happy path — `acme` has 5 items eligible for Hot->Warm migration

Priya runs the CLI against input that triggers 5 placements for `acme`
in Hot at `t0`. The policy is "Hot items older than 24h migrate to
Warm." The CLI's evaluate runs at `t0 + 25h`. The NDJSON sink gains:

- 5 lines with metric `cinder.migrate.count` for `acme`, each with
  `{from: hot, to: warm}` point attributes and `asInt = "1"`.
- 1 line with metric `cinder.evaluate.migrated.count` for `acme`,
  `asInt = "5"`.

Total: 6 new lines for this tenant from this evaluate call. The
collector shows the aggregate as a single time-series point on
`cinder.evaluate.migrated.count`; the operator can drill into the
per-item lines on `cinder.migrate.count` if needed.

#### 2. Zero-eligible tenant — no evaluate line emitted

Priya runs the CLI against input that triggers 3 placements for `acme`
in Hot at `t0`. The CLI's evaluate runs at `t0 + 1h` (before the 24h
threshold). Cinder migrates nothing. The NDJSON sink gains ZERO new
lines for `cinder.evaluate.migrated.count` for `acme` AND ZERO new
lines for `cinder.migrate.count` for `acme`. The dashboard shows no
phantom data points.

#### 3. Mixed tenants in one evaluate — per-tenant counts split correctly

Priya runs the CLI against input that triggers 5 placements for `acme`
in Hot at `t0` and 2 placements for `globex` in Hot at `t0`. The CLI's
evaluate runs at `t0 + 25h`. The NDJSON sink gains:

- 5 `cinder.migrate.count` lines with `tenant_id="acme"`.
- 2 `cinder.migrate.count` lines with `tenant_id="globex"`.
- 1 `cinder.evaluate.migrated.count` line with `tenant_id="acme"` and `asInt = "5"`.
- 1 `cinder.evaluate.migrated.count` line with `tenant_id="globex"` and `asInt = "2"`.

The collector groups by `tenant_id` and reports the per-tenant aggregate
directly.

### UAT Scenarios (BDD)

#### Scenario: Evaluate that migrates N items for one tenant emits N migrate lines AND 1 evaluate line

```gherkin
Given Priya has wired the writer
And tenant acme has placed 5 items in Tier::Hot at t0
And the tier policy migrates Hot items older than 24h to Warm
When cinder.evaluate_at(t0 + 25h, &policy) is called
Then cinder.evaluate_at returns 5
And exactly 5 lines in the sink have metric name "cinder.migrate.count" under tenant acme with attrs {from: hot, to: warm}
And exactly 1 line  in the sink has  metric name "cinder.evaluate.migrated.count" under tenant acme
And that evaluate line's dataPoints[0].asInt equals "5"
```

#### Scenario: Evaluate with zero eligible items emits no evaluate line for that tenant

```gherkin
Given Priya has wired the writer
And tenant acme has placed 3 items in Tier::Hot at t0
And the tier policy migrates Hot items older than 24h to Warm
When cinder.evaluate_at(t0 + 1h, &policy) is called
Then cinder.evaluate_at returns 0
And no line in the sink has metric name "cinder.evaluate.migrated.count" under tenant acme
And no line in the sink has metric name "cinder.migrate.count"          under tenant acme
```

#### Scenario: Two-tenant evaluate emits per-tenant evaluate lines

```gherkin
Given Priya has wired the writer
And tenant acme has placed 5 items in Tier::Hot at t0
And tenant globex has placed 2 items in Tier::Hot at t0
And the tier policy migrates Hot items older than 24h to Warm
When cinder.evaluate_at(t0 + 25h, &policy) is called
Then exactly 1 line has resource.tenant_id="acme"   with metric "cinder.evaluate.migrated.count" and asInt "5"
And  exactly 1 line has resource.tenant_id="globex"  with metric "cinder.evaluate.migrated.count" and asInt "2"
And  exactly 5 lines have resource.tenant_id="acme"   with metric "cinder.migrate.count"
And  exactly 2 lines have resource.tenant_id="globex"  with metric "cinder.migrate.count"
```

### Acceptance Criteria

- [ ] On every `record_evaluate(tenant, migrated)` call, exactly one line is appended to the inner `W`, ending with `\n`, parseable as JSON.
- [ ] That JSON has `scopeMetrics[0].metrics[0].name = "cinder.evaluate.migrated.count"`.
- [ ] That JSON has `scopeMetrics[0].metrics[0].sum.dataPoints[0].asInt = migrated.to_string()` (NOT `"1"`).
- [ ] That dataPoint's `attributes` contain the `tenant_id` point attribute (mirroring the Lumen writer's double-emission of tenant_id).
- [ ] Tenants with zero migrations in a given `evaluate_at` call produce zero `cinder.evaluate.migrated.count` lines (Cinder does not call `record_evaluate` for them; the writer inherits this).
- [ ] The per-item `cinder.migrate.count` lines from US-02 remain emitted by the same `evaluate_at` call (the dual-emission contract is preserved in the SAME NDJSON sink).
- [ ] Multi-tenant `evaluate_at` produces per-tenant evaluate lines with `asInt` equal to that tenant's individual migration count rendered as a string.

### Outcome KPIs

- **Who**: platform operator (Priya), measured at the library contract level
- **Does what**: receives a parseable OTLP-JSON line with metric `cinder.evaluate.migrated.count`, `asInt` equal to the per-tenant migrated count, per (tenant, evaluate-call) pair with at least one migration
- **By how much**: 100% of qualifying (tenant, evaluate) pairs produce one line with `asInt = N.to_string()`; 0% of zero-migration pairs produce a line
- **Measured by**: green tests in `crates/self-observe/tests/cinder_to_otlp_json.rs` (Slice 03 block)
- **Baseline**: 0% (NoopRecorder)

Maps to OK3 in `outcome-kpis.md`.

### Technical Notes

- Adds the third method body to `CinderToOtlpJsonWriter`.
- The `migrated.to_string()` rendering is exact for any usize (OTLP-JSON encodes uint64 as a string, no precision loss).
- The dual-emission test in this slice is the highest-information-density test in the suite — DESIGN wave should preserve the test's cross-event-type assertion shape (count migrate lines AND count evaluate lines from one `evaluate_at` call against the same sink).
- Slice tag: `@infrastructure`.

### Dependencies

- US-01 (writer struct + envelope serde structs + scope name + line-emission function + `Mutex<W>` pattern).
- US-02 (the migrate emission path — the dual-emission test in this slice cross-asserts both metrics).

### Slice

`slices/slice-03-evaluate-events-emit-otlp-json-lines-with-per-tenant-counts.md`
