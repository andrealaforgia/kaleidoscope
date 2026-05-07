# Codex v0 — Definition of Ready validation

All nine DoR items validated for the six user stories US-CO-01
through US-CO-06.

| Item | Evidence | Verdict |
|---|---|---|
| 0. Elevator Pitch present | Every story has Before / After / Decision-enabled in `user-stories.md`. Each "After" line names a real entry point (`cargo test -p codex --test ...` or `cargo test -p spark --test slice_07_codex_lint`) and concrete observable output (a typed Result, a tracing event, an Err variant). | PASS |
| 1. Problem statement clear | Every story has a "Problem" subsection in domain language (typos shipping silently, false positives from incomplete corpus, lack of "did you mean" guidance, etc.). | PASS |
| 2. User/persona with characteristics | Sasha (platform engineer) and Riley (SRE) at acme-observability are named with role + context. | PASS |
| 3. 3+ domain examples with real data | Every story has 2-3 examples with realistic names (`payments-api`, `acme-prod`, `checkout-v2`, `exp-2026-Q2-pricing`, real attribute names like `host.name`, `process.pid`, `telemetry.sdk.language`). | PASS |
| 4. UAT scenarios in Given-When-Then | Every story has 2-3 BDD scenarios in Given-When-Then format covering happy + edge + error paths. | PASS |
| 5. AC derived from UAT | Every story has Acceptance Criteria that map 1:1 to the BDD assertions. AC outcome-focused; no implementation language. | PASS |
| 6. Right-sized | Six elephant-carpaccio slices, each ≤1 day per `story-map.md`. Each slice closes with a `cargo test` invocation. Sister-crate precedent: Sieve and Spark each shipped slices in single dispatches within wall-clock budget. | PASS |
| 7. Technical notes identify constraints | Each story has a "Technical Notes" section calling out DESIGN-wave decisions (exact `BlessedAttribute` shape, prefix-match modelling, multi-violation collection vs short-circuit, suggestion threshold calibration, ADR amendment routing for slice 06). | PASS |
| 8. Dependencies resolved or tracked | The OTel semconv pin already exists in the workspace (Aperture and Spark consume it). Spark v0.1.0 shipped; the slice-06 Spark integration is additive on `#[non_exhaustive]` SparkError, non-breaking. Inter-story dependencies tracked in `story-map.md > Prioritisation`. | PASS |
| 9. Outcome KPIs defined with targets | `outcome-kpis.md` defines six KPIs with numeric targets (100% canonical-Resource validation, 100% upstream corpus blessed, 100% unknown-attribute structured violations, 100% close-typo suggestions, 100% Spark integration surfacing, sub-10ms validation budget). All CI-enforced. | PASS |

## Wave-level checks

| Check | Evidence | Verdict |
|---|---|---|
| Nine locked scope decisions | `wave-decisions.md > Q1-Q9` (Q1-Q6 from Luna's tightened pass; Q7-Q9 added by Bea inline when Luna's open questions called for DISCUSS-locked answers). | PASS |
| Walking-skeleton slice exists and ships end-to-end | Slice 01 in `slices/slice-01-walking-skeleton.md` ships a working `SchemaCatalogue` + minimal seed corpus + integration test. Smallest end-to-end. | PASS |
| Carpaccio taste tests pass per slice | `story-map.md > Carpaccio taste tests` walks through six checks; all pass. | PASS |
| LeanUX antipattern scan | No "Implement-X" stories; no synthetic data masquerading as production; no technical AC. All stories outcome-focused. | PASS |
| Story sizing sanity | No slice ships 4+ new components; no two slices identical-except-for-scale; each slice has a named learning hypothesis. | PASS |
| Cross-feature integration discipline | Slice 06 touches Spark for integration. The change is additive on `#[non_exhaustive]`; the `SparkError::SchemaValidation` variant is non-breaking; the new `SparkConfig::with_strict_schema_lint(bool)` builder is additive. ADR-0012 (Spark error type) and ADR-0013 (Spark dep pinning) gain post-DELIVER amendment notes; routed by Bea. | PASS |

## Wave-level verdict

**READY for handoff to DESIGN.** All nine DoR items pass; the
wave-level checks pass; six slices are sized for single-dispatch
DELIVER cycles per the methodology and the Sieve / Spark precedents.

Sentinel will run the formal review pass; this validation is the
DISCUSS-side self-check that the wave has the artefacts and quality
the methodology requires.
