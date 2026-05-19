# Definition of Ready — Validation

Feature: `cli-read-time-range-v0`
Reviewer: Luna (self-review at DISCUSS close; second pass scheduled at
handoff to DESIGN per `nw-po-review-dimensions` skill).
Date: 2026-05-19.

## Per-story DoR (9-item hard gate)

### US-01: Operator queries a tenant's records for a bounded time window

| # | DoR Item | Status | Evidence |
|---|----------|--------|----------|
| 1 | Problem statement clear, domain language | PASS | "Today the `read` subcommand cannot answer either question directly. The library function `kaleidoscope_cli::read` at `crates/kaleidoscope-cli/src/lib.rs:261-294` always calls `lumen.query(tenant, TimeRange::all())` (line 284), which returns every record the tenant has ever ingested. For a tenant with ten gigabytes of NDJSON, the only ways to extract the incident window are (1) stream the full dump and pipe through `jq`, or (2) write a one-off Rust binary." Uses Lumen, TimeRange, NDJSON, `observed_time_unix_nano`, `jq`, ISO 8601 UTC, half-open interval, tenant — all domain terms grounded in the existing CLI implementation, the lumen crate's `TimeRange` doc-comment, and the prior four `kaleidoscope-cli` features' shipped behaviour. |
| 2 | User/persona identified | PASS | Priya the platform operator, multi-tenant Kaleidoscope deployment for a fintech, already uses `kaleidoscope-cli read <tenant> <data_dir>` for full-tenant dumps and `... --observe-otlp <path>` for ingest-side observability. Now needs time-bounded queries for per-incident response. Same persona as the four reference features. |
| 3 | 3+ domain examples with real data | PASS | Three examples: (1) Yesterday's incident window with both flags set — `acme` / `--since 2026-05-18T14:00:00Z --until 2026-05-18T14:30:00Z` / parsed `since_ns = 1_747_578_000_000_000_000`, `until_ns = 1_747_579_800_000_000_000`. (2) Last 90 minutes with only `--since` set — `--since 2026-05-19T15:30:00Z` / `since_ns = 1_747_668_600_000_000_000`, plus the symmetric `--until`-only example. (3) Typo on `--since` mid-incident — `--since yesterday` / stderr message naming `--since` and the verbatim bad value, exit code 1. Real values throughout: `acme`, `/tmp/data`, real ISO 8601 timestamps, real nanosecond literals, real CLI argv lists. |
| 4 | UAT scenarios (3-7 in Given/When/Then) | PASS | 7 scenarios: bounded window (OK1), no-flag byte equivalence (OK2), `--since`-only (OK3a), `--until`-only (OK3b), invalid `--since` (OK4a), invalid `--until` (OK4b), existing-locked-tests-continue-to-pass meta-scenario. |
| 5 | AC derived from UAT | PASS | 11 AC bullets, each maps to an observable invariant from a scenario (TimeRange-driving control on `read`, half-open `[since, until)` contract, no-flag byte equivalence, two-flag parsing in `main.rs`, half-bounded defaults, ISO 8601 accepted shape, round-trip property, fail-fast contract for `--since`, fail-fast contract for `--until`, `print_usage` doc update, locked-test non-regression meta-AC). |
| 6 | Right-sized | PASS | 1 story, 1 TimeRange-driving control change on `read`, 2 new flag-parse helpers in `main.rs`, 1 new hand-rolled ISO 8601 UTC parser, 1 new test file with 6 test functions, <= 1 day effort. Comparable in size to the predecessor `cli-read-observe-otlp-v0` (the parser-side work roughly balances the absence of cross-writer concurrency work). |
| 7 | Technical notes identify constraints | PASS | File paths, line numbers, manifest entry, dependency posture (no new external crate; hand-rolled parser per D4 in `wave-decisions.md`), explicit non-prescription of internal implementation details (DESIGN-owned choice for the exact signature shape on `read`), explicit half-open `[start, end)` semantics inherited from `lumen::TimeRange`, explicit no-modification list for the locked test files and the lumen `TimeRange` type. |
| 8 | Dependencies tracked | PASS | `lumen::TimeRange` already exists with the correct semantics (`crates/lumen/src/record.rs:97-120`); no change to the `lumen` crate required. `kaleidoscope_cli::read` already opens Lumen and calls `query` with a `TimeRange`; change is at the construction site. Existing hand-rolled `format_iso8601_utc_nanos` formatter is the precedent for the parser direction. `aegis::TenantId` already a dependency. `serde_json` already a dev-dependency. No unresolved external dependencies. |
| 9 | Outcome KPIs defined with measurable targets | PASS | Maps to OK1-bounded-window-filter (principal, 100% of records on stdout satisfy `[since_ns, until_ns)`), OK2-no-flag-byte-equivalent (100% byte equivalence of stdout when neither flag set, locked tests pass green), OK3-half-bounded-supported (100% of `--since`-only / `--until`-only invocations return the unbounded-side records correctly), OK4-invalid-iso8601-fails-fast (100% of invocations with invalid input exit non-zero with named flag in stderr). All four targets are quantitative and falsifiable via the named test file. |

### US-01 DoR Status: PASSED

## Feature-level DoR

| # | Item | Status | Evidence |
|---|------|--------|----------|
| F1 | Journey artefact (visual) | NOT PRODUCED — accepted | Per `wave-decisions.md`, the journey is so thin (one operator action: invoke `read` with two new optional flags) that a separate journey visual would duplicate the Elevator Pitch in US-01. The four reference features in this cluster collectively cover the underlying mental model. Acknowledged gap; will not be remediated. |
| F2 | Journey artefact (YAML schema) | NOT PRODUCED — accepted | Same rationale as F1. The journey schema in `nw-design-methodology` is for multi-step user journeys; this feature has one step (operator runs one `read` command with two new flags). |
| F3 | Journey artefact (Gherkin `.feature`) | INTENTIONALLY OMITTED | Per the project convention (inherited from `cli-read-observe-otlp-v0` and noted in the task brief): this project's acceptance idiom is Rust `#[test]` functions with `// Given / // When / // Then` comment blocks, not Gherkin `.feature` files. The Given/When/Then text lives inside `user-stories.md` (UAT Scenarios section). DISTILL writes the Rust tests. |
| F4 | Shared artefact registry | NOT PRODUCED — accepted | The cross-feature shared artefacts (`tenant_id`, the operator-supplied data directory path, the NDJSON-on-stdout serialisation contract) are tracked in the reference features' registries and reused unchanged here. The new shared artefacts in this feature are: the ISO 8601 UTC text format (already pinned by the existing `format_iso8601_utc_nanos` formatter, which the new parser is the inverse of), the half-open `[start, end)` interval contract (already pinned by `lumen::TimeRange` at `crates/lumen/src/record.rs:97-120`), and the named-flag fail-fast error format (new; pinned inline in this feature's System Constraints in `user-stories.md`). Producing a separate registry would copy material already pinned in source. |
| F5 | Story map | PASS | `discuss/story-map.md` with one-activity backbone, walking-skeleton justification (N/A — the substrate already exists), single-slice rationale, cross-feature alignment, scope assessment. |
| F6 | Prioritization | INTEGRATED into `story-map.md` | The single-slice structure renders a separate `prioritization.md` redundant. The `## Priority Rationale` section in `story-map.md` covers the ordering and partial-ship logic. |
| F7 | Outcome KPIs | PASS | `discuss/outcome-kpis.md` with OK1 (principal), OK2 (guardrail), OK3 (half-bounded), OK4 (fail-fast), each with quantitative target, baseline, and measurement plan. |
| F8 | User stories | PASS | `discuss/user-stories.md` with US-01, including complete Elevator Pitch with the real entry-point shell command (`kaleidoscope-cli read acme /tmp/data --since 2026-05-18T00:00:00Z --until 2026-05-19T00:00:00Z`) and the concrete byte-level output (the half-open `[since_ns, until_ns)` filtered NDJSON stream, with the exact nanosecond literals computed from the ISO 8601 values shown in Domain Example 1). |
| F9 | Per-slice files | PASS | 1 file under `discuss/slices/`. |
| F10 | Wave decisions | PASS | `discuss/wave-decisions.md` with D1-D9 covering both-flags-optional (D1), interval semantics (D2), accepted ISO 8601 shape (D3), hand-rolled-parser posture (D4), out-of-scope items (D5, D6, D8), test file shape (D7), SSOT non-modification (D9). |
| F11 | Right-sized (Elephant Carpaccio) | PASS | 1 story, 1 bounded context (`kaleidoscope-cli` crate), 2 modified files + 1 new test file + 1 manifest line-level change. Estimated effort: <= 1 day. `story-map.md` "Scope Assessment: PASS". |
| F12 | SSOT impact assessed | PASS | `wave-decisions.md` D9: no SSOT modification. Same posture as the four reference features. |
| F13 | Cross-feature contract honoured | PASS | `user-stories.md` System Constraints pin the half-open `[start, end)` semantics, the half-bounded defaults (`u64::MAX` upper, `0` lower), the ISO 8601 UTC accepted shape (`Z` suffix only, 0..=9 fractional-second digits), the no-flag default of `TimeRange::all()`, the round-trip property with the existing formatter, the order-independent flag parsing, and the no-flag non-regression invariant. The lumen `TimeRange` API contract from `crates/lumen/src/record.rs:97-120` is inherited unchanged. |

## Anti-pattern scan

| Anti-pattern | Detected? | Notes |
|--------------|-----------|-------|
| Implement-X | No | Story framed as "Priya runs `kaleidoscope-cli read acme /tmp/data --since X --until Y` and sees only yesterday's records on stdout" — outcome-first from the operator perspective; the parser and the TimeRange-driving wiring are the means, not the end. |
| Generic data | No | Real names: Priya, `acme`. Real paths: `/tmp/data`. Real ISO 8601 timestamps: `2026-05-18T00:00:00Z`, `2026-05-18T14:00:00Z`, `2026-05-18T14:30:00Z`, `2026-05-19T15:30:00Z`. Real nanosecond literals computed for those timestamps (e.g. `1_747_526_400_000_000_000` for the start of `2026-05-18T00:00:00Z`). Real witness `observed_time_unix_nano` values in the scenarios (`{100, 200, 300, 400, 500}` chosen to make boundary inclusion/exclusion testable and demonstrable). Real source-file references with line numbers: `crates/kaleidoscope-cli/src/lib.rs:261-294`, `:283-285`, `:410-420`, `:426-438`; `crates/kaleidoscope-cli/src/main.rs:130-144`, `:146-165`, `:155-165`; `crates/lumen/src/record.rs:97-120`, `:111-114`, `:116-119`. |
| Technical AC | No blocking instances | AC pin the observable invariants (which records appear on stdout, byte equivalence under the no-flag default, fail-fast exit code with the offending flag named in stderr, round-trip property with the existing formatter). The AC explicitly defers the exact signature shape on `read` to DESIGN ("the exact signature shape (a new parameter, a builder, an overload) is DESIGN's choice; the observable property is that the caller can drive any `TimeRange::new(s, e)` into the underlying `lumen.query` call"). |
| Technical scenario titles | No | Scenarios titled by user outcome: "Bounded window query returns only records in `[since, until)`", "No flags is byte-equivalent to today's full-tenant dump", "Invalid ISO 8601 on `--since` fails fast with stderr message". Not "ISO8601Parser returns ParseError" or "TimeRange::new called with parsed nanos". |
| Oversized story | No | 1 story, 7 UAT scenarios (within the 3-7 envelope), 11 AC, <= 1 day effort. Right-sized. |
| Abstract requirements | No | Every AC has a numeric value (witness `observed_time_unix_nano` values, boundary inclusion/exclusion claims), a string literal (flag names `--since` and `--until`, ISO 8601 shape `YYYY-MM-DDTHH:MM:SS[.NNNNNNNNN]Z`, the `Z` suffix), or a byte-level invariant (stdout bytes equal NDJSON re-serialisation, file ends with `\n`, exit code 1). |

## Reviewer self-pass (Dimension 0 — Elevator Pitch, BLOCKING)

| Story | Section present | Real entry point | Concrete output | Job connection | Verdict |
|-------|-----------------|------------------|-----------------|----------------|---------|
| US-01 | yes (Before / After / Decision enabled triple) | yes — `kaleidoscope-cli read acme /tmp/data --since 2026-05-18T00:00:00Z --until 2026-05-19T00:00:00Z` (the actual shell command an operator types) | yes — stdout contains ONLY records whose `observed_time_unix_nano` lies in the half-open interval `[1_747_526_400_000_000_000, 1_747_612_800_000_000_000)`, with the format on stdout unchanged (NDJSON, one record per line, terminated by `\n`). The Elevator Pitch shows the exact nanosecond literals computed from the ISO 8601 boundary values. | yes — "Priya can decide 'what did `acme` write between 14:00 and 14:30 UTC, in the half hour before the latency spike at 14:30?' — answered directly from the storage layer with one CLI invocation, without a multi-gigabyte stream-and-filter detour. Also: 'Replay yesterday's slice through the test pipeline'." | PASS |

Slice-level check (Dimension 0 item 5): the single slice (Slice 01) is
tagged operator-visible (NOT `@infrastructure`). The story's Elevator
Pitch names a real user-invocable CLI command and the byte-level output
(filtered NDJSON stream) the operator sees on stdout. There is no
slice-level infrastructure-only blocking concern. PASS at slice level.

## Confirmation-bias detection (Dimensions 1-4 self-check)

| Dimension | Check | Verdict |
|-----------|-------|---------|
| 1.1 Technology bias | Story specifies the existing CLI surface (`kaleidoscope-cli read`), the existing `lumen::TimeRange` data type, the existing ISO 8601 UTC format (already produced by the project's own `stats` subcommand). No specific date-time library prescribed (hand-rolled parser per D4 in `wave-decisions.md`; explicit rejection of `chrono` and `time`). No specific shell or terminal prescribed. | PASS |
| 1.2 Happy path bias | Story includes the no-flag byte-equivalence scenario (operator who omits the flags must see byte-equivalent behaviour), the half-bounded scenarios (one each for `--since`-only and `--until`-only), the invalid-input scenarios (one each for `--since` and `--until`), AND the locked-tests-pass meta-scenario. 7 scenarios total covering: 1 happy path with both flags, 1 happy path with no flags, 2 half-bounded happy paths, 2 sad paths, 1 meta. Sad paths are 2/7 = ~29% of the scenario surface; not happy-path-biased. The OK4 KPI is itself a sad-path-shaped guarantee. | PASS |
| 1.3 Availability bias | The chosen pattern (mirroring the existing `parse_observe_otlp` flag-parse helper for the new flags, and the inverse-of-existing-formatter posture for the parser) is explicitly justified by `wave-decisions.md` D4 (hand-rolled, no `chrono`/`time` dep) and by the structural parallel in `parse_observe_otlp` at `crates/kaleidoscope-cli/src/main.rs:130-144`. The alternative of pulling a dep is explicitly considered and rejected in D4 with reasons. | PASS |
| 2.1 Missing stakeholder perspectives | Primary: platform operator (Priya). Secondary: Lumen maintainer (covered by the no-modification-to-`lumen::TimeRange` claim — the storage layer's contract is honoured, not extended). Tertiary: existing CLI test maintainers (covered by the meta-AC — the existing `observe_otlp_read_flag.rs` and `observe_otlp_flag.rs` test files continue to pass byte-equivalently). | PASS |
| 2.2 Missing error scenarios | Two error-shaped scenarios (one per flag, OK4) plus the no-flag quiescence scenario (operator forgot the flags — must not silently change behaviour). The CLI failure mode is exit code 1 with the offending flag named in stderr. Out-of-range calendar components (e.g. month 13, day 32, hour 25) are covered by the OK4b witness (`2026-13-32T25:99:99Z`); the parser must reject these because the underlying `civil_from_days`-inverse arithmetic would otherwise produce a wrong nanosecond value silently. | PASS |
| 2.3 Missing NFRs | NFRs covered: stdout byte equivalence under the no-flag default (OK2), half-open boundary correctness (OK1, exercised by witness records at exactly the boundary values `200` and `400`), half-bounded correctness (OK3), fail-fast invariant (OK4 — no Lumen store opened on invalid input, no bytes written to stdout). Round-trip property with the existing formatter is an explicit AC. Hand-rolled parser posture (D4) is justified, including the explicit rejection of external deps. Thread-safety and concurrency are N/A (single-threaded function, no new concurrency introduced). | PASS |
| 3.1 Vague performance requirements | All "100%"/"0%" claims are quantitative and falsifiable via the named test. No "fast", "scalable", "performant" adjectives without a number. The "10 gigabytes of NDJSON" figure in the problem statement is a realistic-magnitude framing of the existing pain, not a performance target for the new feature. | PASS |
| 3.2 Ambiguous requirements | Every AC pins exact string literals (`--since`, `--until`, the ISO 8601 shape `YYYY-MM-DDTHH:MM:SS[.NNNNNNNNN]Z`, the `Z` suffix), exact integer invariants (`since_ns`, `until_ns`, exit code 1, the witness `observed_time_unix_nano` values), exact byte invariants (stdout bytes equal NDJSON re-serialisation, file ends with `\n`), or exact set memberships (the record at exactly `until_ns` is EXCLUDED; the record at exactly `since_ns` is INCLUDED). | PASS |
| 4 Testability | Every AC is a property of stdout bytes (after `read()` returns), the function's return value, the stderr text (for OK4), or the process exit code (for OK4). Every AC is automatable via `serde_json::from_str` + byte comparison + `Result::is_err()` + substring search on the error message. Test names are pre-sketched in the slice file. | PASS |
| 5.1 Largest bottleneck | The largest gap in the operator's per-incident query workflow is "the CLI can only dump the full tenant, so getting yesterday's window requires streaming ten gigabytes and filtering client-side". This feature addresses exactly that gap. | PASS |
| 5.2 Simpler alternatives considered | Considered and rejected: (a) Unix-timestamp flag values (integer seconds or nanoseconds) — rejected because operators think in ISO 8601 (the project's own `stats` subcommand emits ISO 8601 with `Z` suffix); ISO 8601 input matches their mental model. (b) A single `--window <ISO>:<ISO>` flag — rejected because half-bounded forms become awkward and the colon collides with ISO 8601's own colon character. (c) Pulling `chrono` or `time` for parsing — rejected per D4 with explicit dependency-tree-size and unwanted-variant arguments. (d) Adding the flags to other subcommands too (`ingest`, `stats`) — rejected as out-of-scope; only `read` has the bottleneck this feature addresses. | PASS |
| 5.3 Constraint prioritization | The dominant constraint is OK1 (bounded-window correctness, including the boundary inclusion/exclusion behaviour). OK2 (no-flag guardrail), OK3 (half-bounded), OK4 (fail-fast) are correctly weighted as the necessary guardrail, the necessary common-shape support, and the necessary failure-mode contract. | PASS |
| 5.4 Data-justified | This is not a performance optimization (no profiling data needed). The decision to extend the existing `read` subcommand with two new optional flags is justified by the operator's existing muscle memory (the `parse_observe_otlp` precedent at `crates/kaleidoscope-cli/src/main.rs:130-144` makes the new flags' parsing posture obvious) and by the storage layer's existing `TimeRange::new(s, e)` entry point (no new storage-layer API surface needed). | PASS |

## DoR Status: PASSED — with four honestly-recorded artefact gaps

The story-level DoR (9 items × 1 story = 9 checks) is PASSED with
evidence on every item. The feature-level DoR is PASSED with the
following honestly-recorded artefact gaps:

- **F1, F2 (journey artefacts)**: deliberately not produced. The
  journey is one step (operator runs `read` with two new optional
  flags, sees a bounded NDJSON slice on stdout); a multi-step
  journey visual or YAML would duplicate the Elevator Pitch in
  US-01. The four reference features' journey visuals collectively
  cover the underlying mental model. Acknowledged; will not be
  remediated.
- **F3 (Gherkin `.feature` file)**: deliberately not produced. Per
  the project convention, this project's acceptance idiom is Rust
  `#[test]` functions with `// Given / // When / // Then` comment
  blocks. The Given/When/Then specification lives inline in
  `user-stories.md`.
- **F4 (shared artefact registry)**: not produced. The cross-feature
  shared artefacts are tracked in the reference features' registries
  and reused unchanged; the new shared artefacts (ISO 8601 text
  format, half-open interval, named-flag error format) are pinned
  inline in `user-stories.md` System Constraints. Producing a new
  registry that copies the reference features' rows would add no
  value.
- **F6 (separate `prioritization.md`)**: not produced; integrated
  into `story-map.md`'s `## Priority Rationale` section per the
  `nw-leanux-methodology` skill template, because a one-slice
  feature does not earn a separate prioritization document.

Anti-pattern scan clean. Dimension 0 elevator-pitch check PASSES
with the real entry point (`kaleidoscope-cli read acme /tmp/data
--since 2026-05-18T00:00:00Z --until 2026-05-19T00:00:00Z`) and the
concrete byte-level output (the half-open `[since_ns, until_ns)`
filtered NDJSON stream, with exact nanosecond literals computed in
Domain Example 1). Dimensions 1-5 confirmation-bias self-check
PASSES.

The DISCUSS wave is ready to hand off to `nw-solution-architect`
for DESIGN.
