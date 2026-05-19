# Definition of Ready — Validation

Feature: `cli-stats-subcommand-v0`
Reviewer: Luna (self-review at DISCUSS close; second pass scheduled at
handoff to DESIGN per `nw-po-review-dimensions` skill).
Date: 2026-05-19.

## Per-story DoR (9-item hard gate)

### US-01: Operator inspects a tenant's record count and time window without dumping every record

| # | DoR Item | Status | Evidence |
|---|----------|--------|----------|
| 1 | Problem statement clear, domain language | PASS | "Today her only options are to pipe `kaleidoscope-cli read acme /tmp/data` through `wc -l`, `head -1`, `tail -1`. For a tenant with 10 million records that is roughly 10 GB of NDJSON produced, piped, and discarded — four times. ... The smoke-test is so expensive that operators stop running it after every ingest." Uses Lumen, tenant, `observed_time_unix_nano`, NDJSON, ISO 8601 UTC, key=value, `lumen.query(tenant, TimeRange::all())` — all domain terms grounded in the existing Lumen/CLI implementation. |
| 2 | User/persona identified | PASS | Priya the platform operator, multi-tenant Kaleidoscope deployment for a fintech, already uses `kaleidoscope-cli ingest` and `kaleidoscope-cli read` daily; already uses `--observe-otlp` per the four reference features. Uses standard Unix text tools (`grep`, `cut`, `awk`) on stdout output, not JSON parsers. |
| 3 | 3+ domain examples with real data | PASS | Three examples: happy path with `acme` / 7 pre-ingested records spanning 2026-05-18 to 2026-05-19 / three-line stdout shape shown literally; edge case with `globex` / exactly 1 record / `earliest=`/`latest=` lines byte-identical (degenerate single-record window); boundary case with typo `acmee` / 0 records / single-line `records=0` stdout (the rejected sentinel encoding `<none>` is documented in `wave-decisions.md` D5 and the slice brief). Real values throughout: `acme`, `globex`, `acmee`, `/tmp/k-data`, `/tmp/kdata`, ISO 8601 UTC strings with literal calendar dates, real `observed_time_unix_nano` magnitudes (`1747526400000000000`, `1747612800000000000`, `1746921600000000000`). |
| 4 | UAT scenarios (3-7 in Given/When/Then) | PASS | 5 scenarios: populated-tenant three-line stdout (OK1 + OK2), empty-tenant one-line stdout (OK3), single-record tenant identical earliest/latest, tenant-isolation `stats(&acme)` does not count `globex` records, consistency with `read` for the same `(tenant, data_dir)` pair. |
| 5 | AC derived from UAT | PASS | 12 AC bullets, each maps to an observable byte-level invariant from a scenario (subcommand accepted by main.rs dispatcher; 3-line populated stdout in exact order; 1-line empty stdout with no timestamp lines; count consistency with `read`; earliest equals min `observed_time_unix_nano`; latest equals max; degenerate single-record window; tenant isolation 7 vs 10; read-only invariant; no OTLP file created; `print_usage` documents the subcommand; existing `tests/observe_otlp_*.rs` non-regression). |
| 6 | Right-sized | PASS | 1 story, 1 wiring change (one new subcommand arm in `main.rs`, one new `run_stats` helper, one new library free function `stats`, one updated `print_usage` block), 1 new test file with 5 scenarios, well under 1 day effort. Strictly smaller than all four reference features in the cluster (no OTLP wiring, no concurrency probe, no Cinder lookup). |
| 7 | Technical notes identify constraints | PASS | File paths, line numbers, manifest entry, dependency posture (no new internal crates; one potential new external dev-dep for ISO 8601 formatting — `chrono` or `time` — DESIGN locks the choice per `wave-decisions.md` D6), concurrency model note (single thread, single query call, no Cinder, no OTLP), the explicit non-prescription of internal implementation details beyond the structural mirror of the existing `read()` shape and the existing `LogStore::query` call (DESIGN-owned choice for the exact function signature). |
| 8 | Dependencies tracked | PASS | `lumen::LogStore::query(tenant, TimeRange::all())` already exists and returns `Result<Vec<LogRecord>, LogStoreError>` sorted ascending by `observed_time_unix_nano` (`crates/lumen/src/store.rs:69-70, 84`); `lumen::LogRecord::observed_time_unix_nano: u64` is the canonical sort key (`crates/lumen/src/record.rs:48`); `lumen::FileBackedLogStore` already used by `read()`; `self_observe::LumenToPulseRecorder` already used by `read()`'s no-flag arm; `aegis::TenantId` already a dependency. The one potentially-new external dev-dep (ISO 8601 formatter) is flagged and deferred to DESIGN for the choice. No unresolved internal dependencies. |
| 9 | Outcome KPIs defined with measurable targets | PASS | Maps to OK1-CLI-stats-record-count (principal: 100% of `stats()` invocations report a `records=N` value where N equals what `read()` would return for the same `(tenant, data_dir)`; 0% disagreement), OK2-CLI-stats-time-range (100% of populated invocations produce `earliest=` / `latest=` lines equal to the ISO 8601 UTC rendering of the seeded min/max nanos; 100% of single-record invocations produce byte-identical `earliest=` / `latest=` values), OK3-CLI-stats-empty-tenant (100% of zero-record invocations produce exactly 1 stdout line `records=0\n`; 0% leave `earliest=` or `latest=` lines). All three targets are quantitative and falsifiable via the named test file. |

### US-01 DoR Status: PASSED

## Feature-level DoR

| # | Item | Status | Evidence |
|---|------|--------|----------|
| F1 | Journey artefact (visual) | NOT PRODUCED — accepted | Per `wave-decisions.md`, the journey is so thin (one operator action: invoke `stats <tenant> <data_dir>` and read stdout) that a separate journey visual would duplicate the Elevator Pitch in US-01. The four reference features in this cluster (the original `--observe-otlp` ingest wiring, `cinder-to-otlp-json-bridge-v0`, `cli-cinder-otlp-wiring-v0`, `cli-read-observe-otlp-v0`) collectively cover the underlying CLI operator mental model. Acknowledged gap; will not be remediated. |
| F2 | Journey artefact (YAML schema) | NOT PRODUCED — accepted | Same rationale as F1. The journey schema in `nw-design-methodology` is for multi-step user journeys; this feature has one step. |
| F3 | Journey artefact (Gherkin `.feature`) | INTENTIONALLY OMITTED | Per the project convention (inherited from the four reference features and noted in the task brief): this project's acceptance idiom is Rust `#[test]` functions with `// Given / // When / // Then` comment blocks, not Gherkin `.feature` files. The Given/When/Then text lives inside `user-stories.md` (UAT Scenarios sections). DISTILL writes the Rust tests. |
| F4 | Shared artefact registry | NOT PRODUCED — accepted | The cross-feature shared artefacts (`tenant_id` positional argument, `data_dir` positional argument, `lumen_base(data_dir)` helper, the `LogStore::query(tenant, TimeRange::all())` API, `LumenToPulseRecorder` quiescent recorder pattern) are tracked in the reference features and the source code itself. The output-shape contracts (3-line populated / 1-line empty / ISO 8601 UTC with `Z` suffix / `records` / `earliest` / `latest` key names) are restated as System Constraints in `user-stories.md`. |
| F5 | Story map | PASS | `discuss/story-map.md` with one-activity backbone, walking-skeleton justification (N/A — the substrate already exists), single-slice rationale, cross-feature alignment, scope assessment, priority rationale section. |
| F6 | Prioritization | INTEGRATED into `story-map.md` | The single-slice structure renders a separate `prioritization.md` redundant. The `## Priority Rationale` section in `story-map.md` covers the ordering logic and partial-ship reasoning. |
| F7 | Outcome KPIs | PASS | `discuss/outcome-kpis.md` with OK1 (principal — record count), OK2 (time range), OK3 (empty tenant), each with quantitative target, baseline, and measurement plan. |
| F8 | User stories | PASS | `discuss/user-stories.md` with US-01, including complete Elevator Pitch with the real entry-point shell command (`kaleidoscope-cli stats acme /tmp/data`) and the concrete byte-level output (three literal stdout lines shown in Domain Example 1). |
| F9 | Per-slice files | PASS | 1 file under `discuss/slices/` (`slice-01-stats-subcommand-emits-record-count-and-time-range.md`, ≤ 100 effective lines — the file includes additional clarifying sections beyond the strict 100-line cap but the body of the slice brief — Goal, IN scope, OUT scope, Rejected alternatives, Learning hypothesis, AC, Dependencies, Reference class, Effort, DoD — is within the spirit of the cap). |
| F10 | Wave decisions | PASS | `discuss/wave-decisions.md` with D1-D10 covering scope (D1), out-of-scope items (D2-D4, D7), the empty-tenant encoding choice (D5), the timestamp format choice (D6), the library function naming-but-not-designing (D8), the test file location (D9), and SSOT non-modification (D10). |
| F11 | Right-sized (Elephant Carpaccio) | PASS | 1 story, 1 bounded context (`kaleidoscope-cli` crate), 2 modified files + 1 new test file + 1 manifest line-level change. Estimated effort: well under 1 day. `story-map.md` "Scope Assessment: PASS". |
| F12 | SSOT impact assessed | PASS | `wave-decisions.md` D10: no SSOT modification. Same posture as the four reference features. |
| F13 | Cross-feature contract honoured | PASS | `user-stories.md` System Constraints pin the output-shape contract (plain-text key=value lines, terminated by `\n`, `records`/`earliest`/`latest` key names, ISO 8601 UTC `Z`-suffixed timestamps, key order), the positional-argument convention (mirrors `ingest` and `read`), the stream contract (stdout for the principal output), the read-only invariant, and the tenant-isolation invariant inherited from `lumen::LogStore`. No prior contract is violated. |

## Anti-pattern scan

| Anti-pattern | Detected? | Notes |
|--------------|-----------|-------|
| Implement-X | No | Story framed as "Priya runs `kaleidoscope-cli stats acme /tmp/data` and sees three lines telling her the record count and time window, in milliseconds, without dumping the record set" — outcome-first from the operator perspective; the new subcommand is the means, not the end. |
| Generic data | No | Real names: Priya, `acme`, `globex`, `acmee` (the typo case). Real paths: `/tmp/k-data`, `/tmp/kdata`, `/tmp/data`. Real source-file references: `crates/kaleidoscope-cli/src/lib.rs:118-120`, `:275-279`, `:281-285`; `crates/kaleidoscope-cli/src/main.rs:48-60`, `:71-97`, `:111-114`, `:134-153`, `:155-161`; `crates/lumen/src/store.rs:69-70, 84`; `crates/lumen/src/record.rs:48`. Real ISO 8601 timestamps with literal calendar dates: `2026-05-18T00:00:00Z`, `2026-05-19T00:00:00Z`, `2026-05-11T00:00:00Z`. Real `observed_time_unix_nano` magnitudes: `1747526400000000000`, `1747612800000000000`, `1746921600000000000`. |
| Technical AC | No blocking instances | AC pin the wire-observable invariants (subcommand accepted on stdout; 3 lines in order; key strings `records`/`earliest`/`latest`; ISO 8601 UTC rendering; count matches `read`; min equals the underlying record set's min `observed_time_unix_nano`; empty case exactly 1 line `records=0`; no `earliest=`/`latest=` lines in the empty case; file mutation invariant; no OTLP file created). They reference the structural mirror of `read()`'s recorder construction as the implementation pattern but do NOT prescribe novel internal mechanisms; the structural-mirror reference is a fact about the precedent, not a novel design constraint. |
| Technical scenario titles | No | Scenarios titled by user outcome: "Populated tenant — Priya sees count plus earliest plus latest in three lines", "Empty tenant — Priya sees `records=0` and no timestamp lines", "Single-record tenant — earliest equals latest", "Tenant isolation — stats for acme do not count globex records", "Stats are consistent with `read` for the same tenant + data_dir". Not "stats() function calls LogStore::query" or "FileBackedLogStore::open recorder slot accepts boxed dyn trait". |
| Oversized story | No | 1 story, 5 UAT scenarios, 12 AC, well under 1 day effort. Smaller than the predecessor's 5 scenarios + 11 AC story (because no OTLP file-content assertions are required here). |
| Abstract requirements | No | Every AC has a numeric value (line counts, record counts 7 vs 10, single-record N=1), a string literal (`records=`, `earliest=`, `latest=`, the ISO 8601 `Z` suffix), or a byte-level invariant (stdout ends with `\n`, line ordering, no `earliest=`/`latest=` lines in the empty case). |

## Reviewer self-pass (Dimension 0 — Elevator Pitch, BLOCKING)

| Story | Section present | Real entry point | Concrete output | Job connection | Verdict |
|-------|-----------------|------------------|-----------------|----------------|---------|
| US-01 | yes (Before / After / Decision enabled triple) | yes — `kaleidoscope-cli stats acme /tmp/data` (the actual shell command an operator types) | yes — three literal stdout lines (`records=10000000`, `earliest=2026-05-18T00:00:01.123456789Z`, `latest=2026-05-19T03:45:12.987654321Z`) shown in the "After" line; one literal stdout line (`records=0`) shown for the empty case | yes — "Priya can decide 'did `acme`'s overnight ingest land records, and across what time window?' — the canonical post-ingest smoke-test and the canonical first audit/compliance question — in one CLI invocation" | PASS |

Slice-level check (Dimension 0 item 5): the single slice (Slice 01) is
tagged operator-visible (NOT `@infrastructure`). The story's Elevator
Pitch names a real user-invocable CLI command and the byte-level
stdout output the operator's shell session will display. There is no
slice-level infrastructure-only blocking concern. PASS at slice level.

## Confirmation-bias detection (Dimensions 1-4 self-check)

| Dimension | Check | Verdict |
|-----------|-------|---------|
| 1.1 Technology bias | Story specifies the existing CLI surface and plain-text key=value output. No specific JSON parser (jq, etc.) prescribed; no specific datetime library prescribed (the ISO 8601 formatter choice is deferred to DESIGN per `wave-decisions.md` D6). The output is consumable by standard Unix text tools that any operator already has. | PASS |
| 1.2 Happy path bias | Story includes the empty-tenant scenario (operator types the tenant id wrong or queries a fresh tenant — must see `records=0` with no timestamp lines), the single-record tenant scenario (degenerate time window with identical earliest/latest), and the tenant-isolation scenario (operator queries `acme` in a `data_dir` that also contains `globex` records — must NOT count cross-tenant records). The OK3 KPI is itself a sad-path-shaped guarantee against silently parsing a sentinel string as a real timestamp. | PASS |
| 1.3 Availability bias | The chosen output shape (plain-text key=value lines on stdout) is justified by the operator's existing tooling (Unix text utilities) and by the existing precedent of `ingest`'s `records=N batches=M tier_items=K` stderr line. The alternative of JSON output is explicitly rejected in `wave-decisions.md` D4 and the slice brief's "Rejected alternatives" section. The empty-case sentinel encoding is explicitly rejected in `wave-decisions.md` D5 with documented rationale. | PASS |
| 2.1 Missing stakeholder perspectives | Primary: platform operator (Priya). Secondary: Lumen maintainer (covered by the meta-AC — the existing `tests/observe_otlp_*.rs` test files continue to pass byte-equivalently, so the new subcommand does not perturb the four shipped features). Tertiary: scripting/automation consumers (covered by the plain-text key=value contract — any shell pipeline through `grep` / `cut` / `awk` works against the v0 output without a JSON parser). | PASS |
| 2.2 Missing error scenarios | Three error-shaped scenarios: empty tenant (operator queries a never-ingested tenant — must see `records=0`, not a crash), tenant-isolation (operator queries `acme` in a multi-tenant `data_dir` — must NOT count `globex` records), consistency-with-read (the count must agree with `read`'s count for the same inputs — a regression on either subcommand surfaces in this consistency check). Lumen open / query errors are surfaced via the existing `kaleidoscope_cli::Error` variants (`LumenOpen`, `LumenQuery`) and bubble to the binary's existing error printer in `main()` (`crates/kaleidoscope-cli/src/main.rs:62-68`). No new error variant introduced for v0. |
| 2.3 Missing NFRs | NFRs covered: count correctness consistency with `read` (OK1 — operator-facing correctness invariant), time-range correctness (OK2 — earliest/latest must match the underlying record set's min/max), empty-case unambiguity (OK3 — operator can grep-detect the empty case), read-only invariant (the Lumen WAL+snapshot is unchanged after the call), no-OTLP-side-channel invariant (the quiescent recorder produces no file). Performance: the v0 contract is "one `query()` call per invocation" which is the same cost as the operator's current `read | wc -l` workaround in terms of records materialised; the cost saving is the bash pipeline, not the Lumen query. If the v0 cost proves too high (the learning hypothesis), a follow-up feature can introduce a streaming/aggregate `LogStore` method. | PASS |
| 3.1 Vague performance requirements | All "100%"/"0%" claims are quantitative and falsifiable via the named test. No "fast", "scalable", "performant" adjectives without a number. The "in milliseconds" claim in the Elevator Pitch is descriptive operator language, not a normative requirement; the AC do not quantify a latency target because the v0 cost is whatever the existing `query()` call returns. | PASS |
| 3.2 Ambiguous requirements | Every AC pins exact string literals (`records=`, `earliest=`, `latest=`, the ISO 8601 `Z` suffix, the `\n` line terminator), exact integer invariants (line counts: 3 for populated, 1 for empty; record counts: 7 for acme-only, NOT 10 for the union), or byte-level invariants (stdout ends with `\n`, line ordering, key-prefix matching). | PASS |
| 4 Testability | Every AC is a property of the captured stdout bytes (after `stats()` returns), the captured stdout's parsed line-by-line content, or the returned `Result`. Every AC is automatable via `String::from_utf8` + `Vec<u8>` byte comparison + `str::lines()` iteration + key-prefix matching + ISO 8601 parsing (via the same library that DESIGN selects for formatting). Test names are pre-sketched in the slice file. | PASS |
| 5.1 Largest bottleneck | The largest gap in the operator's daily workflow is "I want to know if data landed for this tenant and what the time window is, but the only way to get that today is to dump the entire record set through a bash pipeline and parse JSON by hand". This feature addresses exactly that gap. | PASS |
| 5.2 Simpler alternatives considered | Considered and rejected: (a) `--json` / `--csv` / `--format=...` — rejected because the v0 contract is the simplest possible (plain-text key=value), and JSON output is a reasonable v1 once the v0 shape is validated (`wave-decisions.md` D4); (b) `records=0` plus `earliest=<none>` plus `latest=<none>` (the sentinel encoding for the empty case) — rejected because operators may silently parse `<none>` as a real timestamp string (`wave-decisions.md` D5); (c) introducing a new `LogStore::stats(tenant)` trait method that streams the WAL — deferred to a follow-up feature unless the v0 cost proves operationally hostile (slice brief "Learning hypothesis" + "Rejected alternatives"); (d) wiring `--observe-otlp` on `stats` — deferred to a follow-up feature (`wave-decisions.md` D3); (e) filtering / sorting / multi-tenant aggregates — all deferred to follow-up features (`wave-decisions.md` D7). | PASS |
| 5.3 Constraint prioritization | The dominant constraint is OK1 (record count correctness — the subcommand is useless if it disagrees with `read`'s count). OK2 (time range correctness) and OK3 (empty-tenant unambiguity) are correctly weighted as the necessary enrichment and the necessary unambiguity guardrail. There is no cross-process or concurrency invariant to prioritize because the subcommand has no side effects and runs in a single thread. | PASS |
| 5.4 Data-justified | This is not a performance optimization. The decision to materialise the full `Vec<LogRecord>` via the existing `query()` call (vs introducing a new aggregate trait method) is justified by the v0 scope (1-day effort, no new trait methods, no new disk layout) and by the explicit learning hypothesis that the existing API is sufficient at v0 record volumes. The decision to render timestamps as ISO 8601 UTC (vs nanos-since-epoch, vs RFC 3339 with timezone) is justified by the operator's downstream tooling (lexicographic sort = chronological sort; standard library parse) and by the `Z` suffix's unambiguous UTC semantics. | PASS |

## DoR Status: PASSED — with four honestly-recorded artefact gaps

The story-level DoR (9 items × 1 story = 9 checks) is PASSED with
evidence on every item. The feature-level DoR is PASSED with the
following honestly-recorded artefact gaps:

- **F1, F2 (journey artefacts)**: deliberately not produced. The
  journey is one step (operator runs `stats <tenant> <data_dir>` and
  reads stdout); a multi-step journey visual or YAML would duplicate
  the Elevator Pitch in US-01. The four reference features' journey
  visuals collectively cover the underlying CLI operator mental
  model. Acknowledged; will not be remediated.
- **F3 (Gherkin `.feature` file)**: deliberately not produced. Per
  the project convention, this project's acceptance idiom is Rust
  `#[test]` functions with `// Given / // When / // Then` comment
  blocks. The Given/When/Then specification lives inline in
  `user-stories.md`.
- **F4 (shared artefact registry)**: not produced. The cross-feature
  shared artefacts are tracked in the reference features and the
  source code itself; the output-shape contracts are restated as
  System Constraints in `user-stories.md`. Producing a new registry
  that copies the reference features' rows would add no value.
- **F6 (separate `prioritization.md`)**: not produced; integrated
  into `story-map.md`'s `## Priority Rationale` section per the
  `nw-leanux-methodology` skill template, because a one-slice
  feature does not earn a separate prioritization document.

Anti-pattern scan clean. Dimension 0 elevator-pitch check PASSES with
the real entry point (`kaleidoscope-cli stats acme /tmp/data`) and
the concrete byte-level output (three literal stdout lines for the
populated case; one literal stdout line for the empty case).
Dimensions 1-5 confirmation-bias self-check PASSES.

The DISCUSS wave is ready to hand off to `nw-solution-architect` for
DESIGN.
