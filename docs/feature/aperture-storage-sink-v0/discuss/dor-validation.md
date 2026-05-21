# Definition of Ready Validation: aperture-storage-sink-v0

## Scope Assessment (Elephant Carpaccio Gate)

Oversized signals checked:
- User stories: 3 value stories (US-01/02/03). PASS (<= 10).
- Bounded contexts/modules: 1 new sink crate consuming aperture + 3 pillars via
  stable ports. PASS (the pillars are pre-existing libraries, not new contexts).
- Walking skeleton integration points: brownfield; Slice 01 is the de-facto
  skeleton. PASS.
- Estimated effort: 4-6 days across 3 slices. PASS (<= 2 weeks).
- Independent outcomes: 3 signals could ship separately — and ARE sliced that way.

**Verdict: PASS — right-sized.** Already sliced one signal per slice; each slice
is end-to-end and shippable. No re-slicing required.

---

## DoR Checklist

### Story: US-01 (Logs to lumen)

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear, domain language | PASS | "lumen has no production consumer; accepted telemetry disappears" |
| User/persona with specific characteristics | PASS | Priya Nair, platform operator, self-hosted stack |
| 3+ domain examples with real data | PASS | checkout-api/order 1001; billing-worker/globex; no-tenant refusal |
| UAT in Given/When/Then (3-7) | PASS | 5 scenarios (persist, fidelity, restart, tenant override, refusal) |
| AC derived from UAT | PASS | 5 AC, each traceable to a scenario |
| Right-sized (1-3 days, 3-7 scenarios) | PASS | 1 signal, 5 scenarios, ~2 days |
| Technical notes: constraints/dependencies | PASS | Translation map, FileBackedLogStore::open, Probe, deps listed |
| Dependencies resolved or tracked | PASS | aperture ports + lumen available; crate placement = Q2 (tracked) |
| Outcome KPIs with measurable targets | PASS | KPI-1 100% post-restart fidelity, baseline 0% |

### Story: US-02 (Traces to ray)

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear, domain language | PASS | "ray has no production consumer; spans lost" |
| User/persona with specific characteristics | PASS | Priya Nair, platform operator |
| 3+ domain examples with real data | PASS | two-span checkout trace; events+links; malformed trace id |
| UAT in Given/When/Then (3-7) | PASS | 5 scenarios |
| AC derived from UAT | PASS | 5 AC traceable |
| Right-sized | PASS | 1 signal, 5 scenarios, ~2 days; scaffold reused |
| Technical notes | PASS | Translation map to ray types, FileBackedTraceStore::open, deps |
| Dependencies resolved or tracked | PASS | Depends on US-01 scaffold (tracked); ray available |
| Outcome KPIs with measurable targets | PASS | KPI-2 100% post-restart fidelity |

### Story: US-03 (Metrics to pulse)

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear, domain language | PASS | "pulse has no production consumer; points lost" |
| User/persona with specific characteristics | PASS | Priya Nair, platform operator |
| 3+ domain examples with real data | PASS | cpu gauge 0.42; request.count sum 7 + http.route; histogram refusal |
| UAT in Given/When/Then (3-7) | PASS | 5 scenarios |
| AC derived from UAT | PASS | 5 AC traceable |
| Right-sized | PASS | 1 signal, 5 scenarios, ~2 days; gauge+sum only |
| Technical notes | PASS | Translation map to pulse types, FileBackedMetricStore::open, deps |
| Dependencies resolved or tracked | PASS | Depends on US-01 scaffold (tracked); pulse available |
| Outcome KPIs with measurable targets | PASS | KPI-3 100% post-restart fidelity |

---

## DoR Status: PASSED (all three stories, all 9 items)

Open questions tracked for DESIGN (do not block DoR):
- Q1: tenant-resolution attribute key name (stories assume tenant.id-then-default-then-refuse).
- Q2: crate placement (new aperture-storage-sink crate; aperture must not depend on pillars).
