# Definition of Ready — Validation

Feature: `cli-get-tier-subcommand-v0`
Reviewer: Luna (self-review at DISCUSS close; second pass scheduled
at handoff to DESIGN per `nw-po-review-dimensions` skill).
Date: 2026-05-19.

## Per-story DoR (9-item hard gate)

### US-01: Operator confirms a single item's current tier before manual action

| # | DoR Item | Status | Evidence |
|---|----------|--------|----------|
| 1 | Problem statement clear, domain language | PASS | "What she canNOT do today is answer 'what tier is item `acme/batch-00042` in right now?' from a single CLI invocation. ... three subprocess invocations, three `FileBackedTieringStore::open` calls, three full per-tier scans via `list_by_tier`, three pipes to `grep`. The `||` short-circuit also makes the exit code semantics confusing (`grep` returns non-zero on no-match) ... For a tier-decision question that is operationally one read — 'look up the tier for one (tenant, item_id) pair' — this is operator-hostile." Uses tier, `ItemId`, `TieringStore`, `get_tier`, `list_by_tier`, Cinder, hot/warm/cold, `place()`, lower-case — all domain terms grounded in the existing Cinder/CLI implementation. |
| 2 | User/persona identified | PASS | Priya the platform operator, multi-tenant Kaleidoscope deployment for a fintech, already uses `kaleidoscope-cli ingest`, `read`, `stats`, `list-items`, `place`, `migrate` daily. Uses standard Unix text tools (`grep`, `cut`, `awk`, `test`) on stdout output. Expects fail-fast behaviour (non-zero exit + descriptive stderr) on unknown item ids. Does NOT expect the subcommand to touch the Lumen store. |
| 3 | 3+ domain examples with real data | PASS | Five examples: happy-path Hot with `acme/batch-00042` (pre-flight before migrate); happy-path Cold with `acme/batch-00007` (incident-time audit on alert); happy-path Warm with `acme/batch-00050` (scripted assertion in pipeline using `test "$(...)" = "tier=warm"`); error case (unknown item) with `acme/batch-00099` showing the empty-stdout + stderr-with-`unknown item`+item-id+tenant + non-zero-exit + no-store-mutation contract; tenant-isolation case with `acme` and `globex` both holding an item id `acme/batch-00042` showing the two reads return different per-tenant tiers. Real values throughout: `acme`, `globex`, `acme/batch-00042`, `acme/batch-00007`, `acme/batch-00050`, `acme/batch-00099`, `acme/batch-00001`, `acme/batch-00000`, `/tmp/k-data`, `/tmp/kdata`, `/tmp/data`, real tier names (`hot`, `warm`, `cold`), real source-file references. |
| 4 | UAT scenarios (3-7 in Given/When/Then) | PASS | 5 scenarios: happy-path Hot (OK1), happy-path Warm (OK1), happy-path Cold (OK1), unknown-item fail-fast (OK2), tenant-isolation (OK3). |
| 5 | AC derived from UAT | PASS | 11 AC bullets, each maps to an observable byte-level / state-level invariant from a scenario (one literal `tier=hot/warm/cold\n` stdout line on each happy-path success; post-call `get_tier(tenant, item)` returns the SAME tier — read-only invariant; empty stdout + non-zero exit + stderr containing `unknown item` + item id + tenant on unknown-item; tenant-isolation two-call sequence returns `tier=hot` for `acme` and `tier=warm` for `globex`; Lumen store byte-equivalent before/after every invocation; Cinder store byte-equivalent before/after every invocation — read-only; new acceptance test file added; locked test files continue to pass green unmodified; `print_usage` update optional; no new external dependency). |
| 6 | Right-sized | PASS | 1 story, 1 function-level addition in `lib.rs` (new `get_tier(...)` library function), 1 dispatch wiring change in `main.rs` (new `Some("get-tier")` arm + new `run_get_tier(...)` helper + `print_usage` update), 1 new test file with 5 scenarios, 1 manifest entry. Estimated effort: well under 1 day. Strictly thinner than the predecessor `cli-migrate-subcommand-v0`. |
| 7 | Technical notes identify constraints | PASS | File paths, line numbers, manifest entry, dependency posture (no new external dependency — `cinder::TieringStore::get_tier`, `FileBackedTieringStore::open`, `cinder::CinderRecorder`, `Tier` enum, `ItemId::new`, `cinder_base()` helper, `tier_lowercase()` helper, `parse_positional` helper, `aegis::TenantId` all already wired). The explicit non-prescription of internal implementation details beyond the choice of one new free function shape (DESIGN-locked per `wave-decisions.md` D-FunctionShape) and the choice of error variant (DESIGN-locked per D-ErrorVariant). The hard rule "DO NOT modify any locked test file" is restated as D-LockedTests. |
| 8 | Dependencies tracked | PASS | All Cinder-side APIs exist (`crates/cinder/src/store.rs:85` for `get_tier`; `:55-58` for `MigrateError::UnknownItem` Display text; `:154-160` for the read-only `get_tier` impl); `cinder::FileBackedTieringStore` already used by `list_items()` (`crates/kaleidoscope-cli/src/lib.rs:534`); `cinder::CinderRecorder` already used by `list_items` at the same site; `cinder::Tier` and `cinder::ItemId` already imported; `cinder_base(data_dir)` helper at `crates/kaleidoscope-cli/src/lib.rs:122-124`; `tier_lowercase` helper at `crates/kaleidoscope-cli/src/lib.rs:564-570`; `parse_positional` helper already in `main.rs`; `aegis::TenantId` already a dependency. No unresolved internal or external dependencies. |
| 9 | Outcome KPIs defined with measurable targets | PASS | Maps to OK1-CLI-get-tier-success (principal: 100% of valid invocations against placed items produce the exact `tier=<lowercase>\n` stdout line for the tier returned by `get_tier`), OK2-CLI-get-tier-unknown-item-fail-fast (100% of unplaced-item invocations produce non-zero exit + stderr containing the substrings `unknown item`, item id, and tenant + no store mutation), OK3-CLI-get-tier-tenant-isolation (100% of cross-tenant same-`ItemId` reads return the respective per-tenant tier, faithful to the `(TenantId, ItemId)` placement key). All three targets are quantitative and falsifiable via the named test file. |

### US-01 DoR Status: PASSED

## Feature-level DoR

| # | Item | Status | Evidence |
|---|------|--------|----------|
| F1 | Journey artefact (visual) | NOT PRODUCED — accepted | The journey is so thin (one operator action: invoke `get-tier <tenant> <data_dir> <item_id>` and read stdout) that a separate journey visual would duplicate the Elevator Pitch in US-01. The predecessor features' journey visuals collectively cover the underlying CLI operator mental model. Acknowledged gap; will not be remediated. |
| F2 | Journey artefact (YAML schema) | NOT PRODUCED — accepted | Same rationale as F1. The journey schema is for multi-step user journeys; this feature has one step. |
| F3 | Journey artefact (Gherkin `.feature`) | INTENTIONALLY OMITTED | Per the project convention: the acceptance idiom is Rust `#[test]` functions with `// Given / // When / // Then` comment blocks, not Gherkin `.feature` files. The Given/When/Then text lives inside `user-stories.md` (UAT Scenarios sections). DISTILL writes the Rust tests. |
| F4 | Shared artefact registry | NOT PRODUCED — accepted | The cross-feature shared artefacts (`tenant` positional argument, `data_dir` positional argument, `cinder_base(data_dir)` helper, `TieringStore::get_tier` API, `CinderRecorder` quiescent recorder pattern, `tier_lowercase` rendering helper, the `unknown item` stderr phrasing inherited from `MigrateError::UnknownItem`) are tracked in the predecessor features and the source code itself. The output-shape contracts are restated as System Constraints in `user-stories.md`. |
| F5 | Story map | PASS | `discuss/story-map.md` with one-activity backbone, walking-skeleton justification (N/A — the predecessor cluster's substrate already exists and the Cinder `TieringStore::get_tier` API already exists), single-slice rationale, cross-feature alignment, scope assessment, priority rationale section, all three KPI guardrails articulated. |
| F6 | Prioritization | INTEGRATED into `story-map.md` | The single-slice structure renders a separate `prioritization.md` redundant. The `## Priority Rationale` section in `story-map.md` covers the ordering logic and partial-ship reasoning. |
| F7 | Outcome KPIs | PASS | `discuss/outcome-kpis.md` with OK1 (principal — get-tier-success correctness), OK2 (unknown-item fail-fast), OK3 (tenant-isolation), each with quantitative target, baseline, and measurement plan. |
| F8 | User stories | PASS | `discuss/user-stories.md` with US-01, including complete Elevator Pitch with the real entry-point shell command (`kaleidoscope-cli get-tier acme /tmp/data acme/batch-00042`) and the concrete byte-level output (`tier=hot` shown in the "After" line, the three-subprocess `list-items` + grep chain shown in the "Before" line). |
| F9 | Per-slice files | PASS | 1 file under `discuss/slices/` (`slice-01-get-tier-subcommand-reports-current-tier.md`) covering Goal, IN scope, OUT scope, Rejected alternatives, Learning hypothesis, AC, Dependencies, Reference class, Effort, DoD. |
| F10 | Wave decisions | PASS | `discuss/wave-decisions.md` with D1-D4 (pre-wave decisions), D-OutputShape (`tier=<lowercase>\n`), D-StderrWording (mirror `MigrateError::UnknownItem`), D-NoLumenTouch (Cinder-only), D-ReadOnly (no recorder hook, no `--observe-otlp`), D-Tenant-Isolation (inherited from `TieringStore`), D-FunctionShape (DESIGN-locked signature), D-ErrorVariant (DESIGN-locked variant naming), D-NewTestFile (new file location), D-LockedTests (do-not-modify-locked-files hard rule), D-OutOfScope-Bulk / D-OutOfScope-Json / D-OutOfScope-Observe / D-OutOfScope-FullEntry (the four task-brief out-of-scope items), D-NoSSOT. |
| F11 | Right-sized (Elephant Carpaccio) | PASS | 1 story, 1 bounded context (`kaleidoscope-cli` crate), 2 modified files in `src/`, 1 new test file, 1 manifest line-level change. Estimated effort: well under 1 day. `story-map.md` "Scope Assessment: PASS". |
| F12 | SSOT impact assessed | PASS | `wave-decisions.md` D-NoSSOT: no SSOT modification. Same posture as the predecessor features. |
| F13 | Cross-feature contract honoured | PASS | `user-stories.md` System Constraints pin the output-shape contract (one literal `tier=<lowercase>\n` line on success; non-zero exit + stderr line on unknown-item), the positional-argument convention (extends the predecessor `<tenant> <data_dir>` shape with one new positional argument `<item_id>` — same prefix as `migrate` and `place` minus the tier), the stream contract (stdout for success, stderr for failure — inherited from the binary's existing main.rs error printer), the lower-case tier rendering convention (`tier_lowercase` at `lib.rs:564-570`), the unknown-item stderr language (mirroring `MigrateError::UnknownItem` at `store.rs:55-58`), the no-Lumen-touch invariant, the read-only invariant (Cinder store byte-equivalent before/after every invocation), and the tenant-isolation invariant (inherited from `cinder::TieringStore`). The locked test files for the prior features continue to pass green UNMODIFIED. |

## Anti-pattern scan

| Anti-pattern | Detected? | Notes |
|--------------|-----------|-------|
| Implement-X | No | Story framed as "Priya runs `kaleidoscope-cli get-tier acme /tmp/data acme/batch-00042` and sees `tier=hot` on stdout in milliseconds, exit 0, without writing Rust and without running three `list-items` invocations" — outcome-first from the operator perspective. The three operator uses enabled (pre-flight before manual migrate / scripted pipeline assertion / incident-time single-item audit) are named explicitly. |
| Generic data | No | Real names: Priya, `acme`, `globex`. Real item ids: `acme/batch-00042`, `acme/batch-00007`, `acme/batch-00050`, `acme/batch-00099`, `acme/batch-00001`, `acme/batch-00000`. Real paths: `/tmp/k-data`, `/tmp/kdata`, `/tmp/data`. Real tier names: `hot`, `warm`, `cold`. Real source-file references: `crates/kaleidoscope-cli/src/lib.rs:122-124, :534, :564-570`; `crates/cinder/src/store.rs:55-58, :71-72, :85, :119, :154-160`. |
| Technical AC | No blocking instances | AC pin the wire-observable invariants (one literal `tier=hot/warm/cold\n` stdout line on success; empty stdout + non-empty stderr + non-zero exit on the unknown-item path; post-call `get_tier(tenant, item)` returns the same tier — read-only; Lumen and Cinder stores byte-equivalent before and after every invocation; tenant-isolation two-call sequence). They reference the structural mirror of `list_items`'s recorder construction as an implementation pattern but do NOT prescribe novel internal mechanisms. |
| Technical scenario titles | No | Scenarios titled by user outcome: "Operator queries an item placed in Hot — sees `tier=hot` on stdout", "Operator queries an item placed in Warm — sees `tier=warm` on stdout", "Operator queries an item placed in Cold — sees `tier=cold` on stdout", "Operator queries an item that was never placed — fail-fast with stderr naming the missing item", "Tenant isolation — querying `acme/batch-00042` for `acme` does not surface `globex`'s same-named item". Not "TieringStore::get_tier is called" or "FileBackedTieringStore::open returns adapter". |
| Oversized story | No | 1 story, 5 UAT scenarios, 11 AC, well under 1 day effort. Comparable to / strictly thinner than the predecessor. |
| Abstract requirements | No | Every AC has a specific value (string literal `tier=`, the `\n` line terminator, substring assertions for stderr content including `unknown item` and the verbatim item id and tenant), a numeric exit code (0 for success, non-zero for failure), or a state-level invariant (post-call `get_tier` returns the same `Option<Tier>` value, Lumen and Cinder directory byte-equivalence). |

## Reviewer self-pass (Dimension 0 — Elevator Pitch, BLOCKING)

| Story | Section present | Real entry point | Concrete output | Job connection | Verdict |
|-------|-----------------|------------------|-----------------|----------------|---------|
| US-01 | yes (Before / After / Decision enabled triple) | yes — `kaleidoscope-cli get-tier acme /tmp/data acme/batch-00042` (the actual shell command an operator types — NEW subcommand on the existing binary) | yes — one literal stdout line (`tier=hot`) shown in the "After" line; companion error-case stderr line behaviour described concretely (`unknown item "acme/batch-00099" for tenant acme`) | yes — "Priya can decide and execute three operationally distinct uses from one CLI invocation: 1. 'Before I run `migrate acme /tmp/data acme/batch-00042 cold`, confirm that the item is in Hot or Warm (not already in Cold)' ... 2. 'In my deployment pipeline, assert that `acme/batch-00042` is in Cold before declaring the migration job successful' ... 3. 'An alert just fired naming `acme/batch-00042`. Audit the specific item: what tier is it in right now?' ..." | PASS |

Slice-level check (Dimension 0 item 5): the single slice (Slice 01)
is tagged operator-visible (NOT `@infrastructure`). The story's
Elevator Pitch names a real user-invocable CLI command and the
byte-level stdout output the operator's shell session will display.
There is no slice-level infrastructure-only blocking concern.
PASS at slice level.

## Confirmation-bias detection (Dimensions 1-4 self-check)

| Dimension | Check | Verdict |
|-----------|-------|---------|
| 1.1 Technology bias | Story specifies the existing CLI binary, the existing `TieringStore::get_tier` trait API, and a plain-text one-line stdout shape. No specific JSON parser prescribed; no specific datetime library prescribed; no new external dependency. The output is consumable by standard Unix text tools (`grep`, `cut`, `awk`, `test`) that any operator already has. | PASS |
| 1.2 Happy path bias | Story includes two non-happy-path scenarios out of five: unknown-item fail-fast (OK2) and tenant-isolation guardrail (OK3). The three happy-path scenarios cover all three tier values (Hot, Warm, Cold) so no single tier is under-tested. The OK2 KPI is itself a sad-path-shaped guarantee. | PASS |
| 1.3 Availability bias | The chosen output shape (plain-text `tier=<lowercase>\n`) is justified by the `stats` aesthetic (`hot=N`, `warm=N`, `cold=N`) and by the operator's existing tooling. The unknown-item stderr language (`unknown item`) is justified by reference to `MigrateError::UnknownItem`'s `Display` impl at `crates/cinder/src/store.rs:55-58` (locked by `tests/migrate_subcommand.rs:319`). The no-Lumen-touch decision is justified against the alternative of opening both stores. The no-`--observe-otlp` decision is justified by the structural fact that `get_tier`'s impl at `crates/cinder/src/store.rs:154-160` does NOT call any recorder method. | PASS |
| 2.1 Missing stakeholder perspectives | Primary: platform operator (Priya). Secondary: Cinder maintainer (covered by the meta-AC — the existing locked test files continue to pass green UNMODIFIED). Tertiary: scripting/automation consumers (covered by the explicit operator-friendly stdout/stderr contracts: one literal line on success, one stderr line containing the verbatim item id and tenant on failure, non-zero exit). | PASS |
| 2.2 Missing error scenarios | One distinct error scenario specified: unknown-item (OK2 — `get_tier(...) -> None`). There are NO other parse-side error paths for this subcommand because there is no tier argument and no flag. Plus the no-Lumen-touch invariant covering both call branches (success, unknown-item). Plus the tenant-isolation invariant covering the cross-tenant read-leak guardrail. Cinder open errors are surfaced via the existing `kaleidoscope_cli::Error::CinderOpen` variant and bubble to the binary's existing error printer in `main()`. At most one new error variant introduced (the `CinderUnknownItem { tenant, item }` variant, DESIGN's call). | PASS |
| 2.3 Missing NFRs | NFRs covered: get-tier-success correctness (OK1 — operator-facing correctness invariant), fail-fast on unknown item (OK2 — operator-facing fail-fast guarantee), tenant-isolation (OK3 — operator-facing key-invariant guardrail), no-Lumen-touch invariant (the Lumen WAL+snapshot is byte-equivalent before and after every call), read-only invariant (the Cinder WAL+snapshot is byte-equivalent before and after every call). Performance: the v0 contract is "one `get_tier` call per invocation" — O(1) HashMap lookup in the in-memory adapter; the file-backed adapter's complexity is whatever the underlying snapshot lookup costs. Acceptable at v0 tier sizes. | PASS |
| 3.1 Vague performance requirements | All "100%"/"0%" claims are quantitative and falsifiable via the named test. No "fast", "scalable", "performant" adjectives without a number. The "in milliseconds" claim in the Elevator Pitch is descriptive operator language inherited from the predecessor features, not a normative requirement; the AC do not quantify a latency target. | PASS |
| 3.2 Ambiguous requirements | Every AC pins exact string literals (`tier=`, the `\n` line terminator), exit code values (0 for success, non-zero for failure), substring assertions on stderr (the verbatim item id, the verbatim tenant, the `unknown item` token), or state-level equality on `get_tier().Option<Tier>`. | PASS |
| 4 Testability | Every AC is a property of (a) the captured stdout bytes after the get-tier function returns, (b) the captured stderr bytes (or the subprocess stderr in the unknown-item subprocess test variant), (c) the returned `Result`, (d) a subsequent `cinder.get_tier(tenant, item)` call against a freshly-opened Cinder store, (e) a subsequent `list_by_tier(tenant, tier).len()` count (for the no-mutation guarantee on the unknown-item path), or (f) the presence/byte-content of the `lumen_base(data_dir)` directory. Every AC is automatable via `String::from_utf8` + `Vec<u8>` byte comparison + `str::contains` substring matching + `assert_eq!` integer / enum comparison + filesystem checksum or existence assertion. | PASS |
| 5.1 Largest bottleneck | The largest gap in the operator's daily Cinder workflow for the single-item tier-query question is "I want to know what tier item X is in for tenant Y, but the only way today is three subprocess `list-items` invocations + grep chain with ambiguous exit-code semantics". This feature addresses exactly that gap by adding the missing CLI surface. | PASS |
| 5.2 Simpler alternatives considered | Considered and rejected: (a) bulk lookup shape (`get-tier <tenant> <data_dir> <item_ids_file>`) — rejected in `wave-decisions.md` D-OutOfScope-Bulk; (b) full `get-entry` shape returning the `TierEntry` triple including `placed_at` and `migrated_at` — rejected in D-OutOfScope-FullEntry, belongs to a future `cli-get-entry-subcommand-v0`; (c) `--observe-otlp` wiring — rejected in D-OutOfScope-Observe / D-ReadOnly, `get_tier` has no recorder hook; (d) JSON output — rejected in D-OutOfScope-Json, plain-text matches the `stats` aesthetic. | PASS |
| 5.3 Constraint prioritization | The dominant constraint is correctness (OK1 — the stdout content must match the underlying API's outcome). The fail-fast guardrail (OK2 — unknown item) is the only input-error path the operator can encounter. OK3 (tenant-isolation) is correctly weighted as the guardrail against accidentally introducing a cross-tenant read leak that diverges from the underlying `(TenantId, ItemId)` key invariant. There is no cross-process or concurrency invariant to prioritize because the extension is a single O(1) read in a single thread. | PASS |
| 5.4 Data-justified | This is not a performance optimization. The decision to use one `get_tier` call per invocation (vs `get_entry` returning the full triple) is justified by the v0 scope (under-1-day effort, narrowest answer to the narrowest question). The decision to ship the single-item primitive (vs bulk lookup) is justified by the standard "v0 ships the primitive, v1 ships ergonomic batching" pattern. The decision NOT to accept `--observe-otlp` is justified by the structural fact that `get_tier`'s impl at `crates/cinder/src/store.rs:154-160` does not call any recorder method — there is no OTLP signal to attach. | PASS |

## DoR Status: PASSED — with four honestly-recorded artefact gaps

The story-level DoR (9 items × 1 story = 9 checks) is PASSED with
evidence on every item. The feature-level DoR is PASSED with the
following honestly-recorded artefact gaps (inherited posture from
the predecessor features):

- **F1, F2 (journey artefacts)**: deliberately not produced. The
  journey is one step (operator runs `get-tier <tenant>
  <data_dir> <item_id>` and reads stdout); a multi-step journey
  visual or YAML would duplicate the Elevator Pitch in US-01.
  Acknowledged; will not be remediated.
- **F3 (Gherkin `.feature` file)**: deliberately not produced.
  Per the project convention, this project's acceptance idiom is
  Rust `#[test]` functions with `// Given / // When / // Then`
  comment blocks. The Given/When/Then specification lives inline
  in `user-stories.md`.
- **F4 (shared artefact registry)**: not produced. The
  cross-feature shared artefacts are tracked in the predecessor
  features and the source code itself; the output-shape contracts
  are restated as System Constraints in `user-stories.md`.
- **F6 (separate `prioritization.md`)**: not produced; integrated
  into `story-map.md`'s `## Priority Rationale` section per the
  `nw-leanux-methodology` skill template, because a one-slice
  feature does not earn a separate prioritization document.

Anti-pattern scan clean. Dimension 0 elevator-pitch check PASSES
with the real entry point (`kaleidoscope-cli get-tier acme
/tmp/data acme/batch-00042` — a NEW subcommand on the existing
binary) and the concrete byte-level output (one literal `tier=hot`
stdout line on success; non-empty stderr line containing
`unknown item`, the verbatim item id, and the verbatim tenant on
the unknown-item failure path). Dimensions 1-5 confirmation-bias
self-check PASSES.

The DISCUSS wave is ready to hand off to `nw-solution-architect`
for DESIGN.
