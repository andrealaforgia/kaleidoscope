# Definition of Ready — Validation

Feature: `cinder-to-otlp-json-bridge-v0`
Reviewer: Luna (self-review at DISCUSS close; second pass scheduled at
handoff to DESIGN per `nw-po-review-dimensions` skill).
Date: 2026-05-18.

## Per-story DoR (9-item hard gate)

### US-01: Cinder `place` events emit one OTLP-JSON ResourceMetrics line per call

| # | DoR Item | Status | Evidence |
|---|----------|--------|----------|
| 1 | Problem statement clear, domain language | PASS | "The CLI's --observe-otlp <path> ... leaves the Cinder store with NoopRecorder, so every cinder.place call during ingest produces zero lines." Uses Cinder, Lumen, NDJSON, OTLP, sidecar, collector — all domain terms grounded in the existing CLI implementation. |
| 2 | User/persona identified | PASS | Priya the platform operator, multi-tenant fintech Kaleidoscope deployment, already uses `--observe-otlp` for Lumen via an existing sidecar + collector + dashboard chain. Specific. |
| 3 | 3+ domain examples with real data | PASS | Three examples: `acme` with `trade-2026-05-18-001`/Hot (happy path with byte-level JSON line shown), three-tier serialisation with `trade-001`/`trade-002`/`trade-003`, two-tenant isolation with `acme`/`globex` and concrete line counts. |
| 4 | UAT scenarios (3-7 in Given/When/Then) | PASS | 4 scenarios in user-stories.md, 6 corresponding tests planned in slice file (the test file adds NDJSON shape + Send/Sync compile check on top of the 4 user-facing scenarios). |
| 5 | AC derived from UAT | PASS | 12 AC bullets, each maps to an observable byte-level invariant from a scenario (JSON-parseable, key/value pairs, exact string literals). |
| 6 | Right-sized | PASS | 1 method body + the OTLP-JSON serde structs (shared with slices 02/03), 6 tests, ~4h effort. |
| 7 | Technical notes identify constraints | PASS | File paths, re-export, test harness substrate (SharedBuf), serde-struct duplication justification (D7), slice tag. |
| 8 | Dependencies tracked | PASS | `cinder` v0.1.0 already added (by Pulse-sink sibling), `aegis` existing, `serde`/`serde_json` already in Cargo.toml. No new external crates. |
| 9 | Outcome KPIs defined with measurable targets | PASS | Maps to OK1: 100% of place calls produce exactly one parseable line. Measured by green Slice 01 tests. |

**US-01 DoR Status: PASSED**

### US-02: Cinder `migrate` events emit one OTLP-JSON line per call with direction attributes

| # | DoR Item | Status | Evidence |
|---|----------|--------|----------|
| 1 | Problem statement clear, domain language | PASS | "Priya needs to see the direction of every tier migration per tenant on her cross-process collector ... failed migrate MUST NOT produce a spurious line — otherwise the operator's collector cannot distinguish real migrations from bookkeeping errors." Distinguishes success vs failure semantically. |
| 2 | User/persona identified | PASS | Priya, with explicit "wants failed migrations to leave no trace in the NDJSON stream" wrinkle that sets US-02 apart from US-01. |
| 3 | 3+ domain examples with real data | PASS | Three examples: Hot->Warm happy path on `acme` with `trade-2026-05-18-001`, `ghost-item` failed migrate, two-tenant opposite-direction migrations with `a1`/`g1`. |
| 4 | UAT scenarios (3-7 in Given/When/Then) | PASS | 3 scenarios, 3 corresponding tests. |
| 5 | AC derived from UAT | PASS | 7 AC bullets, each maps to an observable byte-level invariant. |
| 6 | Right-sized | PASS | 1 method body, 3 tests, ~2h effort. |
| 7 | Technical notes identify constraints | PASS | Reuses Slice 01 infrastructure; no new file; lowercase helper reused; flags DESIGN choice between `[OtlpAttr; N]` parameterised and `Vec<OtlpAttr>`. |
| 8 | Dependencies tracked | PASS | Depends on US-01. Documented in Story + Slice file. |
| 9 | Outcome KPIs defined with measurable targets | PASS | Maps to OK2: 100% successful migrates -> 1 correct line, 0% failed migrates -> any line. |

**US-02 DoR Status: PASSED**

### US-03: Cinder `evaluate` events emit one OTLP-JSON line per (tenant, evaluate-call) with per-tenant migrated count

| # | DoR Item | Status | Evidence |
|---|----------|--------|----------|
| 1 | Problem statement clear, domain language | PASS | "evaluate_at is per-tenant aggregate ... per-item migrations need to remain visible as cinder.migrate.count lines (US-02), so the writer must not deduplicate ... zero-eligible tenants get no line." All three semantic wrinkles called out in domain terms. |
| 2 | User/persona identified | PASS | Priya, with explicit "wants the dual emission to remain visible and unsurprising for cross-check" + "no ghost evaluate lines" wrinkles. |
| 3 | 3+ domain examples with real data | PASS | Three examples: 5-item happy path for `acme` at t0+25h, zero-eligible at t0+1h, mixed 5/2 across `acme`/`globex`. Concrete line counts and `asInt` string values shown. |
| 4 | UAT scenarios (3-7 in Given/When/Then) | PASS | 3 scenarios, 3 tests planned. |
| 5 | AC derived from UAT | PASS | 7 AC bullets, each maps to an observable byte-level invariant. |
| 6 | Right-sized | PASS | 1 method body + the dual-emission cross-event test, ~3h effort. |
| 7 | Technical notes identify constraints | PASS | `migrated.to_string()` precision note, dual-emission contract note, DESIGN preservation hint for the cross-metric assertion shape. |
| 8 | Dependencies tracked | PASS | Depends on US-01 + US-02. Documented in Story + Slice file. |
| 9 | Outcome KPIs defined with measurable targets | PASS | Maps to OK3: 100% non-zero-tenant evaluates -> 1 correct line, 0% zero-eligible -> any line. |

**US-03 DoR Status: PASSED**

## Feature-level DoR

| # | Item | Status | Evidence |
|---|------|--------|----------|
| F1 | Journey artefact (visual) | PASS | `discuss/journey-observe-cinder-via-otlp-json-visual.md` |
| F2 | Journey artefact (YAML schema) | PASS | `discuss/journey-observe-cinder-via-otlp-json.yaml` |
| F3 | Journey artefact (Gherkin) | PASS | `discuss/journey-observe-cinder-via-otlp-json.feature` |
| F4 | Shared artefact registry | PASS | `discuss/shared-artifacts-registry.md` with 9 entries, each with source/consumers/risk/validation |
| F5 | Story map | PASS | `discuss/story-map.md` with backbone, walking-skeleton justification, cross-bridge alignment, scope assessment |
| F6 | Prioritization | PASS | `discuss/prioritization.md` with V x U / E scores and riskiest-assumption-first override |
| F7 | Outcome KPIs | PASS | `discuss/outcome-kpis.md` with OK1-OK5 (3 leading + 2 guardrails) |
| F8 | User stories | PASS | `discuss/user-stories.md` with US-01/02/03, each with Elevator Pitch + @infrastructure override note |
| F9 | Per-slice files | PASS | 3 files under `slices/` |
| F10 | Wave decisions | PASS | `discuss/wave-decisions.md` with D1-D10 |
| F11 | Right-sized (Elephant Carpaccio) | PASS | 3 stories, 1 bounded context (`self-observe` crate), ~1 day total effort. `story-map.md` "Scope Assessment: PASS". |
| F12 | SSOT impact assessed | PASS | `wave-decisions.md` D10: no SSOT modification at v0. Same posture as the Pulse-sink sibling D7. |
| F13 | Cross-bridge contract honoured | PASS | `wave-decisions.md` D1 + D3 lock metric names and tier serialisation identical to `cinder-to-pulse-bridge-v0`. Reviewer diff-check at handoff time. |

## Anti-pattern scan

| Anti-pattern | Detected? | Notes |
|--------------|-----------|-------|
| Implement-X | No | All stories framed as "Priya runs the CLI with `--observe-otlp` ... her sidecar sees ..." — outcome-first from the operator perspective; the writer is the means, not the end. |
| Generic data | No | Real names everywhere: Priya, `acme`, `globex`, `trade-2026-05-18-001`, `ghost-item`, `a1`, `g1`. Real path: `/var/log/k/observe.ndjson`. Real timestamps: `t0`, `t0+25h`, with example uint64 `"1747569600123456789"`. |
| Technical AC | None blocking | AC name the metric name, scope name, point attributes, asInt values, and JSON-shape invariants that the writer produces. Some AC reference the trait method names (`record_place`) because the trait IS the user-facing API at this library-level — the writer implements `cinder::MetricsRecorder` and that is the entry point. The AC do NOT prescribe internal implementation choices (e.g. they do not say "use `[OtlpAttr; 2]` for the attribute array" — they say "the attributes contain {key, value}" entries; the DESIGN wave picks the Rust shape). |
| Technical scenario titles | No | Scenarios titled by user outcome: "Place under a tenant produces one OTLP-JSON line under that tenant", not "record_place invokes serde_json::to_string + write_all". |
| Oversized stories | No | Each story 3-4 user-facing scenarios, +1-3 cross-cutting property tests, ~2-4h effort. |
| Abstract requirements | No | Every AC has a numeric value, string literal, or JSON-shape assertion grounded in real data. Byte-level JSON line shown in US-01 example 1 and in `journey-observe-cinder-via-otlp-json-visual.md`. |

## Reviewer self-pass (Dimension 0 — Elevator Pitch, BLOCKING)

| Story | Section present | Real entry point | Concrete output | Job connection | Verdict |
|-------|-----------------|------------------|-----------------|----------------|---------|
| US-01 | yes | `self_observe::CinderToOtlpJsonWriter::new` invoked by the CLI follow-up; the lines themselves are consumed by the operator's existing sidecar + collector + dashboard | yes, full byte-level JSON line shown in the Elevator Pitch's "After" clause | yes ("decide is acme's Hot-tier placement rate normal today from the same cross-process dashboard she already uses for Lumen") | PASS |
| US-02 | yes | same | yes, attribute shape shown in "After" clause | yes ("decide is acme's Hot->Warm migration rate consistent with the configured tier policy") | PASS |
| US-03 | yes | same | yes, dual-emission shape described in "After" clause | yes ("decide did the last hourly evaluate run produce the expected migration volume for acme") | PASS |

Slice-level check (Dimension 0 item 5): EVERY story is `@infrastructure`.
The reviewer's blocking rule "if every story in a slice is
`@infrastructure`, the slice has no release value" applies in spirit but
is deliberately overridden here by feature scope. The override is
documented:

- In `user-stories.md` under "Note on the `@infrastructure` slice rule",
  with explicit justification that the writer's OUTPUT (NDJSON lines)
  IS itself an observable surface consumed by an already-deployed
  sidecar + collector + dashboard chain.
- In `wave-decisions.md` D9, where Andrea pre-decided the library-first
  / CLI-second split.

This is the same posture taken by the sibling Pulse-sink feature
(`cinder-to-pulse-bridge-v0/discuss/dor-validation.md`), which has
already shipped through this gate.

**Override accepted by reviewer**. The library substrate ships first;
the CLI surface ships second; the two-feature split is intentional and
makes each side independently shippable and reviewable. Two analogous
overrides have shipped successfully in this codebase
(`cinder-to-pulse-bridge-v0`, `lumen-otlp-json-writer` via
`crates/self-observe/src/lumen_otlp_json.rs`).

## Confirmation-bias detection (Dimensions 1-4 self-check)

| Dimension | Check | Verdict |
|-----------|-------|---------|
| 1.1 Technology bias | Stories specify only the library trait (`cinder::MetricsRecorder`) and the output format (OTLP-JSON `ResourceMetrics` NDJSON). No collector technology (Datadog, Prometheus, NewRelic) prescribed. The sidecar is mentioned as an existing component, not a technology choice. | PASS |
| 1.2 Happy path bias | Every story has a sad-path scenario: US-01 has "no place call means zero bytes". US-02 has "failed migrate emits no line". US-03 has "zero eligible items emits no evaluate line". | PASS |
| 1.3 Availability bias | Stories acknowledge two precedents (`LumenToOtlpJsonWriter` for shape, `CinderToPulseRecorder` for event handling). Each precedent is explicitly justified, not "same as before". | PASS |
| 2.1 Missing stakeholder perspectives | Primary stakeholder: platform operator (Priya). Secondary: Cinder maintainer (guardrail OK4 ensures no behaviour change). Tertiary: sidecar/collector operators (OK5 ensures NDJSON validity). All three represented. | PASS |
| 2.2 Missing error scenarios | Error scenarios covered: failed migrate (US-02), zero-eligible evaluate (US-03), no events fired (US-01). Best-effort emission on write failure is documented (D5) but explicitly not tested at v0 because the test substrate (Vec<u8>) cannot fail. Acknowledged in `journey-observe-cinder-via-otlp-json-visual.md` "Failure modes acknowledged" table. | PASS |
| 2.3 Missing NFRs | NFRs covered: thread-safety (Send + Sync, US-01), atomicity (Mutex<W> + per-line write, D6), best-effort emission (D5), NDJSON validity (OK5), cross-bridge consistency (D1, D3). | PASS |
| 3.1 Vague performance requirements | All "100%"/"0%" claims are quantitative and falsifiable via the named test. No "fast", "scalable", "performant" adjectives without a number. | PASS |
| 3.2 Ambiguous requirements | Every AC pins exact string literals (`"cinder.place.count"`, `"kaleidoscope.cinder"`, `"hot"`) or exact integer values (`2`, `true`, `"1"`). No multi-interpretable terms. | PASS |
| 4 Testability | Every AC is a property of the inner `Write` buffer's contents after a sequence of trait calls. Every AC is automatable via `serde_json::from_str` + field navigation. Test names are pre-written in the slice files. | PASS |
| 5.1 Largest bottleneck | The largest gap in the cross-process observability story is "Cinder events are invisible to the existing sidecar+collector chain". This feature addresses exactly that gap. The Pulse-sink sibling addresses a different (in-process) gap. | PASS |
| 5.2 Simpler alternatives considered | Considered and rejected: (a) adding the Cinder writer to the existing Lumen writer file — rejected because the two writers handle different traits with different attribute shapes. (b) extracting a shared OTLP-JSON serde module — rejected at v0 (D7, rule of three). (c) deferring the writer until the CLI wiring feature — rejected because the CLI feature would then carry the OTLP-JSON envelope work on top of CLI plumbing, making it oversized. | PASS |
| 5.3 Constraint prioritization | The dominant constraint is the cross-bridge metric-name contract with the Pulse-sink sibling (D1). It is correctly weighted as HIGH risk in the artefact registry and as a reviewer diff-check at DoD time. | PASS |
| 5.4 Data-justified | This is not a performance optimization. The decision to clone the Lumen OTLP-JSON pattern is justified by the existence of one shipped, validated precedent + one shipped, validated sibling (Pulse-sink). | PASS |

## DoR Status: PASSED

All 3 stories pass the 9-item DoR. All 13 feature-level items pass.
Anti-pattern scan clean. Dimension 0 elevator-pitch check passes with
documented all-infrastructure override (same posture as the shipped
Pulse-sink sibling). Dimensions 1-5 confirmation-bias self-check
passes.

The DISCUSS wave is ready to hand off to `nw-solution-architect` for DESIGN.
