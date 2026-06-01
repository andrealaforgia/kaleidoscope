# Definition of Ready — gateway-tracing-subscriber-v0

9-item hard gate. Each item passes with evidence.

| # | DoR Item | Status | Evidence |
|---|---|---|---|
| 1 | Problem statement clear, domain language | PASS | Each story opens with a named-operator problem in operability terms (Priya Nair, Marco Bianchi); no solution prescribed in the problem. |
| 2 | User/persona with specific characteristics | PASS | Platform SRE (Priya Nair) running the ingest tier; developer (Marco Bianchi) on a bare run. `## Who` block per story. |
| 3 | 3+ domain examples with real data | PASS | Each of US-01..US-04 has 3 examples with real paths (`/srv/kaleidoscope/pillar`), tenants (`acme`), substrate classes (`fsync-noop`, `sink`), and named operators. |
| 4 | UAT in Given/When/Then (3-7 scenarios) | PASS | US-01: 3, US-02: 3, US-03: 3, US-04: 2. Scenario titles describe operator outcomes, not implementation. |
| 5 | AC derived from UAT | PASS | Each story's `## Acceptance Criteria` maps 1:1 from its scenarios. |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | 1 crate touched, 0 new integration points, ~1 day. Scope Assessment: PASS in story-map.md. |
| 7 | Technical notes: constraints/dependencies | PASS | `## System Constraints` (anti-coupling, no new crate, WIRE contract) + per-story Technical Notes pinning the install-point constraint. |
| 8 | Dependencies resolved or tracked | PASS | story-map.md `## Dependencies`: install-point precedence (US-02 stricter than US-01) and `tracing-subscriber` add (flag 1), both tracked, neither blocking. |
| 9 | Outcome KPIs defined with measurable targets | PASS | outcome-kpis.md K1-K5, each with numeric/boolean target and a black-box measurement method. |

## Verifier WIRE contract (verifier-007) — pinned

The verifier asserts only the rendered stderr shape; the install
wiring/home is the implementer's choice. Exact events the gateway emits,
read from source (for the landing ping to the verifier):

| Event | Level | Fields | Site (today) | Renders today? |
|---|---|---|---|---|
| `gateway_starting` | info | `pillar_root` | main.rs:89 | NO (pre-install) |
| `health.startup.refused` | error | `substrate`, `reason` | main.rs:102 (probe_or_refuse fail arm) | NO (pre-install) |
| `listener_bound` | info | `transport`, `addr` | aperture transport.rs:47/114, inside spawn | YES (post-install) |

Root cause: the gateway calls `aperture::spawn`, which installs the
subscriber inside `compose::spawn` (compose.rs:111). But
`gateway_starting` and `health.startup.refused` fire in the gateway's
own `main` BEFORE that call, so they are dropped. The fix installs the
subscriber before main.rs line 102.

## Decisions recorded

- Decision 1 feature type: Backend (operability).
- Decision 2 walking skeleton: No (brownfield defect closure).
- Decision 3 research depth: Lightweight.
- Decision 4 JTBD: No.

## Notes

- Peer review: not run for this wave (self-contained, per orchestrator
  instruction). Item 18 of the skill's success criteria is waived for
  this delivery by explicit constraint.
- DIVERGE artifacts: none present for this feature; not required for an
  operability defect closure with a fixed WIRE contract. Noted as a
  non-blocking gap in wave-decisions.md.
