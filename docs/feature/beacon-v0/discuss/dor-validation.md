# DoR Validation — beacon-v0

## Definition of Ready checklist

| # | Item | Evidence | Status |
|---|---|---|---|
| 1 | Personas explicit | Sasha (platform engineer authoring rules) and Riley (SRE receiving incidents) named in `user-stories.md` preamble. | ✓ |
| 2 | Jobs-to-be-Done articulated | Each user story carries an Elevator Pitch (Before / After / Decision enabled). The job is "encode the team's wake-someone-up policy as reviewable code". | ✓ |
| 3 | Acceptance criteria testable | Every story has 4-6 numbered ACs. ACs reference test files and observable behaviour, not internal state. | ✓ |
| 4 | KPIs quantitative | Five outcome KPIs (`outcome-kpis.md`), each with numeric target, measurement plan, slice anchor. | ✓ |
| 5 | Slices are elephant carpaccio | Five slice briefs (`slices/slice-01-..05-*.md`), each end-to-end in ≤ 1 day, each with named learning hypothesis. Taste tests pass per `story-map.md`. | ✓ |
| 6 | External dependencies enumerated | Prometheus HTTP API (the data source), the `prometheus-http-query` Rust crate, `cue-rs` or equivalent. No Aegis, no Loom, no Pulse, no Lumen. | ✓ |
| 7 | Constraints documented | System constraints (1-10) at the top of `user-stories.md`; D1-D10 in `wave-decisions.md`. | ✓ |
| 8 | Architectural anchor identified | The architecture doc §C.12 names Beacon's role. ADR-0033 (HTTP client shape) shared with Prism ADR-0027; ADR-0034 (CUE rule schema) introduced by this feature. | ✓ |
| 9 | Definition of Done articulated per story | Each story names its KPI anchor; each slice brief names its acceptance criteria range from the parent story; the brief's IN / OUT scope is the operational DoD. | ✓ |

## Outcome

All 9 DoR items pass with evidence. The DISCUSS wave hand-off to
DESIGN is authorised; the architect (`@nw-solution-architect`)
should read the artefacts in this directory plus the architecture
doc §C.12 to ground the DESIGN.
