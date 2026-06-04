# Definition of Ready Validation — claims-honesty-pass-v0

- **Analyst**: Luna (nw-product-owner)
- **Date**: 2026-06-05
- **Stories**: US-01 .. US-06
- **Gate**: 9-item hard gate, each item with evidence.

## US-01 — README codename honesty

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear, domain language | PASS | "the headline table overstates four crates' capabilities" — reader-trust framing, no implementation. |
| User/persona with specific characteristics | PASS | Devin Okafor, senior platform engineer at Northwind Logistics evaluating to replace a Datadog bill; reads README first then verifies code. |
| 3+ domain examples with real data | PASS | Corrected Spark row; Strata row + cost line; durability not-in-scope check — real file:line, real crate names. |
| UAT in Given/When/Then (3-7) | PASS | 5 scenarios (Spark, Strata+cost, Cinder, Loom, row-matches-lib.rs). |
| AC derived from UAT | PASS | 5 AC, one per scenario. |
| Right-sized (1-3 days, 3-7 scenarios) | PASS | XS prose, 5 scenarios, single README. |
| Technical notes: constraints/dependencies | PASS | README only; roadmap + durability block off-limits; aligns to crate lib.rs. |
| Dependencies resolved or tracked | PASS | None — crates already honest in lib.rs; independent slice. |
| Outcome KPIs with measurable targets | PASS | KPI-1: 4/4 rows + 1 line corrected, 0 residual; grep guard. |

### DoR Status: PASSED

## US-02 — Codex stub-declaration honesty

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear | PASS | "a delivered crate declaring itself an unbuilt stub" — domain language. |
| Persona | PASS | Devin reading Cargo.toml/test headers to gauge finishedness. |
| 3+ domain examples | PASS | Corrected Cargo.toml block; slice_04 header vs body; no-genuinely-RED check. |
| UAT (3-7) | PASS | 4 scenarios. |
| AC from UAT | PASS | 4 AC. |
| Right-sized | PASS | XS, 7 loci, 4 scenarios. |
| Technical notes | PASS | Doc/comment only; guardrail: confirm green before edit. |
| Dependencies | PASS | None; codex delivered. |
| Outcome KPIs | PASS | KPI-2: 0/7 stub declarations remain; grep guard + codex suite green. |

### DoR Status: PASSED

## US-03 — Stale scaffold-over-green doc comments

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear | PASS | "module/handler doc contradicts the code two lines below it". |
| Persona | PASS | Devin trusting the prominent doc over the buried per-fn note. |
| 3+ domain examples | PASS | Corrected qhc module doc; corrected trace handler doc; in-flight-not-touched check. |
| UAT (3-7) | PASS | 4 scenarios incl. the in-flight-intact guard. |
| AC from UAT | PASS | 4 AC. |
| Right-sized | PASS | S, 2 touched loci, 4 scenarios. |
| Technical notes | PASS | Bidirectional guard named; explicit DO-NOT-TOUCH list in slice brief. |
| Dependencies | PASS | None; both crates delivered/green. |
| Outcome KPIs | PASS | KPI-3: 2/2 corrected, 0 in-flight touched; bidirectional grep guard. |

### DoR Status: PASSED

## US-04 — Harness validation-depth honesty

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear | PASS | "a conformance harness overstating its own conformance" — thesis-critical, domain language. |
| Persona | PASS | Devin trusting telemetry to the harness, reading "wire spec" as "semantic". |
| 3+ domain examples | PASS | Corrected lib.rs header; corrected README status; the 4-byte-trace_id boundary body. |
| UAT (3-7) | PASS | 4 scenarios incl. the semantic-boundary behaviour test. |
| AC from UAT | PASS | 4 AC. |
| Right-sized | PASS | S prose + 1 behaviour test; 4 scenarios. |
| Technical notes | PASS | Pure prose for depth+status; framing split to US-06; no behaviour change. |
| Dependencies | PASS | None; harness delivered/green. |
| Outcome KPIs | PASS | KPI-4: 3/3 depth + 1 status corrected; grep + semantic-boundary test. |

### DoR Status: PASSED

## US-05 — query-api `step` honesty (DESIGN flag #1)

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear | PASS | "the README implies a stepped grid; behaviour returns raw points." |
| Persona | PASS | Devin wiring Prometheus/Grafana tooling, expecting `step` to re-sample. |
| 3+ domain examples | PASS | Qualified README (document); step=15s vs 60s; omitted-step boundary. |
| UAT (3-7) | PASS | 3 scenarios, each written to hold under BOTH document and implement outcomes. |
| AC from UAT | PASS | 3 AC, decision-neutral (claim matches behaviour either way). |
| Right-sized | PASS | S-M; the document path is XS, the implement path is its own slice; 3 scenarios. |
| Technical notes | PASS | DESIGN flag #1 named; DISCUSS recommends document; mutation only if implement. |
| Dependencies | PASS | Tracked: DESIGN must pick document-vs-implement. Verifier's black-box in flight (collaborator, not blocker). In-code field doc already honest. |
| Outcome KPIs | PASS | KPI-5: black-box result == documented claim, 0 gap. |

### DoR Status: PASSED (DESIGN decision tracked as a resolved-by-DESIGN dependency, not a blocker to readiness)

## US-06 — Harness `GrpcProtobuf` framing honesty (DESIGN flag #2)

| DoR Item | Status | Evidence/Issue |
|----------|--------|----------------|
| Problem statement clear | PASS | "`GrpcProtobuf` is accepted but inert; the caller gets a confusing decode failure." |
| Persona | PASS | Devin feeding gRPC-framed OTLP bytes, expecting prefix handling. |
| 3+ domain examples | PASS | Corrected framing note; both-framings on stripped bytes; length-prefixed boundary. |
| UAT (3-7) | PASS | 3 scenarios, decision-neutral. |
| AC from UAT | PASS | 3 AC. |
| Right-sized | PASS | S-M; document path XS; may share US-04's PR; 3 scenarios. |
| Technical notes | PASS | DESIGN flag #2 named; DISCUSS recommends document; mutation only if honour. |
| Dependencies | PASS | Tracked: DESIGN picks document-vs-honour. Enum doc already admits the limitation. |
| Outcome KPIs | PASS | KPI-6: framing claim == behaviour, 0 confusing failures; doc guard + both-framings test. |

### DoR Status: PASSED (DESIGN decision tracked as resolved-by-DESIGN, not a readiness blocker)

---

## Aggregate

| Story | DoR |
|-------|-----|
| US-01 | PASSED |
| US-02 | PASSED |
| US-03 | PASSED |
| US-04 | PASSED |
| US-05 | PASSED |
| US-06 | PASSED |

**Feature DoR: PASSED (6/6 stories).**

Note on the two document-vs-implement stories (US-05, US-06): readiness does NOT
require DISCUSS to pre-decide document vs implement. The stories, scenarios, and
AC are deliberately written to hold under EITHER outcome ("the claim matches the
behaviour, whichever DESIGN chooses"). The decision is a tracked, resolved-by-
DESIGN dependency — exactly the kind of constraint DoR item 8 contemplates — not
an unresolved blocker. This is the honesty-pass framing: the requirement is "make
the prose true", which is satisfiable both by correcting the prose and by changing
the code.
