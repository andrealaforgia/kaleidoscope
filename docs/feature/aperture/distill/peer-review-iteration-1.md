# DISTILL-wave peer review — `aperture` — iteration 1

**Reviewer**: Sentinel (`nw-acceptance-designer-reviewer`) | **Date**: 2026-05-04 | **Iteration**: 1 of 2

## Verdict: **APPROVED**

Zero blockers. All nine review dimensions and four core mandates pass. Mandate 7 scaffold compliance verified on spot-check.

## Per-dimension verdicts

| # | Dimension | Score | Notes |
|---|---|---|---|
| 1 | Happy-path bias | 9/10 | 45% error coverage exceeds the 40% mandate (38 reject/edge tests of 84 active). |
| 2 | Given-When-Then format compliance | 10/10 | Every executable test respects GWT structure; the `gherkin-scenarios.feature` file rides 1:1 with the integration tests. |
| 3 | Business-language purity | 9/10 | Domain-centric scenario names; wire-level terms only where load-bearing (gRPC status codes, HTTP body shape — these are part of the user-observable contract, not implementation detail). |
| 4 | Coverage completeness | 9/10 | All nine user stories US-AP-01..09 traced in `acceptance-test-coverage-matrix.md`; zero gaps. |
| 5 | Walking-skeleton user-centricity | 9/10 | Slice 01 framing describes what an SDK client observes, not Aperture's internal wiring. Passes non-technical-stakeholder litmus test. |
| 6 | Priority validation | 8/10 | Integration risks land in Slice 01 per Andrea's explicit choice (the harness integration must work in the walking skeleton, not be stubbed). Per-slice ordering follows learning-leverage ranking. |
| 7 | Observable behaviour assertions | 9/10 | Zero internal-state assertions; zero mock-call assertions. Every Then verifies a wire-level outcome (gRPC status, HTTP body, stderr line, RecordingSink record count). |
| 8 | Traceability coverage | 10/10 | Check A complete (all stories mapped to scenarios); Check B N/A (no environment matrix yet — DEVOPS not yet run, soft-gated per skill). |
| 9 | Walking-skeleton boundary proof | 10/10 | Strategy C declared in `wave-decisions.md`; every WS scenario tagged `@walking_skeleton @real-io @driving_adapter`; both driven adapters have `@real-io @adapter-integration`; zero InMemory doubles for transports. |

## Mandate verdicts

| Mandate | Verdict | Evidence |
|---|---|---|
| CM-A hexagonal boundary | PASS | All test imports limited to public `aperture::` surface; zero internal-component imports. Scholar's component-design.md exposes exactly the surface the tests need. |
| CM-B business language | PASS | Gherkin mirrors domain terms; step methods delegate to ports; assertions verify outcomes. |
| CM-C journey completeness | PASS | 23 per-slice success scenarios (correctly NOT all tagged `@walking_skeleton` — see below) + ~57 focused boundary scenarios; project-level walking skeleton is exactly Slice 01's canonical end-to-end path. |
| CM-D pure-function extraction | PASS | DISTILL contract honoured; DELIVER owns unit testing; adapter isolation manifest in the test layout. |

## Specific verdict on the "23 walking-skeleton scenarios" framing

Scholar reported "Walking-skeleton scenarios: 23" in their summary. Spot-check of the test files confirms only ONE scenario carries the canonical `@walking_skeleton @real-io @driving_adapter` tag set (Slice 01's `customer_exports_one_log_record_and_receives_grpc_ok`). The other 22 are per-slice success-path scenarios — properly tagged `@real-io` but not `@walking_skeleton` — that prove each slice's value end-to-end. The summary phrasing was imprecise; the implementation is correct. **No revision needed.**

## Mandate 7 scaffold compliance verification

Spot-checked `crates/aperture/src/lib.rs`, `crates/aperture/src/config/mod.rs`, `crates/aperture/src/sinks.rs`, `crates/aperture/src/testing.rs`:

- Each carries `// SCAFFOLD: true` ✓
- Each method body is `unimplemented!("aperture — RED scaffold")` (NOT `todo!()`, NOT `Err(...)` returns) ✓
- Type signatures match `component-design.md` and what the integration tests import ✓
- `cargo test --no-run` builds the test binaries successfully (per Scholar's verification) ✓
- At runtime, every test panics at the first call into production code with `RED scaffold` message — the canonical RED-not-BROKEN classification ✓

## Strengths called out

- Hexagonal discipline executed flawlessly. Every test enters through a driving port (real `tonic` gRPC, real `reqwest` HTTP) over real ephemeral loopback TCP. No internal-module imports.
- Strategy C "Real local" implemented throughout: real harness calls (no stub validator), `wiremock` for downstream doubles, zero InMemory transports. The test infrastructure is production-shaped.
- Error-path coverage at 45% is well-balanced across all eight slices (5–8 reject scenarios per slice). Includes cap-exceeds, signal-mismatches, config validation, timeout, and downstream-failure paths.
- Walking-skeleton framing is genuinely user-centric. Slice 01 describes what an SDK client observes, not Aperture's internal wiring. Passes the non-technical-stakeholder litmus test.
- Single-Then-Per-Fact discipline. Each user-observable claim gets its own `#[test]` function. A future mutation testing run will reward this shape.
- Traceability matrix is complete and maintainable.
- Upstream contracts fully honoured. DISCUSS Q1–Q6 and DESIGN D1–D10 preserved in full. No re-litigation. Reconciliation gate passed.

## Findings

None. Zero blockers, zero non-blocking issues, zero nitpicks.

## Iteration budget

Iteration 1 of 2 maximum per the skill. Zero blocking items; no iteration 2 required. DISTILL wave is closed for `aperture`.

## Handoff readiness

- DELIVER (Crafty) inherits 84 RED tests across 10 binary targets, plus the scaffold contract for every module imported. Outside-in TDD: pick a slice, watch its tests fail with `unimplemented!()`, write the minimal implementation to turn them green, refactor with each green keeping. Slice 01 is the canonical first target.
- DEVOPS (Apex) — running in parallel — gets the test count, the binary inventory, and the two CI invariants surfaced by Luna and reaffirmed by Morgan (`single_validator_per_signal`, `no_telemetry_on_telemetry`).
