# DISCUSS-wave peer review — `otlp-conformance-harness-v0` — iteration 1

**Reviewer**: `nw-product-owner-reviewer` | **Date**: 2026-05-04 | **Iteration**: 1 of 2

## Verdict: **APPROVED FOR HANDOFF TO DESIGN WAVE**

All eight DoR items pass for all seven user stories with evidence. The journey is single-step at the user-visible level (a library function call) with a flat emotional arc appropriate to the domain. All seven elephant-carpaccio slices are thin, independent, and each disproves a specific pre-commitment. Example data is realistic and traceable across steps. Outcome KPIs are measurable with numeric targets. Cross-document coherence is confirmed across journey, story map, user stories, shared artifacts, and KPIs. House style (British English, no human-effort estimates, personas-as-consumers) is enforced throughout. Architectural alignment with the CC0-1.0, integration-plane-first, port-and-adapter, OTel-everywhere posture is confirmed. No antipatterns detected. Approved on the first iteration.

## Dimensions reviewed and verdicts

| Dimension | Verdict |
|---|---|
| Elevator Pitch quality (Before / After / Decision-enabled per story) | PASS |
| Elephant-carpaccio discipline (thinness, taste tests 1–5) | PASS |
| Definition of Ready (9 items × 7 stories with evidence) | PASS |
| Antipattern detection (8 patterns scanned) | PASS — zero detected |
| Journey coherence (flow, emotional arc, shared artifacts, example data) | PASS |
| Bug patterns (version mismatch, hardcoded URLs, paths, missing commands) | PASS |
| Outcome KPIs (measurable, numeric targets, named methods) | PASS |
| Cross-document coherence (journey ↔ story map ↔ stories ↔ KPIs ↔ artifacts) | PASS |
| House style (British English, no human-effort, personas-as-consumers) | PASS |
| Architectural alignment (CC0-1.0, integration-first, port-and-adapter, OTel) | PASS |

## Non-blocking observations

Three items were noted but do not block approval. Each is named, mitigated, and deferred appropriately.

**Observation 1 — Deferred DESIGN decisions.** The wave-decisions and shared-artifacts documents explicitly defer to DESIGN: the choice between `cargo public-api` and `cargo-semver-checks` for public-surface enforcement; the workspace-level `cargo metadata` version-alignment check for `opentelemetry-proto`. These are appropriate deferrals — DISCUSS identifies the integration risk and names the validation method; DESIGN picks the implementation.

**Observation 2 — DEVOPS wave dependency.** US-07 and the outcome-kpis name a CI workflow requirement but defer the workflow-runner choice (GitHub Actions versus a FOSS alternative) to the DEVOPS wave. This is correct under the methodology: DISCUSS names the contract, DEVOPS implements it.

**Observation 3 — `prost` diagnostic quality (medium risk).** US-02's technical notes acknowledge that `prost::DecodeError` does not always provide a useful byte-locus, with the fallback that the violation records `ByteOffset::Unknown` when richer locus is unavailable. Named, mitigated, and accepted as a v0 limitation.

## Slice-by-slice quality summary

| Slice | Story | Disproves | Verdict |
|---|---|---|---|
| 01 | US-01 reject empty input | "Can we ship a Cargo crate on `opentelemetry-proto`?" | THIN, walking skeleton, PASS |
| 02 | US-02 reject malformed protobuf | "Does `prost::DecodeError` carry useful byte locus?" | THIN, piggybacks on US-01, PASS |
| 03 | US-03 reject signal mismatch | "Can asserted-type checking work without a fast type-discriminator?" | THIN, two extra decode attempts on the error path, PASS |
| 04 | US-04 accept logs | "Does the accept-path type-identity contract hold for logs?" | THIN, half-day; reuses decode path, PASS |
| 05 | US-05 accept traces | "Does the signal abstraction generalise?" | THIN, half-day, parameter change, PASS |
| 06 | US-06 accept metrics | "Does the abstraction hold for the most complex signal?" | THIN, half-day, PASS |
| 07 | US-07 lock the contract via corpus | "Can a content-addressed corpus defend the contract?" | THIN, under-a-day for the harness side, PASS |

## Handoff checklist for DESIGN wave (`nw-solution-architect`)

The DESIGN-wave recipient inherits:

1. Seven user stories that each pass the 9-item DoR with evidence.
2. A complete journey schema with three steps, an explicit emotional arc, and four integration checkpoints (cross-step shared-artefact invariants).
3. Seven shared artefacts with documented sources, consumers, and integration risks; the registry names which validations are deferred to DESIGN versus owned by DISCUSS.
4. Elephant-carpaccio slicing confirmed thin and disproof-driven across seven slices.
5. Seven outcome KPIs with measurable targets and named measurement methods (the north star is 0% false-positive rate, enforced as a CI invariant).
6. Cross-document coherence verified.
7. House style and architectural alignment confirmed.

DESIGN's expected output is the crate-level decisions: public surface shape, error-type design, dependency pinning policy, conformance-test-vector layout, and the CI contract that the DEVOPS wave will later implement. The architectural ground is laid in the four prior documents (research, architecture, roadmap, README), so DESIGN will be a tight pass rather than a full architectural exploration.
