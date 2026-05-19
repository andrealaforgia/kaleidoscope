# Outcome KPIs — `cli-get-tier-subcommand-v0`

## Feature

`cli-get-tier-subcommand-v0` — add a new positional subcommand to
the existing CLI binary:
`kaleidoscope-cli get-tier <tenant> <data_dir> <item_id>`. The
subcommand opens the Cinder store under `<data_dir>/cinder.*`,
calls `cinder::TieringStore::get_tier(&tenant, &ItemId::new(item_id))`
(`crates/cinder/src/store.rs:85`), and writes a one-line report to
stdout: `tier=<lowercase>\n` (where `<lowercase>` is `hot` /
`warm` / `cold`). On `get_tier(...) -> None`, exits non-zero with
a stderr line containing the substring `unknown item` plus the
verbatim item id plus the verbatim tenant. v0 introduces no flag,
no JSON output, no bulk lookup, no `get-entry` (placed_at /
migrated_at) shape, no `--observe-otlp` on this subcommand
(`wave-decisions.md` D-OutOfScope-Bulk, D-OutOfScope-Json,
D-OutOfScope-FullEntry, D-OutOfScope-Observe).

## Objective

A single
`kaleidoscope-cli get-tier acme /tmp/data acme/batch-00042`
invocation answers the operator's narrowest tier-decision question
("what tier is this item in?") in one subprocess, one Cinder open,
one O(1) `get_tier` call — replacing the three-subprocess
`list-items hot/warm/cold | grep` chain operators use today. The
result is grep-friendly and pipeable into scripted assertions in
CI / runbooks. Three operationally distinct uses are unified on
the same CLI invocation shape: pre-flight check before manual
`migrate`, scripted assertion in a deployment pipeline, and
incident-time audit on a single item id mentioned in an alert.

## Note on KPI granularity

This feature is operator-visible at the CLI surface. The principal
KPI is OK1 (get-tier-success correctness — the stdout report
content equals `tier=<lowercase>` for the tier returned by
`TieringStore::get_tier(tenant, &item)`). The second KPI is OK2
(unknown-item fail-fast — exit non-zero, stderr names the missing
item AND the token `unknown item` AND the tenant, store
unchanged). The third KPI is OK3 (tenant-isolation — same ItemId
string under different tenants returns the respective per-tenant
tier, faithful to the underlying `(TenantId, ItemId)` placement
key invariant per `crates/cinder/src/store.rs:71-72`).

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| OK1-CLI-get-tier-success | Priya the platform operator, observed at the stdout byte level | Sees, on a single CLI invocation of `kaleidoscope-cli get-tier <tenant> <data_dir> <item_id>` (with the item placed under the tenant in Cinder), the EXACT stdout line `tier=<lowercase>\n` where `<lowercase>` is the lower-case rendering of the tier returned by `cinder.get_tier(tenant, item)`. Exit code 0. Stderr empty. The Cinder store under `<data_dir>` is byte-equivalent before and after the call (read-only). The Lumen store under `<data_dir>/lumen.*` is byte-equivalent before and after the call (never opened — `wave-decisions.md` D-NoLumenTouch). | 100% of `get-tier` invocations against placed items produce the exact stdout report line for the tier returned by `cinder.get_tier(tenant, item)`; 0% of such invocations produce a stdout line whose lower-case tier value disagrees with what `get_tier(tenant, item)` returns at that moment | 0% (no CLI surface for single-item tier query exists today; the operator's only path is the three-subprocess `list-items hot/warm/cold | grep <item>` chain with ambiguous exit-code semantics) | New acceptance test `crates/kaleidoscope-cli/tests/get_tier_subcommand.rs` — three happy-path scenarios (one per tier) seed an item placed in the respective tier and assert the captured stdout from a `get_tier(...)` call equals `tier=hot\n` / `tier=warm\n` / `tier=cold\n` respectively, with exit code 0 and empty stderr | Leading (operator-visible behaviour; principal KPI for this feature) |
| OK2-CLI-get-tier-unknown-item-fail-fast | Priya the platform operator, observed at the stdout / stderr byte level + exit code | Sees, when the `<item_id>` argument names an item that was never `place()`d under the `<tenant>` argument in the Cinder store at `<data_dir>/cinder.*`, exit code non-zero, empty stdout, AND a single stderr line that contains the substrings `unknown item`, the verbatim item id she typed, AND the verbatim tenant string she typed. The Cinder store under `<data_dir>` is byte-equivalent before and after the call (no mutation possible — `get_tier` is read-only). | 100% of `get-tier` invocations against unplaced item ids produce a non-zero exit code, an empty stdout, AND a stderr line containing the substrings `unknown item` + verbatim item id + verbatim tenant; 100% of such invocations leave the Cinder store under `<data_dir>` byte-equivalent before and after; 0% of such invocations silently fall back to a default tier or emit an ambiguous diagnostic | 0% (today the three-subprocess `list-items` + grep chain returns `grep`'s no-match exit code on the LAST tier scanned, conflating "no Cold match" with "item nowhere"; there is no canonical fail-fast stderr line) | Same new test file — unknown-item scenario asserts the call returns `Err`, captured stdout is empty, captured stderr contains the substring `unknown item`, contains the substring `acme/batch-00099`, AND contains the substring `acme`. The post-call `get_tier(acme, acme/batch-00099)` STILL returns `None` (faithful read-only) | Leading (operator-facing fail-fast guarantee; mirrors the established `migrate`'s `UnknownItem` stderr posture per `crates/cinder/src/store.rs:55-58`) |
| OK3-CLI-get-tier-tenant-isolation | Priya the platform operator, observed at the stdout byte level | Sees, when the same `ItemId` string is placed under two different tenants in the same `<data_dir>` (e.g. `acme/batch-00042` in Hot for `acme` AND `acme/batch-00042` in Warm for `globex`), that `get-tier acme ... acme/batch-00042` returns `tier=hot\n` and `get-tier globex ... acme/batch-00042` returns `tier=warm\n` — the two reads return DIFFERENT tiers for the same `ItemId` string because the placement key in `cinder::InMemoryTieringStore` is `(TenantId, ItemId)` per `crates/cinder/src/store.rs:119`, inheriting `TieringStore`'s per-tenant isolation invariant. | 100% of `get-tier` invocations under tenant `T` return the tier `T` placed for that `ItemId` and never `T'`'s tier for the same `ItemId` string; 0% of invocations leak cross-tenant placement information into stdout or stderr | n/a (the `(TenantId, ItemId)` key invariant is established in the underlying `cinder::TieringStore` trait; no prior CLI feature exercises a same-`ItemId`-cross-tenant query) | Same new test file — tenant-isolation scenario places `acme/batch-00042` in Hot for `acme` AND in Warm for `globex`, runs `get_tier(acme, ..., acme/batch-00042)` and asserts stdout `tier=hot\n`, then runs `get_tier(globex, ..., acme/batch-00042)` and asserts stdout `tier=warm\n` | Guardrail (operator-facing faithfulness to the underlying trait's per-tenant key invariant; protects against accidentally introducing a cross-tenant read leak) |

## Metric Hierarchy

- **North Star**: **OK1-CLI-get-tier-success** — the
  get-tier-success correctness KPI. Without it, the subcommand
  cannot do its job. With it alone, the operator already has the
  three operationally distinct uses enabled (pre-flight before
  manual migrate / scripted pipeline assertion / incident-time
  single-item audit) from one CLI invocation.
- **Leading Indicator**:
  - OK2 (unknown-item fail-fast) — proves the `None` arm of the
    underlying `get_tier` API is correctly surfaced to the
    operator with the canonical "unknown item" stderr language
    inherited from `migrate`.
- **Guardrail Metric**:
  - OK3 (tenant-isolation) — protects against accidentally
    introducing a cross-tenant read leak that diverges from the
    underlying trait's per-tenant key invariant.

## Cross-feature alignment

OK1 in this feature is the read-side counterpart of the mutation-
side surface shipped in `cli-migrate-subcommand-v0`: `migrate`
calls `TieringStore::migrate(tenant, item, to_tier, migrated_at)`
and writes `migrated tenant=... item=... from=... to=...` on
stdout; this feature calls `TieringStore::get_tier(tenant, item)`
and writes `tier=...` on stdout. Both subcommands share the same
positional argument prefix (`<tenant> <data_dir> <item_id>`),
the same lower-case tier rendering, the same quiescent Cinder
recorder, the same no-Lumen-touch posture, and the same
fail-fast-on-unknown-item stderr language (mirroring
`MigrateError::UnknownItem` per `crates/cinder/src/store.rs:55-58`).

OK2 in this feature mirrors the fail-fast posture of
`cli-migrate-subcommand-v0` OK2: on `UnknownItem`, exit non-zero
with stderr naming the missing item AND the canonical "unknown
item" token. The exact wording is DESIGN's call
(`wave-decisions.md` D-StderrWording).

| KPI | Cross-feature precedent | This feature |
|-----|-------------------------|--------------|
| OK1 | `cli-migrate-subcommand-v0` OK1 calls `TieringStore::migrate(...)` and writes `migrated ... from=<from> to=<to>` | `get-tier` calls `TieringStore::get_tier(tenant, item)` and writes `tier=<lowercase>` |
| OK2 | `cli-migrate-subcommand-v0` OK2 fail-fast on unknown item id (stderr contains `unknown item` token plus item id) | `get-tier` fail-fast on unknown item id (stderr contains `unknown item` token plus item id plus tenant) |
| OK3 | `cli-migrate-subcommand-v0` tenant-isolation scenario asserts mutation to `acme`'s item does not perturb `globex`'s same-named item | `get-tier` tenant-isolation scenario asserts read for `acme` and read for `globex` return different per-tenant tiers for the same `ItemId` string |

## Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| OK1-CLI-get-tier-success | `crates/kaleidoscope-cli/tests/get_tier_subcommand.rs` — three happy-path scenarios (one per tier) | `cargo test --package kaleidoscope-cli --test get_tier_subcommand` exit code. Each happy-path test pre-places an item for tenant `acme` in the target tier via a direct `FileBackedTieringStore::open(...).place(...)` call (NOT via `ingest()`), then calls the get-tier library function with a captured stdout sink. Asserts the captured stdout equals `tier=<lowercase>\n` for the seeded tier, exit code 0, captured stderr empty | At every commit touching the CLI get-tier path or the Cinder `get_tier` method | `kaleidoscope-cli` maintainer (CI feedback per ADR-0005) |
| OK2-CLI-get-tier-unknown-item-fail-fast | Same test file — unknown-item scenario | Same `cargo test` invocation. The test opens a fresh `data_dir`, populates Cinder for tenant `acme` with at least one OTHER item (e.g. `acme/batch-00001` in Hot — the seed item proves the store opens cleanly), then calls the get-tier library function with `item_id = "acme/batch-00099"` (NOT placed). Asserts the call returns `Err`, captured stdout is empty, captured stderr contains the substrings `unknown item`, `acme/batch-00099`, and `acme` | Same | Same |
| OK3-CLI-get-tier-tenant-isolation | Same test file — tenant-isolation scenario | Same `cargo test` invocation. The test pre-places `acme/batch-00042` for tenant `acme` in tier Hot AND, separately, pre-places `acme/batch-00042` for tenant `globex` in tier Warm in the SAME `data_dir`. Calls the get-tier library function twice — once with `(acme, ..., acme/batch-00042)` and once with `(globex, ..., acme/batch-00042)` — and asserts captured stdout equals `tier=hot\n` for the first call and `tier=warm\n` for the second | Same | Same |

## Hypothesis

We believe that **adding a new positional subcommand
`kaleidoscope-cli get-tier <tenant> <data_dir> <item_id>` that
opens the Cinder store under `<data_dir>/cinder.*`, calls
`cinder.get_tier(tenant, item)`, and writes the one-line stdout
report `tier=<lowercase>\n` on success — bubbling the `None`
case as non-zero exit + stderr line containing the substrings
`unknown item`, the verbatim item id, and the verbatim tenant**
for the **platform operator (Priya)** will achieve **a one-shot,
pipeable, grep-friendly CLI surface for single-item tier query,
unifying three operationally distinct uses (pre-flight before
manual migrate, scripted assertion in a pipeline, incident-time
single-item audit) on the same invocation shape and removing the
operator's need for the three-subprocess `list-items` + grep
chain**.

We will know this is true when:

- The new acceptance test's three happy-path scenarios pass green,
  asserting that `get-tier` against an item placed in tier `T`
  produces stdout `tier=<lowercase(T)>\n` and exit 0 for each of
  `T ∈ {Hot, Warm, Cold}` (OK1).
- The new acceptance test's unknown-item scenario passes green,
  asserting that `get-tier` against an unplaced `acme/batch-00099`
  produces a non-zero exit, empty stdout, AND stderr containing
  the substrings `unknown item`, `acme/batch-00099`, and `acme`
  (OK2).
- The new acceptance test's tenant-isolation scenario passes
  green, asserting that `get-tier(acme, ...)` and `get-tier(globex,
  ...)` for the same `ItemId` string return different per-tenant
  tiers when the two tenants placed the item in different tiers
  (OK3).
- The EXISTING locked acceptance test files
  (`tests/stats_subcommand.rs`,
  `tests/stats_cinder_tier_distribution.rs`,
  `tests/list_items_subcommand.rs`,
  `tests/migrate_subcommand.rs`,
  `tests/place_subcommand.rs`,
  `tests/observe_otlp_*.rs`) continue to pass green UNMODIFIED.
- The dogfood demo runs: `cargo run --bin kaleidoscope-cli --
  ingest acme /tmp/kdata < some_records.ndjson` places a Hot
  Cinder item per batch; `cargo run --bin kaleidoscope-cli --
  get-tier acme /tmp/kdata acme/batch-00000` returns `tier=hot`
  on stdout, exit 0; after `cargo run --bin kaleidoscope-cli --
  migrate acme /tmp/kdata acme/batch-00000 cold`, re-running
  `get-tier acme /tmp/kdata acme/batch-00000` returns
  `tier=cold`, exit 0.

## Handoff to DESIGN

The DESIGN wave (`nw-solution-architect`) should preserve:

1. **The quiescent recorder pattern**: the function constructs a
   `cinder::CinderRecorder` for the
   `FileBackedTieringStore::open(cinder_base(data_dir), recorder)`
   call identically to `list_items` at
   `crates/kaleidoscope-cli/src/lib.rs:534`; no OTLP file is
   created and no `--observe-otlp` flag is accepted in v0
   (`wave-decisions.md` D-OutOfScope-Observe / D-ReadOnly).
2. **The `TieringStore::get_tier` call shape**: exactly one
   `get_tier(tenant, &item)` call per CLI invocation. No
   `get_entry`, no `list_by_tier`, no `evaluate_at`, no
   `migrate`, no `place`.
3. **The stdout output contract**: one literal line on success,
   `tier=<lowercase>\n`, where `<lowercase>` renders via the same
   `tier_lowercase` mapping as
   `crates/kaleidoscope-cli/src/lib.rs:564-570`. No header, no
   JSON, no CSV, no colour codes.
4. **The fail-fast contract on None**: bubble the None case to a
   non-zero exit + stderr line containing the substrings
   `unknown item`, the verbatim item id, and the verbatim tenant.
   Mirror `MigrateError::UnknownItem`'s `Display` impl at
   `crates/cinder/src/store.rs:55-58`.
5. **The no-Lumen-touch contract**: the function MUST NOT open
   `FileBackedLogStore::open(lumen_base(data_dir), ...)`. The
   Lumen WAL+snapshot is byte-equivalent before and after the
   call.
6. **The read-only contract**: the function MUST NOT call any
   mutating Cinder API (`place`, `migrate`, `evaluate_at`). The
   Cinder WAL+snapshot is byte-equivalent before and after the
   call (the `get_tier` impl at
   `crates/cinder/src/store.rs:154-160` is read-only by
   construction).
7. **The tenant-isolation contract**: the `tenant` argument is
   passed to `get_tier` verbatim and the placement key is the
   `(TenantId, ItemId)` pair per `crates/cinder/src/store.rs:119`.
   No cross-tenant read fallback.

The DESIGN wave should NOT introduce flags (`--observe-otlp`,
`--json`, `--format=...`), bulk lookup (multi-item single call),
a full `get-entry` shape (placed_at, migrated_at), or any
multi-tenant aggregation.

## DEVOPS instrumentation needs

No new collection infrastructure. The `get-tier` subcommand is a
narrow read over the existing Cinder WAL+snapshot and emits no
OTLP, no metrics, no logs of its own (the `CinderRecorder` on the
Cinder side is intentionally quiescent and `get_tier` doesn't
invoke any recorder method anyway — see
`crates/cinder/src/store.rs:154-160`). The CI gate is the new
acceptance test's exit code PLUS the unmodified locked test files'
continued green status, per ADR-0005 Gate 1.
