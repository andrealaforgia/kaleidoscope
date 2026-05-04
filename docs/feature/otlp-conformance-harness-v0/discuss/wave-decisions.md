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

---

## Iteration 2 (2026-05-04)

Iteration 1 received two parallel peer reviews:

- `nw-product-owner-reviewer` returned APPROVED with zero blocking items (`peer-review-iteration-1.md`).
- `nw-acceptance-designer-reviewer` (Sentinel) returned APPROVED with caveats (`peer-review-iteration-1-acceptance-designer.md`): four substantive findings (two blocking, two high) plus five non-blocking suggestions.

Iteration 2 applies Sentinel's four substantive fixes plus the cheap, self-contained non-blocking suggestions (1, 2, and 5). Suggestions 3 and 4 are intentionally skipped — 3 is already covered by `shared-artifacts-registry.md`, and 4 is explicitly deferred to DEVOPS by US-07's existing technical notes.

### Substantive fixes applied (line references are post-edit)

| Fix | Severity | Location | Change |
|-----|----------|----------|--------|
| 1 | blocking | `user-stories.md` US-03 AC 2 (line 300) | Replaced the internal-state assertion ("does not enter the alternative-decode path") with an observable, two-part AC: on a matching signal the harness returns `Ok(record)` immediately and the returned record is the typed upstream value (not an intermediate state, surrogate, or harness-local wrapper). Verifiable by a Cargo unit test that pattern-matches on the return value. |
| 2 | blocking | `user-stories.md` US-06 AC 5 (line 583) | Replaced the vague "same return-shape pattern" with the three exact function signatures plus the explicit assertion that all three return the same `OtlpViolation` type on the error path. |
| 3 | high | `user-stories.md` US-02 scenarios (lines 162–188) | Split the original truncated-body scenario into two: one asserting the byte offset is between 40 and 60 inclusive (mutation-resistant against an always-zero locus), one asserting the `observed` field contains one of "unexpected EOF", "wire type error", "missing length-delimited data" (mutation-resistant against a generic "error occurred" string). |
| 4 | high | `user-stories.md` US-04 scenario 2 + AC 2 (lines 372–379, 396) | Reframed the compile-time type-system assertion. The Gherkin scenario now describes runtime-observable downstream usability (the returned record is passed to a function whose parameter type is the upstream `ExportLogsServiceRequest` and the call type-checks and runs without conversion). The type-path identity check moves to AC 2, verifiable by a CI check on the public API; mechanism choice (`cargo expand`, `cargo doc --no-deps` grep, or `cargo public-api`) is explicitly a DESIGN-wave decision. |

### Non-blocking suggestions applied

| # | Location | Change |
|---|----------|--------|
| 1 | `user-stories.md` US-07 technical notes (line 706) | Added explicit "Hash algorithm and storage format" bullet: SHA-256, hex-encoded, stored under `content_hash` in the sibling `.expected.json`, computed at vector creation and re-verified before every validation run. |
| 2 | `user-stories.md` US-02 invalid-varint scenario (line 187) | Added the named decode-error category set to the `observed`-field assertion (same set as Fix 3, plus "invalid varint"). |
| 5 | `user-stories.md` US-01 AC (line 100) and US-04 AC (line 398) | Tightened the no-side-effects ACs to explicitly name the three observable channels (stdout, stderr, logging facade) so the DISTILL author has a clear assertion target. |

### Suggestions skipped (with reason)

- Suggestion 3 (corpus runner `.expected.json` schema confirmation) — already documented in `shared-artifacts-registry.md` under `test_vector_corpus`. No iteration-2 change needed.
- Suggestion 4 (CI runner choice for the public-API check) — explicitly deferred to DEVOPS in US-07's existing technical notes and US-04's AC 2. The contract is named, the mechanism is not prescribed.

### DoR re-verification

`dor-validation.md` is updated for US-02, US-03, US-04, US-06, and US-07 to reflect the revised scenarios and ACs (items 4 and 5 re-verified where the change affected the evidence). All seven stories remain DoR PASS. US-01 and US-05 are unchanged in substance; US-01's AC 4 received a wording tightening (suggestion 5) but its evidence claim is unchanged.

### Iteration budget

This is iteration 2 of the 2-iteration budget per the nWave skill. Edits are surgical; no new stories, scenarios, or ACs were introduced beyond the targeted replacements and the splits Sentinel recommended. The iteration-2 reviewer (Sentinel re-review) verifies the four substantive fixes specifically.
