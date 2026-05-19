# Wave Decisions - `cli-place-subcommand-v0` / DEVOPS

- **Wave**: DEVOPS
- **Author**: Apex (`nw-platform-architect`)
- **Date**: 2026-05-19
- **Mode**: Decisions pre-taken with Andrea (A1-A4); recorded
  verbatim. Inherits the zero-workflow-edit posture established by
  the TEN prior `kaleidoscope-cli` features
  (`cli-cinder-otlp-wiring-v0`, `cli-read-observe-otlp-v0`,
  `cli-stats-subcommand-v0`, `cli-stats-cinder-tier-distribution-v0`,
  `cli-read-time-range-v0`, `cli-stats-time-range-v0`,
  `cli-migrate-subcommand-v0`, `cli-migrate-observe-otlp-v0`,
  `cli-list-items-subcommand-v0`, plus the implicit baseline
  `cli-cinder-otlp-wiring-v0` precursor). **This is the ELEVENTH
  consecutive zero-workflow-edit wave on the `kaleidoscope-cli`
  package.** The one-off investment in
  `gate-5-mutants-kaleidoscope-cli` (commit 2baa05c) has now
  amortised across eleven features.

## Inputs read

1. `CLAUDE.md` - Rust idiomatic; per-feature MT 100% kill per
   ADR-0005 Gate 5.
2. `discuss/outcome-kpis.md` - OK1 (place-success correctness -
   principal / North Star: stdout `placed tenant=<t> item=<id>
   tier=<x>\n` + post-call `get_entry().tier == tier`), OK2
   (overwrite-semantics guardrail - second call overwrites the
   first faithfully; no special-case CLI guard), OK3 (invalid-tier
   fail-fast guardrail; reuses `Error::InvalidTier`), OK4
   (`--observe-otlp` emission - exactly one `cinder.place.count`
   OTLP-JSON line per place call when the flag is set).
3. `design/wave-decisions.md` - DD1 (`pub fn place(tenant,
   data_dir, item_id, tier_arg, writer, otlp_log_path) ->
   Result<(), Error>`, six positional params mirroring `migrate()`
   byte-for-byte; simpler body since `TieringStore::place` returns
   `()`), DD2 (recorder construction = literal copy of
   `migrate()`'s nine-line match; rule-of-three deferred), DD3 (no
   new `Error` variant; `InvalidTier` / `CinderOpen` / `Io` cover
   every failure path), DD4 (100% reuse on the production
   substrate; seventeen existing constructs reused), DD5
   (out-of-scope confirmations: no `--placed-at`, no bulk, no
   pre-flight existence check, no Lumen touch, lower-case only,
   no `--dry-run`/`--json`/`--csv`, locked tests untouched, no
   SSOT change, no new ADR, `Error::InvalidTier` Display accepted
   unchanged).
4. `docs/feature/cli-list-items-subcommand-v0/devops/*` -
   template (10th wave under the same posture; this is the 11th).
5. `.github/workflows/ci.yml` (implicit) -
   `gate-5-mutants-kaleidoscope-cli` confirmed path-filtered on
   `crates/kaleidoscope-cli/**` via `--in-diff` cascade;
   unchanged.
6. `crates/kaleidoscope-cli/Cargo.toml` (implicit) - `aegis`,
   `cinder`, `lumen`, `self-observe`, `pulse`, `tempfile` (dev)
   already declared. Only addition is one new `[[test]]` block.

## Pre-wave decisions (carried in)

| D# | Decision | Value |
|----|----------|-------|
| D1 | `deployment_target` | N/A (CLI subcommand; binary shape unchanged) |
| D2 | `container_orchestration` | N/A |
| D3 | `cicd_platform` | GitHub Actions (existing) |
| D4 | `existing_infrastructure` | Yes (workspace + five-gate CI + per-pkg Gate 5) |
| D5 | `observability_and_logging` | Optional per-call `--observe-otlp <path>`; one `cinder.place.count` OTLP-JSON line per place call via the already-shipped `CinderToOtlpJsonWriter`. Byte-identical to the lines `ingest` already emits. No new sink, no new collector, no new dashboard added by this feature. |
| D6 | `deployment_strategy` | N/A |
| D7 | `continuous_learning` | No (single-feature, no A/B, no flags) |
| D8 | `git_branching_strategy` | Trunk-Based Development (pure trunk; no required-status-checks; no enforce_admins) |
| D9 | `mutation_testing_strategy` | Per-feature, 100% kill per ADR-0005 Gate 5 |

## In-wave decisions (A = Apex)

### [A1] No new CI workflow edit - INHERIT existing Gate 5 job

**Verdict**: **INHERIT** (pre-decided by Andrea). The existing
`gate-5-mutants-kaleidoscope-cli` job auto-covers the two modified
source files (`src/lib.rs`, `src/main.rs`) via its `--in-diff`
cascade and `crates/kaleidoscope-cli/**` path filter. ELEVENTH
consecutive realisation. The mutation surface added by this feature
is small and entirely acceptance-test-observable (DD1's `place(...)`
body: `parse_tier` short-circuit -> recorder match -> store open
-> `TieringStore::place` call -> `writeln!`): removing the
`writeln!(writer, "placed tenant=... tier=...")` line is killed by
OK1's byte-exact stdout assertion; swapping the `tier` argument
passed to `TieringStore::place` (e.g. always passing `Tier::Hot`)
is killed by OK1's post-call `get_entry().tier == requested_tier`
oracle on a `tier_arg = "cold"` happy-path; introducing a guard
(`if get_entry(...).is_some() { return Err(...) }`) before the
place call is killed by OK2's overwrite-semantics scenario
(pre-place Hot, then `place(...tier_arg = "cold")`, assert Ok and
`get_entry().tier == Tier::Cold`); reordering `parse_tier(...)?`
to run AFTER `FileBackedTieringStore::open(...)?` is killed by
OK3's "store-unopened" guardrail (the invalid-tier sub-scenarios
seed a pre-existing item and assert byte-equivalence of the Cinder
state before and after); swapping `Some(path)` and `None` arms in
the recorder match is killed by OK4 (flag-present asserts the file
exists with one `cinder.place.count` line; flag-absent asserts no
file is created at the candidate path); replacing
`CinderToOtlpJsonWriter::new(file)` with `CinderRecorder` in the
`Some(path)` arm is killed by OK4's substring assertion (`acme`
tenant id + `hot` tier point attribute). The wire-invisible
`SystemTime::now()` -> `UNIX_EPOCH` substitution is acknowledged
as a possible inherited survivor (it does not appear in any OK1-OK4
assertion since neither stdout, `get_entry().tier`, nor the
`cinder.place.count` OTLP-JSON line carries the `placed_at`
timestamp at the byte level the tests inspect); same posture as
prior waves that exposed this survivor.

### [A2] Gate 1 inherits via `[[test]]` block - ZERO workflow edits total

**Verdict**: No Gate 1 workflow edit (pre-decided). `cargo test
--workspace --all-targets --locked` auto-discovers via the new
`[[test]]` block in `crates/kaleidoscope-cli/Cargo.toml`:

```toml
[[test]]
name = "place_subcommand"
path = "tests/place_subcommand.rs"
```

A1 + A2 = **ZERO workflow edits**. ci.yml is byte-untouched.
Crafty lands the new `place(...)` free function (`src/lib.rs`),
the new `run_place` / `run_place_with` binary-side helpers plus
the `Some("place")` dispatch arm and usage-text update
(`src/main.rs`), the new test file
(`tests/place_subcommand.rs`), and the `[[test]]` block
(`Cargo.toml`) in ONE atomic commit per ADR-0005's "tests and
source land together" rule.

### [A3] Zero new external dependencies

**Verdict**: Zero new crate (pre-decided). Verified by DESIGN
reuse verdict (seventeen existing constructs reused; only new
things created are internal to the crate). `cinder_base`,
`FileBackedTieringStore::open`, `cinder::NoopRecorder` (aliased
`CinderRecorder`), `self_observe::CinderToOtlpJsonWriter`,
`std::fs::OpenOptions::new().create(true).append(true)`,
`TieringStore::place`, `ItemId`, `Tier`, `parse_tier`,
`tier_lowercase`, `Error::InvalidTier`, `Error::CinderOpen`,
`Error::Io` (+ `From<io::Error>`), `parse_positional`,
`parse_observe_otlp`, `TenantId`, the `Box<dyn CinderRec + Send +
Sync>` coercion idiom are all already in scope. `writeln!` is
core/std. Zero `[dependencies]`, zero `[dev-dependencies]`, zero
`deny.toml` change. Only `Cargo.toml` addition is the `[[test]]`
block above.

### [A4] No new toolchain pin

**Verdict**: Inherits workspace stable Rust (`rust-toolchain.toml`).
No Gate 2/3 graduation (binary crate). Free functions, six
positional params, `impl Write` parameter, `Option<&Path>`
parameter, `match`, `Box<dyn Trait + Send + Sync>`, `writeln!`,
`Result<(), Error>` are all stable since edition 2018; nothing
in DD1-DD5 reaches for a nightly feature or a recent MSRV bump.
The `self_observe::CinderToOtlpJsonWriter` adapter and the
ADR-0039 §8 file-open incantation are already in scope from
`cli-cinder-otlp-wiring-v0` / `cli-migrate-observe-otlp-v0`.

## Skipped artefacts (N/A)

`platform-architecture.md` (Morgan's app-architecture sufficient),
`observability-design.md` (no new dashboard or alert added;
`cinder.place.count` is already wired through the operator's
existing sidecar/collector/dashboard chain from prior features -
this feature adds a new emitter for an existing line shape, not a
new line shape), `monitoring-alerting.md` (CI gates ARE the
project-side alerts; operator-side alerting on the
`cinder.place.count` series is the operator's concern, not the
project's), `infrastructure-integration.md` (no external
integrations - pure local Cinder mutation; the optional OTLP-JSON
sidecar is an append to an operator-supplied file path, not a
network call), `branching-strategy.md` (D8: pure trunk),
`continuous-learning.md` (D7).

## Constraints for DISTILL / DELIVER

| When | What |
|------|------|
| DISTILL | Author `tests/place_subcommand.rs` with RED scenarios: OK1 happy path (fresh `data_dir`; call `place(&acme, &dir, "acme/bootstrap-00001", "hot", &mut stdout_sink, None)`; assert captured stdout EQUALS `placed tenant=acme item=acme/bootstrap-00001 tier=hot\n`, captured stderr empty, exit Ok; follow-up: open a fresh `FileBackedTieringStore` handle and assert `get_entry(&acme, &ItemId::new("acme/bootstrap-00001")).unwrap().tier == Tier::Hot`); OK2 overwrite-semantics (pre-place `acme/bootstrap-00007` in Hot for `acme` via a direct `FileBackedTieringStore::open(...).place(...)` call with a fixed `placed_at`; call `place(&acme, &dir, "acme/bootstrap-00007", "cold", &mut stdout_sink, None)`; assert captured stdout EQUALS `placed tenant=acme item=acme/bootstrap-00007 tier=cold\n`, captured stderr empty, exit Ok; follow-up: assert `get_entry(...).tier == Tier::Cold` AND `placed_at` was bumped per `crates/cinder/src/store.rs:147-149`); OK3 invalid tier (two sub-scenarios: `tier_arg = "HOT"` upper-case and `tier_arg = "lukewarm"` typo; each seeds at least one pre-existing item for `acme` in Hot via a direct place; calls `place(&acme, &dir, "acme/bootstrap-00001", "HOT", ...)`; asserts `Err(Error::InvalidTier { value })` with `value == "HOT"` (or `"lukewarm"`); asserts captured stdout empty; asserts captured stderr contains the verbatim invalid value as a substring; follow-up: reopens the Cinder store and asserts the pre-seeded item's `get_entry(...).tier` is byte-equivalent to the pre-call state); OK4 `--observe-otlp` emission (two sub-scenarios: flag present and flag absent; flag-present invokes `place(&acme, &dir, "acme/bootstrap-00001", "hot", &mut stdout_sink, Some(&observe_log_path))`, asserts the file at `observe_log_path` exists, contains exactly one line, and that line contains the substrings `cinder.place.count`, `acme`, and `hot`; flag-absent invokes the same call with `otlp_log_path = None` and asserts no file exists at the candidate path - verifying no implicit file creation). |
| DISTILL | Inline-duplicate the test harness helpers (`tenant`, `temp_root`, `cleanup`, `cinder_base`, `place_item`, `bin`, `ndjson`) per DESIGN's eleventh-inline-duplication note; rule-of-three extraction is a separate refactoring task (ELEVENTH inline duplication; rule of three undecuply discharged). |
| DELIVER | Land the new `place(...)` free function in `lib.rs` + the new `run_place` / `run_place_with` helpers in `main.rs` + the `Some("place")` dispatch arm + the usage-text update + the new test file + the `[[test]]` block in ONE atomic commit. |
| DELIVER | DO NOT edit `.github/workflows/ci.yml` (A1 + A2). |
| DELIVER | DO NOT edit assertions in any locked test file. The TWELVE prior `tests/*.rs` files (`stats_subcommand.rs`, `stats_cinder_tier_distribution.rs`, `stats_time_range.rs`, `read_time_range.rs`, `observe_otlp_flag.rs`, `observe_otlp_cinder_wiring.rs`, `observe_otlp_read_flag.rs`, `migrate_subcommand.rs`, `migrate_observe_otlp_flag.rs`, `list_items_subcommand.rs`, `ingest_and_read_roundtrip.rs`, `cli_binary_smoke.rs`) are byte-untouched. ANY non-mechanical edit to any of them in the DELIVER commit's diff auto-rejects review. |
| DELIVER | DO NOT introduce a new flag beyond `--observe-otlp` (no `--placed-at`, no `--dry-run`, no `--json`, no `--csv`, no `--format=...`, no bulk) per DESIGN DD5 §1-§6. |
| DELIVER | DO NOT introduce a new `Error` variant. `Error::InvalidTier`, `Error::CinderOpen`, `Error::Io` are reused via direct delegation (DESIGN DD3). `TieringStore::place` returns `()`; there is no `PlaceError` to lift. |
| DELIVER | DO NOT introduce a pre-flight `get_entry` call before `TieringStore::place` (DESIGN DD1 rationale 2; DD5 §3). Faithful to overwrite-semantics. |
| DELIVER | DO NOT add `-p kaleidoscope-cli` to Gate 2 / Gate 3 (no graduation; A4). |
| DELIVER | 100% mutation kill on `src/lib.rs` + `src/main.rs` before review approval. The new surface is small and fully acceptance-observable; the one acknowledged inherited survivor is `SystemTime::now()` -> `UNIX_EPOCH` (wire-invisible to OK1-OK4 since neither stdout nor `get_entry().tier` nor the `cinder.place.count` byte-level assertion carries the `placed_at` timestamp). |
| DELIVER | Existing tests MUST pass unchanged in assertions. No mechanical signature-match suffixes are needed this wave (no signature growth on any existing function; `place` is a brand-new free function). |

## Hand-off

**Next agent**: `nw-acceptance-designer` (DISTILL wave).

**Deliverables produced**:

| Artefact | Path |
|----------|------|
| Environment inventory | `docs/feature/cli-place-subcommand-v0/devops/environments.yaml` |
| Wave decisions (this file) | `docs/feature/cli-place-subcommand-v0/devops/wave-decisions.md` |
| Per-KPI instrumentation | `docs/feature/cli-place-subcommand-v0/devops/kpi-instrumentation.md` |
| CI/CD pipeline confirmation | `docs/feature/cli-place-subcommand-v0/devops/ci-cd-pipeline.md` |
