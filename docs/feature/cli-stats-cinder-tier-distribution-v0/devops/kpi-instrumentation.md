# KPI Instrumentation - `cli-stats-cinder-tier-distribution-v0`

- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19
- **Source-of-truth**: `docs/feature/cli-stats-cinder-tier-distribution-v0/discuss/outcome-kpis.md`

## Why this document is short

Extension of an existing CLI subcommand, library-shape feature.
KPIs are byte-level stdout assertions enforced by `cargo test`
against `kaleidoscope_cli::stats_with_tiers(...)`. No new
dashboard, no new alert, no new CI job. The function body holds
only the quiescent `LumenToPulseRecorder` (inherited from
`stats()`) and the quiescent `CinderRecorder` alias for
`NoopRecorder` (DD3); zero OTLP emission per
`outcome-kpis.md > DEVOPS instrumentation needs`. CI pipeline IS
the measurement instrument; a failing acceptance test IS the
alert.

## Per-KPI design

### OK1 - per-tier counts byte-equal to `list_by_tier().len()` (principal)

> 100% of `stats_with_tiers()` invocations write
> `hot=H` / `warm=W` / `cold=C` lines where each value equals
> `cinder.list_by_tier(tenant, tier).len()`. Each line is emitted
> ONLY when its count is non-zero (Option B).

| Field | Value |
|-------|-------|
| Type | Leading (principal / north-star) |
| Baseline | 0% (no CLI surface for tier counts exists today; operator path is a hand-written Rust harness) |
| Target | 100%: per-tier line values agree with `list_by_tier(tenant, tier).len()`; zero-count tiers emit no line |
| Data source | `tests/stats_cinder_tier_distribution.rs` populated-multi-tier (seed `acme` with 5 Hot + 12 Warm + 47 Cold, assert lines 4/5/6 = `hot=5` / `warm=12` / `cold=47`) + hot-only (seed only Hot, assert `hot=H` appears and no `warm=` / `cold=` lines) |
| Collection | `cargo test --workspace --all-targets --locked` exit code |
| CI gate | Gate 1 |
| Frequency | Every commit |
| Owner | kaleidoscope-cli maintainer; enforced by CI |
| Data path | Test result -> workflow status -> GitHub commit status |
| Alerting | Workflow run = "failure" -> GitHub notification to commit author |
| Dashboard | GitHub Actions "All workflows", filter `main`. Mutation artefact `mutants-out-kaleidoscope-cli` (A1) supplements: surviving mutant on `list_by_tier(..).len()` arithmetic, on the `match tier` key map, or on the array-literal order flags OK1 weakness. |

### OK2 - per-tenant isolation (leading indicator)

> `stats_with_tiers(acme, ...)` reports `hot=H_acme` where
> `H_acme` equals `list_by_tier(acme, Hot).len()` alone, NEVER
> the cross-tenant union with `globex`.

| Field | Value |
|-------|-------|
| Type | Leading |
| Baseline | 0% (no CLI surface for per-tier counts; no inherited per-tenant invariant to validate) |
| Target | 100%: cross-tenant placements in the SAME `data_dir` never leak into the queried tenant's counts |
| Data source | `tests/stats_cinder_tier_distribution.rs` tenant-isolation (seed `acme` with 5 Hot + `globex` with 9 Hot into same `data_dir`; assert `stats_with_tiers(acme)` -> `hot=5`, NEVER `hot=14`, NEVER `hot=9`) |
| Collection | Same as OK1 |
| CI gate | Gate 1 |
| Frequency | Every commit |
| Owner | kaleidoscope-cli maintainer; enforced by CI |
| Data path | Same as OK1 |
| Alerting | Same as OK1 |
| Dashboard | Same as OK1. Mutation supplements: surviving mutant on the `tenant` parameter forwarding (e.g. hardcoded tenant arg, or `&TenantId::ANY`) flags OK2 weakness; structurally rare because `list_by_tier`'s trait signature accepts `&TenantId`. |

### OK3 - empty-Lumen + Option B selective emission (guardrail)

> Empty-Lumen tenant -> `records=0\n` first, no `earliest=` /
> `latest=`, followed by selectively-emitted non-zero Cinder
> lines in `hot` -> `warm` -> `cold` order. Empty-Lumen
> AND empty-Cinder -> exactly `records=0\n` (byte-equivalent to
> predecessor).

| Field | Value |
|-------|-------|
| Type | Guardrail (disambiguates orphan tier metadata from never-touched-tenant in a grep-friendly way) |
| Baseline | n/a (no CLI surface distinguishes empty-Lumen-empty-Cinder from empty-Lumen-non-empty-Cinder today) |
| Target | 100% on empty-Lumen: `records=0\n` first, no timestamp lines, Cinder lines selectively emitted in canonical order; 100% on empty-Lumen-empty-Cinder: stdout exactly `records=0\n` |
| Data source | `tests/stats_cinder_tier_distribution.rs` orphan-cinder (seed `acme` with H=2, W=0, C=1; no Lumen ingest; assert stdout == `records=0\nhot=2\ncold=1\n`) + the UNMODIFIED `tests/stats_subcommand.rs` empty-tenant test (predecessor oracle for the empty-Lumen-empty-Cinder case via separate never-ingested tenant `acmee`) |
| Collection | Same as OK1 |
| CI gate | Gate 1 |
| Frequency | Every commit |
| Owner | kaleidoscope-cli maintainer; enforced by CI |
| Data path | Same as OK1 |
| Alerting | Same as OK1 |
| Dashboard | Same as OK1. Mutation supplements: surviving mutant on the `if count > 0` guard (-> `>=`) would surface `hot=0\nwarm=0\ncold=0` in the orphan-cinder probe; surviving mutant on the array order would surface `cold=1\nhot=2\n` instead of `hot=2\ncold=1\n`. |

### OK4 - backwards-compatibility for zero-Cinder tenants (guardrail)

> For tenants with all-zero Cinder placements (regardless of
> Lumen state), stdout is byte-equivalent to the predecessor's
> output for the same `(tenant, data_dir)` pair.

| Field | Value |
|-------|-------|
| Type | Guardrail (operator-facing non-regression; protects every existing `stats`-consuming shell pipeline) |
| Baseline | n/a (predecessor IS the baseline; OK4 is the no-regression contract) |
| Target | 100%: zero-Cinder tenants emit exactly the predecessor's stdout; no `hot=` / `warm=` / `cold=` line introduced |
| Data source | `tests/stats_cinder_tier_distribution.rs` backwards-compat (seed Lumen with N records via `ingest()`-equivalent or direct Lumen `append`, open EMPTY Cinder `FileBackedTieringStore` with NO `place()` calls, assert stdout == `records=N\nearliest=...\nlatest=...\n` exactly) + UNMODIFIED `tests/stats_subcommand.rs` (DISCUSS D10 locked oracle: the predecessor's full populated / single-record / empty-tenant assertions all continue to pass green against the repointed dispatcher) |
| Collection | Same as OK1 |
| CI gate | Gate 1 |
| Frequency | Every commit |
| Owner | kaleidoscope-cli maintainer; enforced by CI |
| Data path | Same as OK1 |
| Alerting | Same as OK1 |
| Dashboard | Same as OK1. Mutation supplements: surviving mutant on the `if count > 0` guard (-> always-emit) breaks every assertion in the locked `tests/stats_subcommand.rs` file (every populated-tenant case there places one Hot per batch via `ingest()`'s `flush()`, so an always-emit mutant adds a fourth `hot=N` line and breaks the "exactly 3 non-empty lines" assertion at lines 210/328/416). The locked file is the strongest mutation oracle on the guard. |

## Cross-KPI considerations

The locked `tests/stats_subcommand.rs` (DISCUSS D10) is the
load-bearing oracle for OK4 AND a high-value mutation oracle for
the `if count > 0` guard. Because that file calls `ingest()` which
auto-places one Hot Cinder per batch, ANY mutant that turns the
guard into always-emit produces a `hot=N` line that breaks the
predecessor's "exactly 3 non-empty lines" assertion. This is why
the locked file MUST stay UNMODIFIED through DELIVER -- it is
the mechanical proof of the OK4 contract.

The new `tests/stats_cinder_tier_distribution.rs` covers what the
locked file CANNOT: the orphan-cinder case (empty-Lumen +
non-empty-Cinder), the tenant-isolation case (two tenants in the
same `data_dir`), and the multi-tier case (Hot + Warm + Cold all
non-zero simultaneously).

Mutation classes covered in concert via Gate 5:
- `if count > 0` -> `>=` / `==` / `< 0`: caught by OK4
  byte-equivalence (locked file) and by OK3 orphan-cinder
  positive assertion.
- `match tier { Tier::Hot => "hot", ... }` arm-swap or constant
  replacement: caught by OK1 multi-tier with distinct counts
  5/12/47.
- Array-literal reorder `[Tier::Hot, Tier::Warm, Tier::Cold]`:
  caught by OK1 multi-tier asserting exact line indices and by
  OK3 orphan-cinder asserting `hot=2\ncold=1\n` order.
- `list_by_tier(tenant, tier).len()` off-by-one / constant: caught
  by OK1 exact-count assertion + OK2 cross-tenant exclusion.
- `writeln!` elision or `\n` strip: caught by OK1/OK4 line-count
  + OK3 trailing-newline assertion.

## Summary - KPI to CI gate mapping

| KPI | What it measures | CI gate | Test file |
|-----|------------------|---------|-----------|
| OK1 | Per-tier counts agree with `list_by_tier().len()`; Option B selective emission | Gate 1 | `tests/stats_cinder_tier_distribution.rs` (populated-multi-tier + hot-only) |
| OK2 | Cross-tenant placements never leak | Gate 1 | `tests/stats_cinder_tier_distribution.rs` (tenant-isolation) |
| OK3 | Empty-Lumen + Option B; `records=0\n` first, Cinder lines in `hot` -> `warm` -> `cold` order | Gate 1 | `tests/stats_cinder_tier_distribution.rs` (orphan-cinder) |
| OK4 | Byte-equivalent to predecessor for zero-Cinder tenants | Gate 1 | `tests/stats_cinder_tier_distribution.rs` (backwards-compat) + UNMODIFIED `tests/stats_subcommand.rs` |
| (supplementary) | Mutation kill rate on new fn + dispatcher repoint | Gate 5 (existing, A1 INHERITED) | Mutations of `src/lib.rs` + `src/main.rs` |
