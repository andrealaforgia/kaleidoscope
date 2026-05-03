# Wave Decisions — `otlp-conformance-harness-v0` (DISCUSS)

> **Wave**: DISCUSS (nw-product-owner / Luna).
> **Date**: 2026-05-03.
> **Author**: Luna, single-pass.
> **Companion documents**: `journey-validate-otlp-bytes-visual.md`, `journey-validate-otlp-bytes.yaml`, `story-map.md`, `shared-artifacts-registry.md`, `outcome-kpis.md`, `user-stories.md`, `dor-validation.md`.

---

## Decisions agreed before the wave started (recorded for posterity)

These were settled with Andrea before Luna began work and are recorded here so the DESIGN wave does not re-litigate them.

- **D1. Feature type**: Backend. The harness is a pure Rust library consumed via Cargo. No UI, no network surface, no human-in-the-loop UX.
- **D2. Walking skeleton**: No separate walking-skeleton story. Per the brief, the harness *is* the first concrete slice of Kaleidoscope's overall walking skeleton; Slice 01 (US-01) functions as the project-level skeleton.
- **D3. UX research depth**: Lightweight. The journey is narrow (`bytes -> Result<record, violation>`), and there is no UX surface to research.
- **D4. JTBD analysis**: No. The motivations are uncontroversial — validate that a byte sequence is a valid OTLP message before passing it on.

Per the skill, with JTBD = No, Luna executed `*journey for otlp-conformance-harness-v0` then `*story-map` then `*gather-requirements` with outcome KPIs, in that order, in a single pass.

## Wave-internal decisions made during DISCUSS

### W1. The harness is a validation gate, not a parser

The harness uses `opentelemetry-proto` to parse incoming bytes but exposes the upstream types unchanged on the accept path. It does not introduce a competing OTLP type system. This decision lets Aperture, Codex, every storage engine, and third-party callers use the harness's accept-path output as a drop-in replacement for direct `opentelemetry-proto` decoding, without conversion overhead.

### W2. The violation rule set is closed

Every rejection is one of a small, named set of rules: `EmptyInput`, `WireType::ProtobufDecode`, `WireType::SignalMismatch` in v0. Adding a rule is a minor version bump. This is what makes downstream pattern-matching safe.

### W3. Signal-type inference is out of scope

The caller asserts which `validate_*` function to invoke based on its own routing context (HTTP path, gRPC method). The harness does not infer the signal type. Inference would require a heuristic (peek the first tag) that is wrong on invalid input by design and adds an attack surface for confused-deputy errors. Symmetric: an asserted-but-mismatched signal produces `WireType::SignalMismatch`, which is the correct outcome.

### W4. Profiles is out of scope for v0

The OpenTelemetry Profiles signal is still in development at the spec version pinned for Phase 0. The harness covers the three OTLP stable signals (logs, traces, metrics). Profiles is added as a follow-up release once the upstream signal stabilises. The Strata storage engine in Phase 6 is the first Kaleidoscope component that depends on profiles validation; that schedule has 18+ months of slack relative to the harness's v0 schedule.

### W5. Semantic-conventions checks are not in the harness

Required-attribute checks per OpenTelemetry semantic conventions belong in Codex (Phase 0), not in the harness. The harness's job is wire-level conformance: does this byte sequence decode cleanly as the asserted OTLP signal type? The semantic question — does this record carry the required `service.name`, `host.name`, etc. — is Codex's contract. Splitting these responsibilities keeps each component small enough to be auditable.

### W6. The harness emits no telemetry of its own

Per the project's no-telemetry-on-telemetry commitment in the roadmap (section A.2), the harness writes nothing to stdout, stderr, or any logger. All diagnostic information is carried by the `OtlpViolation` value. This is enforced as a UAT scenario (US-01, US-04) and will be re-enforced in DESIGN by a crate-level lint or a side-effect-free integration test.

### W7. The corpus is the contract

Slice 07 (US-07) ships the reference test-vector corpus and the CI gate that runs it. Until the corpus exists, the harness's contract lives in disparate hand-written tests. After the corpus exists, every accept path is defended by an accept vector and every reject rule is defended by at least one reject vector. The corpus is a first-class deliverable on the same level as the public API.

### W8. The journey is single-step at the user-visible level

There is no multi-step UX. Steps 1–3 in the journey schema (receive bytes, validate, surface verdict) are an analytical decomposition of a single function call. The emotional arc is deliberately flat: cautious → briefly tense → confident-or-informed. Walter's hierarchy is satisfied at the functional and reliable layers; usability and pleasurable layers do not apply (no human-in-the-loop interaction).

## Risks and mitigations

| Risk                                                                                                  | Probability | Impact   | Mitigation                                                                                              |
|--------------------------------------------------------------------------------------------------------|-------------|----------|----------------------------------------------------------------------------------------------------------|
| `prost::DecodeError` does not provide enough byte-locus information for useful diagnostics             | Medium      | Low      | US-02 records `ByteOffset::Unknown` and explains the limitation; richer locus reporting is a follow-up.  |
| The OpenTelemetry SDK we use to capture corpus vectors gets a breaking-change release before Phase 1   | Low         | Medium   | Pin the SDK version in the corpus capture program; document the version in each vector's `.expected.json`. |
| Downstream consumers (Aperture, Codex) want a richer violation type (e.g. multiple violations per call)| Medium      | Low      | v0 returns the first violation only; multi-violation reporting is a non-breaking minor-version addition. |
| Profiles support is requested before the OTel signal stabilises                                        | Low         | Low      | W4 explicitly defers; profiles validation is a separate v0.1 (or v1) of the harness.                     |
| DIVERGE wave artefacts (`recommendation.md`, `job-analysis.md`) absent — wave executes without them    | Certain     | Low      | Recorded explicitly: this DISCUSS wave bootstraps `docs/feature/`. The brief from Andrea is the authority. |
| CI workflow runner choice (GitHub Actions vs other) not yet made                                       | Certain     | Low      | US-07 names the contract (corpus must run on every commit) without prescribing the runner — that is DEVOPS's call. |

## Missing-DIVERGE note

This DISCUSS wave executed without prior `docs/feature/otlp-conformance-harness-v0/diverge/recommendation.md` or `job-analysis.md`. Per the brief, no DIVERGE wave was run; Andrea's instructions plus the four prior architecture documents (README, `kaleidoscope-architecture.md`, `kaleidoscope-implementation-roadmap.md`, the observability research) substituted. The brief documents this as deliberate — the project's architectural posture is settled enough that JTBD analysis would not have surfaced new motivations.

The DESIGN wave should treat the brief plus the four prior architecture documents as the upstream context, not search for a non-existent DIVERGE corpus.

## Handoff to DESIGN

Recipient: `nw-solution-architect`.

Required reading order (each is a few pages):

1. `wave-decisions.md` (this file).
2. `journey-validate-otlp-bytes-visual.md` — the narrow journey and emotional arc.
3. `journey-validate-otlp-bytes.yaml` — structured schema with embedded Gherkin.
4. `user-stories.md` — seven LeanUX stories (US-01 to US-07), each with Elevator Pitch, Problem, Who, Solution, Domain Examples, UAT Scenarios, AC, KPIs, Technical Notes, Dependencies.
5. `story-map.md` — backbone, slices, walking-skeleton note, priority rationale.
6. `shared-artifacts-registry.md` — seven shared artefacts with sources, consumers, integration risk.
7. `outcome-kpis.md` — seven KPIs with baselines, measurement plan, hypothesis.
8. `dor-validation.md` — the 9-item gate, passed for every story with evidence.

DESIGN's expected output:
- Crate-level decisions: public surface (the three `validate_*` functions, `Framing`, `SignalType`, `OtlpViolation`, `Rule`), error-type design, dependency on `opentelemetry-proto` (version pinning strategy), corpus directory layout (concrete file naming), CI workflow contract (runner-neutral).
- DESIGN will be light because the architectural ground is already laid — the harness is one Cargo crate at `crates/otlp-conformance-harness/` in a Cargo workspace at the Kaleidoscope project root.

## Handoff to DEVOPS (via DISTILL)

Recipient: `platform-architect`, with `acceptance-designer` as the intermediate consumer of the journey YAML and the per-story Gherkin scenarios.

DEVOPS's expected output (informed by `outcome-kpis.md`):
- The CI workflow that runs `cargo test -p otlp-conformance-harness --all-targets` on every commit affecting the crate.
- The verdict-counts artefact contract per build (the dashboard requirement is informational, not blocking, for v0).
- The hash-verification step for corpus vectors (described in shared-artifacts-registry under `test_vector_corpus`).

## Definition-of-Ready status

All seven user stories have passed the 9-item Definition of Ready hard gate. Evidence is in `dor-validation.md`. Peer review next.

## Next-step instruction (for the parent agent)

Invoke `nw-product-owner-reviewer` against `docs/feature/otlp-conformance-harness-v0/discuss/`. After review approval (max 2 iterations per the skill), proceed with handoff to DESIGN (`nw-solution-architect`) and prepare the DISTILL handoff package for `acceptance-designer`.
