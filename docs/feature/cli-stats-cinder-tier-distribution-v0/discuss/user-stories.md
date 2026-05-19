<!-- markdownlint-disable MD024 -->

# User Stories — `cli-stats-cinder-tier-distribution-v0`

## System Constraints (apply to every story)

- Rust idiomatic per `CLAUDE.md`: data + free functions + traits where
  polymorphism is genuinely needed. The change is a free-function-level
  extension on `kaleidoscope_cli`; no new trait is introduced and no
  existing trait is modified. The existing `cinder::TieringStore`
  trait's `list_by_tier(tenant, tier)` method (`crates/cinder/src/store.rs:101-102`)
  is the only Cinder API consumed.
- License: AGPL-3.0-or-later, matching the rest of the workspace.
- The acceptance idiom for this project is Rust `#[test]` functions
  with `// Given / // When / // Then` comment blocks, not Gherkin
  `.feature` files. The Given/When/Then text in the UAT Scenarios
  sections below is the specification; DISTILL translates it into
  `#[test]` functions in
  `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`
  (NEW file) mirroring the harness pattern already in
  `crates/kaleidoscope-cli/tests/stats_subcommand.rs` (the locked
  predecessor file, which MUST NOT be modified — see
  `wave-decisions.md` D10).
- Output-shape contract: stdout is plain-text key=value lines, one
  stat per line, terminated by `\n`. The keys reserved by this
  feature are exactly `hot`, `warm`, `cold` (lower-case, matching the
  established convention of the predecessor's `records` / `earliest`
  / `latest` keys). No header line, no trailing blank line, no JSON,
  no CSV, no colour codes, no Unicode box drawing.
- Key ordering contract (extended from the predecessor): when present,
  the lines appear in the order `records`, `earliest`, `latest`,
  `hot`, `warm`, `cold`. Existing Lumen-side lines always come before
  Cinder-side lines. Within Cinder, the order is the forward-lifecycle
  order `Tier::Hot` → `Tier::Warm` → `Tier::Cold`
  (`crates/cinder/src/tier.rs:38-44`).
- Empty-render contract (Option B per `wave-decisions.md`
  D-EmptyRender): a `hot=` / `warm=` / `cold=` line is emitted ONLY
  when the count for that tier is non-zero. A tier with zero items
  produces no line at all. This preserves byte-equivalent
  backwards-compatibility for any tenant with no Cinder placements
  (OK4) and gives the operator the same `wc -l == 0` disambiguation
  pattern the predecessor uses for the empty-tenant case.
- Stream contract: the additional tier-distribution lines are written
  to **stdout** (same stream as the existing `records=` / `earliest=`
  / `latest=` lines, for the same reason the predecessor uses stdout —
  the stats ARE the principal output).
- No-flag contract: the `stats` subcommand still accepts NO optional
  flags. No `--observe-otlp`, no `--cinder-only`, no `--no-cinder`, no
  `--items`, no `--format=...`. The Cinder lines are emitted
  unconditionally when their count is non-zero.
- Tenant-isolation contract: `stats` for tenant `acme` MUST NOT count
  Cinder items belonging to tenant `globex`. Inherited from
  `cinder::TieringStore`'s per-tenant isolation invariant
  (`crates/cinder/src/store.rs:71-72` documents per-tenant isolation
  as a trait-level semantic).
- Read-only contract: `stats` mutates nothing on the Cinder side
  either. No `place()` calls, no `migrate()` calls, no
  `evaluate_at()` calls. The Cinder WAL+snapshot under
  `<data_dir>/cinder.*` is unchanged after the call.
- Backwards-compatibility contract (OK4): for any tenant whose Cinder
  placements are all zero, `stats()`'s stdout is byte-equivalent to
  the predecessor (`cli-stats-subcommand-v0`)'s output for the same
  `(tenant, data_dir)` pair. The locked
  `tests/stats_subcommand.rs` test file MUST continue to pass green
  unmodified (`wave-decisions.md` D10).

---

## US-01: Operator inspects a tenant's tier distribution without writing Rust

### Elevator Pitch

- **Before**: Priya wants to know "are the warm-to-cold migrations
  actually happening for `acme`?" or "is `acme`'s hot tier
  ballooning?" or "should I expect `acme`'s read latency to be high
  because most items are in cold?" Today her only options are to
  either write a Rust harness that opens
  `cinder::FileBackedTieringStore` and calls `list_by_tier` for each
  tier, OR to inspect the `cinder.*` snapshot files directly with
  whatever JSON/Bincode tooling she can pull together. Both paths are
  operationally hostile: the Rust harness requires a workspace
  checkout and a `cargo build`; the snapshot inspection requires
  knowing the on-disk layout and reasoning about WAL+snapshot
  merging. As a result she does NOT routinely check the tier
  distribution after ingest runs, and tier-related operational issues
  (migrations not happening, hot tier ballooning, cold items
  predominating) go unobserved until a downstream symptom surfaces.

- **After**: Priya runs the SAME existing CLI invocation she already
  uses for the record-count + time-window smoke test:

  ```text
  kaleidoscope-cli stats acme /tmp/data
  ```

  Stdout, in milliseconds, now prints up to six lines instead of up
  to three:

  ```text
  records=10000000
  earliest=2026-05-18T00:00:01.123456789Z
  latest=2026-05-19T03:45:12.987654321Z
  hot=42
  warm=315
  cold=9643
  ```

  She knows immediately that `acme` has 42 items hot, 315 warm, and
  9643 cold — a ratio that tells her migrations have been actively
  pushing items down the tier ladder (the cold count dominates),
  reads on this tenant will be slow on average (cold predominates),
  and the hot tier is healthy (small). If she wants just the hot
  count: `kaleidoscope-cli stats acme /tmp/data | grep ^hot= | cut
  -d= -f2`. If she wants the distribution as a quick visual:
  `grep -E '^(hot|warm|cold)='`. No pipeline that materialises every
  Cinder placement; no Rust code; no `cinder.*` snapshot inspection.

  When `acme` has no items in a particular tier, that line is simply
  absent: `kaleidoscope-cli stats acme /tmp/data` on a fresh tenant
  with only hot placements prints `records=N` + timestamps +
  `hot=H`, and the absence of `warm=` / `cold=` lines is the
  unambiguous signal that those tiers are empty (same
  `grep | wc -l == 0` disambiguation pattern the predecessor uses for
  `earliest=` / `latest=` in the empty-Lumen case).

  Crucially, for any tenant that has no Cinder placements at all
  (every tier count is zero), the stdout is BYTE-EQUIVALENT to what
  the predecessor (`cli-stats-subcommand-v0`) produced — Priya's
  existing shell scripts that `wc -l` on the stats output for those
  tenants continue to return the same line count.

- **Decision enabled**: Priya can decide three operationally distinct
  questions from one CLI invocation:
  1. "Are warm-to-cold migrations actually happening for `acme`?"
     (rising cold count over time, falling warm count, hot stable).
  2. "Should I expect read latency on `acme` to be high?" (cold
     predominates → yes; hot predominates → no).
  3. "Is the hot tier ballooning?" (hot count growing without bound
     → migrations broken or policy too lax).

### Problem

Priya the platform operator runs a multi-tenant Kaleidoscope
deployment. After the predecessor (`cli-stats-subcommand-v0`) shipped,
she has a one-shot answer to "how much Lumen data does this tenant
have, and over what time window?". She does NOT yet have a one-shot
answer to "how is this tenant's data distributed across the Cinder
tiers RIGHT NOW?".

The Cinder tier distribution is the operationally critical signal for
three orthogonal decisions:

1. **Migration health**: "are the warm-to-cold migrations actually
   running for `acme`?" Today this requires either inspecting
   `cinder.*` snapshot deltas across time (no tooling supports it),
   or writing a Rust harness that opens the store and calls
   `list_by_tier` per tier and per tenant.
2. **Read-latency expectation**: "if I run a query against `acme`,
   should I expect it to be fast (mostly hot) or slow (mostly cold)?"
   Today this requires the same Rust harness or guesswork.
3. **Hot-tier health**: "is the hot tier ballooning for `acme`?"
   Today this requires the same Rust harness, run periodically — and
   in practice nobody runs it.

All three reduce to "give me the count of items in each tier for one
tenant". The underlying API
(`cinder::TieringStore::list_by_tier(tenant, tier)`, returning
`Vec<ItemId>` per `crates/cinder/src/store.rs:101-102`) is already
adequate; the gap is the missing CLI surface.

### Who

Priya the platform operator | runs a multi-tenant Kaleidoscope
deployment for a fintech | already uses `kaleidoscope-cli ingest`,
`kaleidoscope-cli read`, and `kaleidoscope-cli stats` daily
(inherited from the predecessor and three reference features) | wants
a one-shot, pipeable answer to "what is the tier distribution for this
tenant?" without writing Rust or inspecting `cinder.*` snapshots |
uses standard Unix text tools (`grep`, `cut`, `awk`) on stdout output,
not JSON parsers | does NOT want her existing `stats`-consuming shell
scripts to break (the byte-equivalence contract).

### Solution

Extend the existing `stats` subcommand so the SAME invocation:

```text
kaleidoscope-cli stats <tenant_id> <data_dir>
```

…ALSO opens `cinder::FileBackedTieringStore::open(cinder_base(data_dir),
recorder)` with a quiescent `cinder::NoopRecorder` (the same pattern
`ingest`'s no-flag arm uses at
`crates/kaleidoscope-cli/src/lib.rs:170-174`) and emits up to three
additional stdout lines AFTER the existing `records=` / `earliest=` /
`latest=` lines:

- `hot=<H>` where `H == cinder.list_by_tier(tenant, Tier::Hot).len()` —
  emitted only when `H > 0`.
- `warm=<W>` where `W == cinder.list_by_tier(tenant, Tier::Warm).len()` —
  emitted only when `W > 0`.
- `cold=<C>` where `C == cinder.list_by_tier(tenant, Tier::Cold).len()` —
  emitted only when `C > 0`.

Lines appear in the order `hot` → `warm` → `cold` (the forward
lifecycle order from `crates/cinder/src/tier.rs:38-44`). Exit code is
`0` regardless of which tiers are non-empty.

The function signature is DESIGN-locked per `wave-decisions.md` D9.
Two plausible shapes:
1. Extend the existing `stats(tenant, data_dir, writer)` to take a
   third capability (e.g. `Option<&dyn TieringStore>`), and have the
   binary pass `Some(&cinder_store)`.
2. Add a parallel `stats_with_cinder(tenant, data_dir, writer)`
   function with the same signature shape, leaving the existing
   `stats()` untouched.

An in-place modification of the existing `stats()` (without a new
parameter) is RULED OUT by `wave-decisions.md` D9 because the
predecessor's locked test file
(`crates/kaleidoscope-cli/tests/stats_subcommand.rs`) uses `ingest()`
fixtures which place Cinder Hot items per batch
(`crates/kaleidoscope-cli/src/lib.rs:243-244`), so an in-place
modification would change the byte-level stdout of the predecessor's
test cases and break the locked file.

The `Error` type reuses the existing `kaleidoscope_cli::Error`
variants — at minimum `CinderOpen(MigrateError)` is already wired
(`crates/kaleidoscope-cli/src/lib.rs:77`); no new error variant is
introduced for v0.

### Domain Examples

#### 1. Happy path — Priya inspects `acme` with a populated multi-tier distribution

Priya has previously run, on a daily cadence over the past week:

```text
kaleidoscope-cli ingest acme /tmp/k-data < acme-day-{1,2,...,7}.ndjson
```

Each ingest run placed one Cinder Hot item per batch (per the existing
`flush()` at `crates/kaleidoscope-cli/src/lib.rs:243-244`). A separate
maintenance process has subsequently migrated older items down the
tier ladder so that the current distribution for `acme` is:

- 5 items in Hot (recent batches not yet migrated)
- 12 items in Warm (mid-age batches migrated once)
- 47 items in Cold (oldest batches migrated twice)

And the Lumen side has 10 million records spanning the 7-day window.
She runs:

```text
kaleidoscope-cli stats acme /tmp/k-data
```

Stdout contains exactly six lines, in order:

```text
records=10000000
earliest=2026-05-12T00:00:00.000000000Z
latest=2026-05-19T03:45:12.987654321Z
hot=5
warm=12
cold=47
```

The output ends with a trailing `\n`. Exit code is `0`. Stderr is
empty. Priya pipes through `grep ^cold= | cut -d= -f2` and gets
`47`; she pipes through `grep -E '^(hot|warm|cold)='` and gets the
three-line distribution summary. The Cinder store under
`/tmp/k-data` is unchanged after the call (no `place()` calls, no
`migrate()` calls).

#### 2. Edge case — Priya inspects `acme` with hot placements but no warm and no cold

Priya checks a tenant immediately after a fresh ingest run, before
any maintenance migration has run. `acme` has 3 batches' worth of
Cinder Hot placements and zero items in Warm and zero in Cold:

```text
kaleidoscope-cli stats acme /tmp/k-data
```

Stdout contains exactly four lines:

```text
records=300
earliest=2026-05-19T00:00:00.000000000Z
latest=2026-05-19T00:14:59.999999999Z
hot=3
```

No `warm=` line. No `cold=` line. Their absence is the unambiguous
signal that those tiers are empty — same disambiguation pattern as the
predecessor's empty-tenant case. `grep ^warm= /tmp/output | wc -l`
returns `0`, which Priya can script against in the same way she
already scripts against the predecessor's `earliest=` / `latest=`
absence.

#### 3. Edge case (the unusual but possible case) — Priya inspects `acme` with no Lumen records but non-zero Cinder placements

Priya inherits a `data_dir` from a previous deployment where the Lumen
WAL+snapshot has been pruned for retention but the Cinder snapshot
still references items that no longer exist in Lumen (orphan tier
metadata — operationally meaningful and exactly the thing she would
want to know about):

```text
kaleidoscope-cli stats acme /tmp/inherited-data
```

Stdout contains exactly three lines:

```text
records=0
hot=2
cold=1
```

No `earliest=` line (the Lumen tenant is empty, per the predecessor's
OK3). No `warm=` line (warm count is zero, per Option B). The
`records=0` + non-zero Cinder lines together signal "this tenant has
no live Lumen data but still has Cinder placements outstanding — go
investigate". `grep ^records=0` AND `grep -E '^(hot|warm|cold)='`
return both truthy, which is the operator's "orphan tier metadata"
detection.

#### 4. Boundary case (the most-common legacy case) — backwards-compatibility for tenants without Cinder placements

A tenant `legacy_acme` was ingested under a much older code path that
did not place Cinder items at all (or `legacy_acme` simply has no
Cinder placements because the Cinder snapshot was reset). The Lumen
side has 4 records.

```text
kaleidoscope-cli stats legacy_acme /tmp/k-data
```

Stdout contains exactly three lines — BYTE-EQUIVALENT to the
predecessor's output for the same `(tenant, data_dir)` pair:

```text
records=4
earliest=2026-05-18T00:00:00.000000000Z
latest=2026-05-18T00:00:03.000000000Z
```

No `hot=` line (count is zero). No `warm=` line. No `cold=` line.
Operators with existing shell scripts that `wc -l` the stats output
for these tenants continue to see the same `3`. This is the OK4
backwards-compatibility contract.

### UAT Scenarios (BDD)

#### Scenario: Populated tenant with multi-tier distribution — Priya sees six lines in order

```text
Given Priya has pre-ingested records for tenant acme into /tmp/k-data such that Lumen has N > 0 records
And Cinder has been populated for tenant acme such that list_by_tier(acme, Hot).len() == H > 0, list_by_tier(acme, Warm).len() == W > 0, list_by_tier(acme, Cold).len() == C > 0
When Priya invokes the stats subcommand against tenant acme, /tmp/k-data, with a captured stdout sink
And the call returns Ok
Then the captured stdout contains exactly 6 non-empty lines, in order
And line 1 equals `records=N`
And line 2 begins with `earliest=`
And line 3 begins with `latest=`
And line 4 equals `hot=H`
And line 5 equals `warm=W`
And line 6 equals `cold=C`
And the stdout ends with `\n`
```

#### Scenario: Populated Lumen, hot-only Cinder — Priya sees four lines

```text
Given Priya has pre-ingested records for tenant acme into /tmp/k-data such that Lumen has N > 0 records
And Cinder has been populated for tenant acme such that list_by_tier(acme, Hot).len() == H > 0, list_by_tier(acme, Warm).len() == 0, list_by_tier(acme, Cold).len() == 0
When Priya invokes the stats subcommand against tenant acme, /tmp/k-data, with a captured stdout sink
And the call returns Ok
Then the captured stdout contains exactly 4 non-empty lines, in order
And line 1 equals `records=N`
And line 4 equals `hot=H`
And no line begins with `warm=`
And no line begins with `cold=`
And the stdout ends with `\n`
```

#### Scenario: Empty-Lumen, populated-Cinder (orphan tier metadata) — Priya sees `records=0` plus non-zero Cinder lines

```text
Given the Lumen store at /tmp/k-data contains zero records for tenant acme
And Cinder at /tmp/k-data has been populated for tenant acme such that list_by_tier(acme, Hot).len() == 2 and list_by_tier(acme, Cold).len() == 1 and list_by_tier(acme, Warm).len() == 0
When Priya invokes the stats subcommand against tenant acme, /tmp/k-data, with a captured stdout sink
And the call returns Ok
Then the captured stdout contains exactly 3 non-empty lines, in order
And line 1 equals `records=0`
And line 2 equals `hot=2`
And line 3 equals `cold=1`
And no line begins with `earliest=`
And no line begins with `latest=`
And no line begins with `warm=`
And the stdout ends with `\n`
```

#### Scenario: Backwards-compatibility — tenant with populated Lumen and zero Cinder placements

```text
Given Priya has populated Lumen for tenant legacy_acme into /tmp/k-data with 4 records
And Cinder at /tmp/k-data has zero placements for tenant legacy_acme — list_by_tier(legacy_acme, Hot).len() == 0, list_by_tier(legacy_acme, Warm).len() == 0, list_by_tier(legacy_acme, Cold).len() == 0
When Priya invokes the stats subcommand against tenant legacy_acme, /tmp/k-data, with a captured stdout sink
And the call returns Ok
Then the captured stdout contains exactly 3 non-empty lines, in order
And line 1 equals `records=4`
And line 2 begins with `earliest=`
And line 3 begins with `latest=`
And no line begins with `hot=`
And no line begins with `warm=`
And no line begins with `cold=`
And the stdout is byte-equivalent to what the predecessor (`cli-stats-subcommand-v0`) would have produced for the same (tenant, data_dir) pair
And the stdout ends with `\n`
```

#### Scenario: Tenant isolation — Cinder tier counts for `acme` do not count `globex` placements

```text
Given Priya has populated Cinder at /tmp/k-data for tenant acme such that list_by_tier(acme, Hot).len() == 5
And Priya has separately populated Cinder at /tmp/k-data for tenant globex such that list_by_tier(globex, Hot).len() == 9
And Lumen at /tmp/k-data has N > 0 records for both tenants
When Priya invokes the stats subcommand against tenant acme, /tmp/k-data, with a captured stdout sink
And the call returns Ok
Then the line beginning with `hot=` shows the count 5 (NOT 14 and NOT 9)
And the count 5 equals list_by_tier(acme, Hot).len() against the Cinder store at /tmp/k-data
```

### Acceptance Criteria

- [ ] When tenant has `H > 0` hot, `W > 0` warm, and `C > 0` cold
  Cinder placements (and `N > 0` Lumen records), the captured stdout
  contains exactly 6 non-empty lines in the order
  `records=N`, `earliest=...`, `latest=...`, `hot=H`, `warm=W`,
  `cold=C`, terminated by `\n`.
- [ ] When tenant has `H > 0` hot but `W == 0` and `C == 0` (and
  `N > 0` Lumen records), the captured stdout contains exactly 4
  non-empty lines: `records=N`, `earliest=...`, `latest=...`,
  `hot=H`. No `warm=` line; no `cold=` line.
- [ ] When tenant has `N == 0` Lumen records AND non-zero Cinder
  placements (e.g. `H == 2`, `W == 0`, `C == 1`), the captured stdout
  contains exactly 3 non-empty lines: `records=0`, `hot=2`, `cold=1`.
  No `earliest=` line, no `latest=` line, no `warm=` line.
- [ ] When tenant has `N > 0` Lumen records AND zero Cinder
  placements (every tier count is zero), the captured stdout contains
  exactly 3 non-empty lines: `records=N`, `earliest=...`, `latest=...`.
  No `hot=` line, no `warm=` line, no `cold=` line. This output is
  byte-equivalent to what the predecessor (`cli-stats-subcommand-v0`)
  would have produced for the same `(tenant, data_dir)` pair (OK4).
- [ ] When tenant has `N == 0` Lumen records AND zero Cinder
  placements (the never-touched tenant), the captured stdout contains
  exactly 1 non-empty line: `records=0`. This output is byte-
  equivalent to the predecessor's OK3 contract.
- [ ] The `hot=` value equals `cinder.list_by_tier(tenant, Tier::Hot).len()`
  against a `FileBackedTieringStore::open(cinder_base(data_dir),
  recorder)` opened in the same call. Likewise for `warm=` and
  `cold=`.
- [ ] When two tenants `acme` (5 hot Cinder items) and `globex` (9 hot
  Cinder items) coexist in one `data_dir`, the `stats` subcommand
  invoked with tenant `acme` reports `hot=5` (NOT 14 and NOT 9). The
  same per-tenant isolation holds for `warm=` and `cold=`.
- [ ] The library function does not mutate the Cinder WAL or snapshot
  under `cinder_base(data_dir)` (read-only invariant — assertable by
  computing a checksum of the directory before and after, or by
  re-querying with the same call and observing identical output).
- [ ] No new error variant is introduced. The function reuses the
  existing `kaleidoscope_cli::Error` variants
  (`crates/kaleidoscope-cli/src/lib.rs:73-84`), including
  `CinderOpen(MigrateError)` for Cinder store-open errors and
  `LumenOpen(LogStoreError)` / `LumenQuery(LogStoreError)` for the
  inherited Lumen-side errors.
- [ ] The existing acceptance test file
  `crates/kaleidoscope-cli/tests/stats_subcommand.rs` continues to
  pass green UNMODIFIED under
  `cargo test --package kaleidoscope-cli` (the predecessor's
  OK1/OK2/OK3 byte-level oracle remains the byte-level oracle for the
  no-Cinder-placement cases per OK4).
- [ ] The new acceptance test file
  `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`
  is added (NEW file, mirroring the harness pattern of
  `stats_subcommand.rs`) with assertions covering the five UAT
  scenarios above.
- [ ] `print_usage` in `crates/kaleidoscope-cli/src/main.rs` (lines
  71-97) — if it documents the `stats` subcommand's output shape —
  is updated to mention the additional tier-distribution lines, OR
  is left unchanged if the existing description is generic enough.
  Choice deferred to DESIGN.

### Outcome KPIs

- **Who**: platform operator (Priya), observed at the stdout byte
  level on the existing `kaleidoscope-cli stats <tenant> <data_dir>`
  CLI invocation.
- **Does what**: receives the tenant's Cinder tier distribution
  (`hot=H` / `warm=W` / `cold=C`, selectively emitted for non-zero
  tiers per Option B) as additional plain-text key=value stdout lines
  AFTER the existing Lumen-side `records=` / `earliest=` / `latest=`
  lines, on the same invocation that already gives her the Lumen
  side, without writing Rust code or inspecting `cinder.*` snapshot
  files.
- **By how much**: 100% of `stats()` invocations where
  `list_by_tier(tenant, Tier::Hot).len() == H` (likewise Warm and
  Cold) produce a `hot=H` stdout line when `H > 0` (likewise for
  warm/cold) and produce NO `hot=` line when `H == 0` (likewise for
  warm/cold) (OK1); 100% of `stats()` invocations where some other
  tenant `globex` has placements in the same `data_dir` produce
  per-tier counts that reflect ONLY the queried tenant's placements
  (OK2); 100% of `stats()` invocations against an empty-Lumen tenant
  with non-zero Cinder placements still produce `records=0\n` as the
  first line, followed by the selectively-emitted non-zero Cinder
  lines (OK3); 100% of `stats()` invocations against a tenant with
  zero Cinder placements (regardless of Lumen state) produce stdout
  byte-equivalent to what the predecessor (`cli-stats-subcommand-v0`)
  produced for the same `(tenant, data_dir)` pair (OK4).
- **Measured by**: new acceptance test
  `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`
  covering the four KPIs across the five UAT scenarios above. PLUS
  the existing
  `crates/kaleidoscope-cli/tests/stats_subcommand.rs` test file
  continuing to pass green UNMODIFIED (the byte-level oracle for OK4
  is unchanged).
- **Baseline**: 0% — today there is no CLI surface for tier
  distribution at all; operators answer the same question by writing
  Rust harnesses against `cinder::FileBackedTieringStore` or by
  inspecting `cinder.*` snapshot files by hand.

Maps to OK1-CLI-stats-tier-counts (principal),
OK2-CLI-stats-tier-tenant-isolation,
OK3-CLI-stats-no-records-no-timestamps-still, and
OK4-CLI-stats-backwards-compatible-populated-then-cinder-zero in
`outcome-kpis.md`.

### Technical Notes

- The exact library function shape is DESIGN-locked per
  `wave-decisions.md` D9. Two plausible shapes named in the brief
  (extend `stats()` with a third optional capability, OR add a
  parallel `stats_with_cinder()`); a third shape (in-place
  modification of `stats()` without a new parameter) is RULED OUT
  because the predecessor's locked test file uses `ingest()` fixtures
  which place Cinder Hot items per batch
  (`crates/kaleidoscope-cli/src/lib.rs:243-244`).
- The function internally constructs
  `FileBackedTieringStore::open(cinder_base(data_dir), recorder)` with
  a quiescent `cinder::NoopRecorder` (the same pattern `ingest`'s
  no-flag arm uses at `crates/kaleidoscope-cli/src/lib.rs:170-174`).
  It calls `cinder.list_by_tier(tenant, Tier::Hot)`,
  `cinder.list_by_tier(tenant, Tier::Warm)`,
  `cinder.list_by_tier(tenant, Tier::Cold)` — exactly three calls —
  and reduces each to its `.len()`. For each non-zero count, it writes
  one `writeln!(writer, "<key>={count}")` line, in the order
  `hot` → `warm` → `cold`.
- Modified file: `crates/kaleidoscope-cli/src/lib.rs` —
  `stats()` (or a new sibling function; DESIGN locks the choice)
  gains the Cinder-side computation and the conditional emission of
  the three tier lines. The existing `cinder_base(data_dir)` helper
  at line 122-124 is reused unchanged.
- Possibly modified file: `crates/kaleidoscope-cli/src/main.rs` — if
  DESIGN chooses the new-sibling-function shape, the `Some("stats")`
  dispatch arm must be updated to call the new function. If DESIGN
  chooses to extend the existing `stats()`'s signature, the dispatch
  arm forwards the new capability argument.
- New test file:
  `crates/kaleidoscope-cli/tests/stats_cinder_tier_distribution.rs`.
  Mirrors the harness pattern from
  `crates/kaleidoscope-cli/tests/stats_subcommand.rs` (the predecessor's
  locked file).
- Manifest: `crates/kaleidoscope-cli/Cargo.toml` gains a new
  `[[test]]` entry `name = "stats_cinder_tier_distribution", path =
  "tests/stats_cinder_tier_distribution.rs"`. The `cinder` dependency
  is already present.
- DO NOT modify
  `crates/kaleidoscope-cli/tests/stats_subcommand.rs` —
  `wave-decisions.md` D10 makes this a hard contract.
- Slice tag: not `@infrastructure` — this story directly enables
  three operator-visible decisions on a real CLI surface
  (`kaleidoscope-cli stats <tenant> <data_dir>`).

### Dependencies

- `cinder::TieringStore::list_by_tier(tenant, tier)` already exists
  and returns `Vec<ItemId>` per `crates/cinder/src/store.rs:101-102`.
- `cinder::FileBackedTieringStore` already implements `TieringStore`
  and is already constructed by `ingest()`
  (`crates/kaleidoscope-cli/src/lib.rs:179-180`).
- `cinder::NoopRecorder` is the quiescent recorder used by
  `ingest()`'s no-flag arm (`crates/kaleidoscope-cli/src/lib.rs:170-174`);
  already a `kaleidoscope-cli` dependency.
- `cinder::Tier` enum (`Hot`, `Warm`, `Cold`) at
  `crates/cinder/src/tier.rs:28-32`; already a `kaleidoscope-cli`
  dependency via the `cinder::Tier` import at
  `crates/kaleidoscope-cli/src/lib.rs:58`.
- The existing `cinder_base(data_dir)` helper at
  `crates/kaleidoscope-cli/src/lib.rs:122-124` is reused unchanged.
- `aegis::TenantId` already a dependency.
- No new external dependencies.
- No new internal crate dependencies.

### Slice

`slices/slice-01-stats-includes-cinder-tier-distribution.md`
