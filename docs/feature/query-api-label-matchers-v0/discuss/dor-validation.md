# Definition of Ready Validation: query-api-label-matchers-v0

British English. No em dashes. 9-item hard gate. Each story passes every item with evidence.

## Story: US-06 (equality matcher)

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| 1 Problem statement clear, domain language | PASS | Sara Okafor, on-call SRE for checkout, gets a dozen overlapping series for a noisy metric mid-incident; slice 01 rejects any `{`. |
| 2 User/persona with specific characteristics | PASS | On-call SRE, tenant "acme-prod", filtering a known noisy metric to one service by exact value during an incident. |
| 3 3+ domain examples with real data | PASS | Single matcher narrows checkout; two ANDed matchers (service.name + code="200"); empty-string `code=""` matches absent label. Real metric `http_requests_total`, real labels. |
| 4 UAT in Given/When/Then (3-7) | PASS | 5 scenarios: single matcher, ANDed, empty-string-absent, bare-name-unchanged, empty-arm. |
| 5 AC derived from UAT | PASS | 8 AC, each traceable to a scenario (parser accepts form, present/equal, empty-string-absent, AND, derived-set, bare-name, empty-arm, envelope). |
| 6 Right-sized (1-3 days, 3-7 scenarios) | PASS | 5 scenarios, ~1 day, single demonstrable behaviour (equality filtering). |
| 7 Technical notes: constraints/dependencies | PASS | Parser change in selector.rs; filter before to_matrix on merge_labels set; gate-5-mutants coverage. |
| 8 Dependencies resolved or tracked | PASS | Depends on shipped query-range-api-v0 (US-01..US-05). Pulse `query`, aegis TenantId, matrix `merge_labels` all exist and verified. |
| 9 Outcome KPIs with measurable targets | PASS | KPI 1 (narrows to matching series), KPI 2 (100% semantics correctness incl empty-string); baseline 0%. |

### DoR Status: PASSED

## Story: US-07 (inequality matcher)

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| 1 Problem statement clear, domain language | PASS | Sara wants "every service except the noisy batch"; the subtle part is `!=` on absent labels and `!=""`. |
| 2 User/persona with specific characteristics | PASS | On-call SRE excluding a known noisy series; needs exact absent-label and empty-string `!=` semantics. |
| 3 3+ domain examples with real data | PASS | `service.name!="batch"` excludes batch; `code!="500"` KEEPS absent-code series; `code!=""` keeps only present-non-empty. |
| 4 UAT in Given/When/Then (3-7) | PASS | 4 scenarios: exclude named, keep-absent, `!=""` present-non-empty, AND composition. |
| 5 AC derived from UAT | PASS | 6 AC, each traceable (`!=` accepted, absent-or-different keep, `!=""` rule, AND, derived-set, envelope/empty). |
| 6 Right-sized (1-3 days, 3-7 scenarios) | PASS | 4 scenarios, ~1 day, single behaviour (inequality filtering). |
| 7 Technical notes | PASS | Absent-label/empty arms flagged as regression-prone; shares parser/predicate with US-06. |
| 8 Dependencies tracked | PASS | Depends on US-06's parser/predicate; same shipped substrate as US-06. |
| 9 Outcome KPIs | PASS | KPI 2 covers `!=` arms; 0 wrongly kept/dropped; baseline 0%. |

### DoR Status: PASSED

## Story: US-08 (reject regex/malformed)

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| 1 Problem statement clear, domain language | PASS | Sara pastes a regex matcher or fat-fingers a brace; a silent wrong filter mid-incident is worse than refusal. |
| 2 User/persona with specific characteristics | PASS | PromQL-literate on-call operator pasting a richer/malformed matcher; needs an honest, specific rejection. |
| 3 3+ domain examples with real data | PASS | regex `service.name=~"check.*"` rejected; unterminated brace rejected; unquoted value rejected. |
| 4 UAT in Given/When/Then (3-7) | PASS | 4 scenarios: regex reject, unterminated-not-bare-name, unquoted-value, no-header-leak. |
| 5 AC derived from UAT | PASS | 6 AC (non-=/!= operator 400, malformed 400, never-silent-bare-name, isPromError, no-leak, test-per-form). |
| 6 Right-sized (1-3 days, 3-7 scenarios) | PASS | 4 scenarios, well under 1 day, single behaviour (honest rejection). |
| 7 Technical notes | PASS | Extends slice-01 honest-400 discipline; redaction mirrors ADR-0027 section 6 and existing selector.rs test. |
| 8 Dependencies tracked | PASS | Depends on US-06's parser; regex deferred to slice 02b (briefed). |
| 9 Outcome KPIs | PASS | KPI 3 (100% regex/malformed rejected, 0 silent partials); guardrail. |

### DoR Status: PASSED

## Cross-cutting checks

- Anti-patterns: no Implement-X (all start from operator pain); no generic data (Sara,
  acme-prod, http_requests_total, service.name=checkout throughout); no technical AC
  (observable HTTP/series outcomes); no oversized story (max 5 scenarios); 3+ real examples
  each. CLEAN.
- Scenario titles are business outcomes, not implementation ("narrows to the matching
  series", not "filter predicate iterates rows"). CLEAN.
- Solution-neutral: stories pin behaviour (matcher semantics, honest 400) not data
  structures; the parser return type and filter placement are flagged as DESIGN decisions.
  CLEAN.

## Overall DoR: PASSED (3/3 stories, all 9 items each)
