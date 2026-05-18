# KPI Instrumentation - `cli-read-observe-otlp-v0`

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19
- **Source-of-truth for KPIs**: `docs/feature/cli-read-observe-otlp-v0/discuss/outcome-kpis.md`

## Why this document is short

CLI wiring feature. Operator-visible behaviour is supported by
the existing sidecar + collector + dashboard chain (commits
c6b336c, 3af7e82, and the Cinder wiring from
`cli-cinder-otlp-wiring-v0`). KPIs land at the acceptance-test
level via `cargo test`. No new dashboard, no new alert, no new
CI job. The CI pipeline IS the measurement instrument; a
failing acceptance test IS the alert.

## Per-KPI design

### OK1 - Lumen query events present in the sink (principal)

> 100% of `read()` invocations with `otlp_log_path = Some(path)`
> produce exactly one line with metric name `lumen.query.count`,
> scope `kaleidoscope.lumen`, and the per-tenant resource
> attribute, per Lumen query call (one per `read` invocation).

| Field | Value |
|-------|-------|
| Type | Leading (principal, operator-visible behaviour at byte level) |
| Baseline | 0% (today's `read()` constructs `LumenToPulseRecorder` over an in-process Pulse sink; nothing observable to any file) |
| Target | 100% of flagged `read()` invocations produce exactly one `lumen.query.count` line with correct tenant, scope, and `asInt` |
| Data source | `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` - happy-path scenario (pre-ingested records via a setup call, then one `read()` with `otlp_log_path = Some(path)`) |
| Collection method | `cargo test --workspace --all-targets --locked` exit code |
| CI gate enforcing | Gate 1 (`cargo test`) |
| Collection frequency | Every commit touching the workspace |
| Owner | kaleidoscope-cli maintainer; enforced by CI |
| Data path | Test pass/fail -> workflow run status -> GitHub commit status |
| Alerting rule | Workflow run = "failure" -> GitHub notification to commit author |
| Dashboard surface | GitHub Actions "All workflows" view, filter by branch `main`. Mutation-testing artefact (`mutants-out-kaleidoscope-cli`, existing per the prior wave's A3) supplements: a surviving mutant on the `Some(path)` arm (e.g. flipping to `LumenToPulseRecorder` construction, or eliding `OpenOptions::append(true)`) would flag a weakness in the OK1 measurement. |

### OK2 - No-flag non-regression (guardrail)

> When `--observe-otlp` is absent on `read`: returned count
> equals number of pre-ingested records, stdout bytes equal the
> pre-ingested records re-serialised as NDJSON, no file is
> created at any path.

| Field | Value |
|-------|-------|
| Type | Guardrail |
| Baseline | 100% pass at current HEAD (today's `read()` body is the baseline; the `None` arm preserves it byte-equivalently per DD3) |
| Target | 100% pass after wiring edit; identical stdout bytes, identical return value, zero side-channel file creation |
| Data source | `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` - no-flag scenario (asserts (a) returned count = pre-ingested count, (b) stdout bytes = pre-ingested records re-serialised as NDJSON, (c) no file at the path the test would have specified) |
| Collection method | Same as OK1 (Gate 1) |
| CI gate enforcing | Gate 1 |
| Collection frequency | Every commit |
| Owner | kaleidoscope-cli maintainer; enforced by CI |
| Data path | Same as OK1 |
| Alerting rule | Same as OK1 |
| Dashboard surface | Same as OK1 |

### OK3 - Cross-subcommand symmetry (leading)

> After a 6-record `ingest` (batch_size 3) followed by one
> `read()` call against the same `--observe-otlp` path, the
> file contains at least 2 `lumen.ingest.count` lines, at
> least 2 `cinder.place.count` lines, and at least 1
> `lumen.query.count` line; every non-empty line parses as
> JSON; file ends with `\n`.

| Field | Value |
|-------|-------|
| Type | Leading (cross-subcommand symmetry - principal evidence that the operator's single sidecar configuration captures the full Lumen lifecycle) |
| Baseline | n/a (this scenario cannot exist today because `read` cannot emit OTLP at all) |
| Target | 100% pass on the sequential ingest-then-read scenario; metric-name union = {`lumen.ingest.count`, `cinder.place.count`, `lumen.query.count`} |
| Data source | `crates/kaleidoscope-cli/tests/observe_otlp_read_flag.rs` - ingest-then-read scenario (sequential `ingest()` + `read()` against one shared `otlp_log_path`) |
| Collection method | Same as OK1 (Gate 1) |
| CI gate enforcing | Gate 1 |
| Collection frequency | Every commit |
| Owner | kaleidoscope-cli maintainer; enforced by CI |
| Data path | Same as OK1 |
| Alerting rule | Same as OK1 |
| Dashboard surface | Same as OK1 |

## Cross-KPI considerations

Mutation testing supplements OK1/OK3. The existing
`gate-5-mutants-kaleidoscope-cli` job (commit 2baa05c) catches
the case where a surviving mutant on the `read()` wiring means
the acceptance test cannot distinguish the Lumen OTLP-JSON
writer from a `LumenToPulseRecorder` fallback, or cannot detect
the absence of `OpenOptions::append(true)`. CLAUDE.md's
per-feature 100% kill rate applies; the existing job carries
the enforcement at zero workflow-edit cost (A1).

## Summary - KPI to CI gate mapping

| KPI | What it measures | CI gate enforcing | Test file |
|-----|------------------|-------------------|-----------|
| OK1 | Presence of one `lumen.query.count` line per `read()` invocation | Gate 1 | `tests/observe_otlp_read_flag.rs` (happy-path scenario) |
| OK2 | No-flag non-regression: stdout byte-equivalence, no side-channel file | Gate 1 | `tests/observe_otlp_read_flag.rs` (no-flag scenario) |
| OK3 | Cross-subcommand symmetry: all three metric names in one file from one shell session | Gate 1 | `tests/observe_otlp_read_flag.rs` (ingest-then-read scenario) |
| (supplementary) | Test-suite quality / mutation kill rate | Gate 5 (existing `gate-5-mutants-kaleidoscope-cli`, A1: inherited, no workflow edit) | Mutations of `crates/kaleidoscope-cli/src/lib.rs` + `src/main.rs` |
