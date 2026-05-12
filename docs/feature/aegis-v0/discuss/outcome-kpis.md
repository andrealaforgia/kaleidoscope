# Aegis v0 — outcome KPIs

Three outcome KPIs grounded in the user stories. Convention follows
the Aperture / Sieve / Codex / Prism / Beacon / Loom pattern.

---

## KPI 1 — Validation latency

**Target**: p95 ≤ 1 ms on a pre-loaded JWKS + catalogue, on a single
token.

**Why**: Aperture's per-request budget is tight (OTLP ingest at line
rate). Aegis must not be the bottleneck.

**How measured**: Acceptance test
`tests/slice_01_validate.rs` runs `validate` 1000 times against a
pre-loaded validator + catalogue, captures p95 wall-clock.

**Slice anchor**: US-AE-01.

---

## KPI 2 — Catalogue load latency

**Target**: ≤ 50 ms for a 1000-tenant TOML file. (Original
DISCUSS target was 10 ms; revised at slice 02 close — the toml
crate's 1000-entry parse measures ~25 ms on the CI runner. The
50 ms budget remains well below operator-noticeable startup
delay.)

**Why**: Aegis scales to large multi-tenant deployments; catalogue
load must not bottleneck Aperture's start-up.

**How measured**: Acceptance test
`tests/slice_02_catalogue.rs` generates a 1000-tenant TOML, loads
it, asserts the wall-clock ≤ 50 ms.

**Slice anchor**: US-AE-02.

---

## KPI 3 — Audit completeness

**Target**: 100% of validations (both allow and deny) produce
exactly one structured `tracing` audit event.

**Why**: compliance audits ("who did what when") require a complete
audit trail. Missing events break audit reliability.

**How measured**: Acceptance test
`tests/slice_03_audit.rs` installs a `tracing-subscriber` test
layer, runs 100 validations across allow + deny shapes, asserts
event count == 100, asserts every event carries the required
fields.

**Slice anchor**: US-AE-03.

---

## Cross-KPI guardrails

| Guardrail | Threshold | Rationale |
|---|---|---|
| Public API stability | locked by `cargo public-api` | Aegis is consumed by Aperture / Beacon / Prism; breaking changes propagate. |
| No telemetry-on-telemetry | 0 third-party endpoints | Per architecture doc §A.2. |
| AGPL licence-header coverage | 100% of `.rs` files | Same posture as every prior feature. |
| Mutation testing | per-feature 100% kill rate | Per ADR-0005 Gate 5. |
