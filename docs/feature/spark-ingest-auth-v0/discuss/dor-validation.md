# Definition of Ready Validation — spark-ingest-auth-v0

> **Validator**: Luna (`nw-product-owner`), DISCUSS wave. **Date**:
> 2026-06-06. 9-item hard gate; every item must PASS with evidence
> before handoff to DESIGN.

## Story: US-SP-AUTH-01 — programmatic bearer authenticates all three signals

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 1 | Problem statement clear, domain language | PASS | "Marco's telemetry is silently denied since the gateway turned on auth; `SparkConfig` has no auth knob (`with_endpoint` is the only transport knob, F1)." Domain language, no solution prescription. |
| 2 | User/persona with specific characteristics | PASS | Marco Bianchi, backend integrator at `acme-observability`, owns the `payments-api` Rust service; secondary Priya (security operator who minted the token). |
| 3 | 3+ domain examples with real data | PASS | (1) `payments-api` + 3 spans/5 logs/metric, `acme-prod`, role `operator`, accepted; (2) metrics-only batch authenticated; (3) expired token sent honestly, rejected, refreshed. Real service name, tenant, role. |
| 4 | UAT in Given/When/Then (3-7) | PASS | 3 scenarios: bearer-configured-export accepted; token-reaches-all-three-signals; expired-token sent-and-rejected. Business-outcome titles. |
| 5 | AC derived from UAT | PASS | 4 AC including the two mandated keys `a-bearer-configured-export-is-accepted-by-the-authenticated-gateway` and `the-token-reaches-all-three-signals`, each traceable to a scenario. |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | 3 scenarios, ~1 day (config field + uniform exporter helper). Single demonstrable behaviour. |
| 7 | Technical notes: constraints/dependencies | PASS | F1/F2/F5 cited with line numbers; DD1/DD2/DD3 flagged; depends on ADR-0068 (done) + `opentelemetry_otlp` tonic metadata surface. |
| 8 | Dependencies resolved or tracked | PASS | `aegis-ingest-auth-v0` / ADR-0068 — DONE; tonic metadata surface — available. Tracked in Technical Notes. |
| 9 | Outcome KPIs with measurable targets | PASS | KPI-1: 100% of three signals carry the bearer; E01-E04 GREEN; baseline 0% / BLOCKED; measured by E-suite + audit. |

### DoR Status: PASSED

## Story: US-SP-AUTH-02 — OTEL_EXPORTER_OTLP_HEADERS attaches the bearer

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 1 | Problem statement clear, domain language | PASS | "Marco's team manages credentials via manifest, but Spark ignores `OTLP_HEADERS` (F3) — the conventional credential path silently drops the token." |
| 2 | User/persona with specific characteristics | PASS | Marco (manifest-managed credentials); Priya (mints/rotates the token, wants deployment-managed rotation). |
| 3 | 3+ domain examples with real data | PASS | (1) manifest sets `authorization=Bearer%20eyJhbGci...` for `acme-prod`, accepted; (2) both paths set, programmatic wins; (3) empty env var ⇒ no header. Real env-var format and percent-encoding. |
| 4 | UAT in Given/When/Then (3-7) | PASS | 3 scenarios: env-var-attaches-bearer; programmatic-wins-precedence; empty-env-no-credential. |
| 5 | AC derived from UAT | PASS | 4 AC including the mandated key `OTEL_EXPORTER_OTLP_HEADERS-attaches-the-bearer`; precedence + empty-fall-through + v0-scope each traceable. |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | 3 scenarios, ~0.5 day (an env parser + precedence wiring reusing Slice 1's helper). |
| 7 | Technical notes: constraints/dependencies | PASS | F3 cited; DD4 (parse scope, percent-decode) + DD2 (precedence) flagged; mirror `slice_04_env_var_precedence.rs` test pattern. |
| 8 | Dependencies resolved or tracked | PASS | Depends on US-SP-AUTH-01 (the uniform helper + redaction); tracked. |
| 9 | Outcome KPIs with measurable targets | PASS | KPI-2: 100% of valid-env-var exports accepted; 0-rebuild rotation; baseline 0% (F3). |

### DoR Status: PASSED

## Story: US-SP-AUTH-03 — never-log the token + no-auth path preserved

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 1 | Problem statement clear, domain language | PASS | "The token is a credential equivalent to the gateway's HS256 secret; Spark logs resolved config (`emit_init_succeeded`) and derives `Debug` (F4) — a naive field leaks it. Not every endpoint needs auth." |
| 2 | User/persona with specific characteristics | PASS | Marco (logs flow to a shared aggregator; runs unauth-collector tests); Priya (credential hygiene). |
| 3 | 3+ domain examples with real data | PASS | (1) grep `eyJhbGci` in `app.log` → zero hits; (2) `panic!("{config:?}")` shows `<redacted>`; (3) no-token + `http://localhost:4317` unauth collector still accepts. |
| 4 | UAT in Given/When/Then (3-7) | PASS | 3 scenarios: token-never-logged; no-token-no-header-unauth-still-works; no-token-against-remote-silent. |
| 5 | AC derived from UAT | PASS | 4 AC including the two mandated keys `the-token-is-never-logged` and `no-token-no-header-against-an-unauthenticated-endpoint-still-works`; conditional-header + DD5 traceable. |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | 3 scenarios, ~0.5 day (redaction + conditional attachment). |
| 7 | Technical notes: constraints/dependencies | PASS | F4 cited (`observability.rs:53-70`, `config.rs:26`); DD3/DD5 flagged; mirror aegis opaque-key Debug (validator.rs:149-158). |
| 8 | Dependencies resolved or tracked | PASS | Depends on US-SP-AUTH-01 (field exists) + US-SP-AUTH-02 (env path resolves same field); tracked. |
| 9 | Outcome KPIs with measurable targets | PASS | KPI-3: 0 token occurrences on any log/Debug/error surface (defect gate); KPI-4: 100% no-token unauth exports still accepted. |

### DoR Status: PASSED

## Overall: PASSED (3/3 stories, 9/9 items each)

All three stories carry the mandatory Elevator Pitch (Before / After /
Decision enabled) with a real user-invocable entry point (the
`SparkConfig` builder API and the `OTEL_EXPORTER_OTLP_HEADERS` env var)
and concrete observable output (gateway `decision=allow`/`sink_accepted`,
filled dashboards, absent token in logs). The five mandated acceptance
criteria are all present and embedded:

- `a-bearer-configured-export-is-accepted-by-the-authenticated-gateway`
  (US-SP-AUTH-01)
- `the-token-reaches-all-three-signals` (US-SP-AUTH-01)
- `OTEL_EXPORTER_OTLP_HEADERS-attaches-the-bearer` (US-SP-AUTH-02)
- `the-token-is-never-logged` (US-SP-AUTH-03)
- `no-token-no-header-against-an-unauthenticated-endpoint-still-works`
  (US-SP-AUTH-03)

No anti-patterns detected: no Implement-X titles, real data throughout
(Marco Bianchi / `payments-api` / `acme-prod` / real env-var format),
outcome-focused AC (no "use `with_metadata`" in AC — the mechanism is
flagged for DESIGN as DD1), all stories right-sized. Ready for peer
review and DESIGN handoff.
