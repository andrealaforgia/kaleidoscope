# Definition of Ready Validation: query-range-api-v0

9-item hard gate. Each story must pass all items with evidence before DESIGN handoff.

## US-01: Serve a metric time series as a Prometheus matrix

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear, domain language | PASS | Sara Okafor, on-call SRE for "checkout", blind to metrics despite durable storage |
| User/persona with specifics | PASS | On-call SRE + Prism HTTP client with pinned contract |
| 3+ domain examples, real data | PASS | process_cpu_utilization 0.40/0.55/0.61; two-series route=/cart,/pay; half-open boundary point |
| UAT in Given/When/Then (3-7) | PASS | 4 scenarios |
| AC derived from UAT | PASS | 6 AC, each traceable to a scenario |
| Right-sized (1-3 days, 3-7 scen) | PASS | 4 scenarios, ~1.5 days |
| Technical notes: constraints/deps | PASS | Pulse query surface, half-open ns TimeRange, location is DESIGN |
| Dependencies resolved/tracked | PASS | Pulse + aegis exist; RED CARD 1/2/3 tracked in wave-decisions.md |
| Outcome KPIs measurable | PASS | KPI 1 + KPI 2 with method and baseline |

### DoR Status: PASSED

## US-02: Return a calm empty result

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear | PASS | Typo/empty range must not look like a failure during an incident |
| User/persona | PASS | On-call operator exploring names/ranges |
| 3+ domain examples | PASS | Unknown name typo; range before first point; point exactly at end |
| UAT (3-7) | PASS | 3 scenarios |
| AC from UAT | PASS | 3 AC |
| Right-sized | PASS | 3 scenarios, ~0.5 day |
| Technical notes | PASS | Pulse returns Ok(Vec::new()); serialise empty |
| Dependencies | PASS | Shares parser/query path with US-01 |
| Outcome KPIs | PASS | 100% no-match returns empty arm, 0 false errors |

### DoR Status: PASSED

## US-03: Reject an unparseable query

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear | PASS | rate() pasted by a Prometheus-literate operator must be honestly rejected |
| User/persona | PASS | PromQL-literate on-call operator |
| 3+ domain examples | PASS | rate() function; binary operator; empty query |
| UAT (3-7) | PASS | 4 scenarios (incl. header-redaction) |
| AC from UAT | PASS | 4 AC |
| Right-sized | PASS | 4 scenarios, ~1 day |
| Technical notes | PASS | Mirrors ADR-0027 parse-error arm + §6 redaction |
| Dependencies | PASS | Shares parser with US-05 |
| Outcome KPIs | PASS | 100% unsupported -> status:error 400, 0 silent mis-answers |

### DoR Status: PASSED

## US-04: Scope every query to one tenant, fail-closed

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear | PASS | Multi-tenant Pulse; read path must match write path's fail-closed posture |
| User/persona | PASS | Platform owner/security reviewer + scoped operator |
| 3+ domain examples | PASS | Configured tenant; other-tenant same-named metric; no tenant resolvable |
| UAT (3-7) | PASS | 3 scenarios |
| AC from UAT | PASS | 4 AC |
| Right-sized | PASS | 3 scenarios, ~1 day |
| Technical notes | PASS | aegis TenantId; gateway KALEIDOSCOPE_DEFAULT_TENANT; mechanism = RED CARD 1 |
| Dependencies | PASS | RED CARD 1 tracked; behaviour pinned, mechanism deferred to DESIGN |
| Outcome KPIs | PASS | 0 cross-tenant leaks; 100% no-tenant refused |

### DoR Status: PASSED

## US-05: Hold the v0 scope boundary at the contract edge

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear | PASS | Partial answers worse than refusals; boundary must be executable |
| User/persona | PASS | Team/maintainer + on-call operator |
| 3+ domain examples | PASS | Range vector; aggregation; whitespace-trimmed bare name |
| UAT (3-7) | PASS | 3 scenarios |
| AC from UAT | PASS | 4 AC |
| Right-sized | PASS | 3 scenarios, ~0.5 day |
| Technical notes | PASS | Shares parser with US-03; boundary half |
| Dependencies | PASS | Parser shared with US-03 |
| Outcome KPIs | PASS | 100% out-of-scope rejected, 0 partial answers |

### DoR Status: PASSED

## Feature DoR Status: PASSED (5/5 stories, all 9 items each)

Open RED CARDs (1 tenant mechanism, 2 resampling, 3 grouping key) are tracked design
decisions with recommended defaults, not DoR blockers: each story pins BEHAVIOUR; the
unresolved items are mechanism choices owned by DESIGN.
