# Definition of Ready Validation: perf-kpi-ci-non-gating-v0

Persona note: this is an infrastructure / CI-hygiene feature. The
"persona" is the maintainer (Andrea) reading a GitHub Actions result, not
an end user. "Real data" therefore means real commit shas, real job
names, real file paths, and real measured p95 numbers, not invented end
users.

## US-01: A perf breach on noisy CI hardware no longer fails the build

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear, domain language | PASS | "A slow CI fsync turns `gate-1-test` red, indistinguishable from a real regression; the team learns to ignore red." |
| User/persona identified | PASS | Maintainer (Andrea) reading every CI run on `main`/PRs; PR contributors watching checks. |
| 3+ domain examples with real data | PASS | place p95 4.2 ms vs 200 us budget on push abc1234; sluice enqueue 1.1 ms on PR #57; family-wide breach on a slow I/O minute. |
| UAT scenarios (3-7) | PASS | 3 Gherkin scenarios (durable breach stays green; gating job does not opt in; family-wide breach no false red). |
| AC derived from UAT | PASS | 4 AC, each traceable to a scenario and to ci.yml line ~141. |
| Right-sized (1-3 days, 3-7 scenarios) | PASS | Delete one env block; ~part of a 1-day feature; 3 scenarios. |
| Technical notes: constraints/dependencies | PASS | Touches `gate-1-test` only; depends on ADR-0058 guard present; F2 ADR supersession. |
| Dependencies resolved or tracked | PASS | Guard verified present in all 28 tests; US-03 is the paired negative control. |
| Outcome KPIs defined with measurable targets | PASS | KPI-1: 0 perf-attributable Gate-1 reds / 30 days; baseline recurring. |

### DoR Status: PASSED

## US-02: The perf KPIs still run and report in a separate, non-gating job

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear, domain language | PASS | "De-gating must not make the p95 numbers vanish; a real sustained regression must stay observable." |
| User/persona identified | PASS | Maintainer reviewing perf trend. |
| 3+ domain examples with real data | PASS | perf job green with logged numbers (push def5678); breach 5.0 ms non-fatal (push ghi9012); sustained 0.9->9 ms regression over 5 pushes. |
| UAT scenarios (3-7) | PASS | 3 Gherkin scenarios (run+report; breach non-fatal; numbers visible on breach). |
| AC derived from UAT | PASS | 4 AC traceable to scenarios and C4/C5. |
| Right-sized (1-3 days, 3-7 scenarios) | PASS | One new non-gating CI job; 3 scenarios. |
| Technical notes: constraints/dependencies | PASS | Depends on US-01; F3 assert-vs-report choice; DEVOPS owns job vs separate workflow. |
| Dependencies resolved or tracked | PASS | Depends on US-01 (tracked); guard present. |
| Outcome KPIs defined with measurable targets | PASS | KPI-2: 100% of main runs produce readable p95 numbers. |

### DoR Status: PASSED

## US-03: A real correctness regression still fails the build (negative control)

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear, domain language | PASS | "De-gating perf must not loosen correctness gating; red must stay trustworthy in both directions." |
| User/persona identified | PASS | Maintainer interpreting Gate 1 status. |
| 3+ domain examples with real data | PASS | cinder WAL recovery correctness break reds Gate 1; lumen query-correctness break reds Gate 1; pure 10x slowdown stays green. |
| UAT scenarios (3-7) | PASS | 3 Gherkin scenarios (correctness failure reds; de-gating unchanged for correctness; pure slowdown green). |
| AC derived from UAT | PASS | 4 AC including a DISTILL negative-control demonstration. |
| Right-sized (1-3 days, 3-7 scenarios) | PASS | No new code; invariant assertion; 3 scenarios. |
| Technical notes: constraints/dependencies | PASS | Paired with US-01; C2. |
| Dependencies resolved or tracked | PASS | Must validate with US-01; tracked. |
| Outcome KPIs defined with measurable targets | PASS | KPI-3: 100% of Gate-1 reds correctness-attributable. |

### DoR Status: PASSED

## US-04: The durable-op budgets are documented as dev-indicative, not CI-contractual

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear, domain language | PASS | "Durable-op budgets were written for non-durable ops; without an honesty note, a breach is misread as a regression or invites threshold-chasing." |
| User/persona identified | PASS | Future maintainer/contributor encountering a durable-op breach. |
| 3+ domain examples with real data | PASS | place 4.2 ms read correctly via the note; get_tier 50 us in-mem distinguished from durable ops; contributor declines to raise place budget to 5 ms. |
| UAT scenarios (3-7) | PASS | 3 Gherkin scenarios (documented as dev-indicative; attributed to durability; threshold-chasing forbidden). |
| AC derived from UAT | PASS | 4 AC tied to the new ADR and ADR-0049/0060 citation. |
| Right-sized (1-3 days, 3-7 scenarios) | PASS | Pure documentation (new ADR); 3 scenarios. |
| Technical notes: constraints/dependencies | PASS | No code, no threshold change (C3); F4 placement. |
| Dependencies resolved or tracked | PASS | Independent; co-lands with R2; cites ADR-0049/0060/0058. |
| Outcome KPIs defined with measurable targets | PASS | KPI-4 guardrail: 0 threshold-raise commits to durable budgets. |

### DoR Status: PASSED

## Feature-level DoR Status: PASSED (4/4 stories)

All 9 DoR items pass for all 4 stories. No blocking items. Ready for peer
review, then DESIGN handoff (not executed in this wave per the
non-proceed-into-DESIGN instruction).
