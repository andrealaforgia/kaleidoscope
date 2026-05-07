# Peer review — Codex v0 DISCUSS

- **Date**: 2026-05-07
- **Reviewer**: `@nw-product-owner-reviewer` (Sentinel)
- **Wave**: DISCUSS (Luna's tightened pass plus Bea's recovery for the
  remaining seven artefacts; recovery documented in commit `f747a49`)
- **Artefact set**: `docs/feature/codex/discuss/` plus six slice
  briefs at `docs/feature/codex/slices/`
- **Verdict**: **APPROVED** — handoff to DESIGN
- **Critical issues**: 0
- **Blocking issues**: 0
- **Iteration**: 1 of 2 — no revisions required

---

## Executive summary

Codex v0 DISCUSS is rigorous and complete. Nine scope decisions
locked with rationale and rejected alternative each (Q1-Q6 from
Luna's pass; Q7-Q9 from Bea's inline answers to Luna's open
questions). All six user stories carry mandatory Elevator Pitches
whose "After" lines name real entry points and concrete observable
output. Six elephant-carpaccio slices each ≤1 day with named
learning hypotheses; carpaccio taste tests pass. Six numeric
outcome KPIs, all CI-enforced. DoR validated on all nine items.
Zero LeanUX antipatterns.

The cross-feature integration in Slice 06 (Spark adds Codex as
runtime dep, gains a new `SparkError::SchemaValidation` variant) is
additive-only on `#[non_exhaustive]` and is the first real
validation that the Spark error type's non-exhaustive discipline
works as intended. Confidence-building.

---

## Dimension scores (per the methodology checklist)

| Dimension | Verdict | Evidence |
|---|---|---|
| Scope decisions (9 locked) | PASS | Q1-Q9 each carry rationale + rejected alternative |
| User stories (6 complete) | PASS | Elevator Pitches, real entry points, realistic data, BDD scenarios, traceable AC |
| Story map | PASS | Backbone present; walking skeleton thin; six slices ≤1 day; carpaccio taste tests pass |
| Outcome KPIs (6 numeric) | PASS | Each numeric, CI-enforced, with measurement mechanism |
| Definition of Ready | PASS | All 9 items + wave-level checks pass with explicit evidence |
| LeanUX antipatterns | PASS | None found |
| Journey coherence | PASS | Mental model coherent, emotional arc rising, error paths documented, shared artefacts tracked |
| House style | PASS | British English, no human-effort estimation, library framing, AGPL containment |
| Cross-feature integration | PASS | Slice 06 Spark touches additive-only on `#[non_exhaustive]` |

---

## Per-artefact findings

### `wave-decisions.md` — APPROVED

`praise:` Tight scope discipline. Q1-Q6 cover the load-bearing
choices (library vs service, corpus shape, version pinning, tenant
scope, lint shape, integration mechanism) with substantive rejected
alternatives. Q7-Q9 (corpus regeneration ritual, Levenshtein
in-tree, warn-mode tracing shape) are the kinds of operational
decisions teams often defer to DESIGN; locking them here saves
iteration cycles when the architect picks up.

The "Out of scope for v0" trailer is exhaustive and matches the
deferrals named throughout the artefact set: gRPC service surface,
FoundationDB-backed multi-version catalogue, CUE-based corpus,
per-tenant extensions, HTML rendering, Aperture-side lint
integration. All explicitly v1+.

### `user-stories.md` — APPROVED

`praise:` Every Elevator Pitch's "After" line names a real entry
point (a `cargo test` invocation) and concrete observable outcome
(typed `Result`, `LintReport`, `tracing::warn!` event, or
`SparkError::SchemaValidation` variant). This is the gold standard
already established by Sieve and Spark.

Domain examples uniformly use realistic data: `payments-api`,
`acme-prod`, `checkout-v2`, `exp-2026-Q2-pricing`, real OTel
attribute names like `host.name` and `process.pid`. No `user123`-
style placeholders. The attention to attribute-name plausibility is
particularly important for a schema-validation feature where
generic data would mask the integration realism the lint exists to
protect.

### `story-map.md` — APPROVED

`praise:` Walking skeleton is minimal (slice 01: `SchemaCatalogue::new()`
plus one validate call returning `Ok(())`). Subsequent slices
extend cleanly: slice 02 fills the corpus, slice 03 adds the house
attributes, slice 04 lights up the Err path, slice 05 adds
suggestions, slice 06 lands the Spark integration. The dependency
graph is acyclic and respects learning leverage — slice 01 locks
the catalogue shape early so subsequent reshaping cost is bounded.

Carpaccio taste tests all six pass with explicit evidence cited per
check. No slice ships 4+ new components, no two slices are
identical-except-for-scale, no slice runs only on synthetic data.

### `outcome-kpis.md` — APPROVED

Six KPIs, each numeric (100% canonical-Resource validation, 100%
upstream corpus blessed, 100% structured violations, 100% close-typo
suggestions, 100% Spark integration surfacing, sub-10ms validation
budget), each CI-enforced via per-slice integration tests, each
measurable without reading source code.

### `dor-validation.md` — APPROVED

`praise:` Wave-level DoR self-check is rigorous. The table walks all
nine items plus six wave-level checks, cites evidence per entry,
concludes "READY for handoff to DESIGN." Self-policing catches
missing artefacts before the formal review pass.

### `journey-codex.yaml` — APPROVED

Mental model coherent: Sasha believes Codex catches typos at
emission time, the catalogue codifies the pinned OTel semconv plus
the three Kaleidoscope-house attributes, the lint runs inside
`spark::init`. Riley believes she will never see typo'd attributes
in dashboards because they were caught upstream.

Emotional arc rises from "uncertain about attribute names" through
"configured / validated / surfaced" to "trustful". Error paths are
exhaustively documented (close typo with suggestion, far typo
without, empty `feature_flag.` rejected, multiple typos collected
not short-circuited).

### `shared-artifacts-registry.md` — APPROVED

All artefacts registered with source-of-truth, consumers,
integration risk, validation. HIGH-risk: pinned OTel semconv
version, `SchemaCatalogue` public type, `LintReport`/`LintViolation`
types. MEDIUM-risk: Kaleidoscope-house attribute set, generated
corpus file, Levenshtein implementation. LOW-risk: tracing target,
suggestion string template, builder method name. CI invariants
table at the bottom names per-invariant owner and mechanism.

### Six slice briefs — APPROVED

Each brief: outcome added, what it lights up, demo command,
acceptance summary, complexity drivers, out of scope. ≤100 lines
each. Demo commands are reproducible. Cross-feature touches in
slice 06 (Spark) are clearly scoped: additive-only on
`#[non_exhaustive]`, default-warn / opt-in-strict, ADR amendments
routed post-DELIVER per Sieve precedent.

---

## Antipattern scan

Zero antipatterns detected:

- No "Implement-X" stories. All names are user outcomes.
- No generic placeholder data. Realistic names throughout.
- No technical AC. Every AC is outcome-focused.
- No giant stories. Six 1-day slices, 2-3 BDD scenarios per story.
- No tests-after-code language. UAT-first throughout.
- No vague personas. Sasha and Riley named with role and context.
- No missing edge cases. Every story covers happy + edge + error.

---

## Suggestions (non-blocking, polish)

`suggestion (non-blocking):` `story-map.md` line ~46 names the
slice-06 demo as `cargo test -p spark --test slice_07_codex_lint`
("or similar"). `slice-06-spark-integration.md` later uses
`schema_validation_init`. Pinning the exact test name in
`story-map.md` would tighten the trail; alternatively, keep the "or
similar" hedge as intentional DESIGN-phase flexibility. Either
posture is fine.

`suggestion (non-blocking):` Q7's corpus regeneration ritual leaves
the exact mechanism (Rust binary versus shell script versus
`build.rs` companion) to DESIGN. The DISCUSS lock — checked-in
generated artefact regenerated by maintainer ceremony — is the load-
bearing decision; the mechanism shape is correctly DESIGN-scope.

`suggestion (non-blocking):` US-CO-05 Scenario 3 shows
`nearest_blessed_match is Some("feature_flag.{key}") or similar`.
The ambiguity is intentional flexibility (literal template versus
corrected user attribute). DESIGN to settle. Acceptance criteria
already name the format to be confirmed.

`suggestion (non-blocking):` US-CO-06 Technical Notes mentions
"Spark v0.2.0 if Spark ever goes there" as the hypothetical version
for the new variant. Current Kaleidoscope practice is `vN.0.0`
releases per crate. The note as written is accurate and harmless;
no change needed.

---

## Praise

`praise:` Tight scope discipline across nine decisions. Q7-Q9 in
particular are exemplary — operational decisions about regeneration
ritual, dependency posture, and tracing-event verbosity that teams
often defer until mid-implementation are locked here, saving
iteration cycles when DESIGN starts.

`praise:` Production-shaped data uniformly. `payments-api`,
`acme-prod`, `checkout-v2`, `exp-2026-Q2-pricing`, real OTel
attribute names. For a schema-validation feature where generic data
would mask integration realism, the discipline is load-bearing.

`praise:` Slice 06 is the first real validation that
`#[non_exhaustive]` on `SparkError` works as intended. The new
variant lands additive, non-breaking. Confidence-building milestone
for the Spark architecture.

`praise:` Elephant-carpaccio discipline is tight. Six slices ≤1 day
each, each with a named learning hypothesis. Slice 01 locks the
catalogue shape early so reshaping cost is bounded. Slice 05
validates the Levenshtein-distance threshold before slice 06 lands
the Spark integration; failure surfaces early.

`praise:` Wave-level DoR self-check (`dor-validation.md`) is rigorous
self-policing. The table walks all nine items plus six wave-level
checks, cites evidence per entry, concludes "READY". This is how
the methodology should run when the orchestrator and the agent
share the discipline.

---

## Approval

**APPROVED** for handoff to DESIGN.

- Critical issues: 0
- Blocking findings: 0
- Iteration budget: 1 of 2 used. No revisions required.
