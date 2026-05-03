# Cross-Document Consistency Review — Kaleidoscope Project

**Date**: 2026-05-03 | **Reviewer**: Scholar (`nw-researcher-reviewer`) | **Review ID**: cross-doc-consistency-review-20260503-001

## Verdict: **APPROVED**

Kaleidoscope's documentation suite is internally consistent with all five recent pivots: CC0-1.0 public-domain dedication, integration-plane-first phasing with MVP at month 6, port-and-adapter architecture discipline, elimination of human-effort metrics in favour of wall-clock time and conceptual difficulty, and the Ray/Filament component rename. All fifteen optical-instrument component names are used consistently. No contradictions were found between architecture and roadmap phasing. The research baseline document predates the pivots but contains no claims contradicted by current architecture.

---

## Findings by Document

### `README.md`

**praise**: Clean, authoritative identity statement from line 1. "Dedicated to the public domain under CC0-1.0" appears twice and is the correct canonical form. The licensing section is thorough and accurate, correctly explains the public-domain dedication, the permissive-licence fallback for continental Europe, and the non-revocability of the existing code. The CONTRIBUTING posture statement correctly reflects closed-for-now single-author model with CC0 public-domain submission model at contribution-opening time, not a DCO/CLA frame.

**praise**: Component table lists all fifteen components with correct optical-instrument naming. Ray is correctly named throughout; no "Filament" residue. Component descriptions align with roadmap roles: Spark as SDKs, Aperture as OTLP gateway, Pulse/Lumen/Ray/Strata as storage engines, Prism as unified frontend.

**suggestion (non-blocking)**: The README's "Pulse is not a re-skinned Mimir" parallel structure could be sharpened by echoing the document's "with a new logo" framing, but the current phrasing is functionally clear and the point stands.

### `LICENSE`

**praise**: The document is the canonical Creative Commons CC0 1.0 Universal legal code in full. No licence slip-up; no residue of AGPL-3.0 or Apache-2.0 claims on Kaleidoscope's own code. The Waiver is precise and permanent.

### `CONTRIBUTING.md`

**praise**: Single-author posture is explicit. Correctly frames CC0 public-domain model without CLA or DCO. No governance machinery, no contribution-eligibility process beyond "when contribution opens". Signature line makes authorship clear.

### `docs/architecture/kaleidoscope-architecture.md`

**praise**: The phasing table correctly orders phases 0–2 as integration plane (Codex + Spark + OTLP harness; Aperture + Prism v0; Beacon + Aegis + Loom v0), MVP at month 6, then phases 3–6 as storage plane (Lumen, Pulse, Ray, Strata + exemplars), phases 7–9 as durability, native queue, and native authz. This matches the roadmap's section D in all material respects.

**praise**: The port-and-adapter strata model is internally coherent and correctly distinguished: components → ports → adapters → substrate (Apache Foundation libraries) → runtime. The glossary defines each term precisely. No contradiction with the roadmap's section B and C framing of the embed-vs-wrap rule refined into port-and-adapter discipline.

**praise**: All fifteen component names are present and correct. Ray is the canonical name; no "Filament" appears.

### `docs/roadmap/kaleidoscope-implementation-roadmap.md`

**praise**: Section A opens with a tight, accurate restatement of CC0-1.0 posture. The distinction between "public-domain posture does not require dependency cleanliness as a contract" and "still applies dependency-cleanliness as engineering practice" is the correct articulation of the post-pivot position. No residue of AGPL/Apache framing of Kaleidoscope's own code.

**praise**: The roadmap explicitly states effort "is described in conceptual terms (difficulty, integration surface, known unknowns) rather than human-engineer-month units, because work on Kaleidoscope is done by AI agents." This is the correct explanation of why human-effort metrics have been scrubbed. The sentence is repeated in the section D preamble. Both instances are explanatory, not prescriptive — the document correctly avoids using FTE-months, person-days, or salary figures anywhere in the phase summaries.

**praise**: Section B correctly distinguishes library from platform. Section C documents all fifteen components and their build-vs-vendor reasoning. Each component's entry answers four questions: what we build, library substrate, wire/format contract, and explicitly what upstream peer we refuse to use. Ray is correctly named and described. The port-and-adapter discipline is applied consistently to each component.

**praise**: Section D phasing describes phases 0–9 with exit criteria, library dependencies, and difficulty assessments. No human-effort figures; all timing is calendar wall-clock months. The phasing matches the architecture doc's phase table exactly. The build-order DAG is a valid topological ordering of the component dependencies.

**praise**: Section E correctly frames the two strategic commitments: "wall-clock honesty" for the 36-month horizon, and the opt-in nature of the storage plane. The "temptation register" documents four specific pressures (embed ClickHouse, run Grafana behind Prism, bootstrap with SaaS, adopt Confluent Schema Registry) and explains the structural reasons each is rejected.

**praise**: Appendix F lists every named dependency with licence verification at primary source (access date 2026-05-03). No Kaleidoscope-own code is listed as AGPL-3.0 or Apache-2.0; those licences appear only in the allowed/excluded external-library rows.

**issue (blocking)**: The C.9 Strata sub-section closed with the sentence "Building on Arrow + Parquet keeps Strata format-aligned with Lumen, Pulse, and Ray — one columnar substrate across all four pillars." The phrase "all four pillars" was ambiguous about whether Cinder is included; Cinder is the cold tier, not a pillar, but the wording was easy to mis-read. **Recommendation**: explicitly enumerate the four pillars and note Cinder's distinct role.

### `docs/research/observability/otel-compatible-observability-platform-comprehensive-research.md`

**praise**: The research document is explicitly dated 2026-05-03 and authored by `nw-researcher` (Nova), confirming it is the evidence base that informed the architecture and roadmap. As noted in the brief, it predates the pivots and provides background on OTel-compatible observability generally, not Kaleidoscope-specific implementation.

**praise**: Executive summary correctly identifies OpenTelemetry as the vendor-neutral contract, the bifurcation of storage (TSDB for metrics, columnar for logs/traces), and the startup playbook (managed → self-host → build). These findings align with Kaleidoscope's architecture decision to be integration-plane-first on any external OTel backend (month 6), then storage-plane-incremental. No contradiction.

**praise**: Section A establishes foundations — four pillars, OTel scope and maturity, OTLP wire contract, semantic conventions, monitoring methodologies (USE, RED, Four Golden Signals). All cited to primary sources. These form the evidence base for Kaleidoscope's architecture; no contradictions detected.

---

## Cross-Document Concerns

### 1. Identity consistency — verified

All documents use CC0-1.0 as the canonical licence. No "FOSS-strict", "FOSS-forever", "FOSS Contract", or rug-pull framing appears anywhere. The phrase "dedicated to the public domain" appears consistently in README, the roadmap, and the LICENSE file's Statement of Purpose.

### 2. Licence consistency — verified

Spot-check across five locations where Kaleidoscope's licence is named: README opening, README Licensing section, CONTRIBUTING.md, roadmap section A, LICENSE file itself. All five are CC0-1.0. No claim that Kaleidoscope's own code is AGPL-3.0 or Apache-2.0. Dependencies are correctly identified with their own licences (Apache Kafka, AGPL Grafana OnCall for integration only, etc.).

### 3. Phasing consistency — verified

Architecture document phasing table and roadmap section D phases align exactly:

| Phase | Calendar | What ships |
|---|---|---|
| 0 | months 0–2 | Codex + Spark + OTLP harness |
| 1 | months 2–4 | Aperture + Prism v0 |
| 2 | months 4–6 (MVP) | Beacon + Aegis + Loom v0 |
| 3 | months 6–10 | Lumen |
| 4 | months 10–14 | Pulse |
| 5 | months 14–18 | Ray + Sieve v1 |
| 6 | months 18–22 | Strata + exemplars |
| 7 | months 22–26 | Cinder + Sluice + DR |
| 8 | months 26–30 | Native queue |
| 9 | months 30–36 | Native authz + Augur |

Roadmap Gantt chart matches: all phases appear in dependency order, wall-clock durations are consistent.

### 4. Port-and-adapter framing — verified

Architecture document strata view defines the five-layer model: components → ports → adapters → substrate → runtime. Roadmap section B establishes this same framing: "a library is code Kaleidoscope embeds; a platform is a service Kaleidoscope would have to depend on". Section C applies the discipline consistently: NATS JetStream embedded (phase 0), Kafka KRaft as external adapter (phase 7+), FoundationDB as transactional-KV port, SpiceDB as authz port (phases 2–8, replaced by native in phase 9). The "embed-vs-wrap test" is referenced as the load-bearing rule refined into port-and-adapter discipline. No contradiction detected.

### 5. No-human-effort consistency — verified

Spot-check three locations that would have previously contained human-effort metrics:

1. **Roadmap section D phase summaries**: Zero FTE-months, person-days, headcount bands. Difficulty is qualitative: "Modest", "Medium", "High". Wall-clock calendar is explicit.
2. **Roadmap section E**: Discusses calendar time and strategic trade-offs (integration plane fast, storage plane slower) but contains zero salary-cost framing or human-effort estimates.
3. **README "How Kaleidoscope is built"**: No FTE-months or headcount. Three architectural commitments are framed as design rules, not effort estimates.

The two intentional references to "human-engineer-month" in the roadmap are explanatory: they explain *why* the document avoids human-effort framing. Both are correctly authored.

### 6. Component naming consistency — verified

All fifteen optical-instrument components appear consistently across all documents. No "Filament" references in any document. Ray is the canonical trace-component name throughout.

### 7. Cross-reference integrity — verified

- README links to `docs/roadmap/kaleidoscope-implementation-roadmap.md` — file exists.
- README links to `docs/architecture/kaleidoscope-architecture.md` — file exists.
- README links to `docs/research/observability/otel-compatible-observability-platform-comprehensive-research.md` — file exists.
- Architecture document links to `../roadmap/kaleidoscope-implementation-roadmap.md` — correct relative path.
- Roadmap links to `../architecture/kaleidoscope-architecture.md` — correct relative path.
- Roadmap references `../../LICENSE` — correct relative path.

The old filename `kaleidoscope-foss-implementation-roadmap.md` is not referenced anywhere in the live documents.

### 8. Component descriptions coherence — spot-check

| Component | README claim | Roadmap detail | Coherence |
|-----------|--------------|----------------|-----------|
| Lumen | "Datadog Logs, Splunk, Loki, Elastic" | Apache Parquet + DataFusion | Complementary: README names competitors; roadmap details technical substrate. |
| Pulse | "Datadog Metrics, NR Metrics, Cloud Monitoring" | Prometheus TSDB format + PromQL | Complementary: README names competitors; roadmap explains format choice and PromQL compatibility. |
| Ray | "Datadog APM, NR Distributed Tracing, Tempo" | trace_id-partitioned Arrow + Parquet | Complementary: README names competitors; roadmap explains columnar storage. |

All descriptions are coherent; no contradictions found.

### 9. Section A consistency with the rest of the roadmap

Roadmap section A is rewritten post-pivot to tighten the CC0-plus-dependency-policy framing. Sections B–C expand on which dependencies are allowed/excluded and why. Appendix F is the dependency audit — zero non-OSI Kaleidoscope-own code; all exclusions are explained. Sections E and H document the temptations and anti-patterns that pressure the discipline. No contradiction detected. The licencing posture is coherent.

### 10. Knowledge Gaps and Conflicts coherence

Roadmap Knowledge Gap on OTel Profiles status and Conflict on DataFusion-vs-from-scratch query engine are consistent with the architecture document's substrate-vs-port-vs-component layering: DataFusion is substrate (Apache Foundation), exempt from port discipline, so the "conflict" is resolved by treating it as a foundational choice, not a swappable adapter.

---

## Resolved Since Previous Iteration

This is the first formal Scholar cross-document consistency review of the Kaleidoscope project post-pivot. Previous Scholar findings on the architecture and roadmap (during the design phase) have been fully cleared:

- **Ray/Filament naming**: Andrea's decision to rename Filament to Ray (to fit the optical metaphor of light paths through a prism, not light emission) is complete. No Filament residue in any live document.
- **CC0 vs AGPL/Apache licence flip**: The four-licence split (AGPL-3.0 platform / Apache-2.0 SDK / CC-BY-4.0 specs / Trademark) has been replaced by a single CC0-1.0 public-domain dedication. All identity statements updated. No old framing detected.
- **Integration-plane-first phasing**: The storage-engine-first roadmap has been replaced by integration-plane-first with MVP at month 6. Architecture and roadmap phasing tables now align exactly.
- **Port-and-adapter discipline**: The embed-vs-wrap rule has been refined into explicit port-and-adapter architecture with substrate exemption. Both architecture and roadmap documents articulate the discipline consistently.
- **Human-effort scrubbing**: All FTE-months, person-days, headcount, and salary-cost framing has been removed. Effort is now described as conceptual difficulty; timing is wall-clock calendar.

---

## Summary

All five pivots are correctly implemented across the documentation suite. No identity-framing residue. No licence slips. Phasing is consistent between architecture and roadmap. Port-and-adapter discipline is coherent. No human-effort metrics. Component naming is unified. Cross-references resolve. Descriptions between documents are complementary and non-contradictory. The research baseline (pre-pivot) contains no claims contradicted by current architecture. The documentation is ready for external visibility.

The single blocking item raised against roadmap section C.9 (the "all four pillars" wording in Strata's peer-rejection paragraph) was addressed in the same commit that persists this review.
