# Wave Decisions - `cli-migrate-observe-otlp-v0` / DEVOPS

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19
- **Mode**: Decisions pre-taken with Andrea (A1-A4); recorded
  verbatim. Inherits the zero-workflow-edit posture established by
  `cli-cinder-otlp-wiring-v0`, `cli-read-observe-otlp-v0`,
  `cli-stats-subcommand-v0`, `cli-stats-cinder-tier-distribution-v0`,
  `cli-read-time-range-v0`, `cli-stats-time-range-v0`,
  `cli-migrate-subcommand-v0`, and the immediately preceding
  `cli-migrate-observe-otlp-v0` DESIGN wave's handoff. **This is the
  NINTH consecutive zero-workflow-edit wave on the
  `kaleidoscope-cli` package.** The one-off investment in
  `gate-5-mutants-kaleidoscope-cli` (commit 2baa05c) has now
  amortised across nine features.

## Inputs read

1. `CLAUDE.md` - Rust idiomatic; per-feature MT 100% kill per
   ADR-0005 Gate 5.
2. `discuss/outcome-kpis.md` - OK1 (wire shape per successful
   migrate; principal / North Star), OK2 (no-flag byte-equivalence;
   guardrail), OK3 (UnknownItem -> no emission; guardrail), OK4
   (InvalidTier -> no file created; guardrail).
3. `design/wave-decisions.md` - DD1 (sixth `Option<&Path>`
   parameter), DD2 (internal `match otlp_log_path` mirrors the
   `ingest()` shape; open happens AFTER `parse_tier`), DD3
   (`main.rs` thread-through reuses `parse_observe_otlp`), DD4
   (reuse analysis: zero new public type, zero new trait), DD5 (six
   mechanical signature-match call-site updates).
4. `docs/feature/cli-migrate-subcommand-v0/devops/*` - template
   (8th wave under the same posture; this is the 9th).
5. `.github/workflows/ci.yml` - `gate-5-mutants-kaleidoscope-cli`
   confirmed path-filtered on `crates/kaleidoscope-cli/**`;
   `--in-diff` cascade (`origin/main` -> `HEAD~1` -> full)
   unchanged.
6. `crates/kaleidoscope-cli/Cargo.toml` - `aegis`, `cinder`,
   `lumen`, `self-observe`, `pulse`, `tempfile` (dev) already
   declared; `self-observe` already imports `CinderToOtlpJsonWriter`
   into `lib.rs` from the preceding `cli-cinder-otlp-wiring-v0`
   wave. Only addition is one new `[[test]]` block.

## Pre-wave decisions (carried in)

| D# | Decision | Value |
|----|----------|-------|
| D1 | `deployment_target` | N/A (CLI subcommand; binary shape unchanged) |
| D2 | `container_orchestration` | N/A |
| D3 | `cicd_platform` | GitHub Actions (existing) |
| D4 | `existing_infrastructure` | Yes (workspace + five-gate CI + per-pkg Gate 5) |
| D5 | `observability_and_logging` | Operator-side NDJSON sink; project-side N/A (no dashboard or alert added by this feature; existing sidecar contract reused) |
| D6 | `deployment_strategy` | N/A |
| D7 | `continuous_learning` | No (single-feature, no A/B, no flags) |
| D8 | `git_branching_strategy` | Trunk-Based Development (pure trunk; no required-status-checks; no enforce_admins) |
| D9 | `mutation_testing_strategy` | Per-feature, 100% kill per ADR-0005 Gate 5 |

## In-wave decisions (A = Apex)

### [A1] No new CI workflow edit - INHERIT existing Gate 5 job

**Verdict**: **INHERIT** (pre-decided by Andrea). The existing
`gate-5-mutants-kaleidoscope-cli` job auto-covers the two modified
source files (`src/lib.rs`, `src/main.rs`) via its `--in-diff`
cascade and `crates/kaleidoscope-cli/**` path filter. NINTH
consecutive realisation. The mutation surface added by this feature
is small and entirely acceptance-test-observable (DD2's
`match otlp_log_path` arms; DD3's `parse_observe_otlp` reuse +
`as_deref()` propagation): swapping `Some` and `None` arm bodies is
killed by OK1 (file created with one line) vs OK2 (no file created);
moving the `OpenOptions::open(path)?` call before `parse_tier(...)?`
is killed by OK4 (sink file MUST NOT exist after `InvalidTier`);
moving it before the pre-flight `get_entry` is killed by OK3 (sink
contains no `cinder.migrate.count` line for the `UnknownItem` path).

### [A2] Gate 1 inherits via `[[test]]` block - ZERO workflow edits total

**Verdict**: No Gate 1 workflow edit (pre-decided). `cargo test
--workspace --all-targets --locked` auto-discovers via the new
`[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml`:

```toml
[[test]]
name = "migrate_observe_otlp_flag"
path = "tests/migrate_observe_otlp_flag.rs"
```

A1 + A2 = **ZERO workflow edits**. ci.yml is byte-untouched.
Crafty lands the `migrate()` 6-arg signature update (`src/lib.rs`),
the internal `match otlp_log_path` arm (`src/lib.rs`), the
`main.rs` thread-through and usage-text update (`src/main.rs`), the
new test file (`tests/migrate_observe_otlp_flag.rs`), the
`[[test]]` block (`Cargo.toml`), AND the six mechanical
signature-match call-site updates (DD5: one in `main.rs`, one
inline white-box in `lib.rs`, four in the locked
`migrate_subcommand.rs`) in ONE atomic commit per ADR-0005's
"tests and source land together" rule.

### [A3] Zero new external dependencies

**Verdict**: Zero new crate (pre-decided). Verified by DESIGN DD4
reuse analysis: `CinderToOtlpJsonWriter` is already imported from
`self-observe` (inherited from `cli-cinder-otlp-wiring-v0`);
`OpenOptions`, `Path`, `PathBuf` are already in `lib.rs`'s use
list; `parse_observe_otlp` is already a `main.rs`-private helper.
Zero `[dependencies]`, zero `[dev-dependencies]`, zero `deny.toml`
change. Only `Cargo.toml` addition is the `[[test]]` block above.

### [A4] No new toolchain pin

**Verdict**: Inherits workspace stable Rust (`rust-toolchain.toml`).
No Gate 2/3 graduation (binary crate). `Option<&Path>`, `match`
arms, `.as_deref()`, and `OpenOptions::new().create(true).append(true).open(path)?`
are all stable since edition 2015.

## Skipped artefacts (N/A)

`platform-architecture.md` (Morgan's app-architecture sufficient),
`observability-design.md` (no dashboard or alert added; existing
sidecar contract reused), `monitoring-alerting.md` (CI gates ARE
alerts), `infrastructure-integration.md` (no external integrations
at the project boundary; the OTLP/HTTP collector lives at the
operator's deployment boundary), `branching-strategy.md` (D8: pure
trunk), `continuous-learning.md` (D7).

## Constraints for DISTILL / DELIVER

| When | What |
|------|------|
| DISTILL | Author `tests/migrate_observe_otlp_flag.rs` with RED scenarios: OK1 (happy path - seed `acme/batch-00042` in Hot, call `migrate(..., Some(&sink))`, read sink, assert exactly one non-empty line with `metric.name == "cinder.migrate.count"`, `tenant_id == "acme"`, `from == "hot"`, `to == "cold"`, `asInt == "1"`, line ends with `\n`, parses as `serde_json::Value`); OK2 no-flag sub-scenario (call `migrate(..., None)`, assert no file created at the candidate sink path); OK3 (subprocess `migrate acme <data> ghost-item warm --observe-otlp <sink>`, assert non-zero exit, stderr contains `ghost-item`, empty stdout, sink contains no `cinder.migrate.count` line); OK4 (subprocess `migrate acme <data> item_id LUKEWARM --observe-otlp <sink>`, assert non-zero exit, stderr contains `LUKEWARM`, empty stdout, sink file does NOT exist after the call). |
| DISTILL | Inline-duplicate the test harness helpers (`tenant`, `temp_root`, `cleanup`, `cinder_base`, `place_item`, `bin`) per DESIGN DD4 final row; rule-of-three extraction is a separate refactoring task (NINTH inline duplication; rule of three octuply discharged). |
| DELIVER | Land the `migrate()` 6-arg signature + the `match otlp_log_path` arm in `lib.rs` + the `main.rs` `parse_observe_otlp` thread-through + usage-text update + new test file + `[[test]]` block + the SIX mechanical call-site updates (DD5) in ONE atomic commit. |
| DELIVER | DO NOT edit `.github/workflows/ci.yml` (A1 + A2). |
| DELIVER | DO NOT edit assertions in any locked test file. The four `migrate(...)` call sites in `migrate_subcommand.rs` (DD5 #3-#6) gain `, None` as the sixth argument; the assertions are byte-untouched. Precedent: `read()`/`stats_with_tiers()` parameter growths on the two prior waves followed the same posture. |
| DELIVER | DO NOT introduce a new flag name (`--migrate-observe-otlp` or similar). The shared `parse_observe_otlp` helper is the only argv-to-flag-value machinery (DESIGN DD3). |
| DELIVER | DO NOT open the sink file BEFORE `parse_tier(to_tier_arg)?` runs (DESIGN DD2; OK4 guard). |
| DELIVER | DO NOT modify the writer's public API (`CinderToOtlpJsonWriter`); ADR-0039 §1 is locked. |
| DELIVER | DO NOT add any external crate (A3). |
| DELIVER | DO NOT add `-p kaleidoscope-cli` to Gate 2 / Gate 3 (no graduation). |
| DELIVER | 100% mutation kill on `src/lib.rs` + `src/main.rs` before review approval. The new surface is small and fully acceptance-observable; no expected survivors beyond those already acknowledged in the predecessor wave (`SystemTime::now()` -> `UNIX_EPOCH` is wire-invisible on the stdout report; it remains so on the new OTLP line because `record_migrate` does not embed `migrated_at` per ADR-0039 §2). |
| DELIVER | Existing tests (the ten locked files under `tests/` other than the new one) MUST pass unchanged in assertions; `migrate_subcommand.rs` gains the four mechanical `, None` suffixes only. |

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**Deliverables produced**:

| Artefact | Path |
|----------|------|
| Environment inventory | `docs/feature/cli-migrate-observe-otlp-v0/devops/environments.yaml` |
| Wave decisions (this file) | `docs/feature/cli-migrate-observe-otlp-v0/devops/wave-decisions.md` |
| Per-KPI instrumentation | `docs/feature/cli-migrate-observe-otlp-v0/devops/kpi-instrumentation.md` |
| CI/CD pipeline confirmation | `docs/feature/cli-migrate-observe-otlp-v0/devops/ci-cd-pipeline.md` |

---

## Forward-compatibility notes

### Sidecar dashboard panel

The operator's sidecar (existing) tails the `--observe-otlp` sink
file and forwards lines to the OTLP/HTTP collector unchanged. The
collector already accepts `kaleidoscope.cinder /
cinder.place.count` from `cli-cinder-otlp-wiring-v0`; this feature
adds a sibling metric `cinder.migrate.count` in the SAME scope.
The operator extends the existing dashboard by adding one panel
keyed on `cinder.migrate.count` grouped by `tenant_id`, `from`,
`to`. **No project-side action**: the dashboard is operator
infrastructure.

### `--observe-otlp` on future mutation paths

Should a future feature add another state-mutating CLI surface
(e.g. `kaleidoscope-cli delete`, `kaleidoscope-cli compact`), the
DEVOPS shape SHOULD be the same as this feature's: extend the
library function's signature with a trailing `Option<&Path>`,
reuse `parse_observe_otlp` at the binary, construct the writer
inside the `Some` arm AFTER all parse-time validation. The
nine-feature precedent makes this the project's canonical shape.

### Test harness rule-of-three extraction

The new test file is the NINTH inline duplication of the
`tenant`, `temp_root`, `cleanup`, `cinder_base`, `place_item`,
`bin` helper cluster. The rule of three has been octuply
discharged. Extraction to `tests/common/mod.rs` is overdue but
NOT undertaken here (would conflate acceptance landing with
cross-file refactor risk); the next test-touching feature SHOULD
propose it as a paired DD.

### Mutation kill-rate protocol (DELIVER)

1. After tests turn GREEN: `cargo mutants --package
   kaleidoscope-cli --in-diff <(git diff origin/main HEAD --
   crates/kaleidoscope-cli/src/lib.rs
   crates/kaleidoscope-cli/src/main.rs)`.
2. `mutants.out/summary.txt` "undetected" MUST be zero EXCEPT for
   the inherited `SystemTime::now()` -> `UNIX_EPOCH` survivor
   (wire-invisible on the stdout report AND on the OTLP line per
   ADR-0039 §2).
3. Survivor-to-probe map for the new surface: `match
   otlp_log_path` arm-swap (Some <-> None body) killed by OK1
   (file created) vs OK2 (no file created); `OpenOptions::open(path)?`
   call placement before `parse_tier?` killed by OK4 (sink file
   absent post-call); `as_deref()` -> `.cloned()` shape mutations
   killed by Gate 2 (type error). `CinderToOtlpJsonWriter::new(file)`
   call removal killed by OK1 (no `cinder.migrate.count` line);
   wrapping the file in `BufWriter` instead of `CinderToOtlpJsonWriter`
   killed by OK1 (JSON-parse assertion fails).
4. CI oracle: existing `gate-5-mutants-kaleidoscope-cli` on
   merge - surface auto-discovered via `--in-diff`.

Prior precedent: commits 4d20c31 (cli-cinder-otlp-wiring-v0), plus
`cli-read-observe-otlp-v0`, `cli-stats-subcommand-v0`,
`cli-stats-cinder-tier-distribution-v0`, `cli-read-time-range-v0`,
`cli-stats-time-range-v0`, and `cli-migrate-subcommand-v0` DELIVER
waves all hit 100% kill under the same job. This feature is the
NINTH consecutive realisation of the zero-workflow-edit per-package
Gate 5 cycle. The 2baa05c investment continues to compound.
