# Outcome KPIs: query-api-label-matchers-v0

British English. No em dashes.

## Feature: query-api-label-matchers-v0

### Objective
Make the read side actually useful during an incident: an operator filters a noisy metric
down to the series that matter using Prometheus `=` and `!=` label matchers, server-side,
with exactly correct matcher semantics (including the absent-label and empty-string cases).

### Outcome KPIs

| # | Who | Does What | By How Much | Baseline | Measured By | Type |
|---|-----|-----------|-------------|----------|-------------|------|
| 1 | On-call operator (Sara) | Narrows a multi-series metric to the series matching her `=` / `!=` matchers, reading the relevant series instead of a noisy plot | From "must eyeball all series" to "sees exactly the matching series" for a labelled query | Slice 01: any `{` returns 400; no filtering possible | E2E: ingest several services under one metric, query with a matcher, assert only matching series render in Prism | Leading (Outcome) |
| 2 | Query service | Returns EXACTLY the series satisfying all matchers, including the absent-label and empty-string arms | 100% correctness on the matcher semantics matrix (present/absent/empty x `=`/`!=`); 0 wrongly kept or dropped series | n/a (matchers are new) | Acceptance tests asserting the kept/excluded set per matcher case; the semantics matrix in `journey-label-filter-visual.md` is the oracle | Leading (Outcome) |
| 3 | Query service | Rejects a regex or malformed matcher honestly rather than mis-answering | 100% of regex/malformed forms return status:error 400; 0 silent partial filters | n/a | One acceptance test per rejected form; redaction test for header/raw-query leak | Guardrail |
| 4 | Prism's query client | Still receives a response its own validator accepts (envelope unchanged by the matcher feature) | 100% of success/empty/error shapes still round-trip; contract does not regress | 100% (slice 01) | Contract test through Prism's `isPromSuccess`/`isPromError` | Guardrail |
| 5 | Query service | Returns a labelled-query matrix within the latency budget | p95 query+filter latency at most 500 ms on GitHub Actions ubuntu-latest for a single-metric range over <= 1000 points | Slice 01 met the same budget for unfiltered queries | Timed acceptance test in CI (ubuntu-latest) | Leading (Secondary) |

### Metric Hierarchy
- **North Star**: KPI 1 - the operator sees exactly the series she filtered for (the read
  side becomes useful for incident triage).
- **Leading Indicators**: KPI 2 (matcher-semantics correctness), KPI 5 (latency within
  budget).
- **Guardrail Metrics**: KPI 3 (honest 400 for regex/malformed; the ADR-0042 discipline),
  KPI 4 (the pinned Prism contract must NOT regress).

### Measurement Plan
| KPI | Data Source | Collection Method | Frequency | Owner |
|-----|------------|-------------------|-----------|-------|
| 1 | E2E ingest->query(filtered)->render | CI E2E (ubuntu-latest) | Per commit | DELIVER |
| 2 | Matcher-semantics acceptance suite | CI acceptance stage | Per commit | DELIVER |
| 3 | Reject-form acceptance + redaction tests | CI acceptance stage | Per commit | DELIVER |
| 4 | Contract test fixtures + Prism validators | CI acceptance stage | Per commit | DELIVER |
| 5 | Service-emitted query duration + CI timing | Timed test on ubuntu-latest | Per commit | DEVOPS/DELIVER |

### CI realism
All latency budgets are stated against GitHub Actions `ubuntu-latest`, not developer
hardware. KPI 5's 500 ms p95 is the same single-metric, up-to-1000-point budget slice 01
held; the added filter is a single pass over the returned rows and is not expected to move
it. Per project memory: Kaleidoscope is pure trunk-based, CI is feedback not a gate, so
these KPIs are correctness signals, not merge blockers.

### Hypothesis
We believe that `=` and `!=` label matchers on the existing bare-name selector, for the
on-call operator using Prism, will make the read side useful for incident triage. We will
know this is true when the operator sees exactly the series matching her matchers (KPI 1),
the matcher semantics are 100% correct including the absent-label arms (KPI 2), regex and
malformed matchers are honestly rejected (KPI 3), and the pinned Prism contract does not
regress (KPI 4), within the CI latency budget (KPI 5).

## Handoff to DEVOPS (platform-architect)
- Instrument: per-query duration (existing `record_query` seam) now spanning parse + filter;
  matcher-count and kept/total series ratio per query; reject-form counts (regex vs
  malformed).
- Dashboards: query+filter latency p95 (reuse slice-01 panel); matcher-correctness pass
  rate; filtered-vs-unfiltered query ratio (adoption signal for KPI 1).
- Alerting thresholds: KPI 5 p95 > 500 ms; any contract-shape regression (KPI 4) is a hard
  alert; any cross-tenant leak (inherited US-04 guardrail) remains a hard alert.
- Baseline: none needed beyond slice 01; filtering is a new behaviour on an existing path.
- Mutation gate: `gate-5-mutants-query-api` already covers `crates/query-api/src/` via
  `--in-diff`; the new parse + filter logic lands in scope without a new gate.
