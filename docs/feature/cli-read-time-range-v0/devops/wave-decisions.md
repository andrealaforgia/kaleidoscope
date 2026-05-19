# Wave Decisions - `cli-read-time-range-v0` / DEVOPS

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19
- **Mode**: Decisions pre-taken with Andrea (A1-A4); recorded
  verbatim. Mirrors the prior `cli-stats-cinder-tier-distribution-v0`
  DEVOPS posture and inherits the zero-workflow-edit posture
  established by `cli-cinder-otlp-wiring-v0`,
  `cli-read-observe-otlp-v0`, `cli-stats-subcommand-v0`, and
  `cli-stats-cinder-tier-distribution-v0`. **This is the FIFTH
  consecutive zero-workflow-edit wave on the `kaleidoscope-cli`
  package.**

## Inputs read

1. `CLAUDE.md` - Rust idiomatic; per-feature MT 100% kill per
   ADR-0005 Gate 5.
2. `discuss/outcome-kpis.md` - OK1 (bounded-window filter; principal),
   OK2 (no-flag byte-equivalent non-regression), OK3 (half-bounded
   `--since`-only / `--until`-only), OK4 (invalid-ISO8601 fail-fast
   naming the offending flag).
3. `design/wave-decisions.md` - DD1 (Option C: 5th `range: TimeRange`
   parameter on `read()`), DD2 (Option C: parser in `lib.rs` +
   flag-aware wrapper in `main.rs`), DD3 (parser accepts 0..=9
   fractional digits, year range `[1970, 9999]`, Hinnant
   `days_from_civil`), DD4 (RCA: EXTEND + 9 REUSE + 5 CREATE NEW,
   zero new external crate), DD5 (out-of-scope confirmations),
   DEVOPS handoff annotation.
4. `docs/feature/cli-stats-cinder-tier-distribution-v0/devops/*` -
   template (4th wave under the same posture).
5. `.github/workflows/ci.yml:949-1028` -
   `gate-5-mutants-kaleidoscope-cli` confirmed path-filtered on
   `crates/kaleidoscope-cli/**` (line 1006); `--in-diff` cascade
   (`origin/main` -> `HEAD~1` -> full) unchanged.
6. `crates/kaleidoscope-cli/Cargo.toml` - `aegis`, `cinder`,
   `lumen`, `self-observe`, `pulse`, `tempfile` (dev) already
   declared; only addition is one new `[[test]]` block.

## Pre-wave decisions (carried in)

| D# | Decision | Value |
|----|----------|-------|
| D1 | `deployment_target` | N/A (CLI subcommand parameter; binary shape unchanged) |
| D2 | `container_orchestration` | N/A |
| D3 | `cicd_platform` | GitHub Actions (existing) |
| D4 | `existing_infrastructure` | Yes (workspace + five-gate CI + per-pkg Gate 5) |
| D5 | `observability_and_logging` | N/A (no new emission source; `--observe-otlp` composition out of scope per DESIGN DD5 item 4) |
| D6 | `deployment_strategy` | N/A |
| D7 | `continuous_learning` | No (single-feature, no A/B, no flags) |
| D8 | `git_branching_strategy` | Trunk-Based Development (pure trunk; no required-status-checks; no enforce_admins) |
| D9 | `mutation_testing_strategy` | Per-feature, 100% kill per ADR-0005 Gate 5 |

## Differences from the cli-stats-cinder-tier-distribution-v0 template

1. **Additive parameter on existing function, not a new sibling.**
   Prior wave added `stats_with_tiers()` alongside `stats()` (DD1
   Option A). This wave EXTENDS the existing `read()` signature
   from 4 args to 5 args (DD1 Option C). The two locked OK2
   test files (`observe_otlp_read_flag.rs`, `observe_otlp_flag.rs`)
   invoke the binary via subprocess without the new flags, so they
   hit the no-flag default `TimeRange::all()` and remain
   byte-equivalent without edits. The library-direct caller in
   `tests/ingest_and_read_roundtrip.rs` (if any) gains
   `TimeRange::all()` as its 5th argument.
2. **Parser is the rich mutation surface.** Prior wave's new code
   was a 3-element array iteration plus a one-line guard. This
   wave's new code is a hand-rolled ISO 8601 parser (~50 source
   lines) plus `days_from_civil` (~15 lines) plus a `parse_time_range`
   argv-scan (~25 lines). DD2 places the parser next to its inverse
   formatter at `crates/kaleidoscope-cli/src/lib.rs:410-420`,
   keeping the round-trip property `parse(format(ns)) == ns` a
   single-file local check.
3. **Two-file source diff, same package.** `src/lib.rs` (new parser,
   new helper, signature delta) AND `src/main.rs` (new argv helper,
   dispatcher repoint, `write_usage` text delta). Both fall inside
   the existing Gate 5 job's path filter
   (`crates/kaleidoscope-cli/**`); `--in-diff` auto-covers both.
4. **No observability surface.** Prior wave produced a tier-count
   text writer; this wave produces a filtered NDJSON stream
   byte-equivalent in shape to today's `read()` output. No new
   metric, no new dashboard, no new alert.

## In-wave decisions (A = Apex)

### [A1] No new CI workflow edit - INHERIT existing Gate 5 job

**Options**: (1) inherit `gate-5-mutants-kaleidoscope-cli` via
its `--in-diff` filter; (2) per-file Gate 5 fan-out; (3) skip
Gate 5 as "parser is just arithmetic".

**Verdict**: **Option 1** - **INHERIT** (pre-decided by Andrea).

**Rationale**:

- **Amortising investment, fifth realisation.** Commit 2baa05c
  added `gate-5-mutants-kaleidoscope-cli` precisely so subsequent
  kaleidoscope-cli features would cost zero workflow edits. That
  one-off investment now amortises across five consecutive waves:
  `cli-cinder-otlp-wiring-v0`, `cli-read-observe-otlp-v0`,
  `cli-stats-subcommand-v0`, `cli-stats-cinder-tier-distribution-v0`,
  and now this feature. The `--in-diff` cascade (`origin/main` ->
  `HEAD~1` -> full) auto-picks up the diff on `src/lib.rs` (new
  parser + new helper + signature delta) and `src/main.rs` (new
  argv helper + dispatcher repoint + usage text delta) on the
  merge commit. The path filter at ci.yml:1006
  (`crates/kaleidoscope-cli/**`) matches both files structurally.
- **Per-file fan-out is still premature** at N=2 modified files
  (same shape as prior four waves). The existing job mutates both
  in one pass; fan-out adds runner cost with no diagnostic gain.
- **Skipping Gate 5 is the worst option.** The parser is the
  richest mutation surface this package has seen — more arithmetic
  and branch logic than any prior wave's diff. Compile-green
  mutation classes the operator cannot tell apart from correct
  behaviour include length-check off-by-one, separator-byte index
  swap, digit-range guard exclusion, field-range `<= 12` -> `< 12`
  for month, year-range lower-bound boundary at 1970, single-
  constant changes in Hinnant `days_from_civil`, fractional-digit
  left-pad off-by-one, `parse_time_range` flag-binding swap
  (`--since` vs `--until`), half-bounded defaults (`0` -> `1`,
  `u64::MAX` -> `u64::MAX - 1`), closed-lower / open-upper
  boundary inversion, and re-ordering the parse call AFTER
  `lumen.query(...)` (breaks the fail-before-store-open invariant).
  Each is killed by OK1/OK3 boundary witnesses, OK4 invalid-input
  + filesystem-absence probes, and parser round-trip property tests
  — full survivor-to-probe map in forward-compat protocol below.
  Gate 5 is the mechanical oracle. Skipping it on a parser this
  rich is the negligence class CLAUDE.md's per-feature MT rule
  exists to prevent.

**Verdict**: NO edit to `.github/workflows/ci.yml`. Fifth
consecutive wave at zero workflow churn under the same job.

### [A2] Gate 1 inherits via `[[test]]` block - ZERO workflow edits total

**Verdict**: No Gate 1 workflow edit (pre-decided). `cargo test
--workspace --all-targets --locked` (ci.yml:182) auto-discovers
via the `[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml`:

```toml
[[test]]
name = "read_time_range"
path = "tests/read_time_range.rs"
```

A1 + A2 = **ZERO workflow edits**. ci.yml is byte-untouched.
Crafty lands the parser (`src/lib.rs`), the `days_from_civil`
helper (`src/lib.rs`), the `read()` signature delta (`src/lib.rs`),
the `parse_time_range` helper (`src/main.rs`), the `run_read_with`
dispatcher delta (`src/main.rs`), the `write_usage` text delta
(`src/main.rs`), the new test file, and the `[[test]]` block in
ONE atomic commit per ADR-0005's "tests and source land together"
rule.

Trade-off: a malformed `[[test]]` block fails Gate 1 for the
whole workspace. Correct fail-fast.

### [A3] Zero new external dependencies

**Verdict**: Zero new crate (pre-decided). Verified by DESIGN
DD4 RCA + DESIGN handoff: all used types (`lumen::TimeRange`,
`aegis::TenantId`, std I/O traits) are already in
`kaleidoscope-cli`'s use list at
`crates/kaleidoscope-cli/src/lib.rs:55-65`. The new test uses
`tempfile` (already a dev-dependency in the sibling test files).
Zero `[dependencies]`, zero `[dev-dependencies]`, zero
`deny.toml` change. Only `Cargo.toml` addition is the `[[test]]`
block above.

Inherits the no-`chrono`/`time`/`jiff` posture verified by
grep at DESIGN time (zero matches across all `Cargo.toml` and
all `*.rs`).

### [A4] No new toolchain pin

**Verdict**: Inherits workspace stable Rust
(`rust-toolchain.toml`). No Gate 2/3 graduation (binary crate).
The new parser body uses no unstable features; byte-slice
indexing, `u64` arithmetic, `match` on `u8` literals, and array
literals are stable on every supported MSRV. The `days_from_civil`
Hinnant algorithm is pure `i64` arithmetic with a single `as u64`
cast guarded by the year-range check.

## Skipped artefacts (N/A)

`platform-architecture.md` (Morgan's app-architecture
sufficient), `observability-design.md` (DESIGN DD5 item 4 +
DEVOPS handoff: no OTLP), `monitoring-alerting.md` (CI gates ARE
alerts), `infrastructure-integration.md` (no external
integrations), `branching-strategy.md` (D8: pure trunk),
`continuous-learning.md` (D7).

## Constraints for DISTILL / DELIVER

| When | What |
|------|------|
| DISTILL | Author `tests/read_time_range.rs` with RED scenarios for OK1 (bounded `[200, 400)` over witnesses `{100,200,300,400,500}` asserting `{200,300}`), OK2 (no-flag scenario asserts `TimeRange::all()` byte-equivalence), OK3 (`--since`-only and `--until`-only over witnesses `{100,200,300,400}`), OK4 (three invalid-input scenarios + stderr flag-name + stdout-empty + Lumen-store-not-opened assertions) + `[[test]]` block |
| DISTILL | Author as `kaleidoscope_cli::read(...)` library calls into a `Vec<u8>` writer for OK1/OK2/OK3; author OK4 via `run_read_with(...)` (the binary entry point form testable in-process per `crates/kaleidoscope-cli/src/main.rs:155-165`) so the stderr surface and the fail-before-store-open invariant are observable |
| DISTILL | Inline-duplicate the test harness helpers (`tenant`, `record`, `temp_root`, `cleanup`, `ndjson`) per DISCUSS D7; rule-of-three extraction to `tests/common/mod.rs` is a separate refactoring task |
| DISTILL | OK1 witness records MUST include explicit boundary cases: a record at exactly `since_ns` (asserts closed-lower) AND a record at exactly `until_ns` (asserts open-upper, MUST be excluded) |
| DISTILL | OK3 witness set MUST include a record at `observed_time_unix_nano = 0` to kill the "absent `--since` defaults to `1` instead of `0`" mutation class, AND a record near `u64::MAX` to kill the "absent `--until` defaults to `u64::MAX - 1`" mutation class |
| DISTILL | Authoring of the parser unit tests in `crates/kaleidoscope-cli/src/lib.rs`'s `#[cfg(test)] mod tests` block at `lib.rs:457-651` is REQUIRED to discharge the 100% Gate 5 kill rate. Round-trip property `parse(format(ns)) == ns` over a witness set covering: epoch (`0`), `1970-01-01T00:00:00Z` (no-`.` shape), various fractional-digit widths (1, 3, 6, 9 digits), every month boundary, every leap-year boundary (2000, 2024 leap; 1900, 2026 non-leap), and year-range boundaries (1970, 9999). |
| DELIVER | Land parser + `days_from_civil` + `read()` signature delta + `parse_time_range` + `run_read_with` repoint + `write_usage` text delta + new test + `[[test]]` block in ONE atomic commit |
| DELIVER | DO NOT edit `.github/workflows/ci.yml` (A1 + A2) |
| DELIVER | DO NOT modify `tests/observe_otlp_read_flag.rs` (DISCUSS / DD4; locked oracle for OK2) |
| DELIVER | DO NOT modify `tests/observe_otlp_flag.rs` (DISCUSS / DD4; locked oracle for OK2) |
| DELIVER | DO NOT add any external crate (A3) |
| DELIVER | DO NOT add `chrono`, `time`, or `jiff` (DESIGN DD3, DD4) |
| DELIVER | DO NOT modify `crates/lumen/src/record.rs` `TimeRange` (DESIGN DD5 item 1; CLI inherits half-open contract) |
| DELIVER | DO NOT add `-p kaleidoscope-cli` to Gate 2 / Gate 3 (no graduation) |
| DELIVER | 100% mutation kill on `src/lib.rs` + `src/main.rs` before review approval |
| DELIVER | Cite Hinnant URL + public-domain dedication in the Rust source comment above `days_from_civil` per DESIGN DD3 / Atlas HIGH item 1; verify `LICENSING.md`'s `## Third-party algorithms` section is in place (it was added during DESIGN's peer-review pass) |
| DELIVER | Existing tests (`stats_subcommand.rs`, `stats_cinder_tier_distribution.rs`, `observe_otlp_flag.rs`, `observe_otlp_cinder_wiring.rs`, `observe_otlp_read_flag.rs`, `ingest_and_read_roundtrip.rs`) MUST pass unchanged |

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**Deliverables produced**:

| Artefact | Path |
|----------|------|
| Environment inventory | `docs/feature/cli-read-time-range-v0/devops/environments.yaml` |
| Wave decisions (this file) | `docs/feature/cli-read-time-range-v0/devops/wave-decisions.md` |
| Per-KPI instrumentation | `docs/feature/cli-read-time-range-v0/devops/kpi-instrumentation.md` |
| CI/CD pipeline confirmation | `docs/feature/cli-read-time-range-v0/devops/ci-cd-pipeline.md` |

---

## Forward-compatibility notes

### Parser timezone-offset extension

DESIGN DD3 defers `+00:00` / `+0000` offsets and lowercase `z` to
a future extension. Currently-accepted shapes remain accepted
under any extension. THAT feature's DESIGN wave owns the
witness-set expansion. This feature does NOT trigger.

### ReadOptions builder graduation

DESIGN DD1 rejected a builder at N=2 optional knobs. A third
optional knob on `read()` (severity / body / attribute filters,
all out of scope per DD5 item 5) SHOULD revisit the graduation per
Principle 4. This feature does NOT trigger.

### Test harness rule-of-three extraction

DISCUSS D7 defers extraction of `tenant`/`record`/`temp_root`/
`cleanup`/`ndjson` to `tests/common/mod.rs`. This feature is the
SIXTH inline duplication; the rule of three has been quintuply
discharged. Extraction is overdue but NOT undertaken here (would
conflate acceptance landing with cross-file refactor risk); the
next test-touching feature SHOULD propose it as a paired DD.

### Mutation kill-rate protocol (DELIVER)

1. After tests turn GREEN: `cargo mutants --package
   kaleidoscope-cli --in-diff <(git diff origin/main HEAD --
   crates/kaleidoscope-cli/src/lib.rs
   crates/kaleidoscope-cli/src/main.rs)`.
2. `mutants.out/summary.txt` "undetected" MUST be zero.
3. Survivor-to-probe map: length/separator/digit-range -> OK4 with
   targeted positional witness (`"2026X05-18T00:00:00Z"` etc);
   field-range -> OK1 with month=12/day=31/hour=23/minute=59/
   second=59 boundary witness; year-range -> parser unit test with
   `1970-01-01T00:00:00Z` (lower) and `9999-12-31T23:59:59.999999999Z`
   (upper); `days_from_civil` arithmetic -> round-trip property
   test over Hinnant boundary cases (era transitions, century
   boundaries); fractional-digit left-pad -> OK1 witnesses with
   nanos-of-second both having and lacking trailing zeros;
   `parse_time_range` argv binding -> OK3 with asymmetric witness
   counts so flag swap surfaces as count disagreement; half-bounded
   default -> OK3 witness at exactly `0` and near `u64::MAX`;
   fail-before-store-open -> OK4 filesystem-absence probe.
4. CI oracle: existing `gate-5-mutants-kaleidoscope-cli` on
   merge - surface auto-discovered via `--in-diff`.

Prior precedent: commit 4d20c31 plus `cli-read-observe-otlp-v0`,
`cli-stats-subcommand-v0`, and `cli-stats-cinder-tier-distribution-v0`
DELIVER waves all hit 100% kill under the same job. This feature
is the FIFTH consecutive realisation of the zero-workflow-edit
per-package Gate 5 cycle. The 2baa05c investment continues to
compound.
