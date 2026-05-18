# KPI Instrumentation — `cinder-to-otlp-json-bridge-v0`

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-18
- **Source-of-truth for KPIs**: `docs/feature/cinder-to-otlp-json-bridge-v0/discuss/outcome-kpis.md`

## Why this document is short

The DISCUSS-wave outcome-kpis.md is explicit: this is a library-only
feature; the operator persona (Priya) cannot directly exercise the
writer without the post-v0 CLI wiring feature (the `--observe-otlp
<path>` flag must be extended to also wire the Cinder writer; today
it wires only the Lumen writer per commits `c6b336c` and `3af7e82`).
Therefore the "behaviour change" KPIs land at the **library contract
level**, measured through acceptance tests, not through runtime
observability dashboards or production alerting.

There is no Grafana dashboard, no Prometheus alert, no log-stream
aggregation, no error-rate SLO, and no synthetic monitor for this
feature. The CI pipeline IS the measurement instrument. A failing
acceptance test IS the alert. The git-history-of-green is the
historical-measurement record.

This document maps each KPI from outcome-kpis.md to:

1. The collection method (which CI artefact emits the signal).
2. The data path (how the signal reaches a person who can act on it).
3. The alerting rule (what condition triggers human attention).
4. The dashboard surface (what summary view answers "is this KPI
   green?").

For a library-only feature with no deployed runtime, items (3) and (4)
collapse to "CI run status" and "CI run history" respectively.

## Per-KPI design

### OK1 — `cinder.place.count` per `place` call (OTLP-JSON line)

> 100% of `place` calls produce exactly one parseable OTLP-JSON
> ResourceMetrics line with metric name `cinder.place.count`, scope
> `kaleidoscope.cinder`, per-tenant resource attribute, and `tier`
> point attribute; no drops, no duplicates, no shape deviation.

| Field | Value |
|-------|-------|
| Type | Leading (library contract) |
| Baseline | 0% (the CLI's Cinder recorder is `NoopRecorder` today — emits nothing) |
| Target | 100% (every `place` call → one valid OTLP-JSON line on the sink) |
| Data source | `crates/self-observe/tests/cinder_to_otlp_json.rs` — Slice 01 tests |
| Collection method | `cargo test --workspace --all-targets --locked` exit code (Gate 1) |
| CI gate enforcing | Gate 1 — `cargo test` (line 182 of `.github/workflows/ci.yml`) |
| Collection frequency | At every commit affecting the workspace (push or PR) |
| Owner | self-observe crate maintainer (Andrea); enforced by CI |
| Data path | Test pass/fail → workflow run status → GitHub commit status check → PR merge gate / push-to-main alert |
| Alerting rule | Workflow run = "failure" on Gate 1 job → GitHub email/web notification to the commit author |
| Dashboard surface | GitHub Actions "All workflows" view, filter by branch `main`. Green/red per-commit history is the time-series. Mutation-testing artefact (`mutants-out-self-observe`, pre-existing from the Pulse-sink sibling's DISTILL commit) supplements: a surviving mutant on `cinder_otlp_json.rs::record_place` would be a flag that OK1's measurement is weaker than claimed. |
| Acceptance-test scenarios (from BDD) | "Place produces one OTLP-JSON line per call, partitioned per tenant, with tier point attribute" — Slice 01 acceptance scenarios |
| What instrumentation looks like in the test | After one `cinder.place(&tenant("acme"), &item("trade-001"), Tier::Hot, SystemTime::now())` call: `collect_lines(&buf)` returns exactly one `serde_json::Value`; `lines[0]["resourceMetrics"][0]["scopeMetrics"][0]["scope"]["name"] == "kaleidoscope.cinder"`; `lines[0]["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][0]["name"] == "cinder.place.count"`; resource attributes contain `tenant_id == "acme"`; point attributes contain `tenant_id == "acme"` AND `tier == "hot"`. |
| Forward-compatible note for post-v0 CLI feature | The acceptance-test substrate (`SharedBuf(Arc<Mutex<Vec<u8>>>)`) IS the same `W: Write + Send + Sync` API that the CLI follow-up uses with a real `std::fs::File` opened via `--observe-otlp <path>`. The library contract is the unit; the CLI is the surface; the operator-behaviour KPI (OK1-CLI in outcome-kpis.md) is necessarily downstream. |

### OK2 — `cinder.migrate.count` per successful `migrate` call (OTLP-JSON line)

> 100% of successful `migrate` calls produce exactly one parseable
> OTLP-JSON line with metric name `cinder.migrate.count`, `from` and
> `to` point attributes matching the call arguments; 0% of failed
> (`UnknownItem`) calls produce any line.

| Field | Value |
|-------|-------|
| Type | Leading (library contract) |
| Baseline | 0% (NoopRecorder) |
| Target | 100% on success path + 0% on failure path (the negative-case probe) |
| Data source | `crates/self-observe/tests/cinder_to_otlp_json.rs` — Slice 02 tests |
| Collection method | Same as OK1 (Gate 1, `cargo test`) |
| CI gate enforcing | Gate 1 |
| Collection frequency | Every commit |
| Owner | self-observe crate maintainer; enforced by CI |
| Data path | Same as OK1 |
| Alerting rule | Same as OK1 |
| Dashboard surface | Same as OK1 |
| Acceptance-test scenarios | "Successful migrate emits one OTLP-JSON line with from+to attributes" + "Failed migrate (UnknownItem) emits zero lines" — Slice 02 acceptance scenarios |
| What instrumentation looks like in the test | After `cinder.migrate(&tenant, &item, Tier::Hot, Tier::Warm, t_now).unwrap()`: `collect_lines(&buf)` returns exactly one line with `metrics[0]["name"] == "cinder.migrate.count"` and point attributes containing both `from == "hot"` and `to == "warm"`. After `cinder.migrate(&tenant, &unknown_item, ...)` returning `Err(UnknownItem)`: `collect_lines(&buf)` returns zero lines for the migrate event (or, if a prior successful migrate exists in the same test, the previous count is unchanged). The negative case is the harder probe — it tests that the writer is NOT invoked by Cinder on the failure path (a contract inherited from `crates/cinder/src/store.rs:174-188` and structurally identical to the Pulse-sink sibling's OK2). |

### OK3 — `cinder.evaluate.migrated.count` per `evaluate_at` with migrations (OTLP-JSON line + dual-emission)

> 100% of (tenant, evaluate) pairs with N>=1 migrations produce
> exactly one OTLP-JSON line with `asInt = N.to_string()`; 0% of
> (tenant, evaluate) pairs with 0 migrations produce a line. The
> dual-emission contract from DISCUSS D8 also holds: one `evaluate_at`
> call producing N migrations emits N `cinder.migrate.count` lines
> AND 1 `cinder.evaluate.migrated.count` line, interleaved in the
> same byte stream per tenant per Cinder's `evaluate_at` cascade.

| Field | Value |
|-------|-------|
| Type | Leading (library contract) — dual-emission probe |
| Baseline | 0% (NoopRecorder) |
| Target | Conditional emission: line IFF N>=1; if emitted, `asInt = N.to_string()` (per DISCUSS D4, per ADR-0039 §2) |
| Data source | `crates/self-observe/tests/cinder_to_otlp_json.rs` — Slice 03 tests |
| Collection method | Same as OK1 (Gate 1, `cargo test`) |
| CI gate enforcing | Gate 1 |
| Collection frequency | Every commit |
| Owner | self-observe crate maintainer; enforced by CI |
| Data path | Same as OK1 |
| Alerting rule | Same as OK1 |
| Dashboard surface | Same as OK1 — with the explicit note that Slice 03's dual-emission test is the HIGHEST-INFORMATION-DENSITY probe in the suite (one `evaluate_at` call asserts BOTH the per-item `cinder.migrate.count` lines AND the per-tenant `cinder.evaluate.migrated.count` line in the same captured byte stream). A Slice 03 failure can indicate a regression in EITHER the writer OR Cinder's `InMemoryTieringStore::evaluate_at` cascade — diagnose from Cinder's own in-tree tests (they will go red first on a Cinder-side regression). |
| Acceptance-test scenarios | "evaluate_at with N>=1 migrations emits one `cinder.evaluate.migrated.count` line with `asInt = N.to_string()` AND N `cinder.migrate.count` lines" + "evaluate_at with 0 migrations emits zero lines" — Slice 03 acceptance scenarios |
| What instrumentation looks like in the test | After `cinder.evaluate_at(&t_now, &policy_promoting_3_items_for_acme)`: `collect_lines(&buf)` returns 4 lines for acme (3 `cinder.migrate.count` + 1 `cinder.evaluate.migrated.count` with `asInt == "3"`); for a tenant with 0 migrations, returns 0 lines. The conjunction is the dual-emission contract from DISCUSS D8. The `asInt` string is asserted exactly (e.g. `lines[3]["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0]["asInt"] == "3"`) per ADR-0039 §2 row "record_evaluate". |

### OK4 — Cinder API behaviour unchanged (guardrail)

> Zero observable change in Cinder's user-facing API behaviour when
> `CinderToOtlpJsonWriter` is substituted for `NoopRecorder`.

| Field | Value |
|-------|-------|
| Type | Guardrail |
| Baseline | n/a (baseline = current NoopRecorder behaviour) |
| Target | Zero behavioural change at Cinder's API boundary |
| Data source | The acceptance tests themselves: every test calls Cinder identically to its NoopRecorder usage and asserts on Cinder's return values (`Result<...>` types) as well as the captured OTLP-JSON byte stream |
| Collection method | (a) Gate 1 (`cargo test`) — failing assertions on Cinder return values catch behavioural drift; (b) code review at DESIGN handoff + DELIVER review |
| CI gate enforcing | Gate 1 (behavioural assertions on Cinder return values) |
| Collection frequency | Every commit (Gate 1); every wave-close review (code review) |
| Owner | self-observe crate maintainer (Gate 1); Reviewer (code review) |
| Data path | Test pass/fail → workflow status (same as OK1). Code review path: nWave wave-close review surfaces any test that calls Cinder DIFFERENTLY from its NoopRecorder usage, fails the review. |
| Alerting rule | (a) Gate 1 failure on a Cinder-return-value assertion → standard CI alert; (b) reviewer flagging a non-mirror test pattern → review-block. |
| Dashboard surface | Same Gate 1 surface as OK1/OK2/OK3. Code-review record persists in the PR thread / wave-decisions.md "Hand-off" section. |
| Acceptance-test pattern | Every test in `tests/cinder_to_otlp_json.rs` follows the shape `let buf = SharedBuf::new(); let writer = CinderToOtlpJsonWriter::new(buf.clone()); let cinder = InMemoryTieringStore::new(Box::new(writer)); cinder.<method>(...)` — i.e., Cinder is constructed exactly as it would be with NoopRecorder, only the recorder swap is different. Assertions on Cinder's return values precede assertions on the captured byte stream. |
| What instrumentation looks like in the test | `assert_eq!(cinder.migrate(&tenant, &unknown_item, ...), Err(MigrateError::UnknownItem))` followed by `assert_eq!(collect_lines(&buf).len(), 0)` (or "unchanged from prior count"). The first assertion defends OK4; the second defends OK2 (negative case). The two assertions share one test body to lock the simultaneity of the two contracts. |

### OK5 — NDJSON-validity guardrail (cross-writer-safe byte stream)

> 100% of lines in the captured byte stream parse as JSON; the
> stream ends with `\n`; the per-line atomicity holds even when the
> writer is invoked three times in succession in the same thread (and
> by inheritance, holds across writers when the post-v0 CLI feature
> shares one file between the Lumen and Cinder OTLP-JSON writers).

| Field | Value |
|-------|-------|
| Type | Guardrail |
| Baseline | n/a (today only the Lumen writer emits to the cross-process NDJSON stream) |
| Target | 100% of emitted lines are independently parseable as JSON; the byte stream ends with `\n` after every emission; no partial-line truncation, no interleaved bytes across emissions within one thread |
| Data source | `crates/self-observe/tests/cinder_to_otlp_json.rs` — Slice 01 includes the "buffer ends with `\n`" assertion and the "split-on-`\n` yields N parseable JSON lines" assertion. Slices 02 and 03 inherit the same invariants by using the same `SharedBuf` substrate (every test's final assertion includes the line-termination check). |
| Collection method | Same as OK1 (Gate 1, `cargo test`) |
| CI gate enforcing | Gate 1 (the NDJSON-validity assertions are part of every test body that emits) |
| Collection frequency | Every commit |
| Owner | self-observe crate maintainer; enforced by CI |
| Data path | Same as OK1 |
| Alerting rule | Same as OK1 — a Slice 01 NDJSON-termination failure is the substrate-lie probe per ADR-0039 §3, Principle 12c behavioural-check layer. If this assertion fails, the `Mutex<W>` + `write_all(body) + write_all(b"\n") + flush()` triple is broken; investigate the writer's `emit` helper FIRST before suspecting test substrate. |
| Dashboard surface | Same Gate 1 surface as OK1/OK2/OK3/OK4. |
| Acceptance-test scenarios | "Output is NDJSON: one line per event, each terminated by `\n`, all lines parseable as JSON via `serde_json::from_str`" — Slice 01 ndjson-validity test, inherited by Slices 02/03 |
| What instrumentation looks like in the test | After three `cinder.place(...)` calls in sequence: `buf.lock().unwrap().last().copied() == Some(b'\n')` (stream ends with `\n`); `String::from_utf8(buf.lock().unwrap().clone()).unwrap().split('\n').filter(|s| !s.is_empty()).count() == 3` (exactly three lines); each line parses cleanly via `serde_json::from_str::<serde_json::Value>(line).unwrap()`. |
| Cross-writer note (deferred to post-v0 CLI feature) | The cross-writer NDJSON-validity invariant (Lumen and Cinder writers sharing one real `std::fs::File`) is the CLI follow-up feature's tests' responsibility, NOT this feature's. The Lumen OTLP-JSON writer's identical `Mutex<W>` + atomic-triple pattern has already been exercised against a real `File` in production (commits `c6b336c` and `3af7e82`); the Cinder OTLP-JSON writer inherits that substrate confidence. The within-writer invariant (this feature's OK5) is the necessary preliminary; the cross-writer invariant is the dependent KPI on the CLI follow-up. |

## Cross-KPI considerations

### Why mutation testing is part of OK1/OK2/OK3/OK5's measurement, not separate

A green Gate 1 with poor test quality is a false positive on OK1/OK2/
OK3 and OK5. Gate 5 (mutation testing, per-feature, 100% kill rate
per ADR-0005 + CLAUDE.md) is the test-quality probe: a surviving
mutant on `cinder_otlp_json.rs` means the test suite cannot
distinguish the unmutated writer from a behaviourally-different
writer. That is a gap in the KPI measurement itself, not a separate
quality concern.

Treatment: surviving mutants in `cinder_otlp_json.rs` fail Gate 5
(`gate-5-mutants-self-observe`, pre-existing from the Pulse-sink
sibling's DISTILL commit; this feature inherits the job per
wave-decisions.md A3). The DELIVER wave's responsibility (per
CLAUDE.md per-feature MT strategy) is to turn each slice's mutants
100% killed before review approval. Mutations of particular interest
to surface review attention:

- The `tier_lowercase` match arms (mutating `"hot"` → `""` should
  fail Slice 01's tier-attribute assertion).
- The `migrated.to_string()` call in `record_evaluate` (mutating
  the conversion should fail Slice 03's `asInt` exact-string
  assertion).
- The `\n` byte literal in the `write_all(b"\n")` call (mutating
  to `write_all(b"")` should fail Slice 01's NDJSON-termination
  assertion — OK5's substrate-lie probe).
- The `flush()` call inside the `Mutex<W>` critical section
  (removing it should fail the in-memory `SharedBuf` reads in
  Slices 01/02/03, because `Vec<u8>::flush` is a no-op but the
  contract is preserved).

### Why no separate "test-coverage threshold" KPI

OK1/OK2/OK3 are 100% binary (every event type produces its
documented OTLP-JSON line), not statistical. OK5 is also binary
(every emitted line is JSON-parseable and the stream ends with `\n`).
A line-coverage percentage would be a weaker signal than the per-
event contract assertions. The mutation kill rate (Gate 5) is the
supplemental quality signal; line coverage is not part of this
feature's KPI set.

### Why no separate "performance / latency" KPI

The library-side performance is dominated by one `Vec<OtlpAttr>`
allocation per event (≤3 entries), one `serde_json::to_string` call
(linear in line size), and one `Mutex<W>` acquisition with one to
three `write_all` calls inside the critical section (per
`application-architecture.md > Performance Efficiency`). This is
operationally negligible relative to the operator-visible signal-
shape time scale. A latency KPI would be a vanity metric at v0;
outcome-kpis.md explicitly defers operator-time-to-answer to
the post-v0 CLI feature (OK1-CLI, OK2-CLI, OK3-CLI). No v0
instrumentation needed.

### Why no separate "deployment frequency" / DORA metric KPI

The writer has no deployment. DORA metrics (per `platform-
engineering-foundations` skill) apply to deployed services. For a
library, the closest analog is "merge frequency to main", which is
trivially measured by `git log` and does not warrant a dashboard.

### Cross-bridge alignment with the Pulse-sink sibling

OK1/OK2/OK3 here are structurally identical to OK1/OK2/OK3 in
`docs/feature/cinder-to-pulse-bridge-v0/discuss/outcome-kpis.md` —
same three Cinder events, same metric names (per the cross-bridge
contract DISCUSS D1), same per-event attribute schema, same dual-
emission contract on Slice 03. The only difference is the
observation surface (Pulse `MetricStore::query` for the sibling;
parsed `serde_json::Value` from a byte stream here). OK4 is also
shared with the sibling (identical guardrail). OK5 is NEW for this
feature — it is the NDJSON-validity guardrail that the byte-stream
sink imposes but the in-process Pulse sink does not.

## Post-v0 instrumentation (deferred)

When the CLI follow-up feature (e.g.
`kaleidoscope-cli-wires-cinder-otlp-bridge-v0`, or merged with the
Pulse CLI wiring feature) ships, these become measurable:

- **OK1-CLI**: Number of `cinder.*` time series visible on Priya's
  cross-process collector dashboard 60 seconds after a CLI ingest
  invocation with `--observe-otlp <path>`. Target: 3 (one per
  metric name). Baseline: 0.
- **OK2-CLI**: Operator-reported "I see the cinder.* metrics on my
  existing dashboard without touching the sidecar or collector" in
  post-ship survey. Target: 100%. Baseline: 0%.
- **OK3-CLI**: Time-to-first-answer for "did the last `evaluate_at`
  migrate anything for `acme`?" via the cross-process dashboard.
  Target: <30 seconds. Baseline: N/A (today the question is
  unanswerable from the cross-process tool chain because the data
  does not exist there).

Both KPIs require operator-facing instrumentation that does not
exist at the library layer. This feature's KPI design does NOT
include them; the future CLI feature's DEVOPS wave will design the
collection method (likely a CLI invocation timer + a user research
mailout, respectively).

## Summary table — KPI to CI gate mapping

| KPI | What it measures | CI gate enforcing | Test file(s) |
|-----|------------------|-------------------|--------------|
| OK1 | `cinder.place.count` OTLP-JSON library contract | Gate 1 (cargo test) | `tests/cinder_to_otlp_json.rs` — Slice 01 block |
| OK2 | `cinder.migrate.count` OTLP-JSON library contract (success + failure paths) | Gate 1 | `tests/cinder_to_otlp_json.rs` — Slice 02 block |
| OK3 | `cinder.evaluate.migrated.count` + dual-emission contract | Gate 1 | `tests/cinder_to_otlp_json.rs` — Slice 03 block |
| OK4 | Cinder API behaviour unchanged (guardrail) | Gate 1 (assertions on Cinder return values) + code review at DESIGN/DELIVER handoff | `tests/cinder_to_otlp_json.rs` — every test body |
| OK5 | NDJSON-validity (byte-stream guardrail) | Gate 1 (Slice 01 ndjson-termination test, inherited by Slices 02/03 via same SharedBuf substrate) | `tests/cinder_to_otlp_json.rs` — Slice 01 ndjson-validity test |
| (supplementary) | Test-suite quality / mutation kill rate | Gate 5 (`gate-5-mutants-self-observe`, pre-existing from Pulse-sink sibling per A3) | Mutations of `crates/self-observe/src/cinder_otlp_json.rs` |

Every KPI from outcome-kpis.md has a CI gate. Every CI gate has a
single named owner (Gate 1: workspace maintainer; Gate 5: same).
Every alert path is the existing GitHub Actions email/web
notification surface. Zero new tooling, zero new dashboards, zero
new on-call rotations, zero new CI workflow files.
