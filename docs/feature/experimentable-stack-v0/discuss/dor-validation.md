# Definition of Ready Validation - `experimentable-stack-v0`

> The 9-item DoR hard gate, evidenced per story. All nine must PASS before DESIGN handoff. British
> English, no em-dashes. Item 9 (Outcome KPIs) is the ninth item per the nw-product-owner DoR
> checklist; items 1-8 are the LeanUX core.

## Summary

| Story | 1 Problem | 2 Persona | 3 Examples | 4 UAT (3-7) | 5 AC from UAT | 6 Right-sized | 7 Tech notes | 8 Deps | 9 KPIs | Verdict |
|-------|-----------|-----------|------------|-------------|---------------|---------------|--------------|--------|--------|---------|
| US-01 | PASS | PASS | PASS (3) | PASS (3) | PASS | PASS | PASS | PASS | PASS | READY |
| US-02 | PASS | PASS | PASS (3) | PASS (3) | PASS | PASS | PASS | PASS | PASS | READY |
| US-03 | PASS | PASS | PASS (4) | PASS (4) | PASS | PASS | PASS | PASS | PASS | READY |
| US-04 | PASS | PASS | PASS (4) | PASS (4) | PASS | PASS | PASS | PASS | PASS | READY |
| US-05 | PASS | PASS | PASS (3) | PASS (3) | PASS | PASS | PASS | PASS | PASS | READY |
| US-06 | PASS | PASS | PASS (3) | PASS (3) | PASS | PASS | PASS | PASS | PASS | READY |
| US-07 | PASS | PASS | PASS (3) | PASS (3) | PASS | PASS | PASS | PASS | PASS | READY |

**DoR Status: PASSED** for all seven stories. Scenario totals: 23 scenarios, 14 error/edge (61%),
above the >=40% target.

---

## Evidence per story

### US-01 - one command up, reachable in browser

| DoR Item | Status | Evidence |
|----------|--------|----------|
| 1 Problem in domain language | PASS | No compose/Makefile/run script; README is CLI-only (assessment section 3). Stated as the newcomer's "no one-command way in". |
| 2 Persona specific | PASS | Sam Okonkwo, newcomer/evaluator (platform engineer assessing before investing) + Andrea + contributor. |
| 3 Domain examples (3+) | PASS | 3: open Prism after `make up`; endpoints answer empty-success; shared volume + tenant integration. Real values (9090, `request_count`, `acme`). |
| 4 UAT 3-7 | PASS | 3 Gherkin scenarios (loads in browser; endpoints answer; shared volume/tenant). |
| 5 AC from UAT | PASS | 4 AC each traceable to a scenario; observable outcomes, no implementation. |
| 6 Right-sized | PASS | Single outcome (one-command reachable stack); 3 scenarios. |
| 7 Tech notes (constraints) | PASS | Compose topology, Dockerfile.runtime, wrapper choice flagged F1-F5; Prism relative backend noted. |
| 8 Dependencies | PASS | C1 (DONE). Walking skeleton for the rest. |
| 9 Outcome KPIs | PASS | One-command bring-up success to a loaded UI (KPI 2). |

### US-02 - Prism pointed at the runtime, honest empty/error states

| DoR Item | Status | Evidence |
|----------|--------|----------|
| 1 Problem | PASS | First look is an empty stack; Prism must distinguish empty / data / unreachable. Prism is metrics-only and not yet wired to the runtime. |
| 2 Persona | PASS | Sam (first contact is empty stack) + Andrea/contributor. |
| 3 Examples (3+) | PASS | 3: queries the runtime; honest empty state; unreachable-backend message. |
| 4 UAT 3-7 | PASS | 3 scenarios (queries runtime; empty state; unreachable). 2 are edge/error. |
| 5 AC from UAT | PASS | 3 AC, observable UI states. |
| 6 Right-sized | PASS | Single outcome (Prism honest first-look states); 3 scenarios. |
| 7 Tech notes | PASS | Same-origin vs separate service (F4); logs/traces panels out of scope (C5). |
| 8 Dependencies | PASS | US-01. |
| 9 KPIs | PASS | 3/3 states render correctly (KPI 2 family). |

### US-03 - clean, idempotent, fails clearly

| DoR Item | Status | Evidence |
|----------|--------|----------|
| 1 Problem | PASS | Reliability layer; the brief's edges (fresh clean, idempotent re-up, recover, ports-in-use clear error not half-up). |
| 2 Persona | PASS | Repeat user (Sam/Andrea/contributor). |
| 3 Examples (3+) | PASS | 4: fresh clean; idempotent re-up; down-then-up; port-in-use clear error. |
| 4 UAT 3-7 | PASS | 4 scenarios, all 4 edge/error. |
| 5 AC from UAT | PASS | 4 AC mapped to the four cases. |
| 6 Right-sized | PASS | Single outcome (predictable bring-up); 4 scenarios. |
| 7 Tech notes | PASS | Idempotency/clean-vs-down semantics flagged F2; fixed-port flake discipline noted. |
| 8 Dependencies | PASS | US-01. |
| 9 KPIs | PASS | 4/4 reliability/error cases, 0 silent half-ups. |

### US-04 - one-command generator pushes metrics + logs + traces

| DoR Item | Status | Evidence |
|----------|--------|----------|
| 1 Problem | PASS | Generator/sample data is a named must-have (assessment section 7 item 4); empty stack stalls the experiment. |
| 2 Persona | PASS | Sam + Andrea/contributor. |
| 3 Examples (3+) | PASS | 4: all-three-signals; Prism paints; against-down-stack error; re-run safe. Real sample data. |
| 4 UAT 3-7 | PASS | 4 scenarios (2 error/edge). |
| 5 AC from UAT | PASS | 4 AC, observable. |
| 6 Right-sized | PASS | Single outcome (push sample telemetry); 4 scenarios. |
| 7 Tech notes | PASS | Generator implementation options (a-d) surfaced with a recommendation (F3); CLI-is-NDJSON-not-OTLP caveat noted. |
| 8 Dependencies | PASS | US-01; shares vocabulary with US-05/US-06. |
| 9 KPIs | PASS | 3/3 signals queryable; metric painted (KPI 3). |

### US-05 - fresh stack not empty on first look

| DoR Item | Status | Evidence |
|----------|--------|----------|
| 1 Problem | PASS | Key requirement (c); empty first look reads as broken. |
| 2 Persona | PASS | Sam, whose first action is to open Prism. |
| 3 Examples (3+) | PASS | 3: first look shows a metric; tenant-scoped; once-only on restart. |
| 4 UAT 3-7 | PASS | 3 scenarios (2 edge). |
| 5 AC from UAT | PASS | 3 AC. |
| 6 Right-sized | PASS | Single outcome (not-empty first look); 3 scenarios. |
| 7 Tech notes | PASS | Seed mechanism flagged F3; explicit fold-into-US-04 note if DESIGN picks "documented next step". |
| 8 Dependencies | PASS | US-01, US-04. |
| 9 KPIs | PASS | First-look-not-empty 100% (KPI 4). |

### US-06 - getting-started docs

| DoR Item | Status | Evidence |
|----------|--------|----------|
| 1 Problem | PASS | README documents only the CLI; named must-have (assessment section 7 item 3). |
| 2 Persona | PASS | Cold reader (Sam) + Andrea/contributor as reference. |
| 3 Examples (3+) | PASS | 3: complete the loop from docs; minimal config; honest verification limit. |
| 4 UAT 3-7 | PASS | 3 scenarios. |
| 5 AC from UAT | PASS | 4 AC (loop documented; minimal config; honesty; consolidated-not-CLI). |
| 6 Right-sized | PASS | Single outcome (followable getting-started); 3 scenarios. |
| 7 Tech notes | PASS | README vs docs/ home; sample-data vocabulary alignment. |
| 8 Dependencies | PASS | US-01, US-02, US-04; US-07 (CLI demo). |
| 9 KPIs | PASS | Docs-followability, cold reader reaches "see" (KPI 5). |

### US-07 - additive guardrail (existing paths intact)

| DoR Item | Status | Evidence |
|----------|--------|----------|
| 1 Problem | PASS | Key requirement (e); risk of breaking/orphaning the 4 binaries, 3 Dockerfiles, CLI demo. |
| 2 Persona | PASS | Existing user + contributor/CI. |
| 3 Examples (3+) | PASS | 3: binaries build/run; Dockerfiles build; CLI demo preserved. |
| 4 UAT 3-7 | PASS | 3 scenarios, all regression/edge. |
| 5 AC from UAT | PASS | 3 AC. |
| 6 Right-sized | PASS | Single outcome (additivity); 3 scenarios. |
| 7 Tech notes | PASS | Cross-cutting guardrail; new Dockerfile.runtime additive not replacing. |
| 8 Dependencies | PASS | None upstream; guards US-01/04/06. |
| 9 KPIs | PASS | 0 regressions across the pre-existing assets (KPI 6 guardrail). |

---

## Cross-cutting checks

- **Elevator Pitch (Dimension 0)**: every story has Before / After / Decision enabled. Each "After"
  names a real user-invocable entry point (`make up`, `make demo`, opening the documented URL,
  following the README) and concrete observable output (Prism renders; `query_range` returns a
  point; curl returns rows; the docs reach "see"). No story is internal-only.
- **Slice-level value (Dimension 0 slice check)**: each slice has at least one user-visible story -
  Slice 1: US-01/US-02 (browser); Slice 2: US-04/US-05 (a painted metric); Slice 3: US-06 (a doc a
  newcomer reads). No slice is all-infrastructure.
- **Anti-patterns**: no Implement-X titles (all are user outcomes); no generic data (real values:
  `acme`, `request_count`, the declined-checkout log, the trace id, ports 9090/9091/9092); AC are
  observable not technical; no story exceeds 7 scenarios; every requirement carries 3+ examples.
- **Error/edge coverage**: 14/23 scenarios (61%) are error/edge, above the 40% target (per-story
  counts in the summary table).
