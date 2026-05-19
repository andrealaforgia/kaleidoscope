# Wave Decisions - `cli-stats-time-range-v0` / DEVOPS

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19
- **Mode**: Decisions pre-taken with Andrea (A1-A4); recorded
  verbatim. Mirrors the prior `cli-read-time-range-v0` DEVOPS
  posture and inherits the zero-workflow-edit posture established
  by `cli-cinder-otlp-wiring-v0`, `cli-read-observe-otlp-v0`,
  `cli-stats-subcommand-v0`, `cli-stats-cinder-tier-distribution-v0`,
  and `cli-read-time-range-v0`. **This is the SIXTH consecutive
  zero-workflow-edit wave on the `kaleidoscope-cli` package.** The
  one-off investment in `gate-5-mutants-kaleidoscope-cli` (commit
  2baa05c) has now amortised across six features.

## Inputs read

1. `CLAUDE.md` - Rust idiomatic; per-feature MT 100% kill per
   ADR-0005 Gate 5.
2. `discuss/outcome-kpis.md` - OK1 (bounded-window record count;
   principal / North Star), OK2 (bounded-window earliest/latest
   derivation; leading), OK3 (Cinder lines unchanged - pins
   D-CinderScope; guardrail), OK4 (no-flag byte equivalence;
   guardrail).
3. `design/wave-decisions.md` - DD1 (Option A: 4th `range:
   TimeRange` parameter appended to `stats_with_tiers()`), DD2
   (Option (a): single function; Cinder branch ignores `range`),
   DD3 (D-EmptyWindow auto-handled by existing `if let (Some,
   Some)` arm), DD4 (locked-test mechanical update scoped to
   `stats_cinder_tier_distribution.rs` ONLY; five call sites; no
   assertion edits), DD5 (RCA: EXTEND `stats_with_tiers` +
   `run_stats_with` + `write_usage`; REUSE twelve existing
   constructs; CREATE NEW: zero), DD6 (out-of-scope
   confirmations), DEVOPS handoff annotation.
4. `docs/feature/cli-read-time-range-v0/devops/*` - template
   (5th wave under the same posture).
5. `.github/workflows/ci.yml` -
   `gate-5-mutants-kaleidoscope-cli` confirmed path-filtered on
   `crates/kaleidoscope-cli/**`; `--in-diff` cascade
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
| D5 | `observability_and_logging` | N/A (no new emission source; `--observe-otlp` is not on `stats` per DISCUSS) |
| D6 | `deployment_strategy` | N/A |
| D7 | `continuous_learning` | No (single-feature, no A/B, no flags) |
| D8 | `git_branching_strategy` | Trunk-Based Development (pure trunk; no required-status-checks; no enforce_admins) |
| D9 | `mutation_testing_strategy` | Per-feature, 100% kill per ADR-0005 Gate 5 |

## Differences from the cli-read-time-range-v0 template

1. **Thinner mutation surface.** The predecessor introduced the
   `parse_iso8601_utc_nanos` parser (~50 lines), `days_from_civil`
   (~15 lines), and `parse_time_range` (~25 lines) - a rich
   arithmetic-and-branching surface. This feature introduces
   ZERO new source-level constructs: the parser, the helpers, and
   the typed error are REUSED unchanged (DESIGN DD5). The only
   new source diff is one parameter declaration on
   `stats_with_tiers`, one token swap at the Lumen call site
   (`lumen.query(tenant, range)` replaces
   `lumen.query(tenant, TimeRange::all())`), and one new line in
   `run_stats_with` threading the parsed range. Three production
   source lines total.
2. **Two-branch observable surface, not one.** The predecessor
   filters one output stream (the NDJSON record stream). This
   feature has TWO observable branches in the same function -
   Lumen (filtered by `range`) and Cinder (unfiltered, per
   D-CinderScope). OK3's byte-identity assertion across two
   `TimeRange` invocations is the new guardrail shape unique to
   this feature.
3. **Locked-test edit scope narrower.** Predecessor required a
   mechanical 5th-arg update only at `ingest_and_read_roundtrip.rs`
   library-direct call sites. This feature requires mechanical
   4th-arg update at FIVE call sites in
   `stats_cinder_tier_distribution.rs` (DD4). `stats_subcommand.rs`
   requires no edits because it exercises only the legacy
   3-arg `stats()` function (DESIGN out-of-scope, DD6 item 1).
4. **No observability surface.** Same as predecessor: this is a
   query-shape change on stdout, not a new metric emission source.
   `stats` has no `--observe-otlp` flag (DISCUSS, line 233). No
   new metric, no new dashboard, no new alert.

## In-wave decisions (A = Apex)

### [A1] No new CI workflow edit - INHERIT existing Gate 5 job

**Options**: (1) inherit `gate-5-mutants-kaleidoscope-cli` via
its `--in-diff` filter; (2) per-file Gate 5 fan-out; (3) skip
Gate 5 as "no new arithmetic".

**Verdict**: **Option 1** - **INHERIT** (pre-decided by Andrea).

**Rationale**:

- **Amortising investment, sixth realisation.** Commit 2baa05c
  added `gate-5-mutants-kaleidoscope-cli` precisely so subsequent
  kaleidoscope-cli features would cost zero workflow edits. That
  one-off investment now amortises across six consecutive waves:
  `cli-cinder-otlp-wiring-v0`, `cli-read-observe-otlp-v0`,
  `cli-stats-subcommand-v0`, `cli-stats-cinder-tier-distribution-v0`,
  `cli-read-time-range-v0`, and now this feature. The `--in-diff`
  cascade (`origin/main` -> `HEAD~1` -> full) auto-picks up the
  three-line diff on `src/lib.rs` (parameter declaration plus
  token swap) and `src/main.rs` (one new `let range = ...;` line
  plus the `stats_with_tiers(...)` call-site update plus
  `write_usage` text delta) on the merge commit. The path filter
  on `crates/kaleidoscope-cli/**` matches both files structurally.
- **Per-file fan-out is still premature** at N=2 modified files
  (same shape as prior five waves). The existing job mutates both
  in one pass; fan-out adds runner cost with no diagnostic gain.
- **Skipping Gate 5 is the wrong option even on this thin diff.**
  The mutation surface is small but non-trivial: replacing
  `lumen.query(tenant, range)` with `lumen.query(tenant,
  TimeRange::all())` (parameter-drop mutation) would silently
  pass OK4 yet fail OK1/OK2 catastrophically. Other compile-green
  mutations include: swapping `parse_time_range` for a constant
  `TimeRange::all()` at `run_stats_with` (killed by OK1
  bounded-window assertions); reordering the Cinder loop to
  consume `range` (killed by OK3 byte-identity assertion); off-
  by-one in the open-upper boundary inherited from `lumen::query`
  (out of scope, killed by Lumen's own Gate 5). Each is killed
  by OK1/OK2/OK3/OK4 acceptance witnesses. Gate 5 is the
  mechanical oracle; the per-feature MT rule in `CLAUDE.md`
  requires it regardless of diff size.

**Verdict**: NO edit to `.github/workflows/ci.yml`. Sixth
consecutive wave at zero workflow churn under the same job.

### [A2] Gate 1 inherits via `[[test]]` block - ZERO workflow edits total

**Verdict**: No Gate 1 workflow edit (pre-decided). `cargo test
--workspace --all-targets --locked` auto-discovers via the
`[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml`:

```toml
[[test]]
name = "stats_time_range"
path = "tests/stats_time_range.rs"
```

A1 + A2 = **ZERO workflow edits**. ci.yml is byte-untouched.
Crafty lands the `stats_with_tiers` signature delta (`src/lib.rs`),
the Lumen call-site token swap (`src/lib.rs`), the
`run_stats_with` dispatcher delta (`src/main.rs`), the
`write_usage` text delta for the `stats` subcommand (`src/main.rs`),
the new test file, the mechanical 4th-arg update at
`stats_cinder_tier_distribution.rs`'s five call sites, and the
`[[test]]` block in ONE atomic commit per ADR-0005's "tests and
source land together" rule.

Trade-off: a malformed `[[test]]` block fails Gate 1 for the
whole workspace. Correct fail-fast.

### [A3] Zero new external dependencies

**Verdict**: Zero new crate (pre-decided). Verified by DESIGN
DD5 RCA + DESIGN handoff: all used types (`lumen::TimeRange`,
`aegis::TenantId`, std I/O traits) are already in
`kaleidoscope-cli`'s use list. The new test uses `tempfile`
(already a dev-dependency in the sibling test files). Zero
`[dependencies]`, zero `[dev-dependencies]`, zero `deny.toml`
change. Only `Cargo.toml` addition is the `[[test]]` block above.

Inherits the no-`chrono`/`time`/`jiff` posture verified by grep
at DESIGN time (zero matches across all `Cargo.toml` and all
`*.rs`).

### [A4] No new toolchain pin

**Verdict**: Inherits workspace stable Rust
(`rust-toolchain.toml`). No Gate 2/3 graduation (binary crate).
The signature delta uses no unstable features; all touched code
is positional-argument plumbing. The reused parser
(`parse_iso8601_utc_nanos`) and reused helpers (`parse_time_range`,
`parse_flag_iso`) were verified stable at the predecessor wave.

## Skipped artefacts (N/A)

`platform-architecture.md` (Morgan's app-architecture
sufficient), `observability-design.md` (DESIGN handoff: no OTLP;
`stats` has no `--observe-otlp`), `monitoring-alerting.md` (CI
gates ARE alerts), `infrastructure-integration.md` (no external
integrations), `branching-strategy.md` (D8: pure trunk),
`continuous-learning.md` (D7).

## Constraints for DISTILL / DELIVER

| When | What |
|------|------|
| DISTILL | Author `tests/stats_time_range.rs` with RED scenarios: OK1 (bounded `[200, 400)` over witnesses `{100,200,300,400,500}` asserting `records=2`); OK2 (same bounded run asserts `earliest=...000000200Z` and `latest=...000000300Z` - windowed, not global; companion empty-window scenario asserts `records=0\n` with NO `earliest=` / `latest=` lines per D-EmptyWindow); OK3 (Cinder byte-identity across two different `TimeRange` invocations on the same tenant/data_dir with non-zero seeding in all three tiers); OK4 (no-flag `TimeRange::all()` scenario asserts byte-equivalence with pre-feature `stats_with_tiers` stdout). |
| DISTILL | Author OK1/OK2/OK3/OK4 as `kaleidoscope_cli::stats_with_tiers(...)` library-direct calls into a `Vec<u8>` writer; the binary entry point form (`run_stats_with`) is testable in-process if the OK4 invalid-input fail-fast path needs to be probed via the binary entry shape. |
| DISTILL | Inline-duplicate the test harness helpers (`tenant`, `record`, `temp_root`, `cleanup`, `ndjson`, `cinder_base`, `lumen_base`, `seed_cinder`, `cinder_count`) per DISCUSS D-Test-file; rule-of-three extraction is a separate refactoring task (SEVENTH inline duplication; rule of three sextuply discharged). |
| DISTILL | OK1 witness records MUST include explicit boundary cases: a record at exactly `since_ns` (asserts closed-lower) AND a record at exactly `until_ns` (asserts open-upper, MUST be excluded). |
| DISTILL | OK3 MUST seed Cinder with non-zero placements in all three tiers (hot, warm, cold) for tenant `acme`, invoke `stats_with_tiers` twice with two materially different `TimeRange` values (e.g. `TimeRange::new(100, 200)` and `TimeRange::new(300, 400)`), and assert the substring of stdout matching `/^(hot|warm|cold)=\d+$/` lines is byte-identical between the two captures. |
| DISTILL | OK4 MUST run the no-flag scenario AND verify the two locked OK4-protection files (`stats_subcommand.rs`, `stats_cinder_tier_distribution.rs`) continue to pass green. |
| DELIVER | Land `stats_with_tiers` signature delta + Lumen call-site token swap + `run_stats_with` dispatcher delta + `write_usage` text delta + new test file + mechanical 4th-arg update at `stats_cinder_tier_distribution.rs`'s five call sites + `[[test]]` block in ONE atomic commit. |
| DELIVER | DO NOT edit `.github/workflows/ci.yml` (A1 + A2). |
| DELIVER | DO NOT modify `tests/stats_subcommand.rs` (DESIGN DD4; locked oracle for OK4 on the legacy 3-arg `stats()` function which this feature does not touch). |
| DELIVER | DO NOT modify any assertion text in `tests/stats_cinder_tier_distribution.rs` (DESIGN DD4; only call-site arguments change). |
| DELIVER | DO NOT modify the legacy `stats()` 3-arg function at `src/lib.rs:312-331` (DESIGN DD5 / DD6 item 1; out-of-scope). |
| DELIVER | DO NOT add any external crate (A3). |
| DELIVER | DO NOT add `chrono`, `time`, or `jiff` (predecessor DESIGN DD3, DD4; this feature inherits). |
| DELIVER | DO NOT modify `crates/lumen/src/record.rs` `TimeRange` (DESIGN DD5; CLI inherits half-open contract). |
| DELIVER | DO NOT touch the Cinder loop at `src/lib.rs:375-380` (DESIGN DD2 / D-CinderScope; structurally identical to today). |
| DELIVER | DO NOT add `-p kaleidoscope-cli` to Gate 2 / Gate 3 (no graduation). |
| DELIVER | 100% mutation kill on `src/lib.rs` + `src/main.rs` before review approval. |
| DELIVER | Existing tests (`stats_subcommand.rs`, `stats_cinder_tier_distribution.rs` post-mechanical-update, `observe_otlp_flag.rs`, `observe_otlp_cinder_wiring.rs`, `observe_otlp_read_flag.rs`, `ingest_and_read_roundtrip.rs`, `read_time_range.rs`) MUST pass unchanged (except DD4's mechanical 4th-arg edit). |

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**Deliverables produced**:

| Artefact | Path |
|----------|------|
| Environment inventory | `docs/feature/cli-stats-time-range-v0/devops/environments.yaml` |
| Wave decisions (this file) | `docs/feature/cli-stats-time-range-v0/devops/wave-decisions.md` |
| Per-KPI instrumentation | `docs/feature/cli-stats-time-range-v0/devops/kpi-instrumentation.md` |
| CI/CD pipeline confirmation | `docs/feature/cli-stats-time-range-v0/devops/ci-cd-pipeline.md` |

---

## Forward-compatibility notes

### `--cinder-at <ISO>` flag for time-bound Cinder snapshots

DISCUSS D-CinderScope explicitly defers a `--cinder-at <ISO>`
flag (time-bound Cinder snapshots) to a future feature. Such a
feature would touch the Cinder loop at `src/lib.rs:375-380` and
would warrant a new ADR per DESIGN DD6 item 9. This feature does
NOT trigger.

### Builder-pattern graduation for `stats_with_tiers`

DESIGN DD1 rejected a builder at N=1 optional knob. A second
optional knob on `stats_with_tiers` (e.g. severity / body
filters, or the `--cinder-at` flag above) SHOULD revisit the
graduation per Principle 4. This feature does NOT trigger.

### Test harness rule-of-three extraction

DISCUSS D-Test-file defers extraction of the harness helpers to
`tests/common/mod.rs`. This feature is the SEVENTH inline
duplication; the rule of three has been sextuply discharged.
Extraction is overdue but NOT undertaken here (would conflate
acceptance landing with cross-file refactor risk); the next
test-touching feature SHOULD propose it as a paired DD.

### Mutation kill-rate protocol (DELIVER)

1. After tests turn GREEN: `cargo mutants --package
   kaleidoscope-cli --in-diff <(git diff origin/main HEAD --
   crates/kaleidoscope-cli/src/lib.rs
   crates/kaleidoscope-cli/src/main.rs)`.
2. `mutants.out/summary.txt` "undetected" MUST be zero.
3. Survivor-to-probe map for the new surface:
   `lumen.query(tenant, range)` -> `lumen.query(tenant,
   TimeRange::all())` parameter-drop mutation killed by OK1
   bounded-window assertion (records=2 not 5); `range` consumed
   by Cinder loop mutation killed by OK3 byte-identity
   assertion; `parse_time_range(args)?` -> `TimeRange::all()`
   constant at `run_stats_with` killed by OK1; closed-lower /
   open-upper boundary inversions killed by exact-boundary
   witnesses at `since_ns` (200) and `until_ns` (400); empty-
   window `if let (Some, Some)` arm inversion killed by OK2
   empty-window scenario.
4. CI oracle: existing `gate-5-mutants-kaleidoscope-cli` on
   merge - surface auto-discovered via `--in-diff`.

Prior precedent: commits 4d20c31 (cli-cinder-otlp-wiring-v0),
plus `cli-read-observe-otlp-v0`, `cli-stats-subcommand-v0`,
`cli-stats-cinder-tier-distribution-v0`, and
`cli-read-time-range-v0` DELIVER waves all hit 100% kill under
the same job. This feature is the SIXTH consecutive realisation
of the zero-workflow-edit per-package Gate 5 cycle. The 2baa05c
investment continues to compound.
