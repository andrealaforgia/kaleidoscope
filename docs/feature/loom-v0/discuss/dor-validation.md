# DoR Validation — loom-v0

| # | Item | Evidence | Status |
|---|---|---|---|
| 1 | Personas explicit | Sasha (platform engineer maintaining the rule catalogue in Git) and Riley (SRE consuming the deployed rules) named in `user-stories.md` preamble. | ✓ |
| 2 | Jobs-to-be-Done articulated | Every story has an Elevator Pitch (Before / After / Decision enabled). The job is "make rule catalogue changes follow the same review discipline as application code". | ✓ |
| 3 | Acceptance criteria testable | Every story has 4-5 numbered ACs. ACs reference exit codes, output formats, and observable file-system behaviour — not internal state. | ✓ |
| 4 | KPIs quantitative | Four outcome KPIs (`outcome-kpis.md`), each with numeric target, measurement plan, slice anchor. | ✓ |
| 5 | Slices are elephant carpaccio | Four slice briefs implied by story 01-04 in `story-map.md`, each end-to-end in ≤ 1 day. Taste tests pass. | ✓ |
| 6 | External dependencies enumerated | Beacon (`beacon::load_rules` as runtime dep), `clap` for CLI, std filesystem. No CUE parser at v0 (TOML per ADR-0034 SPIKE). | ✓ |
| 7 | Constraints documented | System constraints (1-10) at the top of `user-stories.md`; D1-D10 in `wave-decisions.md`. | ✓ |
| 8 | Architectural anchor identified | The architecture doc §C.13 names Loom's role. Beacon ADR-0034 documents the TOML schema Loom consumes. | ✓ |
| 9 | Definition of Done articulated per story | Each story names its KPI anchor; each slice's IN scope is the operational DoD. | ✓ |

## Outcome

All 9 DoR items pass with evidence. The DISCUSS wave hand-off to
DESIGN is authorised; the architect (`@nw-solution-architect`)
should read the artefacts in this directory plus the architecture
doc §C.13 + Beacon ADR-0034 to ground the DESIGN.
