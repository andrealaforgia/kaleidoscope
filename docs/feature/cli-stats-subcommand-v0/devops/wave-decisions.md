# Wave Decisions - `cli-stats-subcommand-v0` / DEVOPS

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19
- **Mode**: Decisions pre-taken with Andrea (D1-D9); in-wave
  Apex decisions recorded below as A-decisions. Mirrors the
  prior four DEVOPS waves; inherits the zero-workflow-edit
  posture established by `cli-cinder-otlp-wiring-v0` and
  `cli-read-observe-otlp-v0`.

## Inputs read

1. `CLAUDE.md` - Rust idiomatic; per-feature MT 100% kill per
   ADR-0005 Gate 5.
2. `discuss/outcome-kpis.md` - OK1 (`records=N` byte-equal to
   `read()`'s count), OK2 (`earliest=`/`latest=` ISO 8601 UTC
   match min/max `observed_time_unix_nano`), OK3 (empty-tenant
   `records=0\n`, exit 0).
3. `design/wave-decisions.md` - DD1 (hand-rolled ISO 8601, zero
   new datetime crates), DD2 (`stats(tenant, data_dir, writer)
   -> Result<usize, Error>`), DD3 (`records.first()`/`.last()`
   via the `LogStore` ascending-order contract), DD4 (RCA),
   DD5 (out-of-scope), DEVOPS handoff annotation.
4. `docs/feature/cli-read-observe-otlp-v0/devops/*` - template.
5. `.github/workflows/ci.yml:949-1028` -
   `gate-5-mutants-kaleidoscope-cli` confirmed path-filtered on
   `crates/kaleidoscope-cli/**` (line 1006).
6. `crates/kaleidoscope-cli/Cargo.toml` - `aegis`, `lumen`,
   `self-observe`, `pulse` already declared; the new
   `[[test]]` block is the only addition.

## Pre-wave decisions (carried in)

| D# | Decision | Value |
|----|----------|-------|
| D1 | `deployment_target` | N/A (CLI subcommand; binary shape unchanged) |
| D2 | `container_orchestration` | N/A |
| D3 | `cicd_platform` | GitHub Actions (existing) |
| D4 | `existing_infrastructure` | Yes (workspace + five-gate CI + per-pkg Gate 5) |
| D5 | `observability_and_logging` | N/A (quiescent recorder; no OTLP from `stats`) |
| D6 | `deployment_strategy` | N/A |
| D7 | `continuous_learning` | No (single-feature, no A/B, no flags) |
| D8 | `git_branching_strategy` | Trunk-Based Development |
| D9 | `mutation_testing_strategy` | Per-feature, 100% kill per ADR-0005 Gate 5 |

## Differences from the cli-read-observe-otlp-v0 template

1. **New public function, not a signature extension.** Prior
   wave extended `read()`'s signature; this adds a wholly new
   `stats()` alongside. Additive, not breaking.
2. **No OTLP emission in the feature surface.** `stats()`
   constructs only the quiescent `LumenToPulseRecorder` (DD4).
   KPI surface is byte-level stdout, not OTLP-JSON parses.
3. **One new private function** (the ISO 8601 formatter, DD1).
   Mutation surface grows by year/month/day arithmetic +
   leap-year handling + the `Some(first)/Some(last)` vs
   `None/None` branch.

## In-wave decisions (A = Apex)

### [A1] No new CI workflow edit - inherit existing Gate 5 job

**Options**: (1) inherit `gate-5-mutants-kaleidoscope-cli` via
its `--in-diff` filter; (2) per-file Gate 5 fan-out; (3) skip
Gate 5 as "small surface".

**Recommendation**: **Option 1** - **INHERIT**.

**Rationale**:

- **Prior waves' investment pays off.** Commit 2baa05c added
  the job precisely so subsequent crate features cost zero
  workflow edits. The `--in-diff` cascade (`origin/main` ->
  `HEAD~1` -> full) auto-picks up the diff on `src/lib.rs`
  (new `stats()` + private formatter) and `src/main.rs` (new
  `run_stats` + extended `print_usage`) on the merge commit.
- **Per-file fan-out is premature** at N=2 modified files. The
  existing job mutates both in one pass; fan-out adds runner
  cost with no diagnostic gain.
- **Skipping Gate 5** violates CLAUDE.md per-feature MT. The
  compile-green mutation classes here are exactly the ones
  operators cannot tell apart from correct behaviour:
  `records.first().map(...)` -> `Some(0)` (yields
  `earliest=1970-01-01T00:00:00.000000000Z` for every populated
  tenant); flipping `Some(first)/Some(last)` to `None/None`
  (yields `records=0` for populated tenants); `:09` -> `:06`
  in the format string (silently downgrades to microsecond
  precision). Gate 5 is the mechanical oracle.

**Verdict**: NO edit to `.github/workflows/ci.yml`. Second
consecutive wave at zero workflow churn under the same job.

### [A2] Gate 1 inherits via `[[test]]` block - ZERO workflow edits total

No Gate 1 workflow edit. `cargo test --workspace --all-targets
--locked` (ci.yml:182) auto-discovers via the `[[test]]` block
in `crates/kaleidoscope-cli/Cargo.toml`.

A1 + A2 = **ZERO workflow edits**. ci.yml is byte-untouched.
Crafty lands the new function + formatter (`src/lib.rs`), the
dispatcher + usage edits (`src/main.rs`), the new test, and
the `[[test]]` block in ONE atomic commit per ADR-0005's
"tests and source land together" rule.

Trade-off: a malformed `[[test]]` block fails Gate 1 for the
whole workspace. Correct fail-fast.

### [A3] Zero new external dependencies

Verified by DD1 + workspace-wide grep: no `chrono`/`time`/
`jiff` in any `Cargo.toml`; DD1 rejects adding any datetime
crate; the formatter uses only `std::fmt::Write` + `u64`
arithmetic. The new test uses `tempfile` (already in the three
sibling test files). Zero `[dependencies]`, zero
`[dev-dependencies]`, zero `deny.toml` change. Only Cargo.toml
addition:

```toml
[[test]]
name = "stats_subcommand"
path = "tests/stats_subcommand.rs"
```

### [A4] No new toolchain pin

Inherits workspace stable Rust (`rust-toolchain.toml`). No
Gate 2/3 graduation (binary crate). Formatter uses no unstable
features.

## Skipped artefacts (N/A)

`platform-architecture.md` (Morgan's app-architecture
sufficient), `observability-design.md` (D5),
`monitoring-alerting.md` (CI gates ARE alerts),
`infrastructure-integration.md` (no external integrations),
`branching-strategy.md` (D8), `continuous-learning.md` (D7).

## Constraints for DISTILL / DELIVER

| When | What |
|------|------|
| DISTILL | Author `tests/stats_subcommand.rs` with RED scenarios for OK1 (populated + tenant-isolation), OK2 (populated + single-record), OK3 (empty-tenant) + `[[test]]` block |
| DISTILL | Author as `kaleidoscope_cli::stats(...)` library calls into a `Vec<u8>` writer (per DD2), NOT subprocess invocations |
| DELIVER | Land new function + formatter + dispatcher + test + `[[test]]` block in ONE atomic commit |
| DELIVER | DO NOT edit `.github/workflows/ci.yml` (A1 + A2) |
| DELIVER | DO NOT add any datetime crate (DD1 + A3) |
| DELIVER | DO NOT add `-p kaleidoscope-cli` to Gate 2 / Gate 3 (no graduation) |
| DELIVER | 100% mutation kill on `src/lib.rs` + `src/main.rs` before review approval |
| DELIVER | Existing tests (`observe_otlp_flag.rs`, `observe_otlp_cinder_wiring.rs`, `observe_otlp_read_flag.rs`, `ingest_and_read_roundtrip.rs`) MUST pass unchanged |

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**Deliverables produced**:

| Artefact | Path |
|----------|------|
| Environment inventory | `docs/feature/cli-stats-subcommand-v0/devops/environments.yaml` |
| Wave decisions (this file) | `docs/feature/cli-stats-subcommand-v0/devops/wave-decisions.md` |
| Per-KPI instrumentation | `docs/feature/cli-stats-subcommand-v0/devops/kpi-instrumentation.md` |
| CI/CD pipeline confirmation | `docs/feature/cli-stats-subcommand-v0/devops/ci-cd-pipeline.md` |

---

## Forward-compatibility notes

### Pre-push hook graduation

Pre-push per-pkg loop iterates `[otlp-conformance-harness,
spark, sieve, codex]`. If kaleidoscope-cli is extracted into a
`kaleidoscope-cli-core` library crate, that feature's DEVOPS
wave MUST add the new crate to Gate 2+3 matrix, pre-push loop,
and pre-commit. This feature does NOT trigger the graduation.

### Datetime-crate posture

DD1 rejects adding `chrono`/`time`/`jiff` for this feature.
If a future feature genuinely needs time-zone handling, its
DESIGN wave re-opens DD1's rejection. Until then, the
hand-rolled formatter inside
`crates/kaleidoscope-cli/src/lib.rs` is the single workspace
path through ISO 8601 rendering. A second consumer SHOULD
extract it under the rule-of-three.

### Mutation kill-rate protocol (DELIVER)

1. After tests turn GREEN: `cargo mutants --package
   kaleidoscope-cli --in-diff <(git diff origin/main HEAD --
   crates/kaleidoscope-cli/src/lib.rs
   crates/kaleidoscope-cli/src/main.rs)`.
2. `mutants.out/summary.txt` "undetected" MUST be zero.
3. Survivors -> tighten: leap-year boundary for day-of-year
   arithmetic survivors; byte-level empty-tenant probe for
   `Some/None` branch survivors; line-count probe for `\n`
   separator survivors. Escalate if truly unkillable.
4. CI oracle: existing `gate-5-mutants-kaleidoscope-cli` on
   merge - surface auto-discovered via `--in-diff`.

Prior precedent: commit 4d20c31 hit 6/6 = 100% kill on Cinder
wiring; `cli-read-observe-otlp-v0` DELIVER mutated `read()`'s
extension to 100% kill under the same job. This feature is the
third consecutive realisation of the zero-workflow-edit
per-package Gate 5 cycle.
