# Outcome KPIs — `cinder-to-otlp-json-bridge-v0`

## Feature

`cinder-to-otlp-json-bridge-v0` — writer that implements
`cinder::MetricsRecorder` and emits each tier event as one line of
OTLP-JSON `ResourceMetrics` to a generic `Write`. Cross-process sibling
of `CinderToPulseRecorder`. OTLP-JSON sibling of
`LumenToOtlpJsonWriter`.

## Objective

Complete the cross-process platform observability picture. Today the CLI
ingest path with `--observe-otlp <path>` produces a sidecar-consumable
NDJSON stream containing **only** `kaleidoscope.lumen` lines. After this
feature ships AND the CLI follow-up wires it in, that same stream
contains BOTH `kaleidoscope.lumen` AND `kaleidoscope.cinder` lines, so
a single sidecar + a single OTLP/HTTP collector + a single dashboard
show the full platform.

## Note on KPI granularity at v0

This feature is library-only. The operator persona (Priya) cannot
directly exercise the writer without the post-v0 CLI wiring feature.
Therefore the "behaviour change" KPIs land at the **library contract
level**, measured through acceptance tests and through the post-v0 CLI
follow-up feature's adoption.

The KPIs below distinguish:

- **Now-measurable (acceptance-test level)**: 100% of the Cinder event
  types produce one OTLP-JSON `ResourceMetrics` line per event with the
  documented shape. Measured by green tests in
  `crates/self-observe/tests/cinder_to_otlp_json.rs` at the close of
  DELIVER.
- **Post-v0-measurable (operator behaviour)**: completeness of the
  cross-process collector dashboard, measured when the CLI follow-up
  ships. The writer is the substrate; the CLI is the surface; the
  operator-behaviour metric is necessarily downstream.

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| OK1 | Platform operator (Priya) — at the library contract level via the acceptance harness | Receives a parseable OTLP-JSON `ResourceMetrics` line with metric name `cinder.place.count`, scope `kaleidoscope.cinder`, per-tenant resource attribute, and `tier` point attribute per `place` call | 100% of `place` calls produce exactly one valid line (no drops, no duplicates, no shape deviation) | 0% (the CLI's Cinder recorder is `NoopRecorder` today — emits nothing) | Acceptance tests Slice 01 | Leading (library contract) |
| OK2 | Platform operator (Priya) | Receives a parseable OTLP-JSON line with metric name `cinder.migrate.count`, `from` and `to` point attributes, per successful `migrate` call | 100% of successful `migrate` calls produce exactly one valid line with attributes matching the call arguments; 0% of failed (`UnknownItem`) calls produce any line | 0% (NoopRecorder) | Acceptance tests Slice 02 | Leading (library contract) |
| OK3 | Platform operator (Priya) | Receives a parseable OTLP-JSON line with metric name `cinder.evaluate.migrated.count`, `asInt` equal to the per-tenant migrated count, per (tenant, evaluate_at call) pair where at least one item was migrated for that tenant | 100% of (tenant, evaluate) pairs with N>=1 migrations produce exactly one line with `asInt = N.to_string()`; 0% of (tenant, evaluate) pairs with 0 migrations produce a line | 0% (NoopRecorder) | Acceptance tests Slice 03 | Leading (library contract) |
| OK4 | Cinder operations in production | Continue to run with identical behaviour and identical error semantics when `CinderToOtlpJsonWriter` is substituted for `NoopRecorder` | Zero observable change in Cinder's user-facing API behaviour (same return values, same error variants, same timing within `O(n)` noise floor) | n/a (baseline = current Noop behaviour) | All slices: every test calls the Cinder API the same way it would with `NoopRecorder`, and asserts unchanged Cinder return values where applicable | Guardrail |
| OK5 | The NDJSON stream as a whole (when Lumen + Cinder writers share one file) | Remains valid as a sequence of independently-parseable JSON lines, each terminated by `\n`, regardless of which writer contributed any given line | 100% of lines parse as JSON; the stream ends with `\n` | n/a (today only Lumen writes) | Slice 01 includes a "buffer ends with `\n`" assertion and a "split-on-`\n` yields N parseable lines" assertion. Cross-writer interleaving against a real `File` is owned by the CLI follow-up feature's tests, NOT this feature. | Guardrail |

## Metric Hierarchy

- **North Star (v0 library scope)**: "Every Cinder event type produces
  one OTLP-JSON ResourceMetrics line with the documented shape on the
  configured `Write` sink." Measured via 100% green acceptance tests in
  `crates/self-observe/tests/cinder_to_otlp_json.rs`.
- **Leading Indicators**: OK1, OK2, OK3 above — each event type's
  emission contract.
- **Guardrail Metrics**:
  - OK4: Cinder's user-facing behaviour does not change.
  - OK5: NDJSON validity holds for the stream as a whole.

## Cross-bridge alignment

The library-contract KPIs (OK1/OK2/OK3) are structurally identical to
the Pulse-sink sibling's OK1/OK2/OK3 in
`docs/feature/cinder-to-pulse-bridge-v0/discuss/outcome-kpis.md`. The
difference is the sink:

| KPI | Pulse-sink sibling | This feature |
|-----|---------------------|--------------|
| OK1 | one queryable Pulse `MetricPoint` per `place` | one parseable OTLP-JSON line per `place` |
| OK2 | one queryable Pulse `MetricPoint` per successful `migrate` | one parseable OTLP-JSON line per successful `migrate` |
| OK3 | one queryable Pulse `MetricPoint` per (tenant, evaluate) | one parseable OTLP-JSON line per (tenant, evaluate) |

This alignment is intentional and reinforces the cross-bridge metric-
name contract (`wave-decisions.md` D1).

## Post-v0 outcome KPIs (deferred to the CLI follow-up feature)

When the CLI follow-up feature (e.g.
`kaleidoscope-cli-wires-cinder-otlp-bridge-v0`, or merged with the Pulse
CLI wiring feature) ships, these become measurable. Listed here for
traceability:

- **OK1-CLI**: Number of `cinder.*` time series visible on Priya's
  cross-process collector dashboard 60 seconds after a CLI ingest
  invocation with `--observe-otlp <path>`. Target: 3 (one per metric
  name). Baseline: 0.
- **OK2-CLI**: Operator-reported "I see the cinder.* metrics on my
  existing dashboard without touching the sidecar or collector" in
  post-ship survey. Target: 100%. Baseline: 0%.
- **OK3-CLI**: Time-to-first-answer for "did the last `evaluate_at`
  migrate anything for `acme`?" via the cross-process dashboard. Target:
  <30 seconds. Baseline: N/A (today the question is unanswerable from
  the cross-process tool chain because the data does not exist there).

## Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| OK1 | `crates/self-observe/tests/cinder_to_otlp_json.rs` (Slice 01 tests) | `cargo test --package self-observe --test cinder_to_otlp_json` exit code | At every commit touching the writer | self-observe crate maintainer (CI feedback per ADR-0005) |
| OK2 | Same test file (Slice 02 tests) | Same | Same | Same |
| OK3 | Same test file (Slice 03 tests) | Same | Same | Same |
| OK4 | Code review at DESIGN handoff + DELIVER review: confirms that Cinder is constructed identically to its prior usage with NoopRecorder | Reviewer eyeballs the test setup | Per wave close | Reviewer |
| OK5 | Same test file: explicit "buffer ends with `\n`" and "split-on-`\n` yields parseable JSON" assertions in Slice 01, inherited by Slices 02/03 | Same `cargo test` invocation | Same | Same |
| OK1-CLI / OK2-CLI / OK3-CLI | CLI follow-up feature's instrumentation + operator survey | TBD by that feature | Post-v0 | TBD |

## Hypothesis

We believe that **emitting Cinder's `MetricsRecorder` events as one
OTLP-JSON `ResourceMetrics` line per event to the same NDJSON sink that
already carries `kaleidoscope.lumen` lines** for the **platform
operator** will achieve **a complete cross-process platform view in a
single OTLP/HTTP collector and a single dashboard, with zero change to
the sidecar, collector, or dashboard**.

We will know this is true when:

- 100% of acceptance tests in `cinder_to_otlp_json.rs` pass green
  (library contract held — measurable at DELIVER close).
- Following the post-v0 CLI follow-up: operators see
  `kaleidoscope.cinder` time series on their existing
  `kaleidoscope.lumen` dashboards within seconds of starting the CLI,
  without touching any sidecar or collector configuration.

## Handoff to DEVOPS / DESIGN

The DESIGN wave should preserve:

1. **The three exact metric names** as the cross-bridge public emission
   contract (matches the Pulse-sink sibling exactly):
   `cinder.place.count`, `cinder.migrate.count`,
   `cinder.evaluate.migrated.count`.
2. **The scope name** `kaleidoscope.cinder` (parallel to
   `kaleidoscope.lumen` at line 138 of `lumen_otlp_json.rs`).
3. **The lowercase tier serialisation** (`hot`/`warm`/`cold`).
4. **The `asInt = migrated.to_string()` encoding** on
   `cinder.evaluate.migrated.count` (NOT `asInt = "1"` + attribute).
5. **The OTLP-JSON envelope shape** — every line has `resource`,
   `scopeMetrics[0].scope`, `scopeMetrics[0].metrics[0]` with `name` and
   `sum`; `sum` has `aggregationTemporality = 2`, `isMonotonic = true`,
   `dataPoints[0]` with `attributes`, `timeUnixNano` (uint64 string),
   `asInt` (uint64 string).
6. **The best-effort emission posture** (`Mutex<W>` guard, `let _ =
   write_all` triple, no panic on serialise/write failure), matching
   `lumen_otlp_json.rs:182-189`.
7. **The atomic per-line write structure** — `write_all(body) +
   write_all(b"\n") + flush`, all inside the Mutex critical section, so
   the cross-writer NDJSON-validity guardrail (OK5) holds when both
   Lumen and Cinder writers share one file.

The DESIGN wave should NOT extract the OTLP-JSON serde structs into a
shared module (rule of three not yet reached; see `wave-decisions.md`
D7).

DEVOPS instrumentation needs (post-v0 CLI feature, not this one): no
new collection infrastructure. The writer appends to the same NDJSON
file the existing sidecar already tails; the operator's existing
dashboards extend by adding `kaleidoscope.cinder`-scoped panels.
