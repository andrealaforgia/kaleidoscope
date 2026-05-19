# Outcome KPIs — `cli-migrate-subcommand-v0`

## Feature

`cli-migrate-subcommand-v0` — add a new positional subcommand to the
existing CLI binary:
`kaleidoscope-cli migrate <tenant_id> <data_dir> <item_id> <to_tier>`.
The subcommand opens the Cinder store under `<data_dir>/cinder.*`,
calls
`cinder::TieringStore::migrate(&tenant, &ItemId::new(item_id), to_tier, SystemTime::now())`
(`crates/cinder/src/store.rs:93-99`), and writes a one-line report
to stdout:
`migrated tenant=<tenant> item=<item_id> from=<from> to=<to>\n`.
On `MigrateError::UnknownItem`, exits non-zero with a stderr line
naming the missing item. On an invalid `<to_tier>` argument (any
spelling other than lower-case `hot` / `warm` / `cold`), exits
non-zero with a stderr line naming the invalid value. v0 introduces
no new flag, no JSON output, no bulk migration, no policy
preview/dry-run, no `--observe-otlp` on this subcommand
(`wave-decisions.md` D-OutOfScope-Bulk, D-OutOfScope-Dryrun,
D-OutOfScope-Observe, D-OutOfScope-Json).

## Objective

A single
`kaleidoscope-cli migrate acme /tmp/data acme/batch-00042 cold`
invocation moves one specific Cinder item between tiers and prints
a one-line operator-visible report of the from/to transition,
giving the operator a one-shot, pipeable, grep-friendly CLI surface
for three operationally distinct decisions ("rebalance this batch
from Hot to Cold", "pull this over-aggressively-migrated item back
to Warm", "test lifecycle by walking an item through tiers"). Until
this feature shipped, those three decisions required writing a
Rust harness against `cinder::FileBackedTieringStore` or running an
off-cycle `evaluate_at` against a hand-tuned policy. v0 collapses
all three to one CLI invocation.

## Note on KPI granularity

This feature is operator-visible at the CLI surface. The principal
KPI is OK1 (migrate-success correctness — the stdout report content
matches the actual underlying `TieringStore::migrate` outcome AND
the post-call `get_entry().tier` equals the requested `to_tier`).
The second KPI is OK2 (unknown-item fail-fast — exit non-zero,
stderr names the missing item, store unchanged). The third KPI is
OK3 (invalid-tier fail-fast — exit non-zero, stderr names the
invalid value, store unchanged). The fourth KPI is OK4 (idempotent
same-tier migrate — the underlying API is idempotent per
`crates/cinder/src/store.rs:167-188`, so migrating an item to its
current tier succeeds, the stdout report shows `from=current
to=current`, and the CLI introduces NO special case for this).

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| OK1-CLI-migrate-success | Priya the platform operator, observed at the stdout byte level | Sees, on a single CLI invocation of `kaleidoscope-cli migrate <tenant> <data_dir> <item_id> <to_tier>` (with `<to_tier>` ∈ {`hot`, `warm`, `cold`} and the item placed under the tenant in Cinder), the EXACT stdout line `migrated tenant=<tenant> item=<item_id> from=<from> to=<to>\n` where `<from>` is the lower-case rendering of the tier the item was in BEFORE the call (read from `cinder.get_entry(tenant, item)` before the migrate call) and `<to>` is the lower-case rendering of the requested target tier. After the call, `cinder.get_entry(tenant, item)` returns `Some(entry)` with `entry.tier` equal to the requested target tier. Exit code 0. Stderr empty. | 100% of `migrate` invocations against placed items with valid lower-case tier arguments produce the exact stdout report line AND a post-call `get_entry(tenant, item).tier` equal to the requested `to_tier`; 0% of such invocations produce a `from` or `to` field that disagrees with what `get_entry(tenant, item)` returns before / after the call | 0% (no CLI surface for tier migration exists today; the operator's only path is a Rust harness that opens `cinder::FileBackedTieringStore` and calls `migrate(...)` against an `ItemId`, then matches on `MigrateError`) | New acceptance test `crates/kaleidoscope-cli/tests/migrate_subcommand.rs` — happy-path scenario seeds an item placed in Hot and asserts the captured stdout from a `migrate(..., warm)` call equals `migrated tenant=acme item=acme/batch-00042 from=hot to=warm\n` AND that `get_entry(acme, acme/batch-00042).unwrap().tier == Tier::Warm` after the call | Leading (operator-visible behaviour; principal KPI for this feature) |
| OK2-CLI-migrate-unknown-item-fail-fast | Priya the platform operator, observed at the stdout / stderr byte level + exit code | Sees, when the `<item_id>` argument names an item that was never `place()`d under the `<tenant>` argument in the Cinder store at `<data_dir>/cinder.*`, exit code non-zero, empty stdout, AND a single stderr line that contains the exact bytes of the `<item_id>` argument she typed. The Cinder store under `<data_dir>` is byte-equivalent before and after the call (no `place()` call, no `migrate()` call mutated state — the `MigrateError::UnknownItem` arm at `crates/cinder/src/store.rs:179-182` returns early). | 100% of `migrate` invocations against unplaced item ids produce a non-zero exit code, an empty stdout, AND a stderr line containing the offending item id verbatim; 100% of such invocations leave the Cinder store under `<data_dir>` byte-equivalent before and after (the `evaluate_at` post-call snapshot is identical to the pre-call snapshot); 0% of such invocations silently insert the item (no `place()` is called) | 0% (today operators have no CLI surface for migration at all, so there is no fail-fast contract to inherit; the underlying `MigrateError::UnknownItem` is already returned by the trait but the Rust harness consuming it is bespoke per operator) | Same new test file — unknown-item scenario asserts the call returns `Err`, captured stdout is empty, captured stderr contains the substring `acme/batch-00099`, and the post-call Cinder state for the queried tenant is identical to the pre-call state (verified via `list_by_tier(tenant, Hot/Warm/Cold).len()` snapshots before and after) | Leading (operator-facing fail-fast guarantee; mirrors the existing fail-fast posture of `--since`/`--until` parse errors at `crates/kaleidoscope-cli/src/main.rs:198-224`) |
| OK3-CLI-migrate-invalid-tier-fail-fast | Priya the platform operator, observed at the stdout / stderr byte level + exit code | Sees, when the `<to_tier>` argument is any spelling other than exactly `hot` / `warm` / `cold` (upper-case `HOT`, mixed-case `Hot`, typo `lukewarm`, empty string, leading/trailing whitespace), exit code non-zero, empty stdout, AND a single stderr line that contains the exact invalid value she typed. The Cinder store under `<data_dir>` is byte-equivalent before and after the call (the tier-argument parse error fires BEFORE any `migrate` call is issued). | 100% of `migrate` invocations with a non-`hot`/`warm`/`cold` tier argument produce a non-zero exit code, an empty stdout, AND a stderr line containing the invalid value verbatim; 100% of such invocations leave the Cinder store under `<data_dir>` byte-equivalent before and after; 0% of such invocations dispatch to the underlying `TieringStore::migrate` API (the parse error short-circuits before the store is opened, or at most before the `migrate` call) | 0% (today the lower-case tier convention is established by the rendering side at `crates/kaleidoscope-cli/src/lib.rs:389-395` but is not enforced as a parse-side contract because no parse-side surface exists) | Same new test file — invalid-tier scenario asserts the call with `to_tier_arg = "HOT"` returns `Err`, captured stdout is empty, captured stderr contains the substring `HOT`, and the post-call Cinder state for the queried tenant is identical to the pre-call state. A second invalid-tier sub-scenario uses `to_tier_arg = "lukewarm"` (a typo) and asserts the stderr contains `lukewarm` | Leading (operator-facing fail-fast guarantee; protects against a silent fallback to a default tier or a confusing diagnostic) |
| OK4-CLI-migrate-idempotent-same-tier | Priya the platform operator, observed at the stdout byte level | Sees, when she invokes `migrate` with `<to_tier>` equal to the tier the item is ALREADY in, exit code 0 and a stdout line `migrated tenant=<tenant> item=<item_id> from=<current> to=<current>\n` where `<current>` is the lower-case rendering of the (unchanged) tier. The CLI surface is faithful to the underlying API: `cinder::InMemoryTieringStore::migrate` at `crates/cinder/src/store.rs:167-188` IS idempotent for the same-tier case (it overwrites `entry.tier = to_tier` and bumps `entry.migrated_at = migrated_at` regardless of whether `from == to`), so the migrate succeeds and the report line shows `from=current to=current`. No special case in the CLI. | 100% of `migrate` invocations where `<to_tier>` equals the item's current tier produce exit code 0 AND a stdout line with `from=<X>` and `to=<X>` for the same lower-case tier `<X>`; 0% of such invocations reject the call (no `MigrateError::AlreadyInTier` or equivalent is invented or surfaced); 100% of such invocations bump the underlying `migrated_at` field (faithfully reflecting the underlying API's behaviour) | n/a (the predecessor lifecycle features documented the `migrate` API's behaviour but no CLI surface inherited a same-tier-migrate posture; the underlying API's idempotence is documented behaviour as of `crates/cinder/src/store.rs:167-188`) | Same new test file — idempotent-same-tier scenario asserts a `migrate(..., cold)` call against an item already in Cold returns Ok, captured stdout equals `migrated tenant=acme item=acme/batch-00007 from=cold to=cold\n`, and post-call `get_entry(acme, acme/batch-00007).unwrap().tier == Tier::Cold` (unchanged). Per the task brief: this is documented behaviour, NOT a special case | Guardrail (operator-facing faithfulness to the underlying API; protects against accidentally introducing a special-case CLI guard that diverges from the trait contract) |

## Metric Hierarchy

- **North Star**: **OK1-CLI-migrate-success** — the migrate-success
  correctness KPI. Without it, the subcommand cannot do its job.
  With it alone, the operator already has the three operationally
  distinct decisions enabled (manual rebalance / compensating
  policy decision / lifecycle test) from one CLI invocation.
- **Leading Indicators**:
  - OK2 (unknown-item fail-fast) — proves the
    `MigrateError::UnknownItem` arm of the underlying trait is
    correctly surfaced to the operator without store mutation.
  - OK3 (invalid-tier fail-fast) — proves the lower-case tier
    contract is enforced fail-fast on the parse side, mirroring
    the rendering-side convention at
    `crates/kaleidoscope-cli/src/lib.rs:389-395`.
- **Guardrail Metrics**:
  - OK4 (idempotent same-tier) — protects against accidentally
    introducing a special-case CLI guard that diverges from the
    underlying trait's idempotent behaviour.

## Cross-feature alignment

OK1 in this feature is the mutation-side counterpart of the read-
side surfaces shipped in the predecessor cluster: `ingest` writes
Hot Cinder items (`crates/kaleidoscope-cli/src/lib.rs:243-244`),
`stats` (extended in `cli-stats-cinder-tier-distribution-v0`) reads
the per-tier counts via
`TieringStore::list_by_tier(tenant, tier).len()`, and this feature
mutates a single item's tier via
`TieringStore::migrate(tenant, item, to_tier, migrated_at)`. The
three subcommands together cover the operator's full Cinder
lifecycle surface (write, read, mutate) without requiring a Rust
harness.

OK3 in this feature mirrors the fail-fast posture of the `--since`
/ `--until` parse error surface at
`crates/kaleidoscope-cli/src/main.rs:198-224` (the OK4 fail-fast
contract from `cli-stats-subcommand-v0`): on a bad parse, the CLI
exits non-zero with stderr naming BOTH the flag (here: the
positional `<to_tier>`) AND the verbatim bad value. The exact
wording is DESIGN's call (`wave-decisions.md` D-StderrWording).

| KPI | Cross-feature precedent | This feature |
|-----|-------------------------|--------------|
| OK1 | `cli-stats-cinder-tier-distribution-v0` OK1 reads `list_by_tier(tenant, tier).len()` and writes `<tier>=<count>` lines | `migrate` calls `TieringStore::migrate(tenant, item, to_tier, SystemTime::now())` and writes the one-line `migrated ... from=<from> to=<to>` report |
| OK2 | `cli-stats-subcommand-v0` OK4 fail-fast on bad ISO 8601 input (stderr names flag + value verbatim) | `migrate` fail-fast on unknown item id (stderr names the item id verbatim) |
| OK3 | `cli-stats-subcommand-v0` OK4 fail-fast on bad ISO 8601 input (stderr names flag + value verbatim) | `migrate` fail-fast on invalid tier argument (stderr names the invalid value verbatim) |
| OK4 | (n/a — no prior feature exercises an idempotent same-tier migrate via a CLI surface) | `migrate` faithfully reports `from=cold to=cold` for an idempotent same-tier call; no special case |

## Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| OK1-CLI-migrate-success | `crates/kaleidoscope-cli/tests/migrate_subcommand.rs` — happy-path scenario | `cargo test --package kaleidoscope-cli --test migrate_subcommand` exit code. The happy-path test pre-places item `acme/batch-00042` for tenant `acme` in tier Hot via a direct `FileBackedTieringStore::open(...).place(...)` call (NOT via `ingest()` which would also seed a Lumen record path the migrate subcommand has no reason to touch), then calls the migrate library function with `to_tier = "warm"` and a captured stdout sink. Asserts the captured stdout equals `migrated tenant=acme item=acme/batch-00042 from=hot to=warm\n`, exit code 0, captured stderr empty, AND `get_entry(acme, acme/batch-00042).unwrap().tier == Tier::Warm` after the call | At every commit touching the CLI migrate path or the Cinder `migrate`/`get_entry` methods | `kaleidoscope-cli` maintainer (CI feedback per ADR-0005) |
| OK2-CLI-migrate-unknown-item-fail-fast | Same test file — unknown-item scenario | Same `cargo test` invocation. The test opens a fresh `data_dir`, populates Cinder for tenant `acme` with a different item (e.g. `acme/batch-00001` in Hot — the seed item is there to prove the store opens cleanly, not as the target of the migrate call), then calls the migrate library function with `item_id = "acme/batch-00099"` (NOT placed). Asserts the call returns `Err`, captured stdout is empty, captured stderr contains the substring `acme/batch-00099`, AND the post-call `list_by_tier(acme, Hot/Warm/Cold).len()` triple is identical to the pre-call triple | Same | Same |
| OK3-CLI-migrate-invalid-tier-fail-fast | Same test file — invalid-tier scenario (two sub-scenarios: upper-case `HOT` and typo `lukewarm`) | Same `cargo test` invocation. Each sub-scenario seeds an item in Hot, then calls the migrate library function with an invalid tier argument. Asserts the call returns `Err`, captured stdout is empty, captured stderr contains the invalid value verbatim, AND the post-call `get_entry(acme, acme/batch-00042).unwrap().tier == Tier::Hot` (unchanged from the pre-call state — the parse error short-circuited the migrate call) | Same | Same |
| OK4-CLI-migrate-idempotent-same-tier | Same test file — idempotent-same-tier scenario | Same `cargo test` invocation. The test pre-places `acme/batch-00007` for tenant `acme` in tier Cold via a direct `FileBackedTieringStore::open(...).place(...)` call, then calls the migrate library function with `to_tier = "cold"` and a captured stdout sink. Asserts captured stdout equals `migrated tenant=acme item=acme/batch-00007 from=cold to=cold\n`, exit code 0, AND `get_entry(acme, acme/batch-00007).unwrap().tier == Tier::Cold` (unchanged). The supplementary oracle is the absence of any "AlreadyInTier" branch in the library function (DESIGN MUST NOT introduce one) | Same | Same |

## Hypothesis

We believe that **adding a new positional subcommand
`kaleidoscope-cli migrate <tenant_id> <data_dir> <item_id>
<to_tier>` (with `<to_tier>` accepted only in lower-case `hot` /
`warm` / `cold`) that opens the Cinder store under
`<data_dir>/cinder.*`, reads the current tier via
`cinder.get_entry(tenant, item)`, calls
`cinder.migrate(tenant, item, to_tier, SystemTime::now())`,
and writes the one-line stdout report `migrated tenant=<tenant>
item=<item_id> from=<from> to=<to>\n` on success — bubbling
`MigrateError::UnknownItem` and the invalid-tier parse error as
non-zero exit + stderr line containing the offending value** for
the **platform operator (Priya)** will achieve **a one-shot,
pipeable, grep-friendly CLI surface for manual tier migration,
unifying three operationally distinct workflows (rebalance,
compensate auto-tiering, lifecycle test) on the same invocation
shape and removing the operator's need to write Rust harnesses
for tier moves**.

We will know this is true when:

- The new acceptance test's happy-path scenario passes green,
  asserting that `migrate` against an `acme/batch-00042` placed
  in Hot, with `to_tier = warm`, produces stdout `migrated
  tenant=acme item=acme/batch-00042 from=hot to=warm\n` AND
  post-call `get_entry().tier == Tier::Warm` (OK1).
- The new acceptance test's unknown-item scenario passes green,
  asserting that `migrate` against an unplaced
  `acme/batch-00099` produces a non-zero exit, empty stdout,
  stderr containing `acme/batch-00099`, AND no mutation to the
  Cinder store (OK2).
- The new acceptance test's invalid-tier scenarios pass green,
  asserting that `migrate` with `to_tier_arg = "HOT"` or
  `to_tier_arg = "lukewarm"` each produces a non-zero exit,
  empty stdout, stderr containing the invalid value, AND no
  mutation to the Cinder store (OK3).
- The new acceptance test's idempotent-same-tier scenario
  passes green, asserting that `migrate(..., cold)` against an
  item already in Cold produces stdout `migrated tenant=acme
  item=acme/batch-00007 from=cold to=cold\n`, exit 0, and a
  post-call tier of Cold (OK4).
- The new acceptance test's tenant-isolation scenario passes
  green, asserting that `migrate(acme, ...)` does not mutate
  `globex`'s same-named item.
- The EXISTING locked acceptance test files
  (`tests/stats_subcommand.rs`,
  `tests/stats_cinder_tier_distribution.rs`,
  `tests/observe_otlp_*.rs`) continue to pass green
  UNMODIFIED under `cargo test --package kaleidoscope-cli`.
- The dogfood demo runs: `cargo run --bin kaleidoscope-cli --
  ingest acme /tmp/kdata < some_records.ndjson` places a Hot
  Cinder item per batch; `cargo run --bin kaleidoscope-cli --
  migrate acme /tmp/kdata acme/batch-00000 cold` returns
  `migrated tenant=acme item=acme/batch-00000 from=hot
  to=cold\n` on stdout, exit 0; the immediately-following
  `cargo run --bin kaleidoscope-cli -- stats acme /tmp/kdata`
  shows the new tier distribution reflecting the move (one
  fewer in Hot, one more in Cold).

## Handoff to DESIGN

The DESIGN wave (`nw-solution-architect`) should preserve:

1. **The quiescent recorder pattern**: the function constructs a
   `cinder::NoopRecorder` for the
   `FileBackedTieringStore::open(cinder_base(data_dir), recorder)`
   call identically to the way `ingest()` does in its no-flag
   arm (`crates/kaleidoscope-cli/src/lib.rs:170-174`); no OTLP
   file is created and no `--observe-otlp` flag is accepted in
   v0 (`wave-decisions.md` D-OutOfScope-Observe).
2. **The `TieringStore::get_entry` + `TieringStore::migrate`
   call shape**: exactly one `get_entry` call per `migrate`
   invocation (to read the `from` tier) and exactly one
   `migrate` call (to perform the mutation). No `evaluate_at`
   call, no `place` call, no `list_by_tier` call.
3. **The stdout output contract**: one literal line on success,
   `migrated tenant=<tenant> item=<item_id> from=<from> to=<to>\n`,
   where `<from>` and `<to>` render via the same lower-case
   mapping (`hot` / `warm` / `cold`) as the predecessor
   feature's `tier_lowercase` helper at
   `crates/kaleidoscope-cli/src/lib.rs:389-395`. No header, no
   JSON, no CSV, no colour codes.
4. **The fail-fast contract on UnknownItem**: bubble
   `MigrateError::UnknownItem` to a non-zero exit + stderr line
   containing the verbatim item id. No silent insert (no
   `place` call). No store mutation.
5. **The fail-fast contract on invalid tier argument**: parse
   the `<to_tier>` argument BEFORE issuing the `migrate` call,
   so an invalid value short-circuits without opening the
   Cinder store (or at most without calling `migrate`). Stderr
   carries the verbatim invalid value.
6. **The no-Lumen-touch contract**: the function MUST NOT open
   `FileBackedLogStore::open(lumen_base(data_dir), ...)`. The
   Lumen WAL+snapshot is byte-equivalent before and after the
   call.
7. **The no-special-case-for-same-tier contract**: the
   underlying `TieringStore::migrate` API is idempotent per
   `crates/cinder/src/store.rs:167-188`. The CLI MUST NOT
   introduce a special case (e.g. an "AlreadyInTier" branch
   that short-circuits the migrate call) for the same-tier
   case. The stdout report faithfully shows `from=X to=X` and
   exits 0.

The DESIGN wave should NOT introduce flags (`--observe-otlp`,
`--dry-run`, `--at`, `--format=...`), bulk migration
(multi-item single call), policy preview (dry-run of
`evaluate_at`), structured output formats (JSON, CSV), or any
multi-tenant aggregation.

## DEVOPS instrumentation needs

No new collection infrastructure. The `migrate` subcommand is a
narrow mutation over the existing Cinder WAL+snapshot and emits no
OTLP, no metrics, no logs of its own (the `NoopRecorder` on the
Cinder side is intentionally quiescent). The CI gate is the new
acceptance test's exit code PLUS the unmodified locked test files'
continued green status, per ADR-0005 Gate 1 (the workspace already
runs `cargo test` on every commit).
