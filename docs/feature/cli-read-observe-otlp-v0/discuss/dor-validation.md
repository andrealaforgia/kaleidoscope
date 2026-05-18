# Definition of Ready — Validation

Feature: `cli-read-observe-otlp-v0`
Reviewer: Luna (self-review at DISCUSS close; second pass scheduled at
handoff to DESIGN per `nw-po-review-dimensions` skill).
Date: 2026-05-19.

## Per-story DoR (9-item hard gate)

### US-01: Lumen query events are visible on the operator's existing `--observe-otlp` stream

| # | DoR Item | Status | Evidence |
|---|----------|--------|----------|
| 1 | Problem statement clear, domain language | PASS | "The CLI's `--observe-otlp <path>` flag, shipped in commit `3af7e82` for `ingest` and extended in `cli-cinder-otlp-wiring-v0` to also carry Cinder events, plumbs `LumenToOtlpJsonWriter` and `CinderToOtlpJsonWriter` into the ingest path — but `read` is unwired. The `read()` library function constructs `LumenToPulseRecorder` over a fresh in-process Pulse store (`crates/kaleidoscope-cli/src/lib.rs:253-255`); that recorder dies at end of call and produces zero bytes anywhere the operator can inspect." Uses Lumen, Cinder, OTLP-JSON, sidecar, collector, NDJSON, `tail -f`, query — all domain terms grounded in the existing CLI implementation and the prior two `--observe-otlp` features' shipped behaviour. |
| 2 | User/persona identified | PASS | Priya the platform operator, multi-tenant Kaleidoscope deployment for a fintech, already uses `--observe-otlp` for ingest-side observability via an existing sidecar + collector + dashboard chain. Same persona as the three reference features. |
| 3 | 3+ domain examples with real data | PASS | Three examples: happy path with `acme` / 6 pre-ingested records / one `read` invocation / one new `lumen.query.count` line shown shape-for-shape; no-flag quiescence with byte-equivalent stdout assertion; ingest-then-read symmetry with the full 5-line file content (2 Lumen-ingest + 2 Cinder-place + 1 Lumen-query). Real values throughout: `acme`, `/tmp/k-data`, `/tmp/k-observe.ndjson`, the shipped commit hash `3af7e82`, the predecessor feature id `cli-cinder-otlp-wiring-v0`. |
| 4 | UAT scenarios (3-7 in Given/When/Then) | PASS | 5 scenarios: Lumen-query-line-presence happy path (OK1), stdout-preservation continuity, no-flag quiescence (OK2), ingest-then-read symmetry (OK3), existing-tests-pass-byte-equivalently meta-scenario. |
| 5 | AC derived from UAT | PASS | 11 AC bullets, each maps to an observable byte-level invariant from a scenario (parameter signature, exact metric-name string literal, exact scope string literal, exact resource-attribute string literal, `asInt` semantics, stdout byte equivalence, file ends with `\n`, line counts in the OK3 scenario, main.rs dispatcher contract, print_usage contract, non-regression meta-AC). |
| 6 | Right-sized | PASS | 1 story, 1 wiring change (one new parameter on `read`, one new match arm, one new `parse_observe_otlp` call in `main.rs`), 1 new test file with 3 scenarios, well under 1 day effort. Strictly smaller than the predecessor `cli-cinder-otlp-wiring-v0` (no cross-writer concurrency probe required). |
| 7 | Technical notes identify constraints | PASS | File paths, line numbers, manifest entry, dependency posture (no new external crate; `self-observe` already re-exports `LumenToOtlpJsonWriter`), concurrency model note (single thread, single query call, single writer — no concurrency probe needed), the explicit non-prescription of internal implementation details beyond the structural mirror of the ingest-side wiring (DESIGN-owned choice for any minor signature variant). |
| 8 | Dependencies tracked | PASS | Prior `--observe-otlp` Lumen wiring shipped at commit `3af7e82`; `cli-cinder-otlp-wiring-v0` shipped; `LumenToOtlpJsonWriter` publicly re-exported from `self-observe`; `aegis` already a `kaleidoscope-cli` dependency; `serde_json` already a dev-dependency. No unresolved external dependencies. |
| 9 | Outcome KPIs defined with measurable targets | PASS | Maps to OK1-CLI-read-lumen-query-events-present (principal, 100% of `read()` invocations with the flag produce exactly one `lumen.query.count` line), OK2-CLI-read-no-side-channel (100% byte equivalence of stdout when flag absent), OK3-CLI-read-ingest-symmetry (both ingest-side and read-side metric types present in one file after one sequential shell session). All three targets are quantitative and falsifiable via the named test file. |

### US-01 DoR Status: PASSED

## Feature-level DoR

| # | Item | Status | Evidence |
|---|------|--------|----------|
| F1 | Journey artefact (visual) | NOT PRODUCED — accepted | Per `wave-decisions.md`, the journey is so thin (one operator action: invoke `read` with the existing-but-newly-accepted flag) that a separate journey visual would duplicate the Elevator Pitch in US-01. The three reference features in this cluster (the original `--observe-otlp` ingest wiring, `cinder-to-otlp-json-bridge-v0`, `cli-cinder-otlp-wiring-v0`) collectively cover the underlying mental model. Acknowledged gap; will not be remediated. |
| F2 | Journey artefact (YAML schema) | NOT PRODUCED — accepted | Same rationale as F1. The journey schema in `nw-design-methodology` is for multi-step user journeys; this feature has one step. |
| F3 | Journey artefact (Gherkin `.feature`) | INTENTIONALLY OMITTED | Per the project convention (inherited from `cli-cinder-otlp-wiring-v0` and noted in the task brief): this project's acceptance idiom is Rust `#[test]` functions with `// Given / // When / // Then` comment blocks, not Gherkin `.feature` files. The Given/When/Then text lives inside `user-stories.md` (UAT Scenarios sections). DISTILL writes the Rust tests. |
| F4 | Shared artefact registry | NOT PRODUCED — accepted | The cross-feature shared artefacts (`tenant_id`, the `--observe-otlp` file path, the `kaleidoscope.lumen` scope name, the metric-name strings `lumen.ingest.count`, `cinder.place.count`, and now `lumen.query.count`) are tracked in the reference features' registries and are reused unchanged here. The metric-name and scope-name contracts are restated as System Constraints in `user-stories.md`. |
| F5 | Story map | PASS | `discuss/story-map.md` with one-activity backbone, walking-skeleton justification (N/A — the substrate already exists), single-slice rationale, cross-feature alignment, scope assessment. |
| F6 | Prioritization | INTEGRATED into `story-map.md` | The single-slice structure renders a separate `prioritization.md` redundant. The `## Priority Rationale` section in `story-map.md` covers the ordering and partial-ship logic. |
| F7 | Outcome KPIs | PASS | `discuss/outcome-kpis.md` with OK1 (principal), OK2, OK3, each with quantitative target, baseline, and measurement plan. |
| F8 | User stories | PASS | `discuss/user-stories.md` with US-01, including complete Elevator Pitch with the real entry-point shell command (`kaleidoscope-cli read acme /tmp/data --observe-otlp /tmp/foo.ndjson > /dev/null`) and the concrete byte-level output (full OTLP-JSON line shape shown in Domain Example 1). |
| F9 | Per-slice files | PASS | 1 file under `discuss/slices/`. |
| F10 | Wave decisions | PASS | `discuss/wave-decisions.md` with D1-D7 covering scope (D1), out-of-scope items (D2, D3, D4), OK3 design (D5), test file shape (D6), and SSOT non-modification (D7). |
| F11 | Right-sized (Elephant Carpaccio) | PASS | 1 story, 1 bounded context (`kaleidoscope-cli` crate), 2 modified files + 1 new test file + 1 manifest line-level change. Estimated effort: well under 1 day. `story-map.md` "Scope Assessment: PASS". |
| F12 | SSOT impact assessed | PASS | `wave-decisions.md` D7: no SSOT modification. Same posture as the three reference features. |
| F13 | Cross-feature contract honoured | PASS | `user-stories.md` System Constraints pin the metric-name (`lumen.query.count`), the scope (`kaleidoscope.lumen`), the file-open mode (`OpenOptions::new().create(true).append(true)`), and the no-flag non-regression invariant. ADR-0039 §1 + §2 correction box constraints inherited unchanged. |

## Anti-pattern scan

| Anti-pattern | Detected? | Notes |
|--------------|-----------|-------|
| Implement-X | No | Story framed as "Priya runs `kaleidoscope-cli read acme /tmp/data --observe-otlp /tmp/foo.ndjson > /dev/null` and sees `lumen.query.count` lines on her existing collector dashboard" — outcome-first from the operator perspective; the wiring change is the means, not the end. |
| Generic data | No | Real names: Priya, `acme`. Real paths: `/tmp/k-data`, `/tmp/k-observe.ndjson`, `/tmp/foo.ndjson`. Real source-file references: `crates/kaleidoscope-cli/src/lib.rs:252-269`, `:253-255`, `:258-260`, `:144`, `:158-164`; `crates/kaleidoscope-cli/src/main.rs:87-88`, `:105-119`, `:121-128`, `:68-84`. Real commit hash: `3af7e82`. Real metric names: `lumen.query.count`, `lumen.ingest.count`, `cinder.place.count`. Real scope names: `kaleidoscope.lumen`. |
| Technical AC | No blocking instances | AC pin the wire-observable invariants (parameter name and shape, metric-name string, scope-name string, `asInt` semantics, stdout byte equivalence, file-ending byte). They reference the structural mirror of the ingest-side wiring as the implementation pattern but do NOT prescribe novel internal mechanisms; the structural-mirror reference is a fact about the precedent, not a novel design constraint. |
| Technical scenario titles | No | Scenarios titled by user outcome: "Read with `--observe-otlp` emits one `lumen.query.count` line per invocation", "Ingest then read in one session share one `--observe-otlp` file". Not "read() function constructs LumenToOtlpJsonWriter" or "FileBackedLogStore::open recorder slot accepts boxed dyn trait". |
| Oversized story | No | 1 story, 5 UAT scenarios, 11 AC, well under 1 day effort. Smaller than the predecessor's 5 scenarios + 10 AC story (because no concurrent-pause scenario is required here). |
| Abstract requirements | No | Every AC has a numeric value (line counts, `asInt` string values), a string literal (metric names, scope names), or a byte-level invariant (file ends with `\n`, stdout bytes equal NDJSON re-serialisation). |

## Reviewer self-pass (Dimension 0 — Elevator Pitch, BLOCKING)

| Story | Section present | Real entry point | Concrete output | Job connection | Verdict |
|-------|-----------------|------------------|-----------------|----------------|---------|
| US-01 | yes (Before / After / Decision enabled triple) | yes — `kaleidoscope-cli read acme /tmp/data --observe-otlp /tmp/foo.ndjson > /dev/null` (the actual shell command an operator types) | yes — a full OTLP-JSON line shape is shown in Domain Example 1 of US-01, and the Elevator Pitch's "After" line names the exact shape Priya will see appended to her sidecar's file (one `lumen.query.count` line with `scope = "kaleidoscope.lumen"`, `tenant_id = "acme"`, `asInt` = match count) | yes — "Priya can decide 'what is `acme`'s query latency and throughput distribution right now, and which tenants are generating the most expensive queries?' from the same cross-process dashboard she already uses for ingest activity" | PASS |

Slice-level check (Dimension 0 item 5): the single slice (Slice 01) is
tagged operator-visible (NOT `@infrastructure`). The story's Elevator
Pitch names a real user-invocable CLI command and the byte-level output
the operator's sidecar will consume. There is no slice-level
infrastructure-only blocking concern. PASS at slice level.

## Confirmation-bias detection (Dimensions 1-4 self-check)

| Dimension | Check | Verdict |
|-----------|-------|---------|
| 1.1 Technology bias | Story specifies the existing CLI surface and the OTLP-JSON NDJSON wire format already established by the prior `--observe-otlp` features. No specific collector technology (Datadog, Prometheus, NewRelic) prescribed. The sidecar and collector are mentioned as already-deployed components the operator chose; no new technology adopted. | PASS |
| 1.2 Happy path bias | Story includes the no-flag quiescence scenario (operator who omits the flag must see byte-equivalent behaviour) and the existing-tests-must-pass meta-scenario. The OK2 KPI is itself a sad-path-shaped guarantee against accidental side-effect introduction in the no-flag path. No active-failure-mode probe (e.g. concurrent-write torn-record probe) because this feature has only one writer and one query call per invocation — there is no concurrency to probe, by construction. |
| 1.3 Availability bias | The chosen pattern (mirroring the ingest-side Lumen wiring) is explicitly justified by ADR-0039 §1 + §2 correction box (locked surface and atomicity pattern) and by the structural parallel at `crates/kaleidoscope-cli/src/lib.rs:147-164`. The alternative of a new flag is explicitly rejected in `wave-decisions.md` (extending the existing flag is the operator's muscle memory). | PASS |
| 2.1 Missing stakeholder perspectives | Primary: platform operator (Priya). Secondary: Lumen maintainer (covered by the meta-AC — the existing `observe_otlp_flag.rs` test file's Lumen-side assertions continue to pass byte-equivalently). Tertiary: sidecar/collector maintainers (covered by inheritance — the NDJSON line shape and file-open mode are unchanged from the prior features). | PASS |
| 2.2 Missing error scenarios | Two error-shaped scenarios: no-flag quiescence (operator forgot the flag — must not silently change `read`'s stdout) and existing-tests-must-pass (regression risk on the ingest side of the wired feature). Best-effort emission posture on `Mutex<W>` poisoning is inherited from the Lumen writer and documented in its wave-decisions document; not re-litigated here. There is no concurrent-writer error scenario to add because the feature has only one writer. |
| 2.3 Missing NFRs | NFRs covered: stdout byte equivalence (OK2), one-line-per-invocation contract (OK1), cross-subcommand symmetry (OK3), non-regression on the existing wired feature (the meta-AC). Thread-safety is inherited from the writer's `Send + Sync` bounds; no new thread-safety claim is made because no concurrency is introduced. | PASS |
| 3.1 Vague performance requirements | All "100%"/"0%" claims are quantitative and falsifiable via the named test. No "fast", "scalable", "performant" adjectives without a number. | PASS |
| 3.2 Ambiguous requirements | Every AC pins exact string literals (`"lumen.query.count"`, `"kaleidoscope.lumen"`, `"lumen.ingest.count"`, `"cinder.place.count"`, `"hot"`) or exact integer/byte invariants (line counts, `asInt` matching the query result count, file ends with `\n`, stdout byte equivalence). | PASS |
| 4 Testability | Every AC is a property of the file contents (after `read()` returns), the stdout sink contents, or the function's return value. Every AC is automatable via `serde_json::from_str` + field navigation + `BufReader` line iteration + `Vec<u8>` byte comparison. Test names are pre-sketched in the slice file. | PASS |
| 5.1 Largest bottleneck | The largest gap in the cross-process operator observability story (after the two prior `--observe-otlp` features) is "Lumen query events are invisible to the existing `--observe-otlp` stream because the `read` subcommand does not support the flag at all". This feature addresses exactly that gap. | PASS |
| 5.2 Simpler alternatives considered | Considered and rejected: (a) a separate `--observe-read-otlp` flag — rejected because it breaks the operator's muscle memory for `--observe-otlp` and creates a second file the sidecar must tail (`wave-decisions.md` D2 spirit, inherited from `cli-cinder-otlp-wiring-v0` D2 reasoning); (b) doing nothing and telling operators to use ingest-side telemetry only — rejected because query latency and per-tenant query expense are first-class operational questions that ingest-side metrics cannot answer; (c) extracting a shared test-harness module across `observe_otlp_flag.rs`, `observe_otlp_cinder_wiring.rs`, and the new file — deferred to a follow-up per `wave-decisions.md` D6 (the extraction is a separate refactoring concern). | PASS |
| 5.3 Constraint prioritization | The dominant constraint is OK1 (presence). OK2 (no-flag guardrail) and OK3 (cross-subcommand symmetry) are correctly weighted as the necessary guardrail and the necessary cross-feature continuity check. There is no principal cross-writer invariant to prioritize because only one writer participates. | PASS |
| 5.4 Data-justified | This is not a performance optimization. The decision to extend the existing flag instead of introducing a new one is justified by the operator's existing muscle memory and by the OK3 symmetry that depends on one shared file across both subcommands. | PASS |

## DoR Status: PASSED — with three honestly-recorded artefact gaps

The story-level DoR (9 items × 1 story = 9 checks) is PASSED with
evidence on every item. The feature-level DoR is PASSED with the
following honestly-recorded artefact gaps:

- **F1, F2 (journey artefacts)**: deliberately not produced. The
  journey is one step (operator runs `read` with the existing-but-
  newly-accepted flag, sees one new line in her file); a multi-step
  journey visual or YAML would duplicate the Elevator Pitch in US-01.
  The three reference features' journey visuals collectively cover
  the underlying mental model. Acknowledged; will not be remediated.
- **F3 (Gherkin `.feature` file)**: deliberately not produced. Per
  the project convention, this project's acceptance idiom is Rust
  `#[test]` functions with `// Given / // When / // Then` comment
  blocks. The Given/When/Then specification lives inline in
  `user-stories.md`.
- **F4 (shared artefact registry)**: not produced. The cross-feature
  shared artefacts are tracked in the reference features' registries
  and reused unchanged; the metric-name / scope-name / file-open
  contracts are restated as System Constraints in `user-stories.md`.
  Producing a new registry that copies the reference features' rows
  would add no value.
- **F6 (separate `prioritization.md`)**: not produced; integrated
  into `story-map.md`'s `## Priority Rationale` section per the
  `nw-leanux-methodology` skill template, because a one-slice feature
  does not earn a separate prioritization document.

Anti-pattern scan clean. Dimension 0 elevator-pitch check PASSES with
the real entry point (`kaleidoscope-cli read acme /tmp/data
--observe-otlp /tmp/foo.ndjson > /dev/null`) and the concrete
byte-level output (full OTLP-JSON line shape in Domain Example 1).
Dimensions 1-5 confirmation-bias self-check PASSES.

The DISCUSS wave is ready to hand off to `nw-solution-architect` for
DESIGN.
