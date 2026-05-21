# Definition of Ready Validation: durable-stores-integration-v0

Hard gate. Each story must pass all 9 items with evidence before DESIGN handoff.

## Story: US-01 — Second triad recovers identically across a platform restart

| DoR Item | Status | Evidence |
|----------|--------|----------|
| 1. Problem statement clear, domain language | PASS | Priya cannot prove metrics+traces+profiles survive a restart together; reasons across three disconnected per-crate suites and hopes. |
| 2. User/persona with specific characteristics | PASS | Priya Nair, platform reliability engineer, storage-plane trust owner, restarting the process (deploy/reboot/OOM). |
| 3. 3+ domain examples with real data | PASS | Happy (acme: 3 metric points, 2-span trace on `checkout`, 1 cpu profile), edge (globex parallel isolation), error (ray returns 0 spans -> count mismatch). |
| 4. UAT in Given/When/Then (3-7) | PASS | 3 scenarios: restart survival; cross-pillar isolation; pillar-loses-data caught. |
| 5. AC derived from UAT | PASS | 6 AC, each traced to a scenario. |
| 6. Right-sized (1-3 days, 3-7 scenarios) | PASS | ~0.4 day, 3 scenarios, one test mirroring first-triad test 1. |
| 7. Technical notes: constraints/dependencies | PASS | dev-deps (ray, strata), `[[test]]` block, helper reuse, NoopRecorder, false-PASS path guard. |
| 8. Dependencies resolved or tracked | PASS | Depends on US-02 co-location (same file/wiring); pulse/ray/strata/aegis shipped and read-confirmed. No external blockers. |
| 9. Outcome KPIs with measurable targets | PASS | KPI-1: 100% compose-and-recover fidelity, zero leakage; baseline 0%; measured by the test target's `test result: ok`. |

### DoR Status: PASSED

### Dimension 0 (Elevator Pitch): PASS
- Presence: Before / After / Decision enabled all present.
- Real entry point: `cargo test -p integration-suite --test v1_three_durable_stores_compose` — a real, runnable target (honest backend entry point; no fake CLI invented).
- Concrete output: `test result: ok. 2 passed` (observable stdout).
- Job connection: Priya decides whether to declare the durable plane trustworthy and green-light the next milestone.

---

## Story: US-02 — Cross-crate tenant-identity contract for signals `@infrastructure`

| DoR Item | Status | Evidence |
|----------|--------|----------|
| 1. Problem statement clear, domain language | PASS | A maintainer could drift `aegis::TenantId`'s shape and silently break per-tenant keying across pillars; the signal triad lacks the contract test the first triad has. |
| 2. User/persona with specific characteristics | PASS | Maintainer touching aegis or a signal-pillar store; wants build-time failure, not 3am production isolation bug. |
| 3. 3+ domain examples with real data | PASS | Happy (one `shared` identity, len==1 in each pillar), edge (no per-adapter tenant type), error (shape drift breaks compilation). |
| 4. UAT in Given/When/Then (3-7) | PASS | 2 scenarios: one identity honoured by all three; no conversion required. |
| 5. AC derived from UAT | PASS | 3 AC, each traced to a scenario/example. |
| 6. Right-sized | PASS | ~0.1 day, 2 scenarios, mirrors first-triad test 2. |
| 7. Technical notes: constraints/dependencies | PASS | Co-located in the same file/wiring as US-01; shared helpers. |
| 8. Dependencies resolved or tracked | PASS | Depends on US-01's dev-dep additions; no external blockers. |
| 9. Outcome KPIs with measurable targets | PASS | KPI-2: 100% — drift is a compile failure; baseline none; measured by compile+pass of the target. |

### DoR Status: PASSED

### Dimension 0 (Elevator Pitch): N/A — honestly `@infrastructure`
- No Elevator Pitch claimed; the story enables no standalone operator decision.
- Slice-level value rule satisfied: US-02 ships in the same feature/slice family as US-01 (a user-visible story), so the release is not all-infrastructure.

---

## Feature-level DoR: PASSED

Both stories pass all 9 items. Slice-level value is satisfied (US-01 is
user-visible). Two scenarios across the feature are explicit "sad path"
coverage (cross-pillar leakage, pillar-loses-data), guarding against happy-path
bias. Scope assessed as right-sized (story-map.md). Ready for peer review.
</content>
