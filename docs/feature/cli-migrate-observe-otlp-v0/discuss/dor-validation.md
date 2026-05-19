# Definition of Ready — Validation

Feature: `cli-migrate-observe-otlp-v0`
Reviewer: Luna (self-review at DISCUSS close; second pass scheduled at
handoff to DESIGN per `nw-po-review-dimensions` skill).
Date: 2026-05-19.

## Per-story DoR (9-item hard gate)

### US-01: Manual tier migrations are visible on the operator's OTLP audit stream

| # | DoR Item | Status | Evidence |
|---|----------|--------|----------|
| 1 | Problem statement clear, domain language | PASS | "State-mutating operator actions need an audit trail. Today `migrate` is fire-and-forget — once the operator runs it, there's no record beyond the operator's shell history." Uses Cinder, NDJSON, OTLP, sidecar, collector, `tail -f`, audit trail — all domain terms grounded in the existing CLI implementation and the prior `--observe-otlp` and `migrate` subcommand features' shipped behaviour. |
| 2 | User/persona identified | PASS | Priya the platform operator, multi-tenant Kaleidoscope deployment for a fintech, already familiar with `kaleidoscope-cli migrate ...` and with `--observe-otlp` on `ingest`. Same persona as the sibling features. |
| 3 | 3+ domain examples with real data | PASS | Four examples: happy path with `acme` / `acme/batch-00042` / `hot → cold` / shown byte-for-byte; no-flag byte-equivalence with `acme/batch-00007` / `hot → hot` (idempotent); unknown-item error path with `ghost-item`; invalid-tier error path with `LUKEWARM`. Real values throughout: `acme`, `/tmp/data`, `/tmp/audit.ndjson`, `acme/batch-00042`, `acme/batch-00007`, real source-line references. |
| 4 | UAT scenarios (3-7 in Given/When/Then) | PASS | 5 scenarios: happy-path-emission, no-flag-byte-equivalence, existing-tests-pass-byte-equivalently meta-scenario, unknown-item-no-emission, invalid-tier-no-file. |
| 5 | AC derived from UAT | PASS | 11 AC bullets, each maps to an observable byte-level invariant from a scenario (JSON-parseable, exact metric-name string literal, exact `asInt` string literal, point-attribute presence with exact string-value, exact stdout byte sequence, file-existence boolean, exit-code boolean). |
| 6 | Right-sized | PASS | 1 story, 1 wiring change (one match-arm-equivalent substitution at `crates/kaleidoscope-cli/src/lib.rs:434` plus the parameter thread-through), 1 new test file, well under 1 day effort. Strictly smaller than `cli-cinder-otlp-wiring-v0` (no cross-writer concurrency to solve). |
| 7 | Technical notes identify constraints | PASS | File paths, line numbers, manifest entry, dependency posture (no new external crate; `self-observe` already re-exports `CinderToOtlpJsonWriter`), recorder-construction-site contract (D-RecorderConstruction), the deliberate non-prescription of the exact `OpenOptions` shape (DESIGN-owned choice within the store-open-time pattern). |
| 8 | Dependencies tracked | PASS | `cli-migrate-subcommand-v0` shipped (`migrate` subcommand and library function exist; pre-flight `get_entry` and `parse_tier` contracts inherited unchanged); `cinder-to-otlp-json-bridge-v0` shipped (`CinderToOtlpJsonWriter::record_migrate` and `cinder.migrate.count` wire contract); `cli-cinder-otlp-wiring-v0` shipped (precedent for store-open-time recorder construction on the `ingest` path); `aegis` already a `kaleidoscope-cli` dependency. No unresolved external dependencies. |
| 9 | Outcome KPIs defined with measurable targets | PASS | Maps to OK1-CLI-migrate-line-shape (principal, 100% of successful migrates produce one line with full wire shape), OK2-CLI-no-flag-byte-equiv (100% pass of locked `migrate_subcommand.rs` test file; 0% files created on no-flag invocations), OK3-CLI-unknown-item-no-emission (0% `cinder.migrate.count` lines on `UnknownItem` path), OK4-CLI-invalid-tier-no-file (0% sink files created on `InvalidTier` path). All four targets are quantitative and falsifiable via the named test files. |

**US-01 DoR Status: PASSED**

## Feature-level DoR

| # | Item | Status | Evidence |
|---|------|--------|----------|
| F1 | Journey artefact (visual) | NOT PRODUCED — accepted | Per `wave-decisions.md` (D2), the journey is so thin (one operator action: run the existing command with one more flag) that a separate journey visual would duplicate the Elevator Pitch in US-01. The sibling feature `cli-cinder-otlp-wiring-v0` also did not produce a journey visual for the same reason. Acknowledged gap; will not be remediated. |
| F2 | Journey artefact (YAML schema) | NOT PRODUCED — accepted | Same rationale as F1. The journey schema in `nw-design-methodology` is for multi-step user journeys; this feature has one step. |
| F3 | Journey artefact (Gherkin `.feature`) | INTENTIONALLY OMITTED | Per the task brief: this project's acceptance idiom is Rust `#[test]` functions with `// Given / // When / // Then` comment blocks, not Gherkin `.feature` files. The Given/When/Then text lives inside `user-stories.md` (UAT Scenarios sections). DISTILL writes the Rust tests. |
| F4 | Shared artefact registry | NOT PRODUCED — accepted | The cross-feature shared artefacts (`tenant_id`, the `--observe-otlp` file path, the `kaleidoscope.cinder` scope name, the metric-name string `cinder.migrate.count`, the lowercase tier strings) are tracked in the reference features' registries. This feature consumes them unchanged; producing a new registry would duplicate without adding value. The cross-bridge metric-name contract from ADR-0039 §2 is recorded as a System Constraint in `user-stories.md`. |
| F5 | Story map | PASS | `discuss/story-map.md` with one-activity backbone, walking-skeleton justification (N/A — the substrate already exists), single-slice rationale, cross-bridge alignment, scope assessment. |
| F6 | Prioritization | INTEGRATED into `story-map.md` | The single-slice structure renders a separate `prioritization.md` redundant. The `## Priority Rationale` section in `story-map.md` covers the ordering and partial-ship logic. |
| F7 | Outcome KPIs | PASS | `discuss/outcome-kpis.md` with OK1 (principal), OK2 (no-flag byte-equivalence guardrail), OK3 (unknown-item emission absence), OK4 (invalid-tier file absence), each with quantitative target, baseline, and measurement plan. |
| F8 | User stories | PASS | `discuss/user-stories.md` with US-01, including complete Elevator Pitch with the real entry-point shell command (`kaleidoscope-cli migrate acme /tmp/data acme/batch-00042 cold --observe-otlp /tmp/audit.ndjson`) and the concrete byte-level output. |
| F9 | Per-slice files | PASS | 1 file under `discuss/slices/` (`slice-01-migrate-observe-otlp.md`, well under 100 lines). |
| F10 | Wave decisions | PASS | `discuss/wave-decisions.md` with D-RecorderConstruction and D1-D8 covering scope, flag posture, out-of-scope locks (bulk migrate, `--dry-run`, JSON output, observe-otlp on from-tier read), and SSOT non-modification. |
| F11 | Right-sized (Elephant Carpaccio) | PASS | 1 story, 1 bounded context (`kaleidoscope-cli` crate), 1 modified file at the library + 1 file touched at the binary + 1 new test file + 1 manifest line-level change. Estimated effort: well under 1 day. `story-map.md` "Scope Assessment: PASS". |
| F12 | SSOT impact assessed | PASS | `wave-decisions.md` D8: no SSOT modification. Same posture as the reference features. |
| F13 | Cross-bridge contract honoured | PASS | `user-stories.md` System Constraints cite ADR-0039 §2 (cross-bridge metric-name contract for `cinder.migrate.count` and the `{tenant_id, from, to}` point-attribute shape) explicitly. The metric name `cinder.migrate.count` and the scope name `kaleidoscope.cinder` are pinned in AC by exact string literal. |

## Anti-pattern scan

| Anti-pattern | Detected? | Notes |
|--------------|-----------|-------|
| Implement-X | No | Story framed as "Priya runs `kaleidoscope-cli migrate acme /tmp/data acme/batch-00042 cold --observe-otlp /tmp/audit.ndjson` and sees one new audit line carrying `tenant_id`, `from`, `to`" — outcome-first from the operator perspective; the recorder substitution is the means, not the end. |
| Generic data | No | Real names: Priya, `acme`, `globex` (in tenant-isolation parallel from sibling). Real paths: `/tmp/data`, `/tmp/audit.ndjson`. Real item ids: `acme/batch-00042`, `acme/batch-00007`, `ghost-item`. Real source-file references: `crates/kaleidoscope-cli/src/lib.rs:424-456`, `:434`, `:431`, `:155-184`, `:272-281`, `:161-175`. Real metric name: `cinder.migrate.count`. Real scope name: `kaleidoscope.cinder`. Real invalid value: `LUKEWARM`. |
| Technical AC | No blocking instances | AC pin the wire-observable invariants (metric name string, scope name string, `asInt` string value, point-attribute key/value pairs, exact stdout byte sequence, file-existence boolean). They do NOT prescribe the exact `OpenOptions` shape, the exact box type, or the exact code path for the conditional branch; those are explicitly DESIGN-owned per `wave-decisions.md`. |
| Technical scenario titles | No | Scenarios titled by user outcome: "Manual migration with `--observe-otlp` emits one `cinder.migrate.count` line", "Migration with `--observe-otlp` absent is byte-equivalent to today", "Existing `migrate_subcommand.rs` tests pass byte-equivalently", "Unknown item with `--observe-otlp` set leaves no `cinder.migrate.count` line", "Invalid tier with `--observe-otlp` set creates no OTLP file". Not "migrate() function constructs CinderToOtlpJsonWriter" or "Box dyn cinder::MetricsRecorder coerces from struct". |
| Oversized story | No | 1 story, 5 UAT scenarios, 11 AC, well under 1 day effort. |
| Abstract requirements | No | Every AC has a numeric value (line counts, `asInt` string values), a string literal (metric names, scope names, tier values, item ids), a boolean (file-exists, exit-code-success), or a byte-level invariant (file ends with `\n`, line parses as JSON, stdout is exactly the given bytes). |

## Reviewer self-pass (Dimension 0 — Elevator Pitch, BLOCKING)

| Story | Section present | Real entry point | Concrete output | Job connection | Verdict |
|-------|-----------------|------------------|-----------------|----------------|---------|
| US-01 | yes (Before / After / Decision enabled triple) | yes — `kaleidoscope-cli migrate acme /tmp/data acme/batch-00042 cold --observe-otlp /tmp/audit.ndjson` (the actual shell command an operator types) | yes — stdout is the existing `migrated tenant=acme item=acme/batch-00042 from=hot to=cold` line byte-for-byte, and the sink file gains a byte-level JSON sample shown in Domain Example 1 | yes — "Priya can answer 'who moved `acme/batch-00042` to Cold yesterday, and from what?' by querying the collector for `cinder.migrate.count` lines with `tenant_id='acme'` and `to='cold'`" | PASS |

Slice-level check (Dimension 0 item 5): the single slice (Slice 01) is
tagged operator-visible (NOT `@infrastructure`). The story's Elevator
Pitch names a real user-invocable CLI command and the byte-level output
the operator's sidecar will consume. There is no slice-level
infrastructure-only blocking concern. **PASS at slice level.**

## Confirmation-bias detection (Dimensions 1-4 self-check)

| Dimension | Check | Verdict |
|-----------|-------|---------|
| 1.1 Technology bias | Stories specify the existing CLI surface and the OTLP-JSON NDJSON wire format. No specific collector technology (Datadog, Prometheus, NewRelic) prescribed. The sidecar and collector are mentioned as already-deployed components the operator chose. | PASS |
| 1.2 Happy path bias | Story includes the no-flag byte-equivalence scenario, the unknown-item error scenario, and the invalid-tier error scenario. The OK3 and OK4 KPIs are sad-path-shaped guarantees: "error paths do NOT emit / do NOT create files". | PASS |
| 1.3 Availability bias | The chosen pattern (mirroring the `ingest`-side store-open-time recorder construction) is explicitly justified by ADR-0039 §1's locked public surface and by the structural parallel at `crates/kaleidoscope-cli/src/lib.rs:155-184`. No alternative recorder-construction shape (e.g., constructing at `migrate()` call time and passing through the Cinder API) is viable under ADR-0039 §1; the choice is collapsed by the existing constraint. | PASS |
| 2.1 Missing stakeholder perspectives | Primary: platform operator (Priya). Secondary: SRE / on-call (covered by the audit-trail use case "who moved this item yesterday?"). Tertiary: sidecar/collector maintainers (covered by the existing wire-shape contract from ADR-0039 §2 — they already process `kaleidoscope.cinder` lines from sibling feature `cli-cinder-otlp-wiring-v0`). | PASS |
| 2.2 Missing error scenarios | Two error-shaped scenarios + one meta-scenario: unknown-item (pre-flight `get_entry` short-circuit), invalid-tier (`parse_tier` short-circuit), and the locked-tests-pass-byte-equivalently meta-scenario. Best-effort emission posture on `Mutex<W>` poisoning is inherited from the writer (`cinder_otlp_json.rs:236-239`) and not re-litigated. | PASS |
| 2.3 Missing NFRs | NFRs covered: wire-shape correctness (OK1), no-flag byte-equivalence (OK2), error-path emission absence (OK3, OK4), thread-safety (inherited from the writer's `Send + Sync` bounds, ADR-0039 §1). | PASS |
| 3.1 Vague performance requirements | All "100%"/"0%" claims are quantitative and falsifiable via the named tests. No "fast", "scalable", "performant" adjectives without a number. | PASS |
| 3.2 Ambiguous requirements | Every AC pins exact string literals (`"cinder.migrate.count"`, `"kaleidoscope.cinder"`, `"hot"`, `"cold"`, `"warm"`, `"acme"`) or exact integer/boolean values (`asInt == "1"`, "stdout is empty", "exit code is non-zero", "file does NOT exist"). | PASS |
| 4 Testability | Every AC is a property of the file contents (after `migrate` returns), of stdout bytes (after the library call), of the subprocess exit code (after `Command::output()`), or of the filesystem (after the call). Every AC is automatable via `serde_json::from_str` + field navigation + `BufReader` line iteration + `Path::exists` + `Command::output` exit-status inspection. Test names are pre-sketched in the slice file. | PASS |
| 5.1 Largest bottleneck | The largest gap in the cross-process operator-observability story for state-mutating actions is "manual migrations leave no audit trail outside the operator's shell history". This feature addresses exactly that gap. The complementary `ingest`-side gap was already filled by `cli-cinder-otlp-wiring-v0`. | PASS |
| 5.2 Simpler alternatives considered | Considered and rejected: (a) a separate `--audit-log` flag — rejected because it would force two flag names for one logical concept (the operator already learned `--observe-otlp`); (b) emitting the audit line via stderr instead of the OTLP sink — rejected because stderr is already used for the existing summary lines and is not aggregated by the operator's sidecar; (c) wiring `--observe-otlp` on the pre-flight `get_entry` read — explicitly out of scope per the task brief (only the actual migrate emits); (d) JSON output of the stdout line — explicitly out of scope per the task brief. | PASS |
| 5.3 Constraint prioritization | The dominant constraint is OK1, the per-migrate wire-shape correctness. It is correctly weighted as the principal KPI. OK2, OK3, OK4 are correctly weighted as the guardrails around it. The no-flag byte-equivalence (OK2) is the second-priority constraint because it is the non-regression contract on existing operator UX. | PASS |
| 5.4 Data-justified | This is not a performance optimization. The decision to extend the existing `--observe-otlp` flag (instead of introducing `--audit-log` or similar) is justified by the operator's existing muscle memory for `--observe-otlp` on `ingest` and by the cross-bridge metric-scope alignment (`kaleidoscope.cinder` already populated by `cinder.place.count`; this feature adds the sibling `cinder.migrate.count` in the same scope). | PASS |

## DoR Status: PASSED — with three honestly-recorded artefact gaps

The story-level DoR (9 items × 1 story = 9 checks) is PASSED with
evidence on every item. The feature-level DoR is PASSED with the
following honestly-recorded artefact gaps:

- **F1, F2 (journey artefacts)**: deliberately not produced. The
  journey is one step (operator runs the existing command with one
  more flag, sees one new audit line); a multi-step journey visual or
  YAML would duplicate the Elevator Pitch in US-01. The sibling
  feature's journey visual covers the underlying mental model.
  Acknowledged; will not be remediated.
- **F3 (Gherkin `.feature` file)**: deliberately not produced. Per the
  task brief, this project's acceptance idiom is Rust `#[test]`
  functions with `// Given / // When / // Then` comment blocks. The
  Given/When/Then specification lives inline in `user-stories.md`.
- **F4 (shared artefact registry)**: not produced. The cross-feature
  shared artefacts are tracked in the reference features' registries;
  the cross-bridge metric-name contract from ADR-0039 §2 is restated
  as a System Constraint in `user-stories.md`. Producing a new
  registry that copies the reference features' rows would add no value.
- **F6 (separate `prioritization.md`)**: not produced; integrated into
  `story-map.md`'s `## Priority Rationale` section per the
  `nw-leanux-methodology` skill template, because a one-slice feature
  does not earn a separate prioritization document.

Anti-pattern scan clean. Dimension 0 elevator-pitch check PASSES with
the real entry point
(`kaleidoscope-cli migrate acme /tmp/data acme/batch-00042 cold --observe-otlp /tmp/audit.ndjson`)
and the concrete byte-level output (exact stdout line + JSON line in
Domain Example 1). Dimensions 1-5 confirmation-bias self-check PASSES.

The DISCUSS wave is ready to hand off to `nw-solution-architect` for
DESIGN.
