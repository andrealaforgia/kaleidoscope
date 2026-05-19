# Wave Decisions - `cli-migrate-subcommand-v0` / DEVOPS

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19
- **Mode**: Decisions pre-taken with Andrea (A1-A4); recorded
  verbatim. Inherits the zero-workflow-edit posture established by
  `cli-cinder-otlp-wiring-v0`, `cli-read-observe-otlp-v0`,
  `cli-stats-subcommand-v0`, `cli-stats-cinder-tier-distribution-v0`,
  `cli-read-time-range-v0`, and `cli-stats-time-range-v0`. **This is
  the SEVENTH consecutive zero-workflow-edit wave on the
  `kaleidoscope-cli` package.** The one-off investment in
  `gate-5-mutants-kaleidoscope-cli` (commit 2baa05c) has now
  amortised across seven features.

## Inputs read

1. `CLAUDE.md` - Rust idiomatic; per-feature MT 100% kill per
   ADR-0005 Gate 5.
2. `discuss/outcome-kpis.md` - OK1 (migrate-success correctness;
   principal / North Star), OK2 (unknown-item fail-fast; leading),
   OK3 (invalid-tier fail-fast; leading), OK4 (idempotent same-tier;
   guardrail).
3. `design/wave-decisions.md` - DD1 (Option A: new free function
   `migrate(tenant, data_dir, item_id, to_tier_arg, writer) ->
   Result<(), Error>`), DD2 (pre-flight `get_entry` discovers the
   `from` tier; no special case for same-tier), DD3 (private
   `parse_tier(s: &str) -> Result<Tier, ()>` matching three literal
   lower-case strings, no trim), DD4 (two new `Error` variants:
   `InvalidTier { value: String }`, `CinderMigrate(MigrateError)`),
   DD5 (stderr wording locked by Display impls; produces
   `kaleidoscope-cli: cinder migrate: ...` and `kaleidoscope-cli:
   <to_tier> "...": expected one of hot, warm, cold`), DD6 (RCA:
   REUSE fourteen existing constructs; CREATE NEW: one private
   helper, two error variants, one public free function, one main.rs
   dispatch arm + run_migrate + usage paragraph), DD7 (out-of-scope
   confirmations), DEVOPS handoff annotation.
4. `docs/feature/cli-stats-time-range-v0/devops/*` - template (6th
   wave under the same posture; this is the 7th).
5. `.github/workflows/ci.yml` - `gate-5-mutants-kaleidoscope-cli`
   confirmed path-filtered on `crates/kaleidoscope-cli/**`;
   `--in-diff` cascade (`origin/main` -> `HEAD~1` -> full)
   unchanged.
6. `crates/kaleidoscope-cli/Cargo.toml` - `aegis`, `cinder`,
   `lumen`, `self-observe`, `pulse`, `tempfile` (dev) already
   declared; only addition is one new `[[test]]` block.

## Pre-wave decisions (carried in)

| D# | Decision | Value |
|----|----------|-------|
| D1 | `deployment_target` | N/A (CLI subcommand; binary shape unchanged) |
| D2 | `container_orchestration` | N/A |
| D3 | `cicd_platform` | GitHub Actions (existing) |
| D4 | `existing_infrastructure` | Yes (workspace + five-gate CI + per-pkg Gate 5) |
| D5 | `observability_and_logging` | N/A (no new emission source; `--observe-otlp` is not on `migrate` per DISCUSS D-OutOfScope-Observe) |
| D6 | `deployment_strategy` | N/A |
| D7 | `continuous_learning` | No (single-feature, no A/B, no flags) |
| D8 | `git_branching_strategy` | Trunk-Based Development (pure trunk; no required-status-checks; no enforce_admins) |
| D9 | `mutation_testing_strategy` | Per-feature, 100% kill per ADR-0005 Gate 5 |

## Differences from the cli-stats-time-range-v0 template

1. **Mutation surface shape differs.** The predecessor was a
   thin-diff parameter-threading feature (~3 production source
   lines: one parameter declaration, one call-site token swap, one
   `let range = ...` line). This feature introduces a wholly new
   public free function `migrate(...)` (~25 lines), a new private
   helper `parse_tier(s)` (~8 lines), two new `Error` variants plus
   Display arms (~6 lines), and a new `run_migrate` binary helper
   plus dispatch arm plus usage paragraph (~15 lines) - ~54 lines of
   new production source. Richer branching surface (the
   `parse_tier` four-way match, the `get_entry` None arm, the
   `migrate` Err arm, the `writeln!` line, the `flush()`); Gate 5
   has more mutants to kill but the same workflow inheritance
   applies.
2. **Mutation, not query.** Six prior `kaleidoscope-cli` waves
   shipped read-side or write-once surfaces (`ingest` writes;
   `read` and `stats` and the two time-range variants read). This
   is the first MUTATION surface on the CLI - the migrate
   subcommand is the only CLI path that calls a state-changing
   trait method (`TieringStore::migrate`) other than the implicit
   `place()` calls inside `ingest`. The DEVOPS posture is
   unchanged (CI gates are unchanged), but the acceptance test
   harness has a new shape: a before/after `get_entry().tier`
   oracle on every scenario plus a Lumen byte-equivalence guard
   (D-NoLumenTouch).
3. **Two new fail-fast branches, not one.** The predecessor had
   one fail-fast parse contract (`--since`/`--until` ISO parse).
   This feature introduces TWO independent fail-fast contracts in
   ONE subcommand: OK2 (unknown-item via pre-flight `get_entry`)
   AND OK3 (invalid tier via `parse_tier`). Two `Err`-returning
   branches with distinct stderr substrings; both probed under
   Gate 1; both mutated under Gate 5.
4. **Locked-test edit scope: zero.** Predecessor required a
   mechanical 4th-arg update at five `stats_with_tiers` call sites
   in `stats_cinder_tier_distribution.rs`. This feature requires
   ZERO edits to any locked test file (`stats_subcommand.rs`,
   `stats_cinder_tier_distribution.rs`, `stats_time_range.rs`,
   `read_time_range.rs`, `observe_otlp_flag.rs`,
   `observe_otlp_cinder_wiring.rs`, `observe_otlp_read_flag.rs`,
   `ingest_and_read_roundtrip.rs`). DESIGN DD7 item 9 pins it: the
   `migrate` free function is wholly new; existing functions are
   untouched. The cluster of seven locked test files collectively
   serves as the no-regression oracle.
5. **No observability surface.** Same as predecessor: `migrate`
   has no `--observe-otlp` flag (DISCUSS D-OutOfScope-Observe).
   The Cinder side opens with a `NoopRecorder` (DESIGN DD1 step
   2). No new metric, no new dashboard, no new alert.

## In-wave decisions (A = Apex)

### [A1] No new CI workflow edit - INHERIT existing Gate 5 job

**Options**: (1) inherit `gate-5-mutants-kaleidoscope-cli` via its
`--in-diff` filter; (2) per-file Gate 5 fan-out; (3) skip Gate 5 on
the grounds that the new function is "narrow."

**Verdict**: **Option 1** - **INHERIT** (pre-decided by Andrea).

**Rationale**:

- **Amortising investment, seventh realisation.** Commit 2baa05c
  added `gate-5-mutants-kaleidoscope-cli` precisely so subsequent
  kaleidoscope-cli features would cost zero workflow edits. That
  one-off investment now amortises across seven consecutive waves
  on the same package. The `--in-diff` cascade auto-picks up the
  three modified source files (`src/lib.rs`, `src/main.rs`) on the
  merge commit; the path filter `crates/kaleidoscope-cli/**`
  matches both structurally.
- **Per-file fan-out remains premature** at N=2 modified files.
  The existing job mutates both in one pass; fan-out adds runner
  cost with no diagnostic gain.
- **Skipping Gate 5 is the wrong option.** The mutation surface
  here is richer than the predecessor (~54 new source lines vs ~3):
  the `parse_tier` four-way match (mutating any arm to a different
  `Tier` variant compiles green and is killed only by OK1's
  exact-tier `get_entry().tier == to_tier` post-condition); the
  `get_entry()` `None` arm short-circuit (deleting the `?` and
  proceeding to `migrate` compiles green and silently calls
  `migrate` on an unknown item, killed by OK2's byte-equivalence
  Cinder snapshot assertion); the `writeln!` field-ordering (`from`
  and `to` swapped compiles green, killed by OK1's exact-bytes
  stdout assertion); the `tier_lowercase` call (replacing one
  invocation with the other compiles green for same-tier cases,
  killed by OK1's distinct `hot`/`warm` from/to scenario);
  `SystemTime::now()` -> `UNIX_EPOCH` compiles green but is not
  observable on the wire (the `migrated_at` field is not in the
  stdout report) and is therefore the one mutation Gate 5 cannot
  kill from acceptance witnesses alone - a small expected survivor
  surface acknowledged below.

**Verdict**: NO edit to `.github/workflows/ci.yml`. Seventh
consecutive wave at zero workflow churn under the same job.

### [A2] Gate 1 inherits via `[[test]]` block - ZERO workflow edits total

**Verdict**: No Gate 1 workflow edit (pre-decided). `cargo test
--workspace --all-targets --locked` auto-discovers via the new
`[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml`:

```toml
[[test]]
name = "migrate_subcommand"
path = "tests/migrate_subcommand.rs"
```

A1 + A2 = **ZERO workflow edits**. ci.yml is byte-untouched.
Crafty lands the new `migrate()` free function (`src/lib.rs`), the
new `parse_tier` private helper (`src/lib.rs`), the two new
`Error` variants + Display arms (`src/lib.rs`), the
`run_migrate` binary helper + dispatch arm + usage paragraph
(`src/main.rs`), the new test file
(`tests/migrate_subcommand.rs`), and the `[[test]]` block
(`Cargo.toml`) in ONE atomic commit per ADR-0005's "tests and
source land together" rule.

Trade-off: a malformed `[[test]]` block fails Gate 1 for the whole
workspace. Correct fail-fast.

### [A3] Zero new external dependencies

**Verdict**: Zero new crate (pre-decided). Verified by DESIGN DD6
RCA + DESIGN handoff: all used types (`TenantId`, `Tier`, `ItemId`,
`MigrateError`, `FileBackedTieringStore`, `TieringStore`,
`NoopRecorder` alias `CinderRecorder`) are already in
`kaleidoscope-cli`'s use list at `crates/kaleidoscope-cli/src/lib.rs:55-59`.
The new test uses `tempfile` (already a dev-dependency). Zero
`[dependencies]`, zero `[dev-dependencies]`, zero `deny.toml`
change. Only `Cargo.toml` addition is the `[[test]]` block above.

Inherits the no-`chrono`/`time`/`jiff` posture verified at prior
DESIGN waves (zero matches across all `Cargo.toml` and all `*.rs`).

### [A4] No new toolchain pin

**Verdict**: Inherits workspace stable Rust (`rust-toolchain.toml`).
No Gate 2/3 graduation (binary crate). The new code uses no
unstable features; `match` on `&str` literals, `?`-propagation,
`writeln!`/`flush()` are all stable since edition 2015.

## Skipped artefacts (N/A)

`platform-architecture.md` (Morgan's app-architecture sufficient),
`observability-design.md` (DESIGN handoff: no OTLP; `migrate` has
no `--observe-otlp`), `monitoring-alerting.md` (CI gates ARE
alerts), `infrastructure-integration.md` (no external
integrations), `branching-strategy.md` (D8: pure trunk),
`continuous-learning.md` (D7).

## Constraints for DISTILL / DELIVER

| When | What |
|------|------|
| DISTILL | Author `tests/migrate_subcommand.rs` with RED scenarios: OK1 (happy path - place `acme/batch-00042` in Hot, call `migrate(..., warm)`, assert stdout equals `migrated tenant=acme item=acme/batch-00042 from=hot to=warm\n`, exit 0, stderr empty, AND post-call `get_entry(acme, acme/batch-00042).unwrap().tier == Tier::Warm`); OK2 (unknown-item - seed a different item, call `migrate(..., "acme/batch-00099", ...)`, assert `Err`, stdout empty, stderr substring contains `acme/batch-00099`, AND pre/post `list_by_tier(...).len()` triple identical); OK3 (invalid tier - two sub-scenarios with `"HOT"` and `"lukewarm"`, each asserts `Err`, stdout empty, stderr substring contains the invalid value verbatim, AND post-call `get_entry().tier` unchanged); OK4 (idempotent same-tier - place `acme/batch-00007` in Cold, call `migrate(..., cold)`, assert stdout equals `migrated tenant=acme item=acme/batch-00007 from=cold to=cold\n`, exit 0, post-call tier Cold). |
| DISTILL | Author all four KPI scenarios as `kaleidoscope_cli::migrate(...)` library-direct calls into a `Vec<u8>` writer (DESIGN DD1; the binary entry shape is exercised in-process by the same harness pattern as siblings). |
| DISTILL | Add a tenant-isolation sub-scenario: place same-named item under two tenants `acme` and `globex` in different tiers; call `migrate(acme, ...)`; assert `globex`'s item is unchanged (`get_entry(globex, ...).unwrap().tier` byte-identical to pre-call). |
| DISTILL | Add a no-Lumen-touch guard sub-scenario (D-NoLumenTouch): capture the on-disk `<data_dir>/lumen.*` tree's contents before and after a successful `migrate` call; assert byte-identity. |
| DISTILL | Inline-duplicate the test harness helpers (`tenant`, `temp_root`, `cleanup`, `cinder_base`, `lumen_base`) per DISCUSS D-NewTestFile; rule-of-three extraction is a separate refactoring task (EIGHTH inline duplication; rule of three septuply discharged). |
| DISTILL | OK3 sub-scenarios MUST include at least one empty-string and one leading/trailing-whitespace input (`""`, `" hot"`, `"hot "`) to pin DESIGN DD3 rationale 2 (no `trim()` call). |
| DELIVER | Land the new `migrate()` free function + new `parse_tier` private helper + new `Error::InvalidTier` and `Error::CinderMigrate` variants + Display arms + `run_migrate` binary helper + dispatch arm + usage paragraph + new test file + `[[test]]` block in ONE atomic commit. |
| DELIVER | DO NOT edit `.github/workflows/ci.yml` (A1 + A2). |
| DELIVER | DO NOT modify ANY existing test file (`stats_subcommand.rs`, `stats_cinder_tier_distribution.rs`, `stats_time_range.rs`, `read_time_range.rs`, `observe_otlp_flag.rs`, `observe_otlp_cinder_wiring.rs`, `observe_otlp_read_flag.rs`, `ingest_and_read_roundtrip.rs`). DESIGN DD7 item 9. |
| DELIVER | DO NOT modify any existing `pub` function (`ingest`, `read`, `stats`, `stats_with_tiers`). `migrate` is a fifth sibling, not a re-shape of any existing function. |
| DELIVER | DO NOT open `FileBackedLogStore::open(lumen_base(data_dir), ...)` in the new function (D-NoLumenTouch). |
| DELIVER | DO NOT call `place()`, `list_by_tier()`, or `evaluate_at()` from the new function (DESIGN DD6 D-No-Side-Effects). Exactly one `get_entry` plus exactly one `migrate` call per invocation. |
| DELIVER | DO NOT introduce a same-tier short-circuit (D-Idempotent / DESIGN DD2). The `migrate` call runs unconditionally; the stdout report shows `from=X to=X` faithfully. |
| DELIVER | DO NOT add any external crate (A3). |
| DELIVER | DO NOT add `chrono`, `time`, or `jiff` (inherits prior posture). |
| DELIVER | DO NOT add `-p kaleidoscope-cli` to Gate 2 / Gate 3 (no graduation). |
| DELIVER | DO NOT use `.eq_ignore_ascii_case()` or `.trim()` in `parse_tier` (DESIGN DD3 rationale 1-2). |
| DELIVER | 100% mutation kill on `src/lib.rs` + `src/main.rs` before review approval, with the one expected survivor explicitly acknowledged: `SystemTime::now()` -> `UNIX_EPOCH` is wire-invisible (the `migrated_at` field is not in the stdout contract) and is therefore not killed by acceptance witnesses. If `cargo mutants` surfaces it, mark it MISSED and reference this paragraph; no test contortion to chase it. |
| DELIVER | Existing tests (eight locked files listed above) MUST pass unchanged. |

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**Deliverables produced**:

| Artefact | Path |
|----------|------|
| Environment inventory | `docs/feature/cli-migrate-subcommand-v0/devops/environments.yaml` |
| Wave decisions (this file) | `docs/feature/cli-migrate-subcommand-v0/devops/wave-decisions.md` |
| Per-KPI instrumentation | `docs/feature/cli-migrate-subcommand-v0/devops/kpi-instrumentation.md` |
| CI/CD pipeline confirmation | `docs/feature/cli-migrate-subcommand-v0/devops/ci-cd-pipeline.md` |

---

## Forward-compatibility notes

### `--observe-otlp` on `migrate`

DISCUSS D-OutOfScope-Observe defers OTLP wiring on this subcommand
to a future feature. A `--observe-otlp` flag would swap the
`NoopRecorder` for a `LumenToOtlpJsonWriter`-backed recorder (the
same shape `ingest` already supports) and would warrant the same
DEVOPS shape `cli-cinder-otlp-wiring-v0` followed for `ingest`.
This feature does NOT trigger.

### `--dry-run` flag on `migrate`

DISCUSS D-OutOfScope-Dryrun defers a `--dry-run` flag (pre-flight
`get_entry` only; skip the `migrate` call). DESIGN noted no `()`
discrimination is needed for the dry-run path because the report
shape (`from=<from> to=<to>`) is identical for actual and
hypothetical migrations; the only difference is whether the
underlying call fires. A future `--dry-run` feature would gate the
`cinder.migrate(...)` call site. This feature does NOT trigger.

### Bulk migration (multi-item single call)

DISCUSS D-OutOfScope-Bulk defers bulk migration. A future feature
would extend the function signature to accept `&[(ItemId, Tier)]`
pairs and emit one stdout line per migration. The signature
graduation to a builder pattern (per Principle 4) SHOULD be
considered at that point. This feature does NOT trigger.

### Test harness rule-of-three extraction

DISCUSS D-NewTestFile defers extraction of the harness helpers to
`tests/common/mod.rs`. This feature is the EIGHTH inline
duplication; the rule of three has been septuply discharged.
Extraction is overdue but NOT undertaken here (would conflate
acceptance landing with cross-file refactor risk); the next
test-touching feature SHOULD propose it as a paired DD.

### Mutation kill-rate protocol (DELIVER)

1. After tests turn GREEN: `cargo mutants --package
   kaleidoscope-cli --in-diff <(git diff origin/main HEAD --
   crates/kaleidoscope-cli/src/lib.rs
   crates/kaleidoscope-cli/src/main.rs)`.
2. `mutants.out/summary.txt` "undetected" MUST be zero EXCEPT for
   the one acknowledged survivor (`SystemTime::now()` ->
   `UNIX_EPOCH`).
3. Survivor-to-probe map for the new surface: `parse_tier` arm
   swap (`"hot" => Tier::Warm`) killed by OK1's `get_entry().tier
   == to_tier` post-condition; `get_entry().None` arm deletion
   (`?` -> `.unwrap_or_default()`) killed by OK2's Cinder
   byte-equivalence; `migrate(...)?` map_err shape mutation killed
   by OK2's stderr substring assertion via the
   `Error::CinderMigrate` Display arm; `writeln!` from/to field
   swap killed by OK1's exact-bytes stdout assertion;
   `tier_lowercase(from)` -> `tier_lowercase(to_tier)` killed by
   OK1's distinct from/to scenario (`hot`/`warm`); `Tier::Hot` ->
   `Tier::Warm` literal swaps in `parse_tier` killed by the same
   four-arm OK1 + OK3 coverage.
4. CI oracle: existing `gate-5-mutants-kaleidoscope-cli` on
   merge - surface auto-discovered via `--in-diff`.

Prior precedent: commits 4d20c31 (cli-cinder-otlp-wiring-v0), plus
`cli-read-observe-otlp-v0`, `cli-stats-subcommand-v0`,
`cli-stats-cinder-tier-distribution-v0`, `cli-read-time-range-v0`,
and `cli-stats-time-range-v0` DELIVER waves all hit 100% kill
under the same job. This feature is the SEVENTH consecutive
realisation of the zero-workflow-edit per-package Gate 5 cycle.
The 2baa05c investment continues to compound.
