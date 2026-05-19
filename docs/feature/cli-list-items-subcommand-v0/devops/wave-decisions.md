# Wave Decisions - `cli-list-items-subcommand-v0` / DEVOPS

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19
- **Mode**: Decisions pre-taken with Andrea (A1-A4); recorded
  verbatim. Inherits the zero-workflow-edit posture established by
  the NINE prior `kaleidoscope-cli` features
  (`cli-cinder-otlp-wiring-v0`, `cli-read-observe-otlp-v0`,
  `cli-stats-subcommand-v0`, `cli-stats-cinder-tier-distribution-v0`,
  `cli-read-time-range-v0`, `cli-stats-time-range-v0`,
  `cli-migrate-subcommand-v0`, `cli-migrate-observe-otlp-v0`, plus
  the implicit baseline `cli-cinder-otlp-wiring-v0` precursor). **This
  is the TENTH consecutive zero-workflow-edit wave on the
  `kaleidoscope-cli` package.** The one-off investment in
  `gate-5-mutants-kaleidoscope-cli` (commit 2baa05c) has now amortised
  across ten features.

## Inputs read

1. `CLAUDE.md` - Rust idiomatic; per-feature MT 100% kill per ADR-0005
   Gate 5.
2. `discuss/outcome-kpis.md` - OK1 (list-items correctness - principal /
   North Star: stdout lines equal the lex-sorted `list_by_tier` result),
   OK2 (tenant isolation - guardrail), OK3 (invalid-tier fail-fast -
   guardrail; reuses `Error::InvalidTier`).
3. `design/wave-decisions.md` - DD1 (`pub fn list_items(tenant, data_dir,
   tier_arg, writer) -> Result<(), Error>`), DD2 (`Vec::sort_unstable`
   at the CLI boundary), DD3 (no stderr summary line on success), DD4
   (`parse_tier` promoted to `pub(crate)`), DD5 (reuse
   `Error::InvalidTier` `Display` verbatim).
4. `docs/feature/cli-migrate-observe-otlp-v0/devops/*` - template (9th
   wave under the same posture; this is the 10th).
5. `.github/workflows/ci.yml` (implicit) - `gate-5-mutants-kaleidoscope-cli`
   confirmed path-filtered on `crates/kaleidoscope-cli/**`; `--in-diff`
   cascade unchanged.
6. `crates/kaleidoscope-cli/Cargo.toml` (implicit) - `aegis`, `cinder`,
   `lumen`, `self-observe`, `pulse`, `tempfile` (dev) already declared.
   Only addition is one new `[[test]]` block.

## Pre-wave decisions (carried in)

| D# | Decision | Value |
|----|----------|-------|
| D1 | `deployment_target` | N/A (CLI subcommand; binary shape unchanged) |
| D2 | `container_orchestration` | N/A |
| D3 | `cicd_platform` | GitHub Actions (existing) |
| D4 | `existing_infrastructure` | Yes (workspace + five-gate CI + per-pkg Gate 5) |
| D5 | `observability_and_logging` | N/A (read-only; no operator-visible event to record; Cinder's `MetricsRecorder` trait has no `record_list` method) |
| D6 | `deployment_strategy` | N/A |
| D7 | `continuous_learning` | No (single-feature, no A/B, no flags) |
| D8 | `git_branching_strategy` | Trunk-Based Development (pure trunk; no required-status-checks; no enforce_admins) |
| D9 | `mutation_testing_strategy` | Per-feature, 100% kill per ADR-0005 Gate 5 |

## In-wave decisions (A = Apex)

### [A1] No new CI workflow edit - INHERIT existing Gate 5 job

**Verdict**: **INHERIT** (pre-decided by Andrea). The existing
`gate-5-mutants-kaleidoscope-cli` job auto-covers the two modified
source files (`src/lib.rs`, `src/main.rs`) via its `--in-diff` cascade
and `crates/kaleidoscope-cli/**` path filter. TENTH consecutive
realisation. The mutation surface added by this feature is small and
entirely acceptance-test-observable (DD1's `list_items(...)` body:
parse -> open -> `list_by_tier` -> `sort_unstable` -> `writeln!` loop):
removing the `sort_unstable()` call is killed by OK1's determinism
sub-scenario (two successive invocations yield byte-identical stdout
ONLY because the boundary sort masks `HashMap` iteration randomness);
swapping the `Some("list-items")` dispatch arm with the `Some("migrate")`
arm is killed by Gate 1 at the binary smoke level; replacing the
`writeln!(writer, "{}", id.0)` with `write!` (no `\n`) is killed by
OK1's byte-exact stdout assertion (`acme/batch-00007\nacme/batch-00041\nacme/batch-00099\n`);
inverting the tier filter (e.g. returning `Hot` instead of the requested
`Cold`) is killed by OK1's decoy-Hot-item exclusion assertion;
short-circuiting the per-tenant filter is killed by OK2 (tenant-isolation
scenario); moving `parse_tier(...)?` AFTER `FileBackedTieringStore::open(...)?`
is killed by OK3 (the parse error MUST short-circuit before the store
is opened).

### [A2] Gate 1 inherits via `[[test]]` block - ZERO workflow edits total

**Verdict**: No Gate 1 workflow edit (pre-decided). `cargo test
--workspace --all-targets --locked` auto-discovers via the new
`[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml`:

```toml
[[test]]
name = "list_items_subcommand"
path = "tests/list_items_subcommand.rs"
```

A1 + A2 = **ZERO workflow edits**. ci.yml is byte-untouched. Crafty
lands the new `list_items(...)` free function (`src/lib.rs`), the
`parse_tier` visibility promotion to `pub(crate)` (`src/lib.rs`), the
new `run_list_items` binary-side helper plus the `Some("list-items")`
dispatch arm and usage-text update (`src/main.rs`), the new test file
(`tests/list_items_subcommand.rs`), and the `[[test]]` block
(`Cargo.toml`) in ONE atomic commit per ADR-0005's "tests and source
land together" rule.

### [A3] Zero new external dependencies

**Verdict**: Zero new crate (pre-decided). Verified by DESIGN reuse
verdict (eleven existing constructs reused; four new things created -
all internal to the crate). `cinder_base`, `FileBackedTieringStore::open`,
`cinder::NoopRecorder` (aliased `CinderRecorder`),
`TieringStore::list_by_tier`, `ItemId` and its `Ord`, `Tier`,
`parse_tier`, `Error::InvalidTier`, `Error::CinderOpen`, `Error::Io`,
`TenantId` are all already in scope. `Vec::sort_unstable()` and
`writeln!` are core/std. Zero `[dependencies]`, zero
`[dev-dependencies]`, zero `deny.toml` change. Only `Cargo.toml`
addition is the `[[test]]` block above.

### [A4] No new toolchain pin

**Verdict**: Inherits workspace stable Rust (`rust-toolchain.toml`).
No Gate 2/3 graduation (binary crate). `pub(crate)`, free functions
on byte slices, `Vec::sort_unstable`, `writeln!`, `Result<(), Error>`,
and `impl Write` parameters are all stable since edition 2018; nothing
in DD1-DD5 reaches for a nightly feature or a recent MSRV bump.

## Skipped artefacts (N/A)

`platform-architecture.md` (Morgan's app-architecture sufficient),
`observability-design.md` (no dashboard or alert added; `list_by_tier`
emits no operator-visible event - Cinder's `MetricsRecorder` trait has
no `record_list` method), `monitoring-alerting.md` (CI gates ARE
alerts), `infrastructure-integration.md` (no external integrations -
pure local Cinder WAL read; no HTTP, no webhook, no third-party API),
`branching-strategy.md` (D8: pure trunk), `continuous-learning.md`
(D7).

## Constraints for DISTILL / DELIVER

| When | What |
|------|------|
| DISTILL | Author `tests/list_items_subcommand.rs` with RED scenarios: OK1 happy path (pre-place three items in Cold for `acme` in non-lex insertion order plus a decoy Hot item; call `list_items(&acme, &dir, "cold", &mut stdout_sink)`; assert captured stdout EQUALS `acme/batch-00007\nacme/batch-00041\nacme/batch-00099\n`); OK1 determinism (call the function TWICE; assert both captured stdouts are byte-identical); OK1 N=0 (call with `tier_arg = "warm"` against a tenant with zero warm items; assert captured stdout is empty); OK2 tenant isolation (pre-place `shared/batch-00042` in Cold for BOTH `acme` and `globex`; call with `(acme, data_dir, "cold")`; assert captured stdout contains exactly one line `shared/batch-00042\n` AND a follow-up `cinder.list_by_tier(globex, Tier::Cold)` returns a `Vec` whose contents (after lex sort) equal the pre-call state); OK3 invalid tier (two sub-scenarios: `tier_arg = "COLD"` and `tier_arg = "lukewarm"`; each asserts `Err`, empty stdout, stderr containing the invalid value verbatim, AND `cinder.list_by_tier(acme, Tier::Hot).len()` unchanged from pre-call). |
| DISTILL | Inline-duplicate the test harness helpers (`tenant`, `temp_root`, `cleanup`, `cinder_base`, `place_item`, `bin`) per DESIGN's eighth-inline-duplication note; rule-of-three extraction is a separate refactoring task (TENTH inline duplication; rule of three nonuply discharged). |
| DELIVER | Land the new `list_items(...)` free function in `lib.rs` + the `parse_tier` visibility promotion to `pub(crate)` + the new `run_list_items` helper in `main.rs` + the `Some("list-items")` dispatch arm + the usage-text update + the new test file + the `[[test]]` block in ONE atomic commit. |
| DELIVER | DO NOT edit `.github/workflows/ci.yml` (A1 + A2). |
| DELIVER | DO NOT edit assertions in any locked test file. The ten prior `tests/*.rs` files (`stats_subcommand.rs`, `stats_cinder_tier_distribution.rs`, `stats_time_range.rs`, `read_time_range.rs`, `observe_otlp_flag.rs`, `observe_otlp_cinder_wiring.rs`, `observe_otlp_read_flag.rs`, `migrate_subcommand.rs`, `migrate_observe_otlp_flag.rs`, `ingest_and_read_roundtrip.rs`, `cli_binary_smoke.rs`) are byte-untouched. ANY non-mechanical edit to any of them in the DELIVER commit's diff auto-rejects review. |
| DELIVER | DO NOT introduce a new flag (`--observe-otlp`, `--json`, `--format=...`, `--limit`, `--offset`) per DESIGN D-OutOfScope-Observe and `discuss/outcome-kpis.md` §"Handoff to DESIGN" item 1. `list_by_tier` is a pure read with no operator-visible event to record. |
| DELIVER | DO NOT introduce a new `Error` variant. `Error::InvalidTier`, `Error::CinderOpen`, `Error::Io` are reused via direct delegation (DESIGN DD5). |
| DELIVER | DO NOT introduce a stderr summary line on success (DD3). Stderr is empty on the happy path. |
| DELIVER | DO NOT add `-p kaleidoscope-cli` to Gate 2 / Gate 3 (no graduation; A4). |
| DELIVER | 100% mutation kill on `src/lib.rs` + `src/main.rs` before review approval. The new surface is small and fully acceptance-observable; no expected survivors. The wire-invisible `SystemTime::now()` -> `UNIX_EPOCH` survivor acknowledged in prior waves does NOT apply here (the `list_items` function does not invoke any clock; the `CinderRecorder` is quiescent and `list_by_tier` does not invoke any recorder method per Cinder's `MetricsRecorder` trait shape). |
| DELIVER | Existing tests MUST pass unchanged in assertions. No mechanical signature-match suffixes are needed this wave (no signature growth on any existing function; `list_items` is a brand-new free function). |

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**Deliverables produced**:

| Artefact | Path |
|----------|------|
| Environment inventory | `docs/feature/cli-list-items-subcommand-v0/devops/environments.yaml` |
| Wave decisions (this file) | `docs/feature/cli-list-items-subcommand-v0/devops/wave-decisions.md` |
| Per-KPI instrumentation | `docs/feature/cli-list-items-subcommand-v0/devops/kpi-instrumentation.md` |
| CI/CD pipeline confirmation | `docs/feature/cli-list-items-subcommand-v0/devops/ci-cd-pipeline.md` |

---

## Forward-compatibility notes

### Dashboard panel: NONE

The operator's existing dashboard chain (sidecar -> OTLP/HTTP collector
-> dashboard) is not touched by this feature. `list_by_tier` emits no
OTLP and no metric: Cinder's `MetricsRecorder` trait has no
`record_list` method, by deliberate ADR-0039 §1 design (the trait
covers state-mutating ports only). A future `list-items --observe-otlp`
would require a sibling trait method (`record_list`) on the Cinder side
FIRST; that is out of scope for v0 and explicitly punted by
`outcome-kpis.md` "DEVOPS instrumentation needs".

### Cross-tenant aggregation, pagination, structured output: NONE

DESIGN locks NO flags (`--observe-otlp`, `--json`, `--format=...`,
`--limit`, `--offset`), no pagination, no cross-tenant aggregate, no
historical state, no structured output formats. Any future iteration
proposing one of these MUST do so as a separate feature with its own
DISCUSS-DESIGN-DEVOPS cycle; this wave does not pre-bake any of them.

### Test harness rule-of-three extraction

The new test file is the TENTH inline duplication of the `tenant`,
`temp_root`, `cleanup`, `cinder_base`, `place_item`, `bin` helper
cluster. The rule of three has been nonuply discharged. Extraction to
`tests/common/mod.rs` remains overdue but NOT undertaken here (would
conflate acceptance landing with cross-file refactor risk); the next
test-touching feature SHOULD propose it as a paired DD. This is the
SAME punt as the prior nine waves; the constancy of the deferment is
itself evidence that no individual wave can absorb the cross-file risk.

### Mutation kill-rate protocol (DELIVER)

1. After tests turn GREEN: `cargo mutants --package kaleidoscope-cli
   --in-diff <(git diff origin/main HEAD --
   crates/kaleidoscope-cli/src/lib.rs
   crates/kaleidoscope-cli/src/main.rs)`.
2. `mutants.out/summary.txt` "undetected" MUST be zero. No inherited
   survivor applies (no clock; no recorder; no time-bearing OTLP line).
3. Survivor-to-probe map for the new surface: removing `sort_unstable()`
   killed by OK1's determinism sub-scenario; inverting the tier filter
   killed by OK1's decoy-Hot exclusion; `writeln!` -> `write!` killed by
   OK1's byte-exact stdout assertion (terminating `\n`); short-circuiting
   the per-tenant filter killed by OK2; reordering `parse_tier` and
   `FileBackedTieringStore::open` killed by OK3's "store-unopened"
   guardrail.
4. CI oracle: existing `gate-5-mutants-kaleidoscope-cli` on merge -
   surface auto-discovered via `--in-diff` (A1).

Prior precedent: NINE consecutive `kaleidoscope-cli` DELIVER waves at
100% kill under the same job. This feature is the TENTH consecutive
realisation of the zero-workflow-edit per-package Gate 5 cycle. The
2baa05c investment continues to compound.
