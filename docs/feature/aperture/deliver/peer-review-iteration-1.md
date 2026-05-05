# DELIVER-wave peer review — `aperture` v0 — iteration 1

**Reviewer**: Crafty (Review Mode, `nw-software-crafter-reviewer`) | **Date**: 2026-05-05 | **Iteration**: 1 of 2

## Verdict: **APPROVED**

Zero blockers, zero critical, zero high, zero medium, zero low. All ten review dimensions plus the nine quality gates (G1-G9) pass. DISTILL contract preserved across all 84 acceptance tests. Mutation kill rate 100% on slice-introduced surface. The Aperture v0 DELIVER cycle is end-to-end closed.

## Per-dimension verdicts

| # | Dimension | Verdict | Notes |
|---|---|---|---|
| 1 | Implementation bias detection | PASS | Every feature traces to DISCUSS AC; no over-engineering, no premature optimisation |
| 2 | Test quality and integrity | PASS | All acceptance tests enter through driving ports; zero internal-class testing; test budget honoured |
| 3 | Completeness and coverage | PASS | All DISCUSS Q1-Q6 + D1-D8 covered; three signals + invariant scenarios; 100% kill on slice-introduced mutations |
| 4 | RPP code-smell detection | PASS | L1-L2 scans clean; no long methods, no dead code, module structure matches design |
| 5 | Priority validation | PASS | Walking skeleton (Slice 01) lands gRPC + harness integration first as the highest-risk seam |
| 6 | DISTILL contract preservation | PASS | Acceptance tests structurally unchanged from RED state; hexagonal boundary held |
| 7 | External validity | PASS | All tests invoke through public entry points; binary in `main.rs` propagates exit codes |
| 8 | Mutation kill rate honesty | PASS | Spot-checks confirm kills are real (production-code simplification or new tests, never test relaxation) |
| 9 | Architecture compliance | PASS | Hexagonal discipline; `OtlpSink` / `Probe` trait boundary; single-validator-per-signal enforced |
| 10 | Feature-specific deep review | PASS | Probe gold-test genuine; SIGTERM equivalence deferral defensible; readiness drain flip well-defended |

## Quality gates (G1-G9)

All pass. G9 (no test modifications to accommodate implementation) verified by exhaustive spot-check across all 12 acceptance test files: zero assertions weakened, zero deletions, zero skips. Tests were strengthened in refactor cycles (new unit tests added to pin mutations), never weakened.

## Self-flagged points — verdicts

1. **DISTILL contract preservation** — VERIFIED CLEAN. All 84 RED tests structurally unchanged. Imports are `aperture::*` and `aperture::testing::*` only.
2. **Single validator per signal CI invariant** — VERIFIED ENFORCED. Three call sites, one each in `app::ingest_logs/traces/metrics`. Runtime invariant test corroborates.
3. **Mutation kill rate honesty** — VERIFIED HONEST. Slice 05's deterministic-test replacement of a 28s timeout-based kill, Slice 06's documented equivalent mutant on `empty_export_logs_service_request_bytes` (proto3 wire-format identity), and Slice 08's deferred-with-rationale 2 mutants on `aperture::run` (behind the SIGTERM signal-fork) are all defensible.
4. **Probe gold-test (Slice 06, ADR-0010 layer 3)** — VERIFIED REAL NETWORK REQUIRED. The lying-fixture scenario (OPTIONS=200 / POST=503) refuses startup; wire-traffic count assertions catch a no-op `probe()` body.
5. **SIGTERM equivalence test (Slice 08)** — COST-BENEFIT SOUND. Process-spawning fixture cost is high, fragility is real (port races, CI portability shears, child-process leaks), benefit is 2 mutants behind a signal-fork already exercised by the production binary on every k8s deployment. Defer to v1.
6. **Probe semantics fork vs design template (Slice 06)** — DOCUMENTED AND PERMITTED. Test contract wins (204-only short-circuit on OPTIONS); ADR-0007 unaffected; component-design.md pseudocode is advisory and gets a future ADR refresh note in slice-06-completion.md.

## Test budget validation

All 8 slices within budget. Walking skeleton (Slice 01) and mutation-pinning unit-test spikes (Slices 04 + 08) are documented overflows for legitimate reasons.

## Architecture decision traceability

10 of 10 design decisions (D1-D10) faithfully implemented and tested:
- Hexagonal + Rust idiomatic (D1, ADR-0006)
- tonic + axum (D2, ADR-0006)
- `async-trait OtlpSink` (D3, ADR-0007)
- TOML + figment + deny_unknown_fields (D4, ADR-0008)
- tracing + JSON to stderr (D5, ADR-0009)
- Per-transport semaphore (D6, ADR-0010)
- `thiserror ApertureError` enum (D7)
- `reqwest` HTTP client + plaintext (D8)
- Earned-Trust probe with three orthogonal enforcement layers (D9, ADR-0007)
- Architectural-rule enforcement via xtask (D10)

## Strengths called out

- **Walking skeleton design**: Slice 01's choice to land the real harness integration on day one rather than a hardcoded reject endpoint pays down integration risk early. Test binary against real OTel SDK + real harness + real sink from the start.
- **Mutation testing discipline**: every slice closes touched files to 100% kill rate. Slice 05's replacement of a 28s timeout-based kill with a deterministic unit test shows genuine understanding. Slice 06's documented equivalent mutant is the mark of a team that understands testing deeply.
- **Probe contract enforcement**: three-layer enforcement (subtype + structural + behavioural) is correctly over-engineered for a service whose external dependency must be proven trustworthy at startup. The lying-fixture pattern is the catalogued v0 substrate vulnerability.
- **Readiness state machine**: clean one-directional `Starting → Ready → Draining` with sticky `Draining`. The 100ms /readyz flip target is well-defended by background shutdown + fast polling.
- **Configuration schema forward-compatibility**: TLS/SPIFFE knobs reserved for Phase 2 Aegis without zero impact on v0 production code; `deny_unknown_fields` on every nested struct.
- **Per-transport semaphore independence**: Slice 05's choice prevents gRPC saturation from blocking HTTP requests, defended by an explicit cross-transport test.
- **Closed event vocabulary discipline**: 20 names total, all declared as constants in `observability::event::*`. Zero leakage. Slice 07's single shared `tls_not_supported_in_v0` for both TLS and SPIFFE is the correct DISCUSS interpretation.

## Findings

None. No blockers, no non-blocking issues, no nitpicks at the substantive level.

## Iteration budget

Iteration 1 of 2 maximum. Zero blocking items; no iteration 2 required. DELIVER wave is closed for `aperture` v0.

## Handoff readiness

The orchestrator (Bea) may proceed directly to:

1. **Graduation lockstep edit** in a single commit:
   - `.github/workflows/ci.yml` Gate 1: scope `-p otlp-conformance-harness` → `--workspace`
   - `.github/workflows/ci.yml` Gate 5: `--package otlp-conformance-harness` → `--package otlp-conformance-harness --package aperture`
   - `scripts/hooks/pre-commit`: remove `--workspace --exclude aperture`, replace with `--workspace`
2. **Tag** `aperture/v0.1.0` with annotated message capturing the milestone.
3. **Push tag + main** to make the milestone canonical.

The Aperture v0 feature is then shipped end-to-end through nWave for the second time on Kaleidoscope, after the OTLP conformance harness v0.1.0 tag set the precedent.
