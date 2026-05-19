# Outcome KPIs — `cli-stats-cinder-tier-distribution-v0`

## Feature

`cli-stats-cinder-tier-distribution-v0` — extend the existing `stats`
subcommand on `kaleidoscope-cli` (shipped in commit `75f15a6` from
`cli-stats-subcommand-v0`) so that the same invocation
`kaleidoscope-cli stats <tenant_id> <data_dir>` ALSO emits up to three
additional plain-text key=value lines on stdout reporting the tenant's
current Cinder tier distribution (`hot=H` / `warm=W` / `cold=C`,
selectively emitted only for tiers where the count is non-zero per
`wave-decisions.md` D-EmptyRender Option B). v0 introduces no new
subcommand, no new flag, no JSON output, no per-item dump, and no
Cinder-only mode (`wave-decisions.md` D2, D3, D4, D5, D6).

## Objective

A single `kaleidoscope-cli stats acme /tmp/data` invocation prints, to
stdout, the existing Lumen-side answer (records count + time window)
AND the new Cinder-side answer (per-tier item counts), giving the
operator a one-shot, pipeable, grep-friendly answer to three
operationally distinct questions: "are warm-to-cold migrations
actually happening for this tenant?", "should I expect this tenant's
read latency to be high?", "is the hot tier ballooning?". Crucially,
for any tenant with zero Cinder placements (the most common legacy
case), stdout is byte-equivalent to the predecessor's output — no
existing operator shell script breaks.

## Note on KPI granularity

This feature is operator-visible at the CLI surface. The principal
KPI is OK1 (tier count correctness — each `hot=` / `warm=` / `cold=`
line reflects the actual `list_by_tier(tenant, tier).len()` for the
queried tenant). The second KPI is OK2 (per-tenant isolation —
`stats(acme)` does not count `globex`'s Cinder placements). The third
KPI is OK3 (the empty-render contract — empty-Lumen tenants with
non-zero Cinder placements still surface the Cinder lines after
`records=0`, but the predecessor's OK3 invariant for the
empty-Lumen-and-empty-Cinder case is preserved). The fourth KPI is
OK4 (backwards-compatibility — for tenants with zero Cinder
placements, the stdout is byte-equivalent to the predecessor's
output for the same `(tenant, data_dir)` pair, regardless of Lumen
state).

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| OK1-CLI-stats-tier-counts | Priya the platform operator, observed at the stdout byte level | Sees, when present, the lines `hot=H` / `warm=W` / `cold=C` where the value `H` (respectively `W`, `C`) equals the exact length of `cinder::TieringStore::list_by_tier(tenant, Tier::Hot)` (respectively `Warm`, `Cold`) against a `FileBackedTieringStore::open(cinder_base(data_dir), recorder)` opened in the same call. Each line is emitted ONLY when its count is non-zero (per `wave-decisions.md` D-EmptyRender Option B). | 100% of `stats()` invocations against any tenant produce per-tier stdout lines whose values equal what `list_by_tier(tenant, tier).len()` would return for the corresponding `(tenant, Tier::Hot/Warm/Cold)` triples; 0% of invocations report a count that disagrees with the underlying `list_by_tier`; 100% of invocations omit the `<tier>=` line when the corresponding count is zero | 0% (no CLI surface for tier counts exists today; the operator's only path is a Rust harness that opens `cinder::FileBackedTieringStore` and calls `list_by_tier` per tier, or `cinder.*` snapshot inspection by hand) | New acceptance test `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs` — populated-tenant-multi-tier scenario asserts the three lines `hot=H` / `warm=W` / `cold=C` appear with the seeded counts; hot-only scenario asserts only the `hot=` line appears, with no `warm=` and no `cold=` lines | Leading (operator-visible behaviour; principal KPI for this feature) |
| OK2-CLI-stats-tier-tenant-isolation | Priya the platform operator, observed at the stdout byte level | Sees `hot=` / `warm=` / `cold=` values reflecting ONLY the queried tenant's Cinder placements, NEVER the cross-tenant union or any other tenant's placements that happen to live in the same `data_dir`. | 100% of `stats(&acme, ...)` invocations against a `data_dir` that ALSO contains Cinder placements for tenant `globex` produce per-tier counts that equal `list_by_tier(acme, tier).len()` (the queried tenant's count alone), NEVER `list_by_tier(acme, tier).len() + list_by_tier(globex, tier).len()` (the cross-tenant union); 0% of such invocations leak a cross-tenant count | 0% (today operators have no CLI surface for per-tier counts at all, so there is no baseline to inherit a per-tenant-isolation invariant from) | Same new test file — tenant-isolation scenario asserts `stats(acme, ...)` reports `hot=5` when `acme` has 5 hot Cinder items and `globex` (in the same `data_dir`) has 9 hot Cinder items (proving stats does NOT count cross-tenant placements) | Leading (operator-visible behaviour; correctness invariant inherited structurally from the `TieringStore` per-tenant isolation semantic at `crates/cinder/src/store.rs:71-72`) |
| OK3-CLI-stats-no-records-no-timestamps-still | Priya the platform operator, observed at the stdout byte level | Sees, for a tenant with `N == 0` Lumen records, the existing predecessor invariant preserved: `records=0` is the first stdout line and no `earliest=` / `latest=` lines appear. ADDITIONALLY (per `wave-decisions.md` D-EmptyRender Option B), if the empty-Lumen tenant has any non-zero Cinder placement, the corresponding `hot=` / `warm=` / `cold=` line appears AFTER `records=0`. If the empty-Lumen tenant has ZERO Cinder placements (the never-touched tenant case), the stdout is exactly `records=0\n` — byte-equivalent to the predecessor's OK3. | 100% of `stats()` invocations against an empty-Lumen tenant produce `records=0\n` as the first stdout line; 0% of such invocations produce any `earliest=` or `latest=` line; 100% of such invocations against a Cinder-empty tenant produce stdout byte-equivalent to `records=0\n`; 100% of such invocations against a Cinder-non-empty tenant produce `records=0\n` followed by the selectively-emitted non-zero `<tier>=<count>` lines in the order `hot` → `warm` → `cold` | n/a (today there is no CLI surface that distinguishes the empty-Lumen-empty-Cinder case from the empty-Lumen-non-empty-Cinder case at all; the operator has no observable signal for orphan tier metadata) | Same new test file — orphan-cinder scenario asserts `records=0\nhot=2\ncold=1\n` for an empty-Lumen tenant with `H=2, W=0, C=1`; PLUS the existing `tests/stats_subcommand.rs` empty-tenant test continuing to pass green UNMODIFIED (because the predecessor's test fixtures DO place Cinder items per batch via `ingest()`, the predecessor's empty-tenant test uses a separate never-ingested tenant `acmee` which has zero Cinder placements — Option B preserves the byte-equivalent output for that case) | Leading (operator-visible behaviour; disambiguates orphan-tier-metadata from never-touched-tenant in a `grep`-friendly way while preserving the predecessor's contract) |
| OK4-CLI-stats-backwards-compatible-populated-then-cinder-zero | Priya the platform operator, observed at the stdout byte level | Sees, for any tenant whose Cinder placements are ALL zero (regardless of Lumen state), the same stdout output the predecessor (`cli-stats-subcommand-v0`) produced for the same `(tenant, data_dir)` pair — byte-equivalent. The `hot=` / `warm=` / `cold=` lines are NOT emitted when their counts are zero (per Option B); existing operator shell scripts that `wc -l` the stats output for these tenants continue to return the same line count (1 for empty-Lumen-empty-Cinder, 3 for populated-Lumen-empty-Cinder). | 100% of `stats()` invocations against tenants with `list_by_tier(tenant, Tier::Hot).len() == 0 AND .Warm == 0 AND .Cold == 0` produce stdout byte-equivalent to the predecessor's output for the same `(tenant, data_dir)` pair; 0% of such invocations introduce any new line that breaks the predecessor's line-count invariant | n/a (the predecessor IS the baseline — OK4 is the contract that the predecessor's behaviour is preserved unchanged for the no-Cinder-placement cases) | Same new test file — backwards-compat scenario seeds a tenant with positive Lumen records and zero Cinder placements (explicitly constructed via a direct `FileBackedTieringStore::open` without any `place()` call, NOT via `ingest()` which would place Hot items per batch) and asserts the stdout is exactly the three lines `records=N\nearliest=...\nlatest=...\n` with no Cinder lines. PLUS the existing `crates/kaleidoscope-cli/tests/stats_subcommand.rs` test file continuing to pass green UNMODIFIED is the supplementary byte-level oracle (per `wave-decisions.md` D10) | Guardrail (operator-facing non-regression; protects every existing `stats`-consuming shell pipeline) |

## Metric Hierarchy

- **North Star**: **OK1-CLI-stats-tier-counts** — the tier-count
  correctness KPI. Without it, the subcommand extension cannot answer
  the operator's question. With it alone, the operator already has
  the migration-health / read-latency-expectation / hot-tier-balloon
  decisions answerable from one CLI invocation.
- **Leading Indicators**:
  - OK2 (tenant isolation) — proves the per-tier counts honour the
    per-tenant isolation invariant inherited from
    `cinder::TieringStore`'s trait-level semantic
    (`crates/cinder/src/store.rs:71-72`).
- **Guardrail Metrics**:
  - OK3 (empty-Lumen + Option B) — surfaces orphan tier metadata
    while preserving the predecessor's empty-tenant contract for the
    never-touched case.
  - OK4 (backwards-compatibility for zero-Cinder-placement tenants)
    — the protection against breaking existing operator shell
    pipelines that script against the predecessor's three-line
    output for populated tenants with no Cinder placements, or the
    one-line output for never-touched tenants. The locked
    `crates/kaleidoscope-cli/tests/stats_subcommand.rs` test file is
    the byte-level oracle for OK4.

## Cross-feature alignment

OK1 in this feature is the inspection-side mirror of the Cinder
placements that `ingest` writes (`ingest`'s `flush()` places one
Hot Cinder item per batch via
`crates/kaleidoscope-cli/src/lib.rs:243-244`). The `hot=H` count from
`stats` should agree, modulo subsequent maintenance migrations, with
the total Hot placements that all prior `ingest` invocations have
made for the tenant.

OK4 in this feature is structurally the same shape as the predecessor
feature's tenant-isolation OK1 reinforcement — it is a guardrail
against accidentally breaking an upstream contract that operators
already rely on. The difference is that OK4 covers a wave-to-wave
contract (the predecessor's stdout shape) rather than a tenant-to-
tenant contract (per-tenant isolation).

| KPI | Cross-feature precedent | This feature |
|-----|-------------------------|--------------|
| OK1 | `cli-stats-subcommand-v0` OK1 writes `records=N` to stdout where N equals `read()`'s count for the same `(tenant, data_dir)` | `stats` writes `hot=H` / `warm=W` / `cold=C` lines to stdout where each value equals `list_by_tier(tenant, tier).len()` for the corresponding `Tier::Hot/Warm/Cold` |
| OK2 | `cli-stats-subcommand-v0` OK1 reinforcement: `stats(&acme, ...)` reports `records=7` when `acme` has 7 records and `globex` has 3 in the same `data_dir`, NEVER 10 | `stats(&acme, ...)` reports `hot=5` when `acme` has 5 Hot Cinder items and `globex` has 9 in the same `data_dir`, NEVER 14 |
| OK3 | `cli-stats-subcommand-v0` OK3 writes exactly `records=0\n` for an empty tenant, no timestamp lines | `stats` writes `records=0\n` for an empty-Lumen tenant, no timestamp lines, PLUS the selectively-emitted non-zero Cinder lines if any (the orphan-tier-metadata case); for the empty-Lumen-and-empty-Cinder case, byte-equivalent to the predecessor |
| OK4 | (n/a — no prior feature has a backwards-compatibility KPI against an immediately-preceding wave's stdout shape) | `stats` for tenants with zero Cinder placements is byte-equivalent to the predecessor's output — protected by the locked predecessor test file |

## Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| OK1-CLI-stats-tier-counts | `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs` — populated-multi-tier scenario and hot-only scenario | `cargo test --package kaleidoscope-cli --test stats_cinder_tier_distribution` exit code. The populated-multi-tier test pre-populates Cinder for tenant `acme` with deterministic placements (e.g. 5 Hot, 12 Warm, 47 Cold — achieved via a fixture that opens `FileBackedTieringStore::open` and calls `place()` per `(tenant, item_id, tier, placed_at)` triple) and asserts the captured stdout from a `stats()` call contains `hot=5` / `warm=12` / `cold=47` as lines 4/5/6. The hot-only test pre-populates only Hot placements and asserts only the `hot=` line appears after the Lumen lines | At every commit touching the CLI stats path or the Cinder `list_by_tier` method | `kaleidoscope-cli` maintainer (CI feedback per ADR-0005) |
| OK2-CLI-stats-tier-tenant-isolation | Same test file — tenant-isolation scenario | Same `cargo test` invocation. The test pre-populates Cinder for tenant `acme` with 5 Hot placements and, separately, Cinder for tenant `globex` with 9 Hot placements into the SAME `data_dir`. It then calls `stats(&acme, ...)` and asserts the `hot=` line reports 5 (NOT 14, NOT 9). | Same | Same |
| OK3-CLI-stats-no-records-no-timestamps-still | Same test file — orphan-cinder scenario | Same `cargo test` invocation. The test opens a fresh `data_dir`, populates Cinder for tenant `acme` (without any Lumen ingest), and asserts (a) `stats()` returns Ok, (b) captured stdout's line 1 equals `records=0`, (c) no line begins with `earliest=`, (d) no line begins with `latest=`, (e) the selectively-emitted non-zero Cinder lines appear in `hot` → `warm` → `cold` order. PLUS the existing `tests/stats_subcommand.rs` empty-tenant test (the predecessor's locked oracle) continues to pass green unmodified, providing the byte-equivalent oracle for the empty-Lumen-and-empty-Cinder case | Same | Same |
| OK4-CLI-stats-backwards-compatible-populated-then-cinder-zero | Same test file — backwards-compat scenario AND the unmodified existing `crates/kaleidoscope-cli/tests/stats_subcommand.rs` test file | Same `cargo test` invocation. The backwards-compat test directly seeds Lumen (via `lumen::FileBackedLogStore::open(...).ingest(...)` or via the `kaleidoscope_cli::ingest()` library function — DESIGN locks the choice) AND opens an EMPTY Cinder `FileBackedTieringStore` (no `place()` calls), then asserts the stdout is exactly the three lines `records=N\nearliest=...\nlatest=...\n` with no Cinder lines. The supplementary oracle is the existing `tests/stats_subcommand.rs` test file: it MUST continue to pass green unmodified (`wave-decisions.md` D10 makes the no-modification rule a hard contract). Any byte-level drift in the stdout for the predecessor's test cases would surface as a failure in the locked file | Same | Same |

## Hypothesis

We believe that **extending the existing `stats` subcommand on
`kaleidoscope-cli` to ALSO emit, after the existing `records=` /
`earliest=` / `latest=` lines, up to three additional `hot=H` /
`warm=W` / `cold=C` plain-text key=value stdout lines (selectively
emitted only for tiers where the count is non-zero, per Option B),
where each value equals `cinder.list_by_tier(tenant, tier).len()`
against a `FileBackedTieringStore::open(cinder_base(data_dir),
recorder)` opened in the same call** for the **platform operator
(Priya)** will achieve **a one-shot, pipeable, grep-friendly answer
to the three operationally distinct tier-distribution questions (are
migrations happening? should reads be slow? is hot ballooning?) that
replaces the operator's current "write a Rust harness or inspect
`cinder.*` snapshots by hand" workflow, without breaking any
existing operator shell pipeline that scripts against the
predecessor's stdout shape for tenants with no Cinder placements**.

We will know this is true when:

- The new acceptance test's populated-multi-tier scenario passes
  green, asserting that `stats()` against an `acme` populated with
  5 Hot + 12 Warm + 47 Cold Cinder items produces lines 4/5/6 of
  stdout equal to `hot=5` / `warm=12` / `cold=47` (OK1).
- The new acceptance test's hot-only scenario passes green,
  asserting that `stats()` against an `acme` populated with only
  Hot Cinder items produces a single `hot=H` line and NO `warm=` /
  `cold=` lines (OK1 + the Option B contract).
- The new acceptance test's tenant-isolation scenario passes green,
  asserting `stats(&acme, ...)` reports `hot=5` when `acme` has 5
  Hot items and `globex` has 9 Hot items in the same `data_dir`,
  NEVER 14 (OK2).
- The new acceptance test's orphan-cinder scenario passes green,
  asserting that `stats()` against an empty-Lumen tenant with
  `H=2, W=0, C=1` produces `records=0\nhot=2\ncold=1\n` with no
  `earliest=` / `latest=` / `warm=` lines (OK3).
- The new acceptance test's backwards-compat scenario passes green,
  asserting that `stats()` against a tenant with positive Lumen
  records and ZERO Cinder placements produces stdout byte-
  equivalent to the predecessor's output (OK4).
- The EXISTING `crates/kaleidoscope-cli/tests/stats_subcommand.rs`
  test file continues to pass green UNMODIFIED under
  `cargo test --package kaleidoscope-cli` (OK4 supplementary
  oracle).
- The dogfood demo runs: `kaleidoscope-cli stats acme /tmp/k-data |
  grep ^hot= | cut -d= -f2` returns the same integer that an
  ad-hoc Rust harness's `list_by_tier(&acme, Tier::Hot).len()`
  returns against the same `cinder_base(/tmp/k-data)` directory.

## Handoff to DESIGN

The DESIGN wave (`nw-solution-architect`) should preserve:

1. **The quiescent recorder pattern**: the function constructs a
   `cinder::NoopRecorder` for the
   `FileBackedTieringStore::open(cinder_base(data_dir), recorder)`
   call identically to the way `ingest()` does in its no-flag arm
   (`crates/kaleidoscope-cli/src/lib.rs:170-174`); no OTLP file is
   created and no `--observe-otlp` flag is accepted in v0
   (`wave-decisions.md` D9 RULES OUT in-place modification of `stats()`
   without a new parameter; the chosen shape — extend `stats()` with
   a third capability OR add `stats_with_cinder()` — is DESIGN's
   decision).
2. **The `TieringStore::list_by_tier(tenant, tier)` call shape**:
   exactly three calls per `stats()` invocation (one per tier),
   taking each result's `.len()` as the count. No `evaluate_at(...)`
   call, no `place(...)` call, no `migrate(...)` call.
3. **The stdout output contract**: plain-text key=value lines, one
   stat per line, terminated by `\n`. The Cinder keys are exactly
   `hot`, `warm`, `cold` (lower-case) and appear in that order
   when present, AFTER the existing Lumen-side lines. A tier with
   zero items produces no line at all (Option B).
4. **The byte-equivalent backwards-compat contract**: for any
   tenant with all-zero Cinder placements, the stdout is byte-
   equivalent to the predecessor's output for the same
   `(tenant, data_dir)` pair. The locked
   `crates/kaleidoscope-cli/tests/stats_subcommand.rs` test file
   must continue to pass green unmodified.

The DESIGN wave should NOT introduce flags
(`--observe-otlp`, `--cinder-only`, `--items`, `--format=...`),
additional output keys (per-item dumps, per-tier histograms),
policy evaluation calls (`evaluate_at`), or any multi-tenant
aggregation.

## DEVOPS instrumentation needs

No new collection infrastructure. The `stats` subcommand remains a
pure read over the existing Lumen+Cinder WAL+snapshot pair and emits
no OTLP, no metrics, no logs of its own (the in-process Pulse sink on
the Lumen side and the `NoopRecorder` on the Cinder side are
intentionally quiescent). The CI gate is the new acceptance test's
exit code PLUS the unmodified predecessor test file's continued
green status, per ADR-0005 Gate 1 (the workspace already runs
`cargo test` on every commit).
