# Peer review — Spark v0 DISCUSS

- **Date**: 2026-05-06
- **Reviewer**: `@nw-product-owner-reviewer` (Sentinel)
- **Wave**: DISCUSS (Luna's overnight pass, orchestrated by Bea)
- **Artefact set**: `docs/feature/spark/discuss/` at commit `284a605`
- **Verdict**: **APPROVED** — ready for handoff to DESIGN (Morgan)
- **Iteration**: 1 of 2 — no revisions required

---

## Executive summary

The Spark v0 DISCUSS wave is comprehensively executed, coherent in journey
design, and ready for immediate handoff to DESIGN. All six user stories
pass the Definition of Ready hard gate with explicit evidence. The journey
design is exceptionally tight; the elephant-carpaccio story map executes
cleanly with zero antipatterns; the shared-artifacts registry is exemplary.
Zero critical blocking issues. One minor polish suggestion noted for future
iterations.

**Next step**: hand off to `@nw-solution-architect` (Morgan) for DESIGN-wave
technology and API-shape decisions.

---

## Critical feedback

`praise:` All DoR items passed (9/9 on all six stories). No stories are
blocked. No iteration cycle needed.

## Blocking issues

None.

---

## High-priority findings

### Journey coherence — verified

All five backbone activities present and coherent:

1. **Configure** — `SparkConfig` builder pattern forces `service.name` at
   construction; optional builder methods for `tenant_id`, `feature_flags`,
   `experiment_id`, `endpoint`, `flush_timeout`.
2. **Lint** — `spark::init` validates config synchronously; returns
   `Err(SparkError::MissingRequiredAttribute { name })` before any OTel SDK
   type is constructed.
3. **Initialise SDK** — `opentelemetry_sdk::Resource` composed with all
   configured house attributes; OTel global providers set; `SparkGuard`
   returned.
4. **Emit telemetry** — Standard OTel API calls
   (`opentelemetry::global::tracer(...).in_span(...)`); every signal
   inherits the Resource.
5. **Shutdown / flush** — `SparkGuard::Drop` calls `force_flush` synchronously
   with configured deadline (default 5 s); observable via `tracing` events
   (INFO on clean flush, WARN on deadline).

No orphan steps. No dead ends. Error paths clear: lint failures exit early
with precise diagnostics, never proceed to init.

**Evidence**: `journey-spark.yaml`, `journey-spark-visual.md`,
`journey-spark.feature`. Cross-checked against all five steps.

### Shared-artifact registry — exemplary

Every `${variable}` referenced in journey or stories is registered with
six mandatory fields: source of truth, displayed-as form, consumers, owner,
integration risk (HIGH/MEDIUM/LOW), validation. Tracing `${service_name}`
from journey Step 1 → Step 3 → Step 4 reveals no drift. Values are
realistic ("payments-api", "acme-prod"), not placeholders. HIGH-risk items
(public API surface, error variants, house-attribute names) are correctly
scoped and defended by CI invariants.

**Evidence**: `shared-artifacts-registry.md`. Verified against
`journey-spark.yaml`, `user-stories.md`, `journey-spark.feature`.

### Story map — elephant-carpaccio executed cleanly

Six slices, each thin end-to-end, each independently valuable, each
demoable in a single session:

1. **Slice 01** — walking skeleton: one span round-trips OTel → OTLP →
   Aperture with house attributes on Resource.
2. **Slice 02** — init error paths: lint variants return precise diagnostics
   before SDK construction.
3. **Slice 03** — feature flags + experiment.id: four house attributes on
   every trace.
4. **Slice 04** — env-var precedence: endpoint resolution honours
   `OTEL_EXPORTER_OTLP_ENDPOINT` with `SparkConfig::with_endpoint` taking
   precedence.
5. **Slice 05** — logs and metrics: all three OTLP signal types carry the
   same four-attribute Resource.
6. **Slice 06** — bounded flush: `SparkGuard::Drop` is observable and
   bounded; deadline-exceeded is loud, never silent.

Dependency graph is acyclic. No slice forward-references a later one. The
six taste tests (end-to-end, demonstrable, independently valuable,
right-sized, vertical not horizontal, riskiest-assumption-first) all pass.

**Evidence**: `story-map.md`, slice briefs.

### Definition of Ready — all 9 items, all 6 stories

| Item | Verdict |
|---|---|
| 0. Elevator Pitch present | All six stories |
| 1. Problem statement clear | Domain language, no technical jargon |
| 2. User/persona with characteristics | Named with role + context |
| 3. 3+ domain examples with real data | Realistic names (payments-api, acme-prod, exp-2026-Q2-pricing) |
| 4. UAT scenarios (3–7 in Given/When/Then) | Range 3–5 per story; happy + edge + error |
| 5. AC derived from UAT | Outcome-focused; no implementation language |
| 6. Right-sized | Each story 1–3 days; each slice demoable in single session |
| 7. Technical notes identify constraints | Names DESIGN-wave decisions Morgan must make |
| 8. Dependencies resolved or tracked | Aperture v0.1.0 shipped; inter-story deps documented |
| 9. Outcome KPIs defined with targets | Six primary KPIs (KPI 1 binary; KPI 2–6 ratios at 100% UAT pass) |

**Evidence**: `dor-validation.md`. Every story's DoR status: PASSED.

### Outcome KPIs — measurable and guardrailed

All six KPIs are CI-enforced at v0. Baselines are greenfield (zero today;
the v0 launch establishes the practice). The Guardrail Metrics section
names four P0 gates: no-telemetry-on-telemetry, single-init invariant,
`forbid(unsafe_code)`, 100% mutation kill rate.

**Evidence**: `outcome-kpis.md`. Cross-checked against `user-stories.md`
KPI sections.

### Antipattern scan — zero critical patterns detected

| Antipattern | Result |
|---|---|
| Implement-X stories | PASS — all stories name user outcomes, never "Implement", "Add", "Create" |
| Generic data | PASS — example data is realistic, no `user123` or `test@test.com` |
| Technical acceptance criteria | PASS — all AC are outcome-focused |
| Giant stories | PASS — six stories; each 1–3 days; each 3–5 scenarios |
| No examples | PASS — all six have 3+ grounded examples with realistic project names |
| Tests after code | PASS — every story embeds BDD UAT before implementation |
| Vague personas | PASS — all personas named with role + context |
| Missing edge cases | PASS — every story covers happy, edge, and error |

---

## Suggestions for Morgan (DESIGN)

`suggestion (non-blocking):` These are DESIGN-wave decisions DISCUSS has
deliberately deferred. Each is named in the relevant story's Technical
Notes section.

1. **OTel semconv version verification.** At the harness's pinned
   `opentelemetry-proto = 0.27.0`, confirm the exact attribute names. If
   semconv diverges from Spark's `feature_flag.*` choice, document the
   migration path in a DESIGN note for Codex Phase 0+.

2. **OTel SDK version pin.** Lock `opentelemetry-otlp` minor version in a
   DESIGN ADR mirroring harness ADR-0003. Name the migration path if a
   future minor version breaks compatibility.

3. **Flush-timeout mechanism.** Decide sequential-vs-concurrent
   three-provider flush and per-provider deadline division. If OTel SDK
   counters are unavailable, accept "best-effort" drained/dropped counts
   with a documented caveat.

4. **GlobalAlreadyInitialised test mechanism.** If OTel's global state
   cannot be reset between tests, flag this for DEVOPS and use a one-shot
   `[[test]]` declaration in `Cargo.toml`.

5. **`SparkGuard` posture.** Is it `#[must_use]`? Does it expose any public
   fields or is it fully opaque? The journey assumes opaque.

---

## Polish suggestions

`nitpick (non-blocking):` Optional polish for future DISCUSS waves; do not
block handoff.

1. **System Constraints boundary.** Constraint #7 in `user-stories.md`
   names the closed `SparkError` variant set, marked DISCUSS-locked. One
   sentence clarifying the DISCUSS/DESIGN boundary here would help: "The
   variant names are DISCUSS-locked; the `#[non_exhaustive]` attribute and
   the `#[derive(...)]` mechanism are DESIGN decisions."

2. **Registry CI-invariant mechanism column.** The shared-artifacts-registry's
   CI-invariants table names invariants and owners but not the test
   mechanism. Adding a "Mechanism" column or a note repeating the test
   location from the story's Technical Notes would make the registry
   independently readable for DEVOPS.

3. **Elevator Pitch entry-point template guidance.** Every Elevator Pitch
   "After" line names two entry points (runtime + test). This is exemplary.
   Adding one sentence to the template at the top of `user-stories.md` would
   make the practice explicit for future DISCUSS waves.

---

## Praise

`praise:` Exceptional journey design. The shift from "unproven integration"
through "standard path works" to "trustworthy" is perfectly calibrated for
a library journey. The wire-level traces in the visual file prove the
contract without ambiguity.

`praise:` Exemplary story template execution. Every story's Elevator Pitch
names a runtime entry point and a test entry point. Every "Decision enabled"
line names a real decision the consumer makes. The Problem statements are
in domain language, never technical jargon.

`praise:` Antipattern-free. Zero Implement-X stories. Zero generic data.
Zero technical AC. All six stories are outcome-focused, outcome-sized, and
outcome-demonstrated. This is rare at DISCUSS; Luna's discipline is
commendable.

`praise:` Comprehensive shared-artifact registry. Every variable has six
fields; HIGH-risk items are correctly scoped; CI invariants are named with
owners. This is the reference implementation for how to document integration
contracts.

---

## Approval decision

**APPROVED.** All gates passed. Zero critical blocking issues. Polish
suggestions recorded but do not block handoff.

**Iteration budget**: 0 of 2 iterations used. No revisions required.

**Handoff target**: `@nw-solution-architect` (Morgan). Read in order:

1. `wave-decisions.md` — locked decisions DESIGN starts from.
2. `journey-spark.yaml` — structured contract.
3. `user-stories.md` — six stories with embedded AC and KPIs.
4. `story-map.md` — six-slice elephant-carpaccio dependency graph.
5. Slice briefs in `slices/` — per-slice demo commands and acceptance proofs.
6. `shared-artifacts-registry.md` — every `${variable}` source and consumer.

**Handoff timeline**: ready for immediate DESIGN handoff.
