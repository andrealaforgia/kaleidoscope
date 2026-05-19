# KPI Instrumentation - `cli-stats-subcommand-v0`

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19
- **Source-of-truth**: `docs/feature/cli-stats-subcommand-v0/discuss/outcome-kpis.md`

## Why this document is short

New CLI subcommand, library-shape feature. KPIs are byte-level
stdout assertions enforced by `cargo test` against
`kaleidoscope_cli::stats(...)`. No new dashboard, no new alert,
no new CI job. The `stats()` body constructs only the quiescent
`LumenToPulseRecorder` (DD4); zero OTLP emission per
outcome-kpis.md "DEVOPS instrumentation needs". CI pipeline IS
the measurement instrument; a failing acceptance test IS the
alert.

## Per-KPI design

### OK1 - `records=N` byte-equal to `read()`'s count (principal)

> 100% of `stats()` invocations write a `records=N\n` line where
> N equals what `read()` would return for the same
> `(tenant, data_dir)`. Tenant isolation honoured.

| Field | Value |
|-------|-------|
| Type | Leading (principal) |
| Baseline | 0% (no `stats` exists today; operator path is `read \| wc -l`) |
| Target | 100% of `stats()` invocations produce `records=N` agreeing with `read()`; tenant isolation honoured |
| Data source | `tests/stats_subcommand.rs` populated-tenant (7 records for `acme`, assert line 1 = `records=7`) + tenant-isolation (`acme`=7, `globex`=3 same `data_dir`, assert `stats(acme)` -> `records=7`) |
| Collection | `cargo test --workspace --all-targets --locked` exit code |
| CI gate | Gate 1 |
| Frequency | Every commit |
| Owner | kaleidoscope-cli maintainer; enforced by CI |
| Data path | Test result -> workflow status -> GitHub commit status |
| Alerting | Workflow run = "failure" -> GitHub notification to commit author |
| Dashboard | GitHub Actions "All workflows", filter `main`. Mutation artefact `mutants-out-kaleidoscope-cli` (A1) supplements: surviving mutant on `records.len()` flags OK1 weakness. |

### OK2 - `earliest=`/`latest=` ISO 8601 UTC match min/max nanos (leading)

> 100% of populated-tenant `stats()` invocations write
> `earliest=<ISO 8601>\n` and `latest=<ISO 8601>\n` whose parsed
> values equal the seeded min/max `observed_time_unix_nano`.
> Single-record tenants yield byte-identical earliest/latest.

| Field | Value |
|-------|-------|
| Type | Leading |
| Baseline | 0% (today operators parse `read \| head -1` / `tail -1` and convert nanos by hand) |
| Target | 100% on populated + single-record; ISO 8601 strings == seeded min/max nanos; single-record == byte-identical |
| Data source | `tests/stats_subcommand.rs` populated (7 records spanning `2026-05-18T00:00:00Z`..`2026-05-19T00:00:00Z`, assert byte-exact ISO 8601) + single-record (1 record, assert earliest == latest byte-for-byte) |
| Collection | Same as OK1 |
| CI gate | Gate 1 |
| Frequency | Every commit |
| Owner | kaleidoscope-cli maintainer; enforced by CI |
| Data path | Same as OK1 |
| Alerting | Same as OK1 |
| Dashboard | Same as OK1. Mutation supplements: surviving mutants on year/month/day arithmetic, format-string ordering, or `records.last()` vs `.first()` flag OK2 weakness. |

### OK3 - Empty-tenant: exactly `records=0\n` (guardrail)

> Zero-record tenant -> exactly one stdout line `records=0\n`,
> no `earliest=` line, no `latest=` line, exit 0.

| Field | Value |
|-------|-------|
| Type | Guardrail (disambiguates empty from populated in a grep-friendly way) |
| Baseline | n/a (closest behaviour `read \| wc -l` returns `0` but is indistinguishable from "read failed silently") |
| Target | 100% on empty-tenant: exactly 1 non-empty line `records=0`, no line begins with `earliest=`, no line begins with `latest=`, stdout ends with `\n`, exit 0 |
| Data source | `tests/stats_subcommand.rs` empty-tenant (fresh `data_dir` OR populated `data_dir` with never-ingested tenant; assert `stats()` returns `Ok(0)`, exactly 1 non-empty line == `records=0`, no `earliest=`/`latest=` lines, stdout ends `\n`) |
| Collection | Same as OK1 |
| CI gate | Gate 1 |
| Frequency | Every commit |
| Owner | kaleidoscope-cli maintainer; enforced by CI |
| Data path | Same as OK1 |
| Alerting | Same as OK1 |
| Dashboard | Same as OK1. Mutation supplements: surviving mutant on `(Some, Some)` vs `(None, None)` branch (flipped arms, or unconditional 3-line write) flags OK3 weakness. |

## Cross-KPI considerations

Mutation testing supplements OK1/OK2/OK3 in concert via the
existing `gate-5-mutants-kaleidoscope-cli` job (commit 2baa05c,
A1 inherited). High-value mutation classes: swapping
`records.first()`/`.last()` (caught by OK2 ordering); replacing
formatter year arithmetic with a constant (caught by OK2 byte-
exact match); eliding `\n` terminator (caught by OK1/OK3 line-
count); flipping empty-case branch to also emit timestamps
(caught by OK3).

## Summary - KPI to CI gate mapping

| KPI | What it measures | CI gate | Test file |
|-----|------------------|---------|-----------|
| OK1 | `records=N` matches `read()`; tenant isolation | Gate 1 | `tests/stats_subcommand.rs` (populated + tenant-isolation) |
| OK2 | `earliest=`/`latest=` ISO 8601 == seeded min/max; single-record degenerate window | Gate 1 | `tests/stats_subcommand.rs` (populated + single-record) |
| OK3 | Empty-tenant: `records=0\n` only, exit 0 | Gate 1 | `tests/stats_subcommand.rs` (empty-tenant) |
| (supplementary) | Mutation kill rate on new fn + formatter | Gate 5 (existing, A1 inherited) | Mutations of `src/lib.rs` + `src/main.rs` |
