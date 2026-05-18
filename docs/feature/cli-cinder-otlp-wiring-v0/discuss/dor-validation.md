# Definition of Ready — Validation

Feature: `cli-cinder-otlp-wiring-v0`
Reviewer: Luna (self-review at DISCUSS close; second pass scheduled at
handoff to DESIGN per `nw-po-review-dimensions` skill).
Date: 2026-05-18.

## Per-story DoR (9-item hard gate)

### US-01: Cinder placements are visible on the operator's existing `--observe-otlp` stream

| # | DoR Item | Status | Evidence |
|---|----------|--------|----------|
| 1 | Problem statement clear, domain language | PASS | "The CLI's `--observe-otlp <path>` flag … leaves the Cinder store with `cinder::NoopRecorder` (`crates/kaleidoscope-cli/src/lib.rs:163`), so every `cinder.place` call during ingest produces zero lines in the file Priya's sidecar is tailing." Uses Cinder, Lumen, NDJSON, OTLP, sidecar, collector, `tail -f`, ingest, batch flush — all domain terms grounded in the existing CLI implementation and the prior `--observe-otlp` feature's shipped behaviour. |
| 2 | User/persona identified | PASS | Priya the platform operator, multi-tenant Kaleidoscope deployment for a fintech, already uses `--observe-otlp` for Lumen via an existing sidecar + collector + dashboard chain. Same persona as the two reference features. |
| 3 | 3+ domain examples with real data | PASS | Three examples: happy path with `acme` / 6 records / batch_size 3 / four interleaved JSON lines shown byte-for-byte; cross-writer concurrent-random-pause with 100+100 emissions and the post-join assertion set; flag-absent with no-file-created and recorder construction confirmation. Real values throughout: `acme`, `/tmp/k-data`, `/tmp/k-observe.ndjson`, the shipped commit hash `3af7e82`. |
| 4 | UAT scenarios (3-7 in Given/When/Then) | PASS | 5 scenarios: Cinder-line-presence happy path, Lumen-line-still-present continuity, cross-writer-NDJSON-validity-under-concurrent-random-pauses (the OK6 probe mandated by ADR-0039 §7), flag-absent quiescence, existing-tests-pass-byte-equivalently meta-scenario. |
| 5 | AC derived from UAT | PASS | 10 AC bullets, each maps to an observable byte-level invariant from a scenario (JSON-parseable, exact metric-name string literal, exact `asInt` string literal, line counts under the concurrent scenario, file-ending byte). |
| 6 | Right-sized | PASS | 1 story, 1 wiring change (one match-arm substitution at `crates/kaleidoscope-cli/src/lib.rs:163`), 1 new test file, well under 1 day effort. Smaller than both reference features. |
| 7 | Technical notes identify constraints | PASS | File paths, line numbers, manifest entry, dependency posture (no new external crate; `self-observe` already re-exports `CinderToOtlpJsonWriter`), concurrency model note, the deliberate non-prescription of the file-sharing mechanism (DESIGN-owned choice). |
| 8 | Dependencies tracked | PASS | `cinder-to-otlp-json-bridge-v0` shipped (writer publicly re-exported from `self-observe`); prior `--observe-otlp` Lumen wiring shipped at commit `3af7e82`; `aegis` already a `kaleidoscope-cli` dependency; `serde_json` already a dev-dependency. No unresolved external dependencies. |
| 9 | Outcome KPIs defined with measurable targets | PASS | Maps to OK6-CLI-cross-writer-ndjson (principal, 100% lines parseable + trailing `\n` under concurrent emission), OK7-CLI-cinder-events-present (100% of `place` calls produce one line), OK8-CLI-no-regression (existing `observe_otlp_flag.rs` tests pass byte-equivalently). All three targets are quantitative and falsifiable via the named test files. |

**US-01 DoR Status: PASSED**

## Feature-level DoR

| # | Item | Status | Evidence |
|---|------|--------|----------|
| F1 | Journey artefact (visual) | NOT PRODUCED — accepted | Per `wave-decisions.md`, the journey is so thin (one operator action: invoke the existing command) that a separate journey visual would duplicate the Elevator Pitch in US-01. The reference feature `cinder-to-otlp-json-bridge-v0` produced a journey visual; this feature deliberately does not, because the journey is identical to that one with the substrate sliding one layer up from `SharedBuf` to the real CLI. Acknowledged gap; will not be remediated. |
| F2 | Journey artefact (YAML schema) | NOT PRODUCED — accepted | Same rationale as F1. The journey schema in `nw-design-methodology` is for multi-step user journeys; this feature has one step. |
| F3 | Journey artefact (Gherkin `.feature`) | INTENTIONALLY OMITTED | Per the task brief: this project's acceptance idiom is Rust `#[test]` functions with `// Given / // When / // Then` comment blocks, not Gherkin `.feature` files. The Given/When/Then text lives inside `user-stories.md` (UAT Scenarios sections). DISTILL writes the Rust tests. |
| F4 | Shared artefact registry | NOT PRODUCED — accepted | The cross-feature shared artefacts (`tenant_id`, the `--observe-otlp` file path, the `kaleidoscope.lumen` / `kaleidoscope.cinder` scope name pair, the metric-name strings `cinder.place.count` and `lumen.ingest.count`) are tracked in the reference features' registries. This feature consumes them unchanged; producing a new registry would duplicate without adding value. The cross-bridge metric-name contract from ADR-0039 §2 is recorded as a System Constraint in `user-stories.md`. |
| F5 | Story map | PASS | `discuss/story-map.md` with one-activity backbone, walking-skeleton justification (N/A — the substrate already exists), single-slice rationale, cross-bridge alignment, scope assessment. |
| F6 | Prioritization | INTEGRATED into `story-map.md` | The single-slice structure renders a separate `prioritization.md` redundant. The `## Priority Rationale` section in `story-map.md` covers the ordering and partial-ship logic. |
| F7 | Outcome KPIs | PASS | `discuss/outcome-kpis.md` with OK6 (principal, inherited from ADR-0039 §7), OK7, OK8, each with quantitative target, baseline, and measurement plan. |
| F8 | User stories | PASS | `discuss/user-stories.md` with US-01, including complete Elevator Pitch with the real entry-point shell command and the concrete byte-level output. |
| F9 | Per-slice files | PASS | 1 file under `discuss/slices/`. |
| F10 | Wave decisions | PASS | `discuss/wave-decisions.md` with D1-D8 covering scope (D1, D5), flag posture (D2), ADR-0039 §7 inheritance (D3), test file shape (D4), out-of-scope locks (D6, D7), and SSOT non-modification (D8). |
| F11 | Right-sized (Elephant Carpaccio) | PASS | 1 story, 1 bounded context (`kaleidoscope-cli` crate), 1 modified file + 1 new test file + 1 manifest line-level change. Estimated effort: well under 1 day. `story-map.md` "Scope Assessment: PASS". |
| F12 | SSOT impact assessed | PASS | `wave-decisions.md` D8: no SSOT modification. Same posture as the reference features. |
| F13 | Cross-bridge contract honoured | PASS | `user-stories.md` System Constraints cite ADR-0039 §2 (cross-bridge metric-name contract) and ADR-0039 §7 (cross-writer NDJSON-validity invariant) explicitly. The metric name `cinder.place.count` and the scope name `kaleidoscope.cinder` are pinned in AC by exact string literal. |

## Anti-pattern scan

| Anti-pattern | Detected? | Notes |
|--------------|-----------|-------|
| Implement-X | No | Story framed as "Priya runs `kaleidoscope-cli ingest acme /tmp/data --observe-otlp /tmp/foo.ndjson < records.json` and sees `cinder.place.count` lines interleaved with `lumen.ingest.count` lines" — outcome-first from the operator perspective; the match-arm substitution is the means, not the end. |
| Generic data | No | Real names: Priya, `acme`. Real paths: `/tmp/k-data`, `/tmp/k-observe.ndjson`, `/tmp/foo.ndjson`. Real source-file references: `crates/kaleidoscope-cli/src/lib.rs:147-163`, `:228`, `:163`. Real commit hash: `3af7e82`. Real metric names: `cinder.place.count`, `lumen.ingest.count`. Real scope names: `kaleidoscope.cinder`, `kaleidoscope.lumen`. |
| Technical AC | No blocking instances | AC pin the wire-observable invariants (metric name string, scope name string, `asInt` string value, point-attribute key/value pairs, file-ending byte). They do NOT prescribe the file-sharing mechanism (`File::try_clone` vs two `OpenOptions` calls vs `Arc<File>`); that is explicitly DESIGN-owned per `wave-decisions.md`. |
| Technical scenario titles | No | Scenarios titled by user outcome: "Ingest with `--observe-otlp` emits one `cinder.place.count` line per batch flush", "Cross-writer NDJSON validity under concurrent random pauses". Not "ingest() function constructs CinderToOtlpJsonWriter" or "Box dyn cinder::MetricsRecorder coerces from struct". |
| Oversized story | No | 1 story, 5 UAT scenarios, 10 AC, well under 1 day effort. The reference features each had 3 stories; this one has 1, in keeping with its strictly smaller change surface. |
| Abstract requirements | No | Every AC has a numeric value (line counts, `asInt` string values), a string literal (metric names, scope names, tier value), or a byte-level invariant (file ends with `\n`, line parses as JSON). |

## Reviewer self-pass (Dimension 0 — Elevator Pitch, BLOCKING)

| Story | Section present | Real entry point | Concrete output | Job connection | Verdict |
|-------|-----------------|------------------|-----------------|----------------|---------|
| US-01 | yes (Before / After / Decision enabled triple) | yes — `kaleidoscope-cli ingest acme /tmp/data --observe-otlp /tmp/foo.ndjson < records.json` (the actual shell command an operator types) | yes — a 4-line byte-level JSON sample showing both Lumen and Cinder lines is included in Domain Example 1 of US-01, and the Elevator Pitch's "After" line names the exact shape Priya will see in her sidecar | yes — "Priya can decide 'did `acme`'s last ingest run actually land batches in the Hot tier, and at what rate?' from the same cross-process dashboard she already uses for Lumen" | PASS |

Slice-level check (Dimension 0 item 5): the single slice (Slice 01) is
tagged operator-visible (NOT `@infrastructure`). The story's Elevator
Pitch names a real user-invocable CLI command and the byte-level output
the operator's sidecar will consume. There is no slice-level
infrastructure-only blocking concern. **PASS at slice level.**

## Confirmation-bias detection (Dimensions 1-4 self-check)

| Dimension | Check | Verdict |
|-----------|-------|---------|
| 1.1 Technology bias | Stories specify the existing CLI surface and the OTLP-JSON NDJSON wire format. No specific collector technology (Datadog, Prometheus, NewRelic) prescribed. The sidecar and collector are mentioned as already-deployed components the operator chose; no new technology adopted. | PASS |
| 1.2 Happy path bias | Story includes the flag-absent quiescence scenario and the concurrent-random-pause scenario (which actively probes for failure modes via scheduling jitter). The OK6 KPI is itself a sad-path-shaped guarantee: "even when concurrent emission occurs, the stream stays valid". | PASS |
| 1.3 Availability bias | The chosen pattern (mirroring the Lumen-side wiring) is explicitly justified by ADR-0039 §1's locked public surface and by the structural parallel at `crates/kaleidoscope-cli/src/lib.rs:147-160`. Alternative file-sharing mechanisms are not pre-judged; the choice is deferred to DESIGN. | PASS |
| 2.1 Missing stakeholder perspectives | Primary: platform operator (Priya). Secondary: Cinder maintainer (covered by OK8 — Lumen-side byte equivalence is the proxy for "no behaviour change to either writer"). Tertiary: sidecar/collector maintainers (covered by OK6 — NDJSON validity preserved). | PASS |
| 2.2 Missing error scenarios | Three error-shaped scenarios: flag-absent (operator forgot the flag), concurrent random pauses (scheduler reorders writes), existing-tests-must-pass (regression risk on the Lumen side). Best-effort emission posture on `Mutex<W>` poisoning is inherited from the writers and documented in their respective wave-decisions documents; not re-litigated here. | PASS |
| 2.3 Missing NFRs | NFRs covered: cross-writer NDJSON validity (OK6), thread-safety (inherited from both writers' `Send + Sync` bounds, ADR-0039 §1 + the Lumen writer's signature), non-regression on the existing wired feature (OK8). | PASS |
| 3.1 Vague performance requirements | All "100%"/"0%" claims are quantitative and falsifiable via the named test. No "fast", "scalable", "performant" adjectives without a number. The concurrent-random-pause scenario quantifies the load (100 emissions per thread, `[0, 5]` ms pauses). | PASS |
| 3.2 Ambiguous requirements | Every AC pins exact string literals (`"cinder.place.count"`, `"lumen.ingest.count"`, `"kaleidoscope.cinder"`, `"hot"`) or exact integer values (`asInt == "1"`, `asInt == "3"`, line counts). | PASS |
| 4 Testability | Every AC is a property of the file contents (after `ingest` returns) or of the test thread joining. Every AC is automatable via `serde_json::from_str` + field navigation + `BufReader` line iteration. Test names are pre-sketched in the slice file. | PASS |
| 5.1 Largest bottleneck | The largest gap in the cross-process operator observability story is "Cinder events are invisible to the existing `--observe-otlp` stream", explicitly mandated by ADR-0039 §7 as the next required follow-up. This feature addresses exactly that mandate. | PASS |
| 5.2 Simpler alternatives considered | Considered and rejected: (a) a separate `--observe-cinder-otlp` flag — rejected because it trivialises OK6 by splitting the stream into two files (`wave-decisions.md` D2); (b) constructing a parallel `read` subcommand wiring — rejected as out of scope (D5); (c) per-event `migrate`/`evaluate` CLI subcommands — out of scope because no CLI call site exists today (D1); (d) extracting the test harness from `observe_otlp_flag.rs` into a shared module — rejected at v0 (rule of three not reached, D4). | PASS |
| 5.3 Constraint prioritization | The dominant constraint is OK6, the cross-writer NDJSON-validity invariant mandated by ADR-0039 §7. It is correctly weighted as the principal KPI and the slice's learning hypothesis. OK7 and OK8 are correctly weighted as the necessary leading and guardrail indicators around it. | PASS |
| 5.4 Data-justified | This is not a performance optimization. The decision to extend the existing flag instead of introducing a new one is justified by ADR-0039 §7's explicit mandate (which assumed one shared file) and by the operator's existing muscle memory for `--observe-otlp`. | PASS |

## DoR Status: PASSED — with two honestly-recorded artefact gaps

The story-level DoR (9 items × 1 story = 9 checks) is PASSED with
evidence on every item. The feature-level DoR is PASSED with the
following honestly-recorded artefact gaps:

- **F1, F2 (journey artefacts)**: deliberately not produced. The
  journey is one step (operator runs the existing command, sees the
  interleaved file); a multi-step journey visual or YAML would
  duplicate the Elevator Pitch in US-01. The reference feature's
  journey visual covers the underlying mental model. Acknowledged;
  will not be remediated.
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
the real entry point (`kaleidoscope-cli ingest … --observe-otlp …`)
and the concrete byte-level output (4-line JSON sample in Domain
Example 1). Dimensions 1-5 confirmation-bias self-check PASSES.

The DISCUSS wave is ready to hand off to `nw-solution-architect` for
DESIGN.
