# Definition of Ready Validation: prism-backend-wiring-v0

## Story: US-01 — The QueryPanel mounts against a valid served config.json

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear | PASS | Priya, paged SRE, cannot see any metric because Prism refuses to mount the QueryPanel without a valid config.json |
| User/persona identified | PASS | On-call SRE (Priya), incident-time, in a browser |
| 3+ domain examples | PASS | valid config mounts; missing label -> shape-failed (dark); no config -> fetch-failed 404 (dark) |
| UAT scenarios (3-7) | PASS | 3 Given/When/Then scenarios (happy mount, shape-failed dark, fetch-failed dark) |
| AC derived from UAT | PASS | 3 AC each tracing to a scenario |
| Right-sized | PASS | part of a 1-2 day slice; 3 scenarios |
| Technical notes | PASS | loader contract, three error arms, origin-root serving, config.json.example reference |
| Dependencies tracked | PASS | depends on Prism loader/QueryPanel (live); ships in Slice 01 |
| Outcome KPIs defined | PASS | who/does-what/by-how-much/baseline/measured-by all present (KPI 1) |

### DoR Status: PASSED

## Story: US-02 — A browser-served Prism reaches query-api and plots a real series

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear | PASS | Browser-served Prism is blocked reaching query-api (no CORS) / path must resolve; one fetch from seeing the metric |
| User/persona identified | PASS | On-call SRE (Priya), incident-time, in a browser |
| 3+ domain examples | PASS | series renders end-to-end; empty calm message; cross-origin blocked without mechanism |
| UAT scenarios (3-7) | PASS | 4 scenarios (series renders, fetch not blocked, path resolves to 200, empty arm) |
| AC derived from UAT | PASS | 4 AC each tracing to a scenario |
| Right-sized | PASS | part of a 1-2 day slice; 4 scenarios |
| Technical notes | PASS | path join, no-CORS-today, fail-closed tenancy, dev proxy, redaction invariant |
| Dependencies tracked | PASS | depends on US-01 and query-api over Pulse (live); ships in Slice 01 |
| Outcome KPIs defined | PASS | who/does-what/by-how-much/baseline/measured-by all present (KPI 2) |

### DoR Status: PASSED

## Solution-neutrality check

The central design fork (CORS vs same-origin) is captured as a requirement
("a browser-served Prism reaches query-api"), NOT decided. Both options with
tradeoffs are recorded in `wave-decisions.md` and the slice brief for DESIGN.

## Feature DoR Status: PASSED (pending peer review)
