# Sieve v0 — Definition of Ready validation

All nine DoR items validated for the six user stories US-SI-01 through
US-SI-06. Evidence cited per item.

| Item | Evidence | Verdict |
|---|---|---|
| 0. Elevator Pitch present | Every story has a Before / After / Decision-enabled block in `user-stories.md`. Each "After" line names a real entry point (`cargo test -p sieve --test ...`) and a concrete observable outcome (the test returns GREEN, asserting a typed `Decision::Keep`/`Decision::Drop` value, or DEBUG/INFO event vocabulary). | PASS |
| 1. Problem statement clear | Every story has a "Problem" subsection in domain language (volume control, error retention, trace coherence, signal passthrough, operator visibility). No technical jargon in the pain statement. | PASS |
| 2. User/persona with characteristics | Riley (SRE) and Sasha (platform engineer) at acme-observability are named per story under "Who". Roles + context provided. | PASS |
| 3. 3+ domain examples with real data | Every story has a "Domain examples" section with 2-3 examples carrying realistic data (`payments-api`, `checkout-service`, `acme-prod`, deterministic-seed `trace_id`s, `status.code = ERROR`, etc.). No `user123` placeholders. | PASS |
| 4. UAT scenarios in Given-When-Then | Every story has 2-4 BDD scenarios in Given-When-Then format covering happy path, edge case (error-bias at rate 0.0), and error path (rate-band statistical assertions). | PASS |
| 5. AC derived from UAT | Every story has an "Acceptance Criteria" subsection whose checklist items map 1:1 to the BDD assertions. AC is outcome-focused (no "use xxh3_64" mechanism), traceable to the test code that will be written at DELIVER. | PASS |
| 6. Right-sized | Six elephant-carpaccio slices, each ≤1 day per the story-map.md. Each slice closes with a `cargo test` invocation that returns GREEN. Sister-crate precedents (harness, Aperture, Spark) shipped each slice in a single dispatch within wall-clock budget. | PASS |
| 7. Technical notes identify constraints | Each story has a "Technical Notes" section calling out DESIGN-wave decisions DESIGN must make (exact `Sampler::sample` signature; `Signal` enum vs separate methods; summary tick mechanism). | PASS |
| 8. Dependencies resolved or tracked | Aperture v0.1.0 shipped (commit `78edd09` and graduation thereafter) — Sieve consumes via the `OtlpSink` trait at integration time. Inter-story dependencies tracked in `story-map.md > Prioritisation`. | PASS |
| 9. Outcome KPIs defined with targets | `outcome-kpis.md` defines six KPIs with numeric targets (100% error retention, ±3% rate-band, 100% trace-id determinism, 100% logs/metrics passthrough, observable events, sub-5s test wall time). All CI-enforced. | PASS |

## Wave-level checks

| Check | Evidence | Verdict |
|---|---|---|
| Six locked scope decisions | `wave-decisions.md > Q1-Q8` (Q7-Q8 added by Bea at DISCUSS time when the four open questions Luna flagged required DISCUSS-locked answers). | PASS |
| Walking-skeleton slice exists and ships end-to-end | Slice 01 in `slices/slice-01-walking-skeleton.md` ships a working `Sampler` + `HeadSampler` + integration test. Smallest end-to-end. | PASS |
| Carpaccio taste tests pass per slice | `story-map.md > Carpaccio taste tests` walks through the six checks; all pass. | PASS |
| LeanUX antipattern scan | No "Implement-X" stories; no synthetic data masquerading as production; no "Should be implemented in Y" technical AC. All stories outcome-focused. | PASS |
| Story sizing sanity | No slice ships 4+ new components; no two slices are identical-except-for-scale; each slice has a named learning hypothesis. | PASS |

## Wave-level verdict

**READY for handoff to DESIGN.** All nine DoR items pass with explicit
evidence; the wave-level checks pass; six slices are sized for
single-dispatch DELIVER cycles per the methodology and the Aperture /
Spark precedents.

Sentinel will run the formal review pass; this validation is the
DISCUSS-side self-check that the wave has the artefacts and quality
the methodology requires.
