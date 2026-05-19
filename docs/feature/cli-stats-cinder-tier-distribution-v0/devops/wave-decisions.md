# Wave Decisions - `cli-stats-cinder-tier-distribution-v0` / DEVOPS

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19
- **Mode**: Decisions pre-taken with Andrea (A1-A4); recorded
  verbatim. Mirrors the prior `cli-stats-subcommand-v0` DEVOPS
  posture; inherits the zero-workflow-edit posture established by
  `cli-cinder-otlp-wiring-v0`, `cli-read-observe-otlp-v0`, and the
  immediate predecessor `cli-stats-subcommand-v0`.

## Inputs read

1. `CLAUDE.md` - Rust idiomatic; per-feature MT 100% kill per
   ADR-0005 Gate 5.
2. `discuss/outcome-kpis.md` - OK1 (per-tier counts byte-equal to
   `list_by_tier(tenant, tier).len()`), OK2 (cross-tenant
   isolation), OK3 (empty-Lumen + Option B selective emission),
   OK4 (byte-equivalent to predecessor for zero-Cinder tenants).
3. `design/wave-decisions.md` - DD1 (Option A: new
   `stats_with_tiers` sibling; existing `stats()` untouched; main
   repointed), DD2 (hardcoded `[Tier::Hot, Tier::Warm, Tier::Cold]`
   iteration + `if count > 0` guard), DD3 (`FileBackedTieringStore
   ::open(cinder_base(data_dir), Box::new(CinderRecorder))`), DD4
   (Option B selective emission), DD5 (RCA: EXTEND shape + 14
   REUSE constructs, zero new types/traits/modules/deps), DD6
   (out-of-scope confirmations), DEVOPS handoff annotation.
4. `docs/feature/cli-stats-subcommand-v0/devops/*` - template.
5. `.github/workflows/ci.yml:949-1028` -
   `gate-5-mutants-kaleidoscope-cli` confirmed path-filtered on
   `crates/kaleidoscope-cli/**` (line 1006).
6. `crates/kaleidoscope-cli/Cargo.toml` - `aegis`, `cinder`,
   `lumen`, `self-observe`, `pulse` already declared (DESIGN
   handoff); the new `[[test]]` block is the only addition.

## Pre-wave decisions (carried in)

| D# | Decision | Value |
|----|----------|-------|
| D1 | `deployment_target` | N/A (CLI subcommand; binary shape unchanged) |
| D2 | `container_orchestration` | N/A |
| D3 | `cicd_platform` | GitHub Actions (existing) |
| D4 | `existing_infrastructure` | Yes (workspace + five-gate CI + per-pkg Gate 5) |
| D5 | `observability_and_logging` | N/A (quiescent recorders both sides; no OTLP from `stats_with_tiers`) |
| D6 | `deployment_strategy` | N/A |
| D7 | `continuous_learning` | No (single-feature, no A/B, no flags) |
| D8 | `git_branching_strategy` | Trunk-Based Development |
| D9 | `mutation_testing_strategy` | Per-feature, 100% kill per ADR-0005 Gate 5 |

## Differences from the cli-stats-subcommand-v0 template

1. **New sibling function, not a brand-new public surface.** Prior
   wave introduced `stats()`; this wave adds `stats_with_tiers()`
   alongside (DD1 Option A). Additive on the library side; the
   `main.rs::run_stats` dispatcher is repointed from `stats` to
   `stats_with_tiers` (one-line change). The locked
   `tests/stats_subcommand.rs` continues to import and exercise
   the original `stats()` UNMODIFIED, serving as the byte-level
   oracle for OK4.
2. **No new private formatter.** Prior wave added a hand-rolled
   ISO 8601 formatter (DD1). This wave reuses it verbatim via the
   inherited Lumen-side body (DD5 REUSE). Mutation surface for
   the new code path is the Cinder iteration loop only.
3. **Two-recorder construction site.** The body holds both a
   quiescent `LumenToPulseRecorder` (Lumen side, inherited from
   `stats()`) and a `CinderRecorder` alias for `NoopRecorder`
   (Cinder side, inherited from `ingest()`'s no-flag arm, DD3).
   Both quiescent; no OTLP file is created.

## In-wave decisions (A = Apex)

### [A1] No new CI workflow edit - INHERIT existing Gate 5 job

**Options**: (1) inherit `gate-5-mutants-kaleidoscope-cli` via
its `--in-diff` filter; (2) per-file Gate 5 fan-out; (3) skip
Gate 5 as "small surface".

**Verdict**: **Option 1** - **INHERIT** (pre-decided by Andrea).

**Rationale**:

- **Prior waves' investment pays off (fourth realisation).**
  Commit 2baa05c added the job precisely so subsequent crate
  features cost zero workflow edits. The `--in-diff` cascade
  (`origin/main` -> `HEAD~1` -> full) auto-picks up the diff on
  `src/lib.rs` (new `stats_with_tiers()`) and `src/main.rs` (the
  one-line `run_stats` repoint) on the merge commit. The path
  filter at ci.yml:1006 (`crates/kaleidoscope-cli/**`) matches
  both files structurally.
- **Per-file fan-out is premature** at N=2 modified files (same
  shape as predecessor wave). The existing job mutates both in
  one pass; fan-out adds runner cost with no diagnostic gain.
- **Skipping Gate 5** violates CLAUDE.md per-feature MT. The
  compile-green mutation classes here are exactly the ones the
  operator cannot tell apart from correct behaviour:
  - `if count > 0` -> `if count >= 0` (would silently emit
    `hot=0\n` / `warm=0\n` / `cold=0\n` lines for every
    invocation, breaking OK4 byte-equivalence).
  - The `for tier in [Tier::Hot, Tier::Warm, Tier::Cold]` array
    literal -> any reordering (would break DISCUSS D8's
    `hot` -> `warm` -> `cold` ordering contract; caught by OK1
    multi-tier test asserting line indices 4/5/6 in order).
  - `match tier { Tier::Hot => "hot", ... }` -> swapping any two
    arms (would silently print `warm=H` for the Hot tier; caught
    by OK1 multi-tier test asserting key-to-count agreement).
  - `list_by_tier(tenant, tier).len()` -> `.len() + 1` or
    `.len() - 1` or constant (caught by OK1 exact-count assertion
    against deterministic seed counts 5/12/47).
  - Eliding the `writeln!` -> caught by OK1 line-count assertion.
  Gate 5 is the mechanical oracle for all of these.

**Verdict**: NO edit to `.github/workflows/ci.yml`. Fourth
consecutive wave at zero workflow churn under the same job.

### [A2] Gate 1 inherits via `[[test]]` block - ZERO workflow edits total

**Verdict**: No Gate 1 workflow edit (pre-decided). `cargo test
--workspace --all-targets --locked` (ci.yml:182) auto-discovers
via the `[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml`:

```toml
[[test]]
name = "stats_cinder_tier_distribution"
path = "tests/stats_cinder_tier_distribution.rs"
```

A1 + A2 = **ZERO workflow edits**. ci.yml is byte-untouched.
Crafty lands the new function (`src/lib.rs`), the dispatcher
repoint (`src/main.rs`), the new test, and the `[[test]]` block
in ONE atomic commit per ADR-0005's "tests and source land
together" rule.

Trade-off: a malformed `[[test]]` block fails Gate 1 for the
whole workspace. Correct fail-fast.

### [A3] Zero new external dependencies

**Verdict**: Zero new crate (pre-decided). Verified by DD5 RCA +
DESIGN handoff: all used types (`Tier`, `ItemId`,
`FileBackedTieringStore`, `TieringStore`, `NoopRecorder` alias
`CinderRecorder`) are already in `kaleidoscope-cli`'s use list at
`crates/kaleidoscope-cli/src/lib.rs:56-59`. The new test uses
`tempfile` (already a dev-dependency in the sibling test files).
Zero `[dependencies]`, zero `[dev-dependencies]`, zero
`deny.toml` change. Only `Cargo.toml` addition is the `[[test]]`
block above.

### [A4] No new toolchain pin

**Verdict**: Inherits workspace stable Rust
(`rust-toolchain.toml`). No Gate 2/3 graduation (binary crate).
The `stats_with_tiers()` body uses no unstable features; the
`for tier in [Tier::Hot, Tier::Warm, Tier::Cold]` array literal,
the `if count > 0` guard, and the `match tier { ... }` are stable
constructs available on every supported MSRV.

## Skipped artefacts (N/A)

`platform-architecture.md` (Morgan's app-architecture
sufficient), `observability-design.md` (D5 + DESIGN handoff: no
OTLP), `monitoring-alerting.md` (CI gates ARE alerts),
`infrastructure-integration.md` (no external integrations),
`branching-strategy.md` (D8), `continuous-learning.md` (D7).

## Constraints for DISTILL / DELIVER

| When | What |
|------|------|
| DISTILL | Author `tests/stats_cinder_tier_distribution.rs` with RED scenarios for OK1 (populated-multi-tier + hot-only), OK2 (tenant-isolation), OK3 (orphan-cinder), OK4 (backwards-compat for zero-Cinder + positive-Lumen) + `[[test]]` block |
| DISTILL | Author as `kaleidoscope_cli::stats_with_tiers(...)` library calls into a `Vec<u8>` writer (per DD1), NOT subprocess invocations |
| DISTILL | Seed Cinder via direct `FileBackedTieringStore::open + place()` (NOT `ingest()` which auto-places one Hot per batch and would conflate OK4 backwards-compat with placement side-effects) |
| DELIVER | Land new function + dispatcher repoint + test + `[[test]]` block in ONE atomic commit |
| DELIVER | DO NOT edit `.github/workflows/ci.yml` (A1 + A2) |
| DELIVER | DO NOT modify `tests/stats_subcommand.rs` (DISCUSS D10; locked oracle for OK4) |
| DELIVER | DO NOT add any external crate (A3) |
| DELIVER | DO NOT add `-p kaleidoscope-cli` to Gate 2 / Gate 3 (no graduation) |
| DELIVER | DO NOT add `Tier::all()` to the `cinder` crate (DD2 hardcodes the array) |
| DELIVER | 100% mutation kill on `src/lib.rs` + `src/main.rs` before review approval |
| DELIVER | Existing tests (`stats_subcommand.rs`, `observe_otlp_flag.rs`, `observe_otlp_cinder_wiring.rs`, `observe_otlp_read_flag.rs`, `ingest_and_read_roundtrip.rs`) MUST pass unchanged |

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**Deliverables produced**:

| Artefact | Path |
|----------|------|
| Environment inventory | `docs/feature/cli-stats-cinder-tier-distribution-v0/devops/environments.yaml` |
| Wave decisions (this file) | `docs/feature/cli-stats-cinder-tier-distribution-v0/devops/wave-decisions.md` |
| Per-KPI instrumentation | `docs/feature/cli-stats-cinder-tier-distribution-v0/devops/kpi-instrumentation.md` |
| CI/CD pipeline confirmation | `docs/feature/cli-stats-cinder-tier-distribution-v0/devops/ci-cd-pipeline.md` |

---

## Forward-compatibility notes

### Tier::all() extraction

DD2 explicitly defers a `Tier::all()` associated function on the
`cinder` crate (rule-of-three: only one in-crate consumer today).
If a second consumer of "iterate all tiers in canonical order"
appears, that feature's DESIGN wave SHOULD extract the array
literal `[Tier::Hot, Tier::Warm, Tier::Cold]` into
`cinder::Tier::all()` (or `pub const TIERS_FORWARD: [Tier; 3]`)
and update both call sites. This feature does NOT trigger the
graduation.

### Original `stats()` deletion

DD1 leaves the original `stats()` reachable from the locked
`tests/stats_subcommand.rs`. Its primary justification is as the
byte-level oracle for OK4. If a future feature renegotiates the
locked test file (e.g. a v1 cleanup pass), THAT feature's DESIGN
wave owns the public-API contraction and the corresponding ADR
update (per DD6 item 10). This feature does NOT trigger the
contraction.

### Mutation kill-rate protocol (DELIVER)

1. After tests turn GREEN: `cargo mutants --package
   kaleidoscope-cli --in-diff <(git diff origin/main HEAD --
   crates/kaleidoscope-cli/src/lib.rs
   crates/kaleidoscope-cli/src/main.rs)`.
2. `mutants.out/summary.txt` "undetected" MUST be zero.
3. Survivors -> tighten:
   - `if count > 0` -> `>=` survivor: add zero-Cinder-tenant byte
     probe asserting absence of `hot=0` / `warm=0` / `cold=0`
     lines (OK4 multi-tier).
   - `match tier` arm-swap survivor: tighten OK1 multi-tier
     scenario to use three DISTINCT counts (5/12/47) so any swap
     surfaces as a key-count disagreement.
   - Array-literal reorder survivor: tighten OK1 multi-tier to
     assert exact line ordering (lines 4/5/6 in `hot` -> `warm`
     -> `cold` sequence).
   - `list_by_tier(tenant, tier).len()` count-off survivor:
     covered by exact-count assertion against deterministic seed.
4. CI oracle: existing `gate-5-mutants-kaleidoscope-cli` on
   merge - surface auto-discovered via `--in-diff`.

Prior precedent: commit 4d20c31 hit 6/6 = 100% kill on Cinder
wiring; `cli-read-observe-otlp-v0` and `cli-stats-subcommand-v0`
DELIVER waves both mutated their respective surfaces to 100%
kill under the same job. This feature is the fourth consecutive
realisation of the zero-workflow-edit per-package Gate 5 cycle.
