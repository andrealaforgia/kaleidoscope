# DoR Validation — aegis-v0

| # | Item | Evidence | Status |
|---|---|---|---|
| 1 | Personas explicit | Sasha (platform engineer wiring tenant identity) and Riley (SRE answering audits) named. | ✓ |
| 2 | Jobs-to-be-Done articulated | Three stories with Elevator Pitches. The job is "make tenancy + audit a typed Rust value the rest of Kaleidoscope can consume". | ✓ |
| 3 | Acceptance criteria testable | 4-7 numbered ACs per story; every AC references typed Result variants or structured event fields. | ✓ |
| 4 | KPIs quantitative | Three outcome KPIs (validation latency p95 ≤ 1ms; catalogue load ≤ 10ms; audit completeness 100%). | ✓ |
| 5 | Slices are elephant carpaccio | Three slices implied by stories 01-03; each ≤ 1 day. | ✓ |
| 6 | External dependencies enumerated | `jsonwebtoken` for JWT decode + verify, `toml` for catalogue parsing, `tracing` for audit events. No FoundationDB / OPA / Dex / Keycloak / OpenBao at v0. | ✓ |
| 7 | Constraints documented | System constraints (1-9) in user-stories.md; D1-D10 in wave-decisions.md. | ✓ |
| 8 | Architectural anchor identified | Architecture doc §C.14 names Aegis's role. ADR-0034 (Beacon's TOML SPIKE outcome) carries forward. | ✓ |
| 9 | Definition of Done articulated per story | Each story names its KPI anchor; each slice's IN scope is the operational DoD. | ✓ |

## Outcome

All 9 DoR items pass. DISCUSS → DESIGN hand-off authorised.
DESIGN collapses into the implementation commit per the Loom
slice-01 precedent — Aegis is small enough that a separate DESIGN
artefact would be ceremony.
