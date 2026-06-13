# Wave Decisions: prism-echarts-paint-e2e-v0 (DESIGN)

Author: Morgan (nw-solution-architect). Wave: DESIGN. Date: 2026-06-13.
Mode: PROPOSE (autonomous, overnight run). Decision 0 scope: APPLICATION.
British English. No em dashes in body.

## Inputs grounded (read-first checklist)

- [x] DISCUSS `wave-decisions.md` — the 2-slice/3-story plan, the decided
  "it painted" observable (paint signal on `finished` + non-empty series +
  non-blank canvas probe), D1-D6, the D4 CI-browser load-bearing flag, the
  honest limit, C1-C10.
- [x] DISCUSS `user-stories.md` — US-PE-01/02/03, ACs, Elevator Pitches,
  UAT (BDD) scenarios.
- [x] DISCUSS `story-map.md`, `outcome-kpis.md`, `dor-validation.md`
  (present in `discuss/`; the carpaccio and KPI framing carried).
- [x] `apps/prism/src/lib/echarts/EChart.tsx` — the jsdom canvas-2D-probe
  skip (`:69-84`), the swallowed paint errors (`:88-98`), `data-tick-count`
  (`:104`). Confirmed: the swallow's comment is backwards (the `instance ===
  null` early-return at `:90` is the real jsdom guard; the catch fires only
  in a real browser).
- [x] `apps/prism/e2e/slice-01-*.spec.ts` + `slice-03-*.spec.ts` — the
  UNIMPLEMENTED bodies + Given/When/Then comments (the contract the specs
  already encode); the `[data-prism-chart-painted="true"]` selector
  (`slice-01:64`); the zero-uncaught-error invariant (`slice-03:33-49`).
- [x] `apps/prism/playwright.config.ts` — `testMatch:
  ['__no-spec-matches-yet__.spec.ts']` (`:57`), the pinned
  `PROMETHEUS_IMAGE_DIGEST` (`:35-41`, SSOT), the per-slice roadmap comment
  (`:43-57`), the chromium/firefox/webkit projects (`:70-74`).
- [x] `apps/prism/src/` broader — `App.tsx` (composition root, config-error
  refusal), `QueryPanel.tsx` (the query flow; `showChart` gates `<EChart>`
  on success only; empty/parse/transport/config render their own visible
  elements; the accessible fallback `<table>` caption), `buildOption.ts`
  (pure; success → non-empty series, non-success → `series: []`),
  `e2e/global-setup.ts` (docker fixture).
- [x] `apps/prism/README.md` — the honest single-PromQL-panel claim.
- [x] `docs/product/architecture/brief.md` — extended with the
  `## Application Architecture — prism-echarts-paint-e2e-v0` section.
- [x] ADR numbering — next free is **0075** (0074 = read-path auth). ADR-0030
  owns the ECharts wrapper this extends; ADR-0073 read as the recent style
  template.
- [x] Four-quadrants prism report finding (never-painted + the swallow) —
  captured via DISCUSS (the report's Q3 finding 2 is reproduced and verified
  on this branch by the DISCUSS code-findings table; re-verified here by
  direct reads of `EChart.tsx`, `QueryPanel.tsx`, `playwright.config.ts`).

## DESIGN decisions (DD) — resolving the DISCUSS flags

| DD | DISCUSS flag | Decision | Where |
|---|---|---|---|
| **DD1** | D1 paint-signal mechanism | `data-prism-chart-painted` on the `<EChart>` container; `"false"` initial + on reset, `"true"` only on ECharts `finished` with `getOption().series` non-empty; reset to `"false"` before each `setOption`. Parallel to `data-tick-count`. `finished` (not `rendered`) for settle-once semantics. | ADR-0075 §D1 |
| **DD2** | D2 non-blank-canvas technique | DOM `<canvas>` `getImageData` pixel-sampling, assert > 1 distinct value. NO ECharts-instance-on-`window` seam (the signal already encodes the non-empty-series check). Fallback `<table>` caption is corroborating, not primary. | ADR-0075 §D2 |
| **DD3** | D3 swallow remediation | Catch-and-surface (not swallow, not re-throw): on real-browser `setOption` throw, leave the signal `"false"` AND `console.error`; page stays interactive. The narrow canvas-probe skip (the `instance===null` early-return) is the real jsdom guard, preserved verbatim. | ADR-0075 §D3 |
| **DD4** | D4 CI-browser job | FLAGGED for DEVOPS (not resolved). DESIGN states WHAT runs: the two un-MARKed specs, headless Chromium only, docker fixture, browser install. DEVOPS owns gate-vs-continue-on-error. Honest limit recorded (C6). | ADR-0075 §D4, brief DEVOPS handoff |
| **DD5** | D5 testMatch un-MARK scope | Graduate exactly `slice-01-*` + `slice-03-*`; correct the roadmap comment truthfully; preserve the digest SSOT byte-for-byte; `test.fixme` the 3 slice-01 perf blocks + 3 out-of-story slice-03 blocks (FM2/FM5/FM6) with disclosed reasons. | ADR-0075 §D5 |
| **DD6** | D6 empty-vs-paint semantics | Reconciled by placement: `<EChart>` mounts only on success, so the empty state has no `data-prism-chart-painted` attribute at all. Assert empty by its visible message text. No second marker. | ADR-0075 §D6 |

The single new ADR is **ADR-0075** (`adr-0075-prism-echarts-paint-verification.md`).
One ADR is warranted: the paint-signal contract + the swallow policy are
architecturally significant (they are the falsifiable observable the whole
feature turns on and a correction to a verified honesty defect), exactly the
DISCUSS guidance ("produce an ADR if the paint-signal contract or the
swallow policy warrants one").

## MANDATORY Reuse Analysis (existing-system-first; CREATE-NEW must be justified)

This is a brownfield feature. The dominant posture is REUSE/EXTEND;
**nothing is created new.**

| Artefact | Verdict | Evidence + what changes |
|---|---|---|
| `apps/prism/e2e/slice-01-walking-skeleton.spec.ts` | **REUSE/EXTEND** | Exists with detailed Given/When/Then pseudocode and `UNIMPLEMENTED` throws. DELIVER un-throws the walking-skeleton + chrome + URL-roundtrip blocks (drops the embedded `<1000ms` line), `test.fixme`s the 3 perf blocks. No new file. |
| `apps/prism/e2e/slice-03-error-and-empty-states.spec.ts` | **REUSE/EXTEND** | Exists with the zero-uncaught-error invariant scaffolded (`:33-49`). DELIVER un-throws FM1/FM3/FM4 + the cumulative sequence; `test.fixme`s FM2/FM5/FM6. No new file. |
| `apps/prism/e2e/slice-02,04,05,06-*.spec.ts` (4 files) | **REUSE/PRESERVE (untouched)** | Out of scope (DISCUSS Scope boundary). Stay UNIMPLEMENTED and OUT of `testMatch`; their scaffold marks stay. No change. |
| `apps/prism/src/lib/echarts/EChart.tsx` | **EXTEND** | Add `data-prism-chart-painted` + the `finished` subscription + the reset-before-`setOption`; narrow the catch to catch-and-surface. The canvas-probe skip and the wrapper shape (ADR-0030) are preserved. Crafter-only (C9). |
| `apps/prism/playwright.config.ts` | **EXTEND (un-MARK)** | Change `testMatch` to the two graduated specs; correct the roadmap comment. `PROMETHEUS_IMAGE_DIGEST` and the projects preserved. |
| `data-prism-chart-painted` attribute | **REUSE the convention** | Not a new pattern: it is parallel to the existing `data-tick-count` doc-hidden attribute (`EChart.tsx:104`); the specs already name the selector (`slice-01:64`). |
| `[data-testid="empty-state"]`, `[data-testid="chart-canvas"]`, `[data-testid="chart-fallback-table"]`, the parse/transport banners | **REUSE (assert existing behaviour)** | Already rendered by `QueryPanel.tsx`. US-PE-02/03 assert the existing honest behaviour; they do not redesign the UI (C1). |
| `e2e/global-setup.ts` + the Prometheus fixture | **REUSE** | The docker fixture and digest SSOT are kept exactly. |
| ECharts `finished` event, `getOption()`, `getImageData` | **REUSE platform APIs** | Standard ECharts + Canvas-2D APIs; no new dependency. |

**CREATE-NEW count: 0** (production/spec artefacts). The only genuinely new
files are DESIGN documents (this file, ADR-0075, upstream-changes.md) and,
downstream, the spec **bodies** (written into the existing files by the
crafter). No new component, container, port, adapter, dependency, or
external system. CREATE-NEW justification is therefore not required.

## Constraints honoured (DISCUSS C1-C10)

- **C1** operator-invisible: a doc-hidden attribute + `finished` sub + a
  narrowed catch; no visible-chart, fidelity-flag, palette, or banner change.
- **C2** narrow jsdom skip + Vitest green: the `instance===null` early-return
  is preserved; jsdom never reaches `setOption` or the `finished` sub.
- **C3** genuine paint, not hollow: the D1 signal ∧ D2 canvas-ink conjunction.
- **C4** falsifiable against today: the paint test reds against HEAD.
- **C5** SSOT + roadmap preserved: digest byte-for-byte; roadmap comment
  corrected not deleted; perf/out-of-story blocks fixme'd not roped in.
- **C6** no "CI-verified" before D4 green: honest interim claim locked.
- **C7** headless Chromium only.
- **C8** pure trunk-based: CI job feedback-first; gate-vs-continue DEVOPS's call.
- **C9** only the crafter writes `apps/prism/src` and the spec bodies.
- **C10** Gate 10 (StrykerJS): paint-signal branch + narrowed-swallow branch
  pinned if component logic is in the changed set.

## Back-propagation to DISCUSS

One disambiguation is recorded in `design/upstream-changes.md`: the slice-01
walking-skeleton main test block carries an embedded `< 1000 ms` wall-clock
assertion in its pseudocode (`slice-01:65-66`); DISCUSS enumerated the two
p95 blocks + the operator-time guardrail as the perf family but did not name
this fourth embedded timing line. DESIGN clarifies it belongs to the same
OUT-OF-SCOPE latency family and is dropped from the in-scope paint body.
This does not contradict DISCUSS (which already excludes the latency KPIs);
it disambiguates within the stated scope boundary. No requirement changed.

## Self-review (no nested reviewer available in this run)

Reviewer dispatch: not nested-invocable in this autonomous run. SELF-REVIEW
against the nw-sa-critique-dimensions performed; verdict recorded below.

| Dimension | Check | Verdict |
|---|---|---|
| Reuse Analysis present + CREATE-NEW justified | table above; CREATE-NEW = 0; brownfield extend-only | **PASS** |
| C4 diagram present | sequence diagram in the brief (query→render→finished→signal→assertion); L1/L2 explicitly unchanged with rationale | **PASS** |
| ADR alternatives (min 2) | ADR-0075 has 5 (A DOM-exists, B window-instance, C bare-removal, D `rendered` event, E full-e2e), each with rejection rationale | **PASS** |
| Paint assertion genuinely falsifiable (not hollow) | D1 signal gated on `finished` + non-empty series, reset across queries; D2 canvas pixel non-uniformity; explicitly reds against HEAD; the hollow `role=figure` check is named and rejected (alt A) | **PASS** |
| Swallow-narrowing is testable | catch-and-surface reds two independent ways (signal stuck false → wait timeout; console.error → zero-error invariant); jsdom path proven unaffected | **PASS** |
| Honest CI-limit stated | C6 reproduced verbatim in ADR + brief DEVOPS handoff; no "CI-verified" before D4 green; interim claim fixed | **PASS** |
| No overstated claims | the claim equals the placement (local-now, CI-pending); no resume-driven tech (no new dep, no new framework); priority validated (the wrong-problem trap — testing latency instead of paint — is avoided: the in-scope test asserts paint, perf is fixme'd) | **PASS** |
| Bias / resume-driven (Dim 1) | no new technology; reuses ECharts, Playwright, Canvas-2D already present; rejects the `window`-instance seam to keep surface minimal | **PASS** |
| Testability (Dim 4) | the entire feature is a testability uplift; the driving entry and the per-AC assertions are specified for DISTILL | **PASS** |
| Priority validation (Dim 5) | Q1 largest gap = the headline chart is unproven (4Q Q3-2, verified); Q2 simpler alt = the hollow DOM check, rejected with rationale; Q3 constraints quantified (perf fixme'd, scope fenced); Q4 data = the verified code-findings | **PASS** |

**Self-review verdict: APPROVED.** 0 critical, 0 high open. The design is
complete, falsifiable, minimal, and honest about its CI limit. Ready for
DISTILL (acceptance-designer) and the D4 handoff to DEVOPS.

## Handoff summary

- **DISTILL** (`nw-acceptance-designer`): un-throw and harden the existing
  slice-01 + slice-03 bodies per the "For Acceptance Designer" section in the
  brief. Every paint AC must RED against HEAD. Do not inherit a DOM-only or
  green-by-vacuum test. Preserve the digest SSOT and the roadmap.
- **DEVOPS** (`nw-platform-architect`): own D4 (the headless-Chromium CI job,
  docker fixture, browser install, gate-vs-continue-on-error). Word the CI
  claim honestly (C6).
- **DELIVER** (`nw-software-crafter`): wire the paint signal (DD1), narrow the
  swallow (DD3), implement the in-scope spec bodies, un-MARK `testMatch`
  (DD5), keep Vitest green (C2) and the chart operator-invisible (C1). Gate 10
  on the changed component logic (C10). NEVER bump to 1.0.0.
</content>
