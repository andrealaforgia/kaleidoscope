# KPI Instrumentation - `cli-cinder-otlp-wiring-v0`

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19
- **Source-of-truth for KPIs**: `docs/feature/cli-cinder-otlp-wiring-v0/discuss/outcome-kpis.md`

## Why this document is short

CLI wiring feature. Operator-visible behaviour is supported by
the existing sidecar + collector + dashboard chain (commits
c6b336c, 3af7e82). KPIs land at the acceptance-test level via
`cargo test`. No new dashboard, no new alert. The CI pipeline IS
the measurement instrument; a failing acceptance test IS the alert.

## Per-KPI design

### OK6 - Cross-writer NDJSON validity (principal)

> 100% of captured NDJSON lines parse independently as JSON AND
> the stream ends with `\n`, even when Lumen and Cinder writers
> emit concurrently to two `File::try_clone` handles over one
> underlying file description (DD1).

| Field | Value |
|-------|-------|
| Type | Leading (principal, inherited from ADR-0039 §7) |
| Baseline | n/a (today only Lumen writes; the cross-writer invariant has no exercise) |
| Target | 100% line-parseability; trailing `\n` present; zero observed cross-line byte interleaving under concurrent-random-pause |
| Data source | `crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs` - concurrent-random-pause scenario (spawns Lumen-driving and Cinder-driving threads against one shared real `File`) |
| Collection method | `cargo test --workspace --all-targets --locked` exit code |
| CI gate enforcing | Gate 1 (`cargo test`) |
| Collection frequency | Every commit touching the workspace |
| Owner | kaleidoscope-cli maintainer; enforced by CI |
| Data path | Test pass/fail → workflow run status → GitHub commit status |
| Alerting rule | Workflow run = "failure" → GitHub notification to commit author |
| Dashboard surface | GitHub Actions "All workflows" view, filter by branch `main`. Mutation-testing artefact (`mutants-out-kaleidoscope-cli`, new per A3) supplements: a surviving mutant on the wiring (e.g. the `try_clone()?` call or the Cinder match arm) would flag a weakness in the OK6 measurement. |

### OK7 - Cinder events present in the sink (one line per `place` call)

> Exactly one OTLP-JSON line with metric name `cinder.place.count`,
> scope `kaleidoscope.cinder`, and the per-tenant resource
> attribute, per `cinder.place(...)` call executed by the ingest
> loop.

| Field | Value |
|-------|-------|
| Type | Leading (operator-visible behaviour) |
| Baseline | 0% (the CLI's Cinder recorder is `cinder::NoopRecorder` today) |
| Target | 100% of `cinder.place` calls produce exactly one line |
| Data source | `crates/kaleidoscope-cli/tests/observe_otlp_cinder_wiring.rs` - happy-path scenario (6 records / batch_size 3 → 2 `cinder.place.count` lines + the existing 2 `lumen.ingest.count` lines) |
| Collection method | Same as OK6 (Gate 1) |
| CI gate enforcing | Gate 1 |
| Collection frequency | Every commit |
| Owner | kaleidoscope-cli maintainer; enforced by CI |
| Data path | Same as OK6 |
| Alerting rule | Same as OK6 |
| Dashboard surface | Same as OK6 |

### OK8 - Lumen-side non-regression (guardrail)

> The existing `tests/observe_otlp_flag.rs` passes unchanged.

| Field | Value |
|-------|-------|
| Type | Guardrail |
| Baseline | 100% pass at commit 3af7e82 |
| Target | 100% pass after wiring edit, no edit to the assertions in the existing test file |
| Data source | `crates/kaleidoscope-cli/tests/observe_otlp_flag.rs` (unchanged) |
| Collection method | `cargo test --package kaleidoscope-cli --test observe_otlp_flag` exit code (covered by Gate 1's `--workspace`) |
| CI gate enforcing | Gate 1 |
| Collection frequency | Every commit |
| Owner | kaleidoscope-cli maintainer; enforced by CI |
| Data path | Same as OK6 |
| Alerting rule | Same as OK6 |
| Dashboard surface | Same as OK6 |

## Cross-KPI considerations

Mutation testing supplements OK6/OK7. The new
`gate-5-mutants-kaleidoscope-cli` job (A3) catches the case where
a surviving mutant on the wiring means the acceptance test cannot
distinguish the wired writer from a `NoopRecorder`-only path.
CLAUDE.md's per-feature 100% kill rate applies.

## Summary - KPI to CI gate mapping

| KPI | What it measures | CI gate enforcing | Test file |
|-----|------------------|-------------------|-----------|
| OK6 | Cross-writer NDJSON validity under concurrent emission | Gate 1 | `tests/observe_otlp_cinder_wiring.rs` (concurrent-random-pause scenario) |
| OK7 | Per-call Cinder line presence in the sink | Gate 1 | `tests/observe_otlp_cinder_wiring.rs` (happy-path scenario) |
| OK8 | Lumen-side non-regression | Gate 1 | `tests/observe_otlp_flag.rs` (unchanged) |
| (supplementary) | Test-suite quality / mutation kill rate | Gate 5 (`gate-5-mutants-kaleidoscope-cli`, new per A3) | Mutations of `crates/kaleidoscope-cli/src/lib.rs` |
