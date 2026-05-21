# Strata v1 — Definition of Ready validation

All nine DoR items PASS.

1. **Problem statement clear, domain language** — PASS. Strata v0
   loses every profile on restart (in-memory only); v1 makes
   ingested profiles survive a process restart with their full
   pprof payload. See user-stories.md elevator pitches.

2. **User / persona identified** — PASS. The operator is the
   platform binary embedding Strata in a long-lived process.

3. **3+ domain examples with real data** — PASS. Each story has
   three examples using concrete tenants (`acme`, `globex`),
   concrete services (`checkout`, `payments`), concrete profile
   types (`cpu`, `heap`) and concrete timestamps.

4. **UAT scenarios (3-7)** — PASS. US-SV1-01 has 6 scenarios,
   US-SV1-02 has 3. Rendered as Rust `#[test]` by DISTILL (no
   Gherkin in this project).

5. **AC derived from UAT** — PASS. 10 ACs for Slice 01, 4 for
   Slice 02, each traceable to a scenario.

6. **Right-sized (1-3 days, 3-7 scenarios)** — PASS. Two carpaccio
   slices ~1 day each; 6 and 3 scenarios. Scope assessment PASS in
   story-map.md. Strata's single per-service index is the simplest
   of the v1 set; the only distinguishing factor is payload weight,
   a KPI-budget concern, not a scope expansion.

7. **Technical notes: constraints / dependencies** — PASS. System
   Constraints section in user-stories.md; format, error-variant,
   single-index, touched-bucket-sort, serde, payload-weight,
   snapshot and recorder decisions in wave-decisions.md D3-D12.

8. **Dependencies resolved or tracked** — PASS. aegis (TenantId),
   serde, serde_json — all already in the workspace and used by the
   five prior v1 adapters. No blocking questions. The one new
   internal dependency is the serde derives on the v0 profile types
   (D6), tracked and additive; D6a confirms plain derive is correct
   (no byte blob, so no base64 question).

9. **Outcome KPIs defined with measurable targets** — PASS. KPI 1
   ingest p95 ≤ 8 ms (raised for payload weight, with reasoning);
   KPI 2 recovery p95 ≤ 2.5 s; KPI 3 durability 100%. CI-realism
   margin baked in from the first commit per the 2026-05-19 lesson.
   See outcome-kpis.md.

## DoR Status: PASSED
