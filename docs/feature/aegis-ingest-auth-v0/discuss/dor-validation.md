# Definition of Ready Validation — aegis-ingest-auth-v0

9-item hard gate. Each item must PASS with evidence before handoff to
DESIGN. Validated 2026-06-06 by Luna (`nw-product-owner`).

## US-AUTH-01 — Walking skeleton: authenticate bearer token on gRPC logs

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 1 | Problem statement clear, domain language | PASS | "Priya cannot certify the gateway enforces tenant identity; aperture's gRPC export never reads metadata, so any caller writes under any tenant." Domain language, no solution. |
| 2 | User/persona with specific characteristics | PASS | Priya (platform-security operator, `acme-observability`, audit-accountable); Diego (SRE, gRPC fleet); Mallory (unauthenticated caller). |
| 3 | 3+ domain examples with real data | PASS | Diego's `payments-api` 3-record batch, `acme-prod`/`operator`; Mallory's tokenless export; Diego's expired token. Real tenants, roles, signals. |
| 4 | UAT in Given/When/Then (3-7) | PASS | 4 scenarios: authenticated accept+tag, no-token reject, expired reject, non-regression. |
| 5 | AC derived from UAT | PASS | 5 AC trace to the 4 scenarios (no-token, valid-tag, expired-reason, one-event, non-regression). |
| 6 | Right-sized (1-3 days, 3-7 scenarios) | PASS | One transport, one signal, the boundary end-to-end. 4 scenarios. Demonstrable in a single session. |
| 7 | Technical notes: constraints/dependencies | PASS | DD1 (config fields), DD3 (sink ripple), single-validator CI invariant, `#[non_exhaustive]` SinkRecord noted. |
| 8 | Dependencies resolved or tracked | PASS | aegis v0 (`Validator`/`TenantContext`/`load_catalogue`) available; ADR-0061 precedent available. No DIVERGE (tracked as R6). |
| 9 | Outcome KPIs with measurable targets | PASS | KPI-1 (100% accepted gRPC logs authenticated), KPI-2 (100% invalid rejected). Baseline 0%. |

### DoR Status: PASSED

## US-AUTH-02 — Fail-closed auth configuration

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 1 | Problem statement clear | PASS | "Priya needs safe-by-default; a flag defaulting OFF turns 'forgot auth' into 'silently open gateway' — the ADR-0061 trap." |
| 2 | Persona specific | PASS | Priya, operator writing `aperture.toml`. |
| 3 | 3+ domain examples real data | PASS | Complete config starts; omitted config refuses; unreadable secret source refuses. |
| 4 | UAT G/W/T (3-7) | PASS | 3 scenarios: complete-config-starts, omission-refuses, secret-never-echoed. |
| 5 | AC from UAT | PASS | 3 AC: starts-authenticated, refuse-to-start-no-bind, secret-never-logged. |
| 6 | Right-sized | PASS | Config-validation seam only (the ADR-0061 seam). 3 scenarios. |
| 7 | Technical notes | PASS | ADR-0061 seam (`RawConfig::into_config`, exit 2, no bind), DD1+DD4. |
| 8 | Dependencies tracked | PASS | ADR-0061 available; DD4 mechanism is DESIGN's. |
| 9 | Outcome KPIs | PASS | KPI-4 (100% omission-refusals exit non-zero, zero listeners). Baseline 0.0. |

### DoR Status: PASSED

## US-AUTH-03 — HTTP transport parity (logs)

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 1 | Problem statement clear | PASS | "One open transport is an open gateway; `handle_logs` never reads `Authorization`." |
| 2 | Persona specific | PASS | Priya, Diego (HTTP fleet), Mallory. |
| 3 | 3+ examples real data | PASS | Diego's `globex-staging` POST; Mallory's tokenless POST; Mallory's `acme-prod-evil` unknown-tenant token. |
| 4 | UAT G/W/T | PASS | 3 scenarios: authenticated tag, no-header 401, unknown-tenant 401. |
| 5 | AC from UAT | PASS | 5 AC incl. RFC-6750 401, non-regression with 415/503. |
| 6 | Right-sized | PASS | One transport leg, one signal. 3 scenarios. |
| 7 | Technical notes | PASS | DD2 (HTTP leg, `WWW-Authenticate: Bearer`), reuses WS spine. |
| 8 | Dependencies tracked | PASS | Depends on US-AUTH-01. Tracked. |
| 9 | Outcome KPIs | PASS | KPI-1 extended to HTTP. Baseline 0%. |

### DoR Status: PASSED

## US-AUTH-04 — Three-signal parity (traces + metrics, both transports)

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 1 | Problem statement clear | PASS | "Open traces/metrics doors; the per-signal handlers read no token." |
| 2 | Persona specific | PASS | Priya, Diego (multi-signal fleet). |
| 3 | 3+ examples real data | PASS | `checkout-api` 3-span gRPC accept; tokenless metrics POST; wrong-audience (`kaleidoscope-query`) traces reject. |
| 4 | UAT G/W/T | PASS | 3 scenarios: traces tag, metrics no-token reject, wrong-audience reject. |
| 5 | AC from UAT | PASS | 5 AC incl. `subject` naming, signal-mismatch non-regression. |
| 6 | Right-sized | PASS | Mirrors logs across 2 signals; aperture's per-signal symmetry makes this low-risk. |
| 7 | Technical notes | PASS | Per-signal symmetry; depends on US-AUTH-01 + 03. |
| 8 | Dependencies tracked | PASS | Tracked. |
| 9 | Outcome KPIs | PASS | KPI-1 full surface (3×2). Baseline 0%. |

### DoR Status: PASSED

## US-AUTH-05 — Legible denials + role question

| # | DoR Item | Status | Evidence/Issue |
|---|----------|--------|----------------|
| 1 | Problem statement clear | PASS | "Fail-closed reject is necessary but not sufficient for ops; Priya must triage by cause." |
| 2 | Persona specific | PASS | Priya (triage), Riley (audit query). |
| 3 | 3+ examples real data | PASS | 412 expired vs 3 forged-signature triage; malformed `not-a-jwt`; `auditor` unknown-role. |
| 4 | UAT G/W/T | PASS | 3 scenarios: malformed-distinct, unknown-role, all-8-reasons-distinct. |
| 5 | AC from UAT | PASS | 4 AC incl. one-event-per-reject, reasons-distinct, DD6-resolved. |
| 6 | Right-sized | PASS | Reason-surfacing across the existing reject paths + one decision (DD6). 3 scenarios. |
| 7 | Technical notes | PASS | `ValidationError::reason()` reused verbatim; DD5+DD6. |
| 8 | Dependencies tracked | PASS | Depends on US-AUTH-01..04. |
| 9 | Outcome KPIs | PASS | KPI-3 (100% denials carry one of 8 distinct reasons). |

### DoR Status: PASSED

## Anti-Pattern Scan

| Anti-Pattern | Found? | Note |
|--------------|--------|------|
| Implement-X | No | Every story starts from a user pain (Priya can't certify; Mallory can write under a victim tenant). |
| Generic data | No | Real tenants (`acme-prod`, `globex-staging`), services (`payments-api`, `checkout-api`), personas (Priya/Diego/Mallory/Riley), roles, audiences. |
| Technical AC | No | AC are observable outcomes (status, nothing-stored, audit reason) not mechanisms; tech choices live in DD1-DD7 for DESIGN. |
| Technical scenario title | No | Titles are business outcomes ("An authenticated client ingests…", "A request with no bearer token is rejected…"). |
| Oversized story | No | Largest is the WS at 4 scenarios; feature sliced WS + 3 releases by outcome; read-path carved out. |
| Abstract requirements | No | 3+ concrete examples per story. |

## Overall: PASSED (5/5 stories, all 9 items each)

Solution-neutrality verified: requirements state WHAT must be observable
(reject/nothing-stored/reason/tagged-tenant/secret-never-logged); the six
DESIGN decisions (DD1-DD6) plus the adjacent doc-fix (DD7) carry the HOW.
