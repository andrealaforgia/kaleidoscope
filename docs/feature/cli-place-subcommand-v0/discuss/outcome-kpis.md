# Outcome KPIs — `cli-place-subcommand-v0`

## Feature

`cli-place-subcommand-v0` — add a new positional subcommand to the
existing CLI binary:
`kaleidoscope-cli place <tenant> <data_dir> <item_id> <tier> [--observe-otlp <path>]`.
The subcommand opens the Cinder store under `<data_dir>/cinder.*`,
calls
`cinder::TieringStore::place(&tenant, &ItemId::new(item_id), tier, SystemTime::now())`
(`crates/cinder/src/store.rs:81`), and writes a one-line report to
stdout: `placed tenant=<t> item=<id> tier=<x>\n`. On an invalid
`<tier>` argument (any spelling other than lower-case `hot` /
`warm` / `cold`), exits non-zero with a stderr line naming the
invalid value. With `--observe-otlp <path>` set, the Cinder
recorder appends exactly ONE `cinder.place.count` OTLP-JSON line
per place call to that path, mirroring the established Cinder-side
OTLP wiring used by `ingest` and `migrate`.

## Objective

A single
`kaleidoscope-cli place acme /tmp/data acme/bootstrap-00001 hot`
invocation records one Cinder placement and prints a one-line
operator-visible confirmation, giving the operator a one-shot,
pipeable, grep-friendly CLI surface for three operationally distinct
decisions ("bootstrap items that exist outside the Lumen ingest
flow", "set up a controlled test scenario by placing N items in
Hot before running `evaluate-policy`", "recover the Cinder catalog
from a manifest after a snapshot corruption"). Until this feature
shipped, items only entered Cinder as a side effect of `ingest`'s
batch flush at `crates/kaleidoscope-cli/src/lib.rs:251-253`. v0
unlocks direct placement without writing Rust.

## Note on KPI granularity

This feature is operator-visible at the CLI surface. The principal
KPI is OK1 (place-success correctness — the stdout report content
matches the request AND the post-call `get_entry().tier` equals
the requested `tier`). OK2 is the overwrite-semantics fidelity
KPI (placing over an existing item updates the entry to the new
tier; no error, no special case). OK3 is the invalid-tier fail-
fast guardrail. OK4 is the `--observe-otlp` emission KPI (exactly
one `cinder.place.count` line per place call when the flag is set).

## Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| OK1-CLI-place-success | Priya the platform operator, observed at the stdout byte level | Sees, on a single CLI invocation of `kaleidoscope-cli place <tenant> <data_dir> <item_id> <tier>` (with `<tier>` ∈ {`hot`, `warm`, `cold`}), the EXACT stdout line `placed tenant=<tenant> item=<item_id> tier=<tier>\n` where `<tier>` is the lower-case rendering of the requested target tier. After the call, `cinder.get_entry(tenant, item)` returns `Some(entry)` with `entry.tier` equal to the requested target tier. Exit code 0. Stderr empty. | 100% of `place` invocations with valid lower-case tier arguments produce the exact stdout report line AND a post-call `get_entry(tenant, item).tier` equal to the requested `tier`; 0% of such invocations produce a `tier` field that disagrees with what `get_entry(tenant, item)` returns after the call | 0% (no CLI surface for direct placement exists today; items only enter Cinder as a side effect of `ingest`'s batch flush at `crates/kaleidoscope-cli/src/lib.rs:251-253`; the operator's only path for direct placement is a Rust harness that opens `cinder::FileBackedTieringStore` and calls `place(...)` against an `ItemId`) | New acceptance test `crates/kaleidoscope-cli/tests/place_subcommand.rs` — happy-path scenario opens a fresh `data_dir`, calls the place library function with `tier_arg = "hot"` and a captured stdout sink, asserts the captured stdout equals `placed tenant=acme item=acme/bootstrap-00001 tier=hot\n` AND that a freshly-opened `FileBackedTieringStore::open(...).get_entry(acme, acme/bootstrap-00001).unwrap().tier == Tier::Hot` after the call | Leading (operator-visible behaviour; principal KPI for this feature) |
| OK2-CLI-place-overwrite-semantics | Priya the platform operator, observed at the stdout byte level + post-call `get_entry().tier` | Sees, when the `<item_id>` argument names an item that ALREADY has a placement for the `<tenant>` argument in the Cinder store, exit code 0, the EXACT stdout line `placed tenant=<tenant> item=<item_id> tier=<new_tier>\n` (where `<new_tier>` is the requested target tier, NOT the previous tier), and a post-call `get_entry(tenant, item).tier` equal to `<new_tier>`. The CLI faithfully reflects the underlying API's overwrite-semantics per `crates/cinder/src/store.rs:78-81` (the trait method docstring: "Overwrites any prior placement for the same key") and per `crates/cinder/src/store.rs:140-152` (the in-memory body unconditionally `state.entries.insert(key, ...)`). No special case in the CLI, no "AlreadyPlaced" branch invented or surfaced, no error returned. | 100% of `place` invocations over an existing item produce exit code 0 AND a stdout line with `tier=<new_tier>` AND a post-call `get_entry(tenant, item).tier == new_tier`; 0% of such invocations reject the call (no error variant invented for the overwrite case); 100% of such invocations bump the underlying `placed_at` field (faithfully reflecting the underlying API's behaviour at `crates/cinder/src/store.rs:147-149` — both `placed_at` and `migrated_at` are set to the `placed_at` argument) | n/a (no prior feature exercises an overwrite-place via a CLI surface; the underlying API's overwrite-semantics is documented behaviour as of `crates/cinder/src/store.rs:78-81`) | Same new test file — overwrite scenario pre-places `acme/bootstrap-00007` in Hot via a direct `FileBackedTieringStore::open(...).place(...)` call, then calls the place library function with the same `(tenant, item_id)` pair but `tier_arg = "cold"`. Asserts captured stdout equals `placed tenant=acme item=acme/bootstrap-00007 tier=cold\n`, exit code 0, AND `get_entry(acme, acme/bootstrap-00007).unwrap().tier == Tier::Cold` (the new tier). Per the task brief: documented behaviour, NOT a special case | Guardrail (operator-facing faithfulness to the underlying API; protects against accidentally introducing a special-case CLI guard that diverges from the trait contract) |
| OK3-CLI-place-invalid-tier-fail-fast | Priya the platform operator, observed at the stdout / stderr byte level + exit code | Sees, when the `<tier>` argument is any spelling other than exactly `hot` / `warm` / `cold` (upper-case `HOT`, mixed-case `Hot`, typo `lukewarm`, empty string, leading/trailing whitespace), exit code non-zero, empty stdout, AND a single stderr line that contains the exact invalid value she typed. The Cinder store under `<data_dir>` is byte-equivalent before and after the call (the tier-argument parse error fires BEFORE any `place` call is issued; the existing `Error::InvalidTier { value }` variant at `crates/kaleidoscope-cli/src/lib.rs:79-81` is reused). | 100% of `place` invocations with a non-`hot`/`warm`/`cold` tier argument produce a non-zero exit code, an empty stdout, AND a stderr line containing the invalid value verbatim; 100% of such invocations leave the Cinder store under `<data_dir>` byte-equivalent before and after; 0% of such invocations dispatch to the underlying `TieringStore::place` API (the parse error short-circuits before the store is opened, or at most before the `place` call) | 0% (today the lower-case tier convention is established by the parse side at `crates/kaleidoscope-cli/src/lib.rs:505-512` for `migrate` and `list-items` but is not yet enforced as a parse-side contract for `place` because no `place` parse-side surface exists) | Same new test file — invalid-tier scenarios assert the call with `tier_arg = "HOT"` returns `Err`, captured stdout is empty, captured stderr contains the substring `HOT`, and the post-call Cinder state is identical to the pre-call state. A second sub-scenario uses `tier_arg = "lukewarm"` (a typo) and asserts stderr contains `lukewarm` | Leading (operator-facing fail-fast guarantee; symmetric with `migrate` OK3 and `list-items` invalid-tier rejection) |
| OK4-CLI-place-observe-otlp-emission | Priya the platform operator, observed at the OTLP-JSON sidecar file content | Sees, when `--observe-otlp <path>` is supplied to the place subcommand, EXACTLY ONE `cinder.place.count` OTLP-JSON line appended to `<path>` per place call. The line carries the `tenant_id` resource attribute equal to the `<tenant>` positional argument and a `tier` point attribute equal to the lower-case rendering of the requested tier (`hot` / `warm` / `cold`). The byte-level wire shape is identical to the `cinder.place.count` line that `ingest` already emits via the same `CinderToOtlpJsonWriter` at `crates/kaleidoscope-cli/src/lib.rs:172-174`. When `--observe-otlp` is NOT supplied, NO file is created at any path. | 100% of `place` invocations with `--observe-otlp <path>` produce exactly one new `cinder.place.count` OTLP-JSON line appended to `<path>` per call (so N back-to-back invocations against the same path produce exactly N lines); 100% of `place` invocations WITHOUT the flag produce zero on-disk OTLP file artefacts; 0% of invocations with the flag emit anything other than exactly one `cinder.place.count` line (no extra `cinder.migrate.count`, no `lumen.ingest.count` — the Cinder `place` call site at `crates/cinder/src/store.rs:151` is the ONLY recorder invocation in the place dispatch path) | 0% (today the operator has no CLI surface for direct placement, observable or otherwise; `ingest`'s `--observe-otlp` arm emits `cinder.place.count` lines as a side effect of batch flush but the operator cannot trigger a single isolated `cinder.place.count` line) | Same new test file — observe-otlp scenario invokes the place library function with `otlp_log_path = Some(<tmp>/observe.log)`, asserts the file exists after the call and contains exactly one line that contains the substrings `cinder.place.count`, `acme` (tenant id), and `hot` (tier). A second sub-scenario invokes the library function with `otlp_log_path = None` and asserts no observe-otlp file exists at the candidate path | Leading (operator-facing observability surface; symmetric with `migrate --observe-otlp` OK1 in `cli-migrate-observe-otlp-v0`) |

## Metric Hierarchy

- **North Star**: **OK1-CLI-place-success** — the place-success
  correctness KPI. Without it, the subcommand cannot do its job.
  With it alone, the operator already has the three operationally
  distinct decisions enabled (bootstrap items outside the ingest
  flow / set up a controlled test scenario / recover from a Cinder
  snapshot corruption against a manifest) from one CLI invocation.
- **Leading Indicators**:
  - OK3 (invalid-tier fail-fast) — proves the lower-case tier
    contract is enforced fail-fast on the parse side, mirroring
    the rendering-side convention at
    `crates/kaleidoscope-cli/src/lib.rs:519-525`.
  - OK4 (--observe-otlp emission) — proves the Cinder OTLP wiring
    works on the place dispatch path with the same byte-level
    contract that `ingest` and `migrate` already emit.
- **Guardrail Metrics**:
  - OK2 (overwrite-semantics) — protects against accidentally
    introducing a special-case CLI guard ("AlreadyPlaced" rejection)
    that diverges from the underlying trait's overwrite-semantics.

## Cross-feature alignment

OK1 in this feature is the missing "C" of Cinder's CRUD: `ingest`
writes Hot Cinder items as a side effect
(`crates/kaleidoscope-cli/src/lib.rs:251-253`), `stats` /
`list-items` read the per-tier counts and the per-tier item list
via `TieringStore::list_by_tier`, `migrate` mutates a single
item's tier via `TieringStore::migrate`. This feature CREATES
(directly) via `TieringStore::place`. The four subcommands
together cover the operator's full Cinder lifecycle surface
(place, read, mutate, list) without requiring a Rust harness.

OK3 in this feature mirrors the fail-fast posture of `migrate`'s
OK3 (`cli-migrate-subcommand-v0`) and `list-items`'s invalid-tier
rejection (`cli-list-items-subcommand-v0`): on a bad parse, the
CLI exits non-zero with stderr naming the verbatim bad value via
the shared `Error::InvalidTier` variant at
`crates/kaleidoscope-cli/src/lib.rs:79-81, :98-100`.

OK4 in this feature mirrors the `--observe-otlp` emission posture
of `cli-migrate-observe-otlp-v0` (one `cinder.migrate.count` line
per migrate call) and `cli-cinder-otlp-wiring-v0` (one
`cinder.place.count` line per place inside `ingest`'s batch
flush). Same `CinderToOtlpJsonWriter` adapter
(`crates/kaleidoscope-cli/src/lib.rs:65`), same file-open shape
(`OpenOptions::create(true).append(true)` per ADR-0039 §8), same
wire format.

| KPI | Cross-feature precedent | This feature |
|-----|-------------------------|--------------|
| OK1 | `cli-migrate-subcommand-v0` OK1 calls `TieringStore::migrate(tenant, item, to_tier, SystemTime::now())` and writes the `migrated ...` report | `place` calls `TieringStore::place(tenant, item, tier, SystemTime::now())` and writes the `placed tenant=<t> item=<id> tier=<x>` report |
| OK2 | `cli-migrate-subcommand-v0` OK4 faithful to underlying API for same-tier idempotent migrate | `place` faithful to underlying API for overwrite-semantics; no CLI special case |
| OK3 | `cli-migrate-subcommand-v0` OK3 fail-fast on invalid tier (stderr names invalid value) | `place` fail-fast on invalid tier (stderr names invalid value) via the same shared `Error::InvalidTier` variant |
| OK4 | `cli-migrate-observe-otlp-v0` OK1 emits one `cinder.migrate.count` line per migrate call; `cli-cinder-otlp-wiring-v0` emits one `cinder.place.count` line per place inside ingest | `place` emits one `cinder.place.count` line per place call when `--observe-otlp` is set |

## Measurement Plan

| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| OK1-CLI-place-success | `crates/kaleidoscope-cli/tests/place_subcommand.rs` — happy-path scenario | `cargo test --package kaleidoscope-cli --test place_subcommand` exit code. The happy-path test opens a fresh temp `data_dir`, calls the place library function with arguments `(acme, data_dir, "acme/bootstrap-00001", "hot", &mut stdout_buf, None)` and a captured stdout sink. Asserts the captured stdout equals `placed tenant=acme item=acme/bootstrap-00001 tier=hot\n`, exit code 0, captured stderr empty, AND that a freshly-opened `FileBackedTieringStore::open(cinder_base(data_dir), ...).get_entry(acme, acme/bootstrap-00001).unwrap().tier == Tier::Hot` after the call | At every commit touching the CLI place path or the Cinder `place` / `get_entry` methods | `kaleidoscope-cli` maintainer (CI feedback per ADR-0005) |
| OK2-CLI-place-overwrite-semantics | Same test file — overwrite scenario | Same `cargo test` invocation. The test pre-places `acme/bootstrap-00007` for tenant `acme` in tier Hot via a direct `FileBackedTieringStore::open(...).place(...)` call (placed_at = a fixed `SystemTime` value), then calls the place library function with arguments `(acme, data_dir, "acme/bootstrap-00007", "cold", &mut stdout_buf, None)`. Asserts the call returns Ok; `stdout_buf` equals exactly `placed tenant=acme item=acme/bootstrap-00007 tier=cold\n`; `stderr_buf` is empty; a freshly-opened `get_entry(acme, acme/bootstrap-00007).unwrap().tier == Tier::Cold` (the new tier, overwriting the previous Hot) | Same | Same |
| OK3-CLI-place-invalid-tier-fail-fast | Same test file — invalid-tier scenarios (two sub-scenarios: upper-case `HOT` and typo `lukewarm`) | Same `cargo test` invocation. Each sub-scenario calls the place library function with an invalid tier argument against a fresh `data_dir`. Asserts the call returns `Err`, captured stdout is empty, captured stderr contains the invalid value verbatim, AND the post-call Cinder state is empty (no item placed because the parse error short-circuited the dispatch). For the second sub-scenario, a pre-existing item is seeded for tenant `acme` in tier Hot and the test asserts the seed item is byte-equivalent before and after | Same | Same |
| OK4-CLI-place-observe-otlp-emission | Same test file — observe-otlp scenarios (two sub-scenarios: flag present, flag absent) | Same `cargo test` invocation. The flag-present sub-scenario calls the place library function with arguments `(acme, data_dir, "acme/bootstrap-00001", "hot", &mut stdout_buf, Some(<tmp>/observe.log))`. Asserts the file at `<tmp>/observe.log` exists after the call, contains exactly one line, AND that line contains the substrings `cinder.place.count`, `acme`, and `hot`. The flag-absent sub-scenario calls the place library function with `otlp_log_path = None` and asserts no file exists at the candidate path (verifying no implicit file creation) | Same | Same |

## Hypothesis

We believe that **adding a new positional subcommand
`kaleidoscope-cli place <tenant> <data_dir> <item_id> <tier>
[--observe-otlp <path>]` (with `<tier>` accepted only in lower-case
`hot` / `warm` / `cold`) that opens the Cinder store under
`<data_dir>/cinder.*`, calls `cinder.place(tenant, item, tier,
SystemTime::now())`, writes the one-line stdout report `placed
tenant=<tenant> item=<item_id> tier=<tier>\n` on success — bubbling
the invalid-tier parse error as a non-zero exit + stderr line
containing the offending value — and (when `--observe-otlp` is set)
appends exactly one `cinder.place.count` OTLP-JSON line per call**
for the **platform operator (Priya)** will achieve **a one-shot,
pipeable, observable CLI surface for direct Cinder placement,
unifying three operationally distinct workflows (bootstrap items
outside ingest / set up controlled test scenarios / recover from
snapshot corruption) on the same invocation shape and removing the
operator's need to write Rust harnesses for placement.**

We will know this is true when:

- The new acceptance test's happy-path scenario passes green,
  asserting that `place(acme, ..., "acme/bootstrap-00001", "hot")`
  against an empty `data_dir` produces stdout `placed tenant=acme
  item=acme/bootstrap-00001 tier=hot\n` AND post-call
  `get_entry().tier == Tier::Hot` (OK1).
- The new acceptance test's overwrite scenario passes green,
  asserting that `place(..., "acme/bootstrap-00007", "cold")`
  against a pre-existing Hot placement produces stdout `placed
  tenant=acme item=acme/bootstrap-00007 tier=cold\n` AND post-call
  `get_entry().tier == Tier::Cold` (OK2).
- The new acceptance test's invalid-tier scenarios pass green,
  asserting that `place(..., tier_arg = "HOT")` and `place(...,
  tier_arg = "lukewarm")` each produce a non-zero exit, empty
  stdout, stderr containing the invalid value, AND no mutation to
  the Cinder store (OK3).
- The new acceptance test's observe-otlp scenarios pass green,
  asserting that `place(..., otlp_log_path = Some(path))` produces
  exactly one `cinder.place.count` line at `path` and that
  `place(..., otlp_log_path = None)` creates no file at the
  candidate path (OK4).
- The EXISTING locked acceptance test files
  (`tests/migrate_subcommand.rs`,
  `tests/migrate_observe_otlp_flag.rs`,
  `tests/list_items_subcommand.rs`,
  `tests/stats_subcommand.rs`,
  `tests/stats_cinder_tier_distribution.rs`,
  `tests/observe_otlp_*.rs`,
  `tests/ingest_and_read_roundtrip.rs`,
  `tests/cli_binary_smoke.rs`) continue to pass green UNMODIFIED
  under `cargo test --package kaleidoscope-cli`.
- The dogfood demo runs: `cargo run --bin kaleidoscope-cli --
  place acme /tmp/kdata acme/bootstrap-00001 hot` returns
  `placed tenant=acme item=acme/bootstrap-00001 tier=hot\n` on
  stdout, exit 0; the immediately-following `cargo run --bin
  kaleidoscope-cli -- list-items acme /tmp/kdata hot` includes
  `acme/bootstrap-00001` in its output; `cargo run --bin
  kaleidoscope-cli -- stats acme /tmp/kdata` shows `hot=1` (or
  one more than the pre-place count).

## Handoff to DESIGN

The DESIGN wave (`nw-solution-architect`) should preserve:

1. **The recorder-construction pattern**: the function constructs
   the Cinder recorder via the same `match otlp_log_path { Some(p)
   => CinderToOtlpJsonWriter::new(file), None => CinderRecorder
   }` shape used by `migrate()` at
   `crates/kaleidoscope-cli/src/lib.rs:435-444`. ADR-0039 §8 file-
   open contract preserved (`OpenOptions::create(true).append(true)`
   once per call).
2. **The `TieringStore::place` call shape**: exactly one `place`
   call per CLI invocation. No `get_entry` pre-flight (unlike
   `migrate`, the `place` API has overwrite-semantics and does NOT
   need to discover a `from` tier). No `migrate` call, no
   `evaluate_at` call, no `list_by_tier` call.
3. **The stdout output contract**: one literal line on success,
   `placed tenant=<tenant> item=<item_id> tier=<tier>\n`, where
   `<tier>` renders via the same lower-case mapping (`hot` /
   `warm` / `cold`) as the predecessor feature's `tier_lowercase`
   helper at `crates/kaleidoscope-cli/src/lib.rs:519-525`. No
   header, no JSON, no CSV, no colour codes.
4. **The fail-fast contract on invalid tier argument**: parse
   the `<tier>` argument BEFORE issuing the `place` call, so an
   invalid value short-circuits without opening the Cinder store
   (or at most without calling `place`). Stderr carries the
   verbatim invalid value via the existing `Error::InvalidTier`
   Display at `crates/kaleidoscope-cli/src/lib.rs:98-100`.
5. **The no-Lumen-touch contract**: the function MUST NOT open
   `FileBackedLogStore::open(lumen_base(data_dir), ...)`. The
   Lumen WAL+snapshot is byte-equivalent before and after the
   call.
6. **The no-special-case-for-overwrite contract**: the underlying
   `TieringStore::place` API is overwrite-semantics per
   `crates/cinder/src/store.rs:78-81, :140-152`. The CLI MUST NOT
   introduce a special case (e.g. an "AlreadyPlaced" branch that
   rejects the place call when the item already exists) for the
   overwrite case. The stdout report faithfully shows the new
   tier and exits 0.
7. **The `--observe-otlp` emission shape**: exactly one
   `cinder.place.count` OTLP-JSON line per place call when the
   flag is set. The line carries the tenant id resource attribute
   and the `tier` point attribute. Byte-identical to the
   `cinder.place.count` lines `ingest` already emits via the same
   `CinderToOtlpJsonWriter`.

The DESIGN wave should NOT introduce flags (`--placed-at`,
`--dry-run`, `--format=...`), bulk placement (multi-item single
call), pre-flight existence checks, structured output formats
(JSON, CSV), or Lumen mutation.

## DEVOPS instrumentation needs

The `--observe-otlp <path>` flag is the operator-facing
instrumentation surface; one line per call appended to the
operator-supplied path. No new collection infrastructure beyond
what `cli-cinder-otlp-wiring-v0` and `cli-migrate-observe-otlp-v0`
already established. The CI gate is the new acceptance test's
exit code PLUS the unmodified locked test files' continued green
status, per ADR-0005 Gate 1.
