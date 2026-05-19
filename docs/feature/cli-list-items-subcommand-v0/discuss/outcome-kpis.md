# Outcome KPIs — `cli-list-items-subcommand-v0`

## Feature

`cli-list-items-subcommand-v0` — add a new positional subcommand
to the existing CLI binary:
`kaleidoscope-cli list-items <tenant_id> <data_dir> <tier>`.

The subcommand opens the Cinder store under `<data_dir>/cinder.*`
read-only, calls
`cinder::TieringStore::list_by_tier(&tenant, tier)`
(`crates/cinder/src/store.rs:102`, the same method already used
by `stats_with_tiers` at
`crates/kaleidoscope-cli/src/lib.rs:383` for the per-tier
counts), sorts the returned `Vec<ItemId>` lexicographically, and
writes one item id per line to stdout. On an invalid `<tier>`
argument (any spelling other than literal lower-case `hot` /
`warm` / `cold`), exits non-zero with a stderr line naming the
invalid value. v0 introduces no new flag, no JSON output, no
pagination, no cross-tenant aggregate, no `--observe-otlp`
(`list_by_tier` is a pure read with nothing operator-visible to
record — Cinder's `MetricsRecorder` trait has no `record_list`
method per `crates/cinder/src/metrics.rs`)
(`wave-decisions.md` D-OutOfScope-*).

## Objective

A single
`kaleidoscope-cli list-items acme /tmp/data cold` invocation
enumerates every Cinder item that tenant `acme` currently has in
the Cold tier, one item id per line on stdout, sorted
lexicographically. This is the natural follow-on to `stats`: when
`stats` shows `cold=47`, this subcommand answers the next
operator question, "which 47 items?". Three operationally
distinct decisions all reduce to this primitive: (a) "which cold
items should I manually migrate back to warm?" (manual
rebalancing follow-up), (b) "are these the items I'm worried
about?" (sanity check against tenant manifest), (c) "pipe each
cold item through `migrate` to walk it back to warm" (scripted
pipelines: `... list-items acme /tmp/data cold | xargs -I {}
kaleidoscope-cli migrate acme /tmp/data {} warm`). Until this
feature shipped, those three decisions required writing a Rust
harness against `cinder::FileBackedTieringStore` or grovelling
through the Cinder snapshot file format. v0 collapses all three
to one CLI invocation.

## Note on KPI granularity

This feature is operator-visible at the CLI surface. The principal
KPI is OK1 (list-items correctness — the stdout contents are
exactly the set of item ids returned by
`cinder.list_by_tier(tenant, tier)`, one per line, in
lexicographic order). The second KPI is OK2 (tenant isolation —
items from other tenants never surface in the output, inherited
from the per-tenant key in `cinder::TieringStore`). The third KPI
is OK3 (invalid-tier fail-fast — exit non-zero, stderr names the
invalid value, store unopened). All three are direct mirrors of
the corresponding `migrate` subcommand KPIs (the predecessor)
applied to a read-only enumeration instead of a single-item
mutation.

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| OK1-CLI-list-items-correctness | Priya the platform operator, observed at the stdout byte level | Sees, on a single CLI invocation of `kaleidoscope-cli list-items <tenant> <data_dir> <tier>` (with `<tier>` in {`hot`, `warm`, `cold`}), N stdout lines (where N equals `cinder.list_by_tier(tenant, tier).len()` at call time), one item id per line, in lexicographic byte-order. When N == 0 (the tenant has zero items in the queried tier), stdout is empty (no header, no placeholder line). Exit code 0. Stderr is empty (modulo an optional `list-items ok: items=N` summary line at DESIGN's discretion per `wave-decisions.md` D-StderrSummary). Two successive invocations with the same `(tenant, data_dir, tier)` tuple produce byte-identical stdout, demonstrating the lex-sort at the CLI boundary masks the `HashMap` iteration order randomisation in `cinder::InMemoryTieringStore::list_by_tier` at `crates/cinder/src/store.rs:190-198`. | 100% of `list-items` invocations against valid lower-case tier arguments produce stdout whose lines are EXACTLY the set returned by `cinder.list_by_tier(tenant, tier)` (multiset equality between the stdout line set and the `Vec<ItemId>` returned), sorted lexicographically; 100% of invocations produce byte-identical stdout under repetition; 0% of invocations include items that were NOT returned by `list_by_tier`; 0% of invocations omit items that WERE returned by `list_by_tier` | 0% (no CLI surface for tier enumeration exists today; the operator's only path is a Rust harness that opens `cinder::FileBackedTieringStore` and calls `list_by_tier(&tenant, tier)` on the result, then iterates the returned `Vec<ItemId>`) | New acceptance test `crates/kaleidoscope-cli/tests/list_items_subcommand.rs` — happy-path scenario pre-places three items in Cold for tenant `acme` (in non-lex insertion order: `acme/batch-00099` first, then `acme/batch-00007`, then `acme/batch-00041`) plus a decoy Hot item that MUST NOT appear in the cold list, then calls the list-items library function with `tier_arg = "cold"` and a captured stdout sink, asserts the captured stdout EQUALS the bytes `acme/batch-00007\nacme/batch-00041\nacme/batch-00099\n`. A second sub-scenario calls the function TWICE in succession with the same arguments and asserts both captured stdouts are byte-identical (determinism). A third sub-scenario calls with `tier_arg = "warm"` against a tenant that has zero warm items and asserts the captured stdout is empty (N=0 case) | Leading (operator-visible behaviour; principal KPI for this feature) |
| OK2-CLI-list-items-tenant-isolation | Priya the platform operator, observed at the stdout byte level + cross-tenant Cinder state | Sees, when tenants `acme` and `globex` both have an item id `shared/batch-00042` placed in the same `<data_dir>` (`acme/shared/batch-00042` in Cold, `globex/shared/batch-00042` in Cold), that invoking `list-items acme /tmp/data cold` produces stdout containing `shared/batch-00042` (and any other cold items for `acme`, sorted lex) but NOT a second `shared/batch-00042` line for `globex`'s entry. The post-call `list_by_tier(globex, Tier::Cold)` STILL returns a `Vec` containing `shared/batch-00042` (unchanged from the pre-call state, demonstrating no mutation and no cross-tenant leakage). | 100% of `list-items` invocations against tenant `T` produce stdout whose lines correspond strictly to entries `(T, *)` in the Cinder store; 0% of invocations surface any entry `(T', *)` where `T' != T`; 100% of invocations leave the cross-tenant Cinder state byte-equivalent before and after | 0% (no CLI surface exists; the underlying `TieringStore::list_by_tier(tenant, tier)` filter at `crates/cinder/src/store.rs:194-196` is the source of the per-tenant invariant; this feature inherits it by direct delegation) | Same new test file — tenant-isolation scenario pre-places `shared/batch-00042` in Cold for both `acme` and `globex`, calls the list-items library function with `(acme, data_dir, "cold")`, asserts captured stdout contains exactly one line `shared/batch-00042\n` (only `acme`'s entry), AND asserts a follow-up `cinder.list_by_tier(globex, Tier::Cold)` call returns a `Vec` whose contents are identical to the pre-call state | Leading (operator-facing safety guarantee; protects against accidentally surfacing another tenant's data through a per-tenant query path) |
| OK3-CLI-list-items-invalid-tier-fail-fast | Priya the platform operator, observed at the stdout / stderr byte level + exit code | Sees, when the `<tier>` argument is any spelling other than exactly `hot` / `warm` / `cold` (upper-case `COLD`, mixed-case `Hot`, typo `lukewarm`, empty string, leading/trailing whitespace), exit code non-zero, empty stdout, AND a single stderr line that contains the exact invalid value she typed. The Cinder store under `<data_dir>` is byte-equivalent before and after the call (the tier-argument parse error fires BEFORE `FileBackedTieringStore::open` is called — the parse precedes the open identically to the `migrate` subcommand's parse-then-open ordering at `crates/kaleidoscope-cli/src/lib.rs:432-446`). | 100% of `list-items` invocations with a non-`hot`/`warm`/`cold` tier argument produce a non-zero exit code, an empty stdout, AND a stderr line containing the invalid value verbatim; 100% of such invocations leave the Cinder store under `<data_dir>` byte-equivalent before and after; 0% of such invocations dispatch to the underlying `TieringStore::list_by_tier` API (the parse error short-circuits before the store is opened) | 0% (today the lower-case tier convention is enforced by the predecessor `migrate` subcommand on its parse side at `crates/kaleidoscope-cli/src/lib.rs:432-434, :475-482` but no equivalent contract exists for `list-items` because `list-items` does not exist) | Same new test file — invalid-tier scenario asserts the call with `tier_arg = "COLD"` returns `Err`, captured stdout is empty, captured stderr contains the substring `COLD`, and a follow-up `cinder.list_by_tier(acme, Tier::Hot).len()` matches the pre-call count (no mutation). A second sub-scenario uses `tier_arg = "lukewarm"` (a typo) and asserts the stderr contains `lukewarm` | Leading (operator-facing fail-fast guarantee; mirrors the established lower-case tier convention enforced by `migrate`) |

## Metric Hierarchy

- **North Star**: **OK1-CLI-list-items-correctness** — the
  correctness KPI. Without it, the subcommand cannot do its job.
  With it alone, the operator already has the three operationally
  distinct decisions enabled (manual rebalancing follow-up /
  sanity check / scripted pipeline) from one CLI invocation.
- **Leading Indicators**:
  - OK2 (tenant isolation) — proves the per-tenant key filter
    at `crates/cinder/src/store.rs:194-196` is correctly
    surfaced to the CLI without cross-tenant leakage.
  - OK3 (invalid-tier fail-fast) — proves the lower-case tier
    contract is enforced fail-fast on the parse side, mirroring
    the established convention from `migrate`.
- **Guardrail Metrics**:
  - Read-only invariant: the Cinder store is byte-equivalent
    before and after every invocation (success and failure
    paths). Asserted alongside each KPI's primary check in the
    new test file. No special line in this table because it is
    a property of every scenario, not a discrete KPI.

## Cross-feature alignment

OK1 in this feature is the item-id-granularity counterpart of
the count-granularity surface already shipped in
`cli-stats-cinder-tier-distribution-v0`. That feature consumes
`list_by_tier(tenant, tier).len()` to emit `hot=N` / `warm=N` /
`cold=N` lines on stdout; this feature consumes the SAME method
to emit one line per item id. The two together form the
operator's complete Cinder read surface at the CLI: counts (for
"how many?") and ids (for "which ones?").

OK2 in this feature inherits the tenant-isolation invariant from
the per-tenant key in `cinder::TieringStore`
(`crates/cinder/src/store.rs:71-72, :119`) and from the explicit
filter inside `list_by_tier` at
`crates/cinder/src/store.rs:194-196`. The seven predecessor
features in the cluster all preserve this invariant; this
feature preserves it at the read-side enumeration surface.

OK3 in this feature mirrors the fail-fast posture of the
`migrate` subcommand's invalid-tier handling
(`crates/kaleidoscope-cli/src/lib.rs:432-434`): on a bad parse,
the CLI exits non-zero with stderr naming the verbatim bad
value. The `Error::InvalidTier { value: String }` variant
already exists (`lib.rs:79-81`); no new variant introduced.

| KPI | Cross-feature precedent | This feature |
|-----|-------------------------|--------------|
| OK1 | `cli-stats-cinder-tier-distribution-v0` OK1 reads `list_by_tier(tenant, tier).len()` and writes count lines | `list-items` reads `list_by_tier(tenant, tier)` and writes one line per item id, sorted lex |
| OK2 | All seven predecessor features inherit tenant isolation; the `migrate` subcommand explicitly tests it | `list-items` inherits the same invariant via direct delegation to the per-tenant `list_by_tier` filter |
| OK3 | `cli-migrate-subcommand-v0` OK3 fail-fast on invalid tier (stderr names value verbatim) via `Error::InvalidTier` | `list-items` reuses the SAME `Error::InvalidTier` variant and the SAME `parse_tier` shape (or direct reuse) |

## Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| OK1-CLI-list-items-correctness | `crates/kaleidoscope-cli/tests/list_items_subcommand.rs` — happy-path and determinism scenarios | `cargo test --package kaleidoscope-cli --test list_items_subcommand` exit code. The happy-path test pre-places three items in Cold for tenant `acme` (`acme/batch-00099`, `acme/batch-00007`, `acme/batch-00041` — intentionally NOT in lex insertion order) plus a decoy Hot item (`acme/batch-00050`) for the same tenant via direct `FileBackedTieringStore::open(...).place(...)` calls, then calls the list-items library function with `tier_arg = "cold"` and a captured stdout sink. Asserts the captured stdout EQUALS the bytes `acme/batch-00007\nacme/batch-00041\nacme/batch-00099\n` (lex-sorted, decoy excluded). The determinism sub-scenario calls the function TWICE in succession and asserts both captured stdouts are byte-identical. The empty-result sub-scenario calls with `tier_arg = "warm"` against a tenant whose Warm tier has zero entries and asserts captured stdout is empty (zero bytes) | At every commit touching the CLI list-items path or the Cinder `list_by_tier` method | `kaleidoscope-cli` maintainer (CI feedback per ADR-0005) |
| OK2-CLI-list-items-tenant-isolation | Same test file — tenant-isolation scenario | Same `cargo test` invocation. The test pre-places `shared/batch-00042` in Cold for BOTH `acme` and `globex` in the same `data_dir` via two direct `place(...)` calls, then calls the list-items library function with `(acme, data_dir, "cold")` and a captured stdout sink. Asserts captured stdout contains exactly one line `shared/batch-00042\n`, AND a follow-up `cinder.list_by_tier(globex, Tier::Cold)` returns a `Vec` whose contents (after lex sort) equal the pre-call state | Same | Same |
| OK3-CLI-list-items-invalid-tier-fail-fast | Same test file — invalid-tier scenario (two sub-scenarios: upper-case `COLD` and typo `lukewarm`) | Same `cargo test` invocation. Each sub-scenario seeds at least one item in Hot (so the Cinder store has content but the parse error should short-circuit before opening), then calls the list-items library function with an invalid tier argument. Asserts the call returns `Err`, captured stdout is empty, captured stderr contains the invalid value verbatim, AND the post-call `cinder.list_by_tier(acme, Tier::Hot).len()` is unchanged from the pre-call snapshot | Same | Same |

## Hypothesis

We believe that **adding a new positional subcommand
`kaleidoscope-cli list-items <tenant_id> <data_dir> <tier>`
(with `<tier>` accepted only in lower-case `hot` / `warm` /
`cold`) that opens the Cinder store under `<data_dir>/cinder.*`
read-only, calls
`cinder.list_by_tier(tenant, tier)`, sorts the returned
`Vec<ItemId>` lexicographically, and writes one item id per
line to stdout — bubbling the invalid-tier parse error as
non-zero exit + stderr line containing the offending value**
for the **platform operator (Priya)** will achieve **a
one-shot, pipeable, grep-friendly CLI surface for tier
enumeration that complements the existing `stats` subcommand
(counts) and the existing `migrate` subcommand (single-item
mutation), giving the operator the FULL Cinder read+mutate
lifecycle at the item-id granularity from the CLI without
writing Rust**.

We will know this is true when:

- The new acceptance test's happy-path scenario passes green,
  asserting that `list-items` against three pre-placed cold
  items for `acme` (placed in non-lex insertion order) produces
  stdout EQUAL to the lex-sorted byte sequence
  `acme/batch-00007\nacme/batch-00041\nacme/batch-00099\n`
  (OK1).
- The new acceptance test's determinism sub-scenario passes
  green, asserting that two successive `list-items` invocations
  with the same `(tenant, data_dir, tier)` tuple produce
  byte-identical stdout (OK1 determinism property).
- The new acceptance test's empty-result sub-scenario passes
  green, asserting that `list-items` for a tier with zero items
  produces empty stdout, exit 0 (OK1 N=0 case).
- The new acceptance test's tenant-isolation scenario passes
  green, asserting that `list-items acme` does not surface
  `globex`'s same-named items, and the post-call
  `list_by_tier(globex, ...)` is unchanged (OK2).
- The new acceptance test's invalid-tier scenarios pass green,
  asserting that `list-items` with `tier_arg = "COLD"` or
  `tier_arg = "lukewarm"` each produces a non-zero exit, empty
  stdout, stderr containing the invalid value, AND no mutation
  to the Cinder store (OK3).
- The EXISTING locked acceptance test files
  (`tests/stats_subcommand.rs`,
  `tests/stats_cinder_tier_distribution.rs`,
  `tests/migrate_subcommand.rs`, `tests/observe_otlp_*.rs`)
  continue to pass green UNMODIFIED under
  `cargo test --package kaleidoscope-cli`.
- The dogfood demo runs:
  `cargo run --bin kaleidoscope-cli -- ingest acme /tmp/kdata
  < some_records.ndjson` places Hot Cinder items;
  `cargo run --bin kaleidoscope-cli -- migrate acme /tmp/kdata
  acme/batch-00000 cold` moves one item to Cold;
  `cargo run --bin kaleidoscope-cli -- list-items acme
  /tmp/kdata cold` produces a single stdout line `acme/batch-00000`,
  exit 0; the scripted-pipeline form
  `cargo run --bin kaleidoscope-cli -- list-items acme
  /tmp/kdata cold | xargs -I {} cargo run --bin
  kaleidoscope-cli -- migrate acme /tmp/kdata {} warm` walks
  each cold item back to warm without leaving the shell.

## Handoff to DESIGN

The DESIGN wave (`nw-solution-architect`) should preserve:

1. **The quiescent recorder pattern**: the function constructs
   a `cinder::CinderRecorder` (NoopRecorder shape) for the
   `FileBackedTieringStore::open(cinder_base(data_dir),
   recorder)` call identically to the way `stats_with_tiers`
   does at `crates/kaleidoscope-cli/src/lib.rs:377-378`; no
   OTLP file is created and no `--observe-otlp` flag is
   accepted in v0 (`wave-decisions.md` D-OutOfScope-Observe).
   The justification is specific: `list_by_tier` is a pure
   read with no operator-visible event to record — Cinder's
   `MetricsRecorder` trait has no `record_list` method.
2. **The `TieringStore::list_by_tier` call shape**: exactly one
   `list_by_tier(&tenant, tier)` call per CLI invocation. No
   `place` call, no `migrate` call, no `evaluate_at` call. The
   subcommand is purely read-only on the Cinder side.
3. **The stdout output contract**: one line per item id, sorted
   lexicographically, each terminated by `\n`. No header, no
   JSON, no CSV, no colour codes. Empty stdout when the result
   set is empty (no placeholder line).
4. **The determinism contract**: the lex-sort at the CLI
   boundary masks the `HashMap` iteration order randomisation
   in `cinder::InMemoryTieringStore::list_by_tier` at
   `crates/cinder/src/store.rs:190-198`. Two successive
   invocations with the same `(tenant, data_dir, tier)` tuple
   MUST produce byte-identical stdout.
5. **The fail-fast contract on invalid tier argument**: parse
   the `<tier>` argument BEFORE issuing any Cinder open, so an
   invalid value short-circuits without touching the
   filesystem. Stderr carries the verbatim invalid value. The
   `Error::InvalidTier { value }` variant already exists and
   carries this contract via its `Display` impl
   (`crates/kaleidoscope-cli/src/lib.rs:98-100`).
6. **The no-Lumen-touch contract**: the function MUST NOT open
   `FileBackedLogStore::open(lumen_base(data_dir), ...)`. The
   Lumen WAL+snapshot is byte-equivalent before and after the
   call.
7. **The read-only contract**: no `place`, no `migrate`, no
   `evaluate_at`. The Cinder WAL+snapshot is byte-equivalent
   before and after every call (success and failure paths).
8. **The tenant-isolation contract**: by direct delegation to
   `list_by_tier(tenant, tier)` which already filters per
   tenant at `crates/cinder/src/store.rs:194-196`. The CLI
   MUST NOT introduce any per-tenant override or cross-tenant
   aggregation.

The DESIGN wave should NOT introduce flags (`--observe-otlp`,
`--json`, `--format=...`, `--limit`, `--offset`), pagination,
cross-tenant aggregate (`list-items <data_dir> <tier>` without
`<tenant>`), historical state (`list-items ... --at <timestamp>`
— deferred to ADR-0039 §7 future feature), or structured output
formats.

## DEVOPS instrumentation needs

No new collection infrastructure. The `list-items` subcommand
is a pure read over the existing Cinder WAL+snapshot and emits
no OTLP, no metrics, no logs of its own (the `CinderRecorder`
on the Cinder side is intentionally quiescent; `list_by_tier`
does not invoke any recorder method because Cinder's
`MetricsRecorder` trait has no `record_list` method at
`crates/cinder/src/metrics.rs`). The CI gate is the new
acceptance test's exit code PLUS the unmodified locked test
files' continued green status, per ADR-0005 Gate 1 (the
workspace already runs `cargo test` on every commit).
