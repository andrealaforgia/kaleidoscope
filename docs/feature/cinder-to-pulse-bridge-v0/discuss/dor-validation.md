# Definition of Ready — Validation

Feature: `cinder-to-pulse-bridge-v0`
Reviewer: Luna (self-review at DISCUSS close; second pass scheduled at
handoff to DESIGN per `nw-po-review-dimensions` skill).
Date: 2026-05-18.

## Per-story DoR (9-item hard gate)

### US-01: Cinder `place` events land as queryable Pulse points

| # | DoR Item | Status | Evidence |
|---|----------|--------|----------|
| 1 | Problem statement clear, domain language | PASS | "Priya wires Cinder with NoopRecorder ... `cinder.place` evaporates into a noop. Diagnostic = patch + rebuild." Uses Cinder, Pulse, tenant — all domain terms. |
| 2 | User/persona identified | PASS | Priya the platform operator, multi-tenant fintech Kaleidoscope deployment, already uses Pulse for Lumen. Specific. |
| 3 | 3+ domain examples with real data | PASS | Three examples with real data: `acme`/`globex` tenants, `trade-2026-05-18-001` item id, explicit `Tier::Hot` / `Tier::Warm` / `Tier::Cold` cases. |
| 4 | UAT scenarios (3-7 in Given/When/Then) | PASS | 4 scenarios in user-stories.md, 4 corresponding tests planned in slice file. |
| 5 | AC derived from UAT | PASS | 7 AC bullets, each maps to an observable behaviour from a scenario. |
| 6 | Right-sized | PASS | 1 method body, 4-5 tests, ~3h effort. |
| 7 | Technical notes identify constraints | PASS | File paths, dependency to add, timestamp source pinned, slice tag. |
| 8 | Dependencies tracked | PASS | `cinder` v0.1.0 (shipped), `pulse` v0.1.0 (shipped), `aegis` (existing). No external new crates. |
| 9 | Outcome KPIs defined with measurable targets | PASS | Maps to OK1: 100% of place calls -> 1 point. Measured by green tests. |

**US-01 DoR Status: PASSED**

### US-02: Cinder `migrate` events land as queryable Pulse points with direction attributes

| # | DoR Item | Status | Evidence |
|---|----------|--------|----------|
| 1 | Problem statement clear, domain language | PASS | "Direction information dies inside NoopRecorder ... failed migrate MUST NOT produce a spurious point." Distinguishes success vs failure semantically. |
| 2 | User/persona identified | PASS | Priya, with explicit "wants failed migrations to leave no trace" wrinkle that sets US-02 apart from US-01. |
| 3 | 3+ domain examples with real data | PASS | Three examples: Hot->Warm happy path, `ghost-item` failed migrate, two-tenant opposite-direction migrations with concrete items `a1`/`g1`. |
| 4 | UAT scenarios (3-7 in Given/When/Then) | PASS | 3 scenarios, 3 corresponding tests. |
| 5 | AC derived from UAT | PASS | 4 AC bullets, each maps to an observable behaviour from a scenario. |
| 6 | Right-sized | PASS | 1 method body, 3 tests, ~2h effort. |
| 7 | Technical notes identify constraints | PASS | Reuses Slice 01 infrastructure; no new file; lowercase helper reused. |
| 8 | Dependencies tracked | PASS | Depends on US-01. Documented in Story + Slice file. |
| 9 | Outcome KPIs defined with measurable targets | PASS | Maps to OK2: 100% successful migrates -> 1 correct point, 0% failed migrates -> any point. |

**US-02 DoR Status: PASSED**

### US-03: Cinder `evaluate` events land as queryable Pulse points with per-tenant migrated counts

| # | DoR Item | Status | Evidence |
|---|----------|--------|----------|
| 1 | Problem statement clear, domain language | PASS | "evaluate_at is per-tenant aggregate ... per-item migrations must remain visible ... zero-eligible tenants get no point." All three semantic wrinkles called out in domain terms. |
| 2 | User/persona identified | PASS | Priya, with explicit "wants the dual emission to remain visible and unsurprising" + "no ghost evaluate points" wrinkles. |
| 3 | 3+ domain examples with real data | PASS | Three examples: 5-item happy path with t0 + 25h, zero-eligible at t0 + 1h, mixed 5/2 across acme/globex. |
| 4 | UAT scenarios (3-7 in Given/When/Then) | PASS | 3 scenarios, 3 tests planned. |
| 5 | AC derived from UAT | PASS | 4 AC bullets, each maps to a scenario. |
| 6 | Right-sized | PASS | 1 method body + the cross-event-type test, ~3h effort. |
| 7 | Technical notes identify constraints | PASS | `as f64` precision note, dual-emission contract note, DESIGN preservation hint. |
| 8 | Dependencies tracked | PASS | Depends on US-01 + US-02. Documented in Story + Slice file. |
| 9 | Outcome KPIs defined with measurable targets | PASS | Maps to OK3: 100% non-zero-tenant evaluates -> 1 correct point, 0% zero-eligible -> any point. |

**US-03 DoR Status: PASSED**

## Feature-level DoR

| # | Item | Status | Evidence |
|---|------|--------|----------|
| F1 | Journey artefact (visual) | PASS | `discuss/journey-observe-cinder-tier-transitions-visual.md` |
| F2 | Journey artefact (YAML schema) | PASS | `discuss/journey-observe-cinder-tier-transitions.yaml` |
| F3 | Journey artefact (Gherkin) | PASS | `discuss/journey-observe-cinder-tier-transitions.feature` |
| F4 | Shared artefact registry | PASS | `discuss/shared-artifacts-registry.md` with 6 entries, each with source/consumers/risk/validation |
| F5 | Story map | PASS | `discuss/story-map.md` with backbone + scope assessment |
| F6 | Prioritization | PASS | `discuss/prioritization.md` with V x U / E scores |
| F7 | Outcome KPIs | PASS | `discuss/outcome-kpis.md` with OK1-OK4 |
| F8 | User stories | PASS | `discuss/user-stories.md` with US-01/02/03 |
| F9 | Per-slice files | PASS | 3 files under `slices/` |
| F10 | Wave decisions | PASS | `discuss/wave-decisions.md` with D1-D7 |
| F11 | Right-sized (Elephant Carpaccio) | PASS | 3 stories, 1 bounded context, ~1 day effort. story-map.md "Scope Assessment: PASS". |
| F12 | SSOT impact assessed | PASS | wave-decisions.md D7: no SSOT modification at v0. |

## Anti-pattern scan

| Anti-pattern | Detected? | Notes |
|--------------|-----------|-------|
| Implement-X | No | All stories framed as "Priya wires the bridge ... she queries Pulse and sees ..." — outcome-first. |
| Generic data | No | Real names everywhere: Priya, acme, globex, `trade-2026-05-18-001`, `ghost-item`, `a1`, `g1`. |
| Technical AC | None blocking | AC name the metric name, attributes, and values that the bridge produces. Some AC reference the trait method names (`record_place`) because the trait IS the user-facing API at this library-level — the bridge implements `cinder::MetricsRecorder` and that is the entry point. The AC do NOT prescribe internal implementation choices (e.g. they do not say "use a `lowercase()` helper" or "store the metric name as a `const`"). |
| Technical scenario titles | No | Scenarios titled by user outcome: "Place under a tenant produces one queryable point under that tenant", not "record_place invokes pulse.ingest". |
| Oversized stories | No | Each story 3-4 scenarios, ~2-3h effort. |
| Abstract requirements | No | Every AC and scenario has a numeric/string assertion grounded in real data. |

## Reviewer self-pass (Dimension 0 — Elevator Pitch, BLOCKING)

| Story | Section present | Real entry point | Concrete output | Job connection | Verdict |
|-------|-----------------|------------------|-----------------|----------------|---------|
| US-01 | yes | `self_observe::CinderToPulseRecorder::new` + `pulse.query` | yes, sample Vec content shown | yes ("decide is acme's Hot-tier placement rate normal today") | PASS |
| US-02 | yes | same | yes, attribute map shown | yes ("decide is acme's Hot->Warm migration rate consistent with policy") | PASS |
| US-03 | yes | same | yes, dual-emission queries shown | yes ("decide did the last hourly evaluate run produce the expected migration volume") | PASS |

Slice-level check (Dimension 0 item 5): EVERY story is `@infrastructure`.
The reviewer's blocking rule "if every story in a slice is
`@infrastructure`, the slice has no release value" applies in spirit but
is deliberately overridden here by feature scope. The override is
documented in `user-stories.md` under "Note on the `@infrastructure`
slice rule" and re-justified in `wave-decisions.md` D6. Andrea
pre-decided this feature is library-only at v0; the user-visible CLI
surface ships in the post-v0 follow-up feature.

**Override accepted by reviewer**. The library substrate ships first;
the CLI surface ships second; the two-feature split is intentional and
makes each side independently shippable and reviewable.

## DoR Status: PASSED

All 3 stories pass the 9-item DoR. All 12 feature-level items pass.
Anti-pattern scan clean. Dimension 0 elevator-pitch check passes with
documented all-infrastructure override.

The DISCUSS wave is ready to hand off to `nw-solution-architect` for DESIGN.
