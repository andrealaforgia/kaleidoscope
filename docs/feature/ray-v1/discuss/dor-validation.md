# Ray v1 — Definition of Ready validation

All nine DoR items PASS.

1. **Problem statement clear, domain language** — PASS. Ray v0
   loses every span on restart (in-memory only); v1 makes ingested
   spans survive a process restart. See user-stories.md elevator
   pitches.

2. **User / persona identified** — PASS. The operator is the
   platform binary embedding Ray in a long-lived process.

3. **3+ domain examples with real data** — PASS. Each story has
   three examples using concrete tenants (`acme`, `globex`),
   concrete services (`checkout`, `payments`), concrete trace ids
   and concrete timestamps.

4. **UAT scenarios (3-7)** — PASS. US-RV1-01 has 6 scenarios,
   US-RV1-02 has 3. Rendered as Rust `#[test]` by DISTILL (no
   Gherkin in this project).

5. **AC derived from UAT** — PASS. 10 ACs for Slice 01, 4 for
   Slice 02, each traceable to a scenario.

6. **Right-sized (1-3 days, 3-7 scenarios)** — PASS. Two carpaccio
   slices ~1 day each; 6 and 3 scenarios. Scope assessment PASS in
   story-map.md. The dual-index wrinkle is a replay-routing detail,
   not a scope expansion.

7. **Technical notes: constraints / dependencies** — PASS. System
   Constraints section in user-stories.md; format, error-variant,
   dual-index, snapshot, serde and recorder decisions in
   wave-decisions.md D3-D11.

8. **Dependencies resolved or tracked** — PASS. aegis (TenantId),
   serde, serde_json — all already in the workspace and used by the
   four prior v1 adapters. No blocking questions. The one new
   internal dependency is the serde derives on the v0 span types
   (D7), tracked and additive.

9. **Outcome KPIs defined with measurable targets** — PASS. KPI 1
   ingest p95 ≤ 2 ms; KPI 2 recovery p95 ≤ 2.5 s; KPI 3 durability
   100%. CI-realism margin baked in from the first commit per the
   2026-05-19 lesson. See outcome-kpis.md.

## DoR Status: PASSED
