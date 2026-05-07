# Prism v0 — Test mapping

- **Wave**: DISTILL
- **Author**: `@nw-acceptance-designer` (Scholar, dispatched by Bea)
- **Date**: 2026-05-08
- **Companion**: `wave-decisions.md`, `test-strategy.md`.

This file maps every user story (US-PR-01..07), every acceptance
criterion, and every KPI (1-5) to the Vitest unit / integration test
file path that pins the pure-function part, the Playwright E2E file
path that pins the user-observable part, the test category, and the
mutation-evidence anchor.

---

## 1. Story-to-test mapping

| Story | AC | Slice | Vitest path | Playwright path | Category | Mutation-evidence anchor |
|---|---|---|---|---|---|---|
| US-PR-01 query → chart | AC-1.1 | 01 | `tests/slice-01-walking-skeleton.test.ts` | `e2e/slice-01-walking-skeleton.spec.ts` | structural + behavioural | `lib/promql/queryRange` GET URL composition |
| US-PR-01 | AC-1.2 | 01 | `tests/slice-01-walking-skeleton.test.ts` | `e2e/slice-01-walking-skeleton.spec.ts` | structural + behavioural | `lib/promql/queryRange` success arm |
| US-PR-01 | AC-1.3 | 01 | `tests/invariant-fidelity.test.ts` (KPI 3 anchor) | (visual baseline at Slice 06) | structural | `lib/echarts/buildOption` `series[i].smooth/connectNulls/sampling` flags |
| US-PR-01 | AC-1.4 | 01 | (latency cannot be measured under JSdom; deferred) | `e2e/slice-01-walking-skeleton.spec.ts` (KPI 1: p95 < 2s over 20 runs) | behavioural | (timeout) |
| US-PR-02 time range | AC-2.1 (relative) | 02 | `tests/slice-02-time-range-and-relative-presets.test.ts` | `e2e/slice-02-time-range-and-relative-presets.spec.ts` | structural + behavioural | RangePicker preset list |
| US-PR-02 | AC-2.1 (Custom) | 05 | `tests/slice-05-absolute-time-range-and-permalink.test.ts` | `e2e/slice-05-absolute-time-range-and-permalink.spec.ts` | structural + behavioural | RangePicker Custom mode toggle |
| US-PR-02 | AC-2.2 | 02 | `tests/slice-02-time-range-and-relative-presets.test.ts` | `e2e/slice-02-time-range-and-relative-presets.spec.ts` | structural + behavioural | URL re-write on picker change |
| US-PR-02 | AC-2.3 | 02 | `tests/slice-02-time-range-and-relative-presets.test.ts` | `e2e/slice-04-auto-refresh.spec.ts` (sliding `now` on tick) | behavioural | `to=now` re-resolution per fetch |
| US-PR-02 | AC-2.4 | 05 | `tests/slice-05-absolute-time-range-and-permalink.test.ts` | `e2e/slice-05-absolute-time-range-and-permalink.spec.ts` | structural + behavioural | refresh disabled on absolute |
| US-PR-02 | AC-2.5 | 05 | `tests/slice-05-absolute-time-range-and-permalink.test.ts` | `e2e/slice-05-absolute-time-range-and-permalink.spec.ts` | structural | from-before-to validation |
| US-PR-03 fidelity + calm errors | AC-3.1 | 01 | `tests/invariant-fidelity.test.ts` | (visual baseline at Slice 06) | structural | byte-equality on `buildOption.series[0].data` |
| US-PR-03 | AC-3.2 | 03 | `tests/slice-03-error-and-empty-states.test.ts` | `e2e/slice-03-error-and-empty-states.spec.ts` | structural + behavioural | `QueryOutcome.parse-error` rendering arm |
| US-PR-03 | AC-3.3 | 03 | `tests/slice-03-error-and-empty-states.test.ts` | `e2e/slice-03-error-and-empty-states.spec.ts` | structural + behavioural | `QueryOutcome.transport-error.network` rendering arm |
| US-PR-03 | AC-3.4 | 03 | `tests/slice-03-error-and-empty-states.test.ts` | `e2e/slice-03-error-and-empty-states.spec.ts` | structural + behavioural | `QueryOutcome.empty` rendering arm |
| US-PR-03 | AC-3.5 | 03 | `tests/slice-03-error-and-empty-states.test.ts` | `e2e/slice-03-error-and-empty-states.spec.ts` | structural + behavioural | "drop previous chart on transport error" branch |
| US-PR-04 permalink | AC-4.1 | 01 | `tests/slice-01-walking-skeleton.test.ts` | (covered by Slice 02 cross-tab) | structural | URL re-write on every state change |
| US-PR-04 | AC-4.2 | 01 | `tests/slice-01-walking-skeleton.test.ts` (within-session reload) | `e2e/slice-01-walking-skeleton.spec.ts` | structural + behavioural | full URL → state hydration |
| US-PR-04 | AC-4.3 | 05 | `tests/slice-05-absolute-time-range-and-permalink.test.ts` | `e2e/slice-05-absolute-time-range-and-permalink.spec.ts` | property + behavioural | absolute roundtrip — KPI 4 |
| US-PR-04 | AC-4.4 | 01 | (out-of-scope-by-construction; no test) | (none) | n/a | n/a (no saved-queries surface to test absence of) |
| US-PR-05 auto-refresh | AC-5.1 | 04 | `tests/slice-04-auto-refresh.test.ts` | `e2e/slice-04-auto-refresh.spec.ts` | structural + behavioural | `RefreshInterval` URL param |
| US-PR-05 | AC-5.2 | 04 | `tests/slice-04-auto-refresh.test.ts` | `e2e/slice-04-auto-refresh.spec.ts` | structural + behavioural | reducer `tick-fired` event |
| US-PR-05 | AC-5.3 | 04 | `tests/slice-04-auto-refresh.test.ts` (no re-mount) | `e2e/slice-04-auto-refresh.spec.ts` (DOM node identity) | structural + behavioural | `<EChart>` `useRef`-stable instance |
| US-PR-05 | AC-5.4 | 04 | `tests/slice-04-auto-refresh.test.ts` (`visibility-changed` event) | `e2e/slice-04-auto-refresh.spec.ts` | structural + behavioural | reducer `Hidden` state transition |
| US-PR-05 | AC-5.5 | 04 | `tests/invariant-fidelity.test.ts` (also covers tick) | (visual baseline at Slice 06) | structural | (subsumed by KPI 3 anchor) |
| US-PR-06 chrome | AC-6.1 | 01 | `tests/slice-01-walking-skeleton.test.ts` (config load) | `e2e/slice-01-walking-skeleton.spec.ts` | structural + behavioural | `lib/config/loader` happy path |
| US-PR-06 | AC-6.2 | 03 | `tests/slice-03-error-and-empty-states.test.ts` (config error) | `e2e/slice-03-error-and-empty-states.spec.ts` | structural + behavioural | composition root refuses to render `<App>` |
| US-PR-06 | AC-6.3 | 01 | `tests/slice-01-walking-skeleton.test.ts` (chrome visible across error states) | `e2e/slice-03-error-and-empty-states.spec.ts` | structural | chrome rendered outside QueryPanel error branch |
| US-PR-07 accessibility | AC-7.1 | 06 | (no Vitest unit; behavioural-only) | `e2e/slice-06-accessibility.spec.ts` (keyboard tab order) | behavioural | (axe-core focus-order) |
| US-PR-07 | AC-7.2 | 06 | `tests/slice-01-walking-skeleton.test.ts` (option `aria.enabled`); `e2e/slice-06-accessibility.spec.ts` (SR-only `<table>` present) | `e2e/slice-06-accessibility.spec.ts` | structural + behavioural | `buildOption.aria.enabled` flag |
| US-PR-07 | AC-7.3 | 06 | (palette swap is a runtime CSS property; no Vitest) | `e2e/slice-06-accessibility.spec.ts` (palette dropdown swap) | behavioural | `lib/echarts/palette` swap |
| US-PR-07 | AC-7.4 | 06 | (contrast measured via axe-core, not Vitest) | `e2e/slice-06-accessibility.spec.ts` | behavioural | (axe-core `color-contrast`) |
| US-PR-07 | AC-7.5 | 06 | `tests/slice-01-walking-skeleton.test.ts` (`buildOption.animation` flag under reducedMotion); `e2e/slice-06-accessibility.spec.ts` (CSS @media) | `e2e/slice-06-accessibility.spec.ts` | structural + behavioural | `buildOption.animation === !reducedMotion` |
| US-PR-07 | AC-7.6 | 06 | (no Vitest unit) | `e2e/slice-06-accessibility.spec.ts` (keyboard-only journey) | behavioural | (axe-core + keyboard event sequence) |

---

## 2. KPI-to-test mapping

| KPI | Vitest path | Playwright path | Three-layer enforcement |
|---|---|---|---|
| KPI 1 — first-chart latency p95 < 2s | (latency unmeasurable in JSdom) | `e2e/slice-01-walking-skeleton.spec.ts` (20-run loop, p95 assert) | Subtype: n/a; Structural: Gate 7 fixture; Behavioural: browser-emitted gauge |
| KPI 2 — iterate latency p95 < 800ms | (latency unmeasurable in JSdom) | `e2e/slice-01-walking-skeleton.spec.ts` (20-iterate loop, p95 assert) | Subtype: n/a; Structural: Gate 7 fixture; Behavioural: browser-emitted gauge |
| KPI 3 — data fidelity 100% | `tests/invariant-fidelity.test.ts` (byte-equality on five-point NaN-bearing fixture); also referenced by `slice-01-walking-skeleton.test.ts` and `slice-04-auto-refresh.test.ts` | (Slice 06 visual-regression baseline) | Subtype: `EChartsOption` types; Structural: byte-equality + Stryker mutation kill rate; Behavioural: visual baseline |
| KPI 4 — URL roundtrip 100% | `tests/slice-01-walking-skeleton.test.ts` (codec property: `decode(encode(state)) === state`) | `e2e/slice-02-time-range-and-relative-presets.spec.ts` (relative cross-tab); `e2e/slice-05-absolute-time-range-and-permalink.spec.ts` (absolute cross-tab) | Subtype: `UrlState` discriminated union; Structural: codec property test; Behavioural: cross-tab byte-equality |
| KPI 5 — page-stays-usable 100% | `tests/slice-03-error-and-empty-states.test.ts` (rendering-arm assertions) | `e2e/slice-03-error-and-empty-states.spec.ts` (four failure modes); `e2e/slice-06-accessibility.spec.ts` (malformed URL + keyboard recovery) | Subtype: `QueryOutcome.kind` exhaustive switch; Structural: Gate 7 four-failure-mode suite + console-error assertion; Behavioural: cross-engine matrix |

---

## 3. Cross-cutting invariant tests

| File | Purpose | Category |
|---|---|---|
| `tests/invariant-public-api.test.ts` | compile-time smoke: imports public types from every `lib/` module; locks the surface against accidental rename / deletion (Codex pattern) | structural (subtype-tier) |
| `tests/invariant-licence-headers.test.ts` | runtime test scanning every `.ts`/`.tsx` under `apps/prism/src/` and tests; asserts AGPL header present (ADR-0032) | structural |
| `tests/invariant-fidelity.test.ts` | KPI 3 byte-equality test against `buildOption` with five-point NaN-bearing fixture | structural |

---

## 4. Test category summary

| Category | Count | Examples |
|---|---|---|
| Structural | most | byte-equality, codec property, exhaustive switch coverage, mutation-evidence anchors |
| Behavioural | several | Playwright keyboard journey, KPI 1 latency, cross-tab roundtrip, axe-core scan |
| Property | 3 | `decode(encode(state)) === state`; "every Run press is a fresh fetch"; (Slice 04) "no schedule-timer effect without prior cancel-timer" |

---

## 5. Slice-to-Story-to-KPI traceability

Per `docs/feature/prism-v0/discuss/user-stories.md > Story-to-slice
traceability` and `outcome-kpis.md > Story coverage` rows:

```
Slice 01 ── US-PR-01 (full)
         ├─ US-PR-02 (default 15-min only)
         ├─ US-PR-03 (fidelity)
         ├─ US-PR-04 (within-session reload)
         └─ US-PR-06 (chrome)
        ── KPI 1, KPI 2 (latency)
        ── KPI 3 (fidelity)
        ── KPI 4 (within-session)

Slice 02 ── US-PR-02 (relative presets)
         └─ US-PR-04 (relative URL roundtrip)
        ── KPI 4 (relative cross-tab)

Slice 03 ── US-PR-03 (errors + empty)
         ├─ US-PR-06 (config error)
         └─ US-PR-04 (URL preserved across errors)
        ── KPI 5 (four failure modes)

Slice 04 ── US-PR-05 (auto-refresh)
        ── KPI 3 (fidelity across ticks)

Slice 05 ── US-PR-02 (absolute Custom)
         └─ US-PR-04 (postmortem-time reproduction)
        ── KPI 4 (absolute cross-tab)

Slice 06 ── US-PR-07 (accessibility)
        ── KPI 5 (keyboard recoverability)
```

Every story has at least one slice; every KPI has at least one slice;
every slice ships at least one story-AC and at least one KPI
contribution.

Coverage check: stories US-PR-01 through US-PR-07 all map to scenarios.
KPIs 1 through 5 all map to scenarios. Total ACs: 30. ACs with no
matching test: AC-4.4 only (out-of-scope-by-construction; the absence
of a saved-queries surface is asserted at the absence-of-grep level by
the architecture, not by a test). Coverage: 29/30 = 0.97 above the
0.95 threshold.

---

## 6. Stryker mutation-evidence anchors per file

| File | Anchored mutation surface |
|---|---|
| `slice-01-walking-skeleton.test.ts` | `lib/promql/queryRange` URL composition + success classification; `lib/url-state/codec` empty/default cases; `lib/config/loader` happy path |
| `slice-02-time-range-and-relative-presets.test.ts` | `lib/url-state/codec` relative offset enum decoding + encoding |
| `slice-03-error-and-empty-states.test.ts` | `lib/promql/queryRange` 400-special case; QueryPanel rendering-arm dispatch over `QueryOutcome.kind` |
| `slice-04-auto-refresh.test.ts` | `lib/auto-refresh/reduce` every transition × every event; backoff-curve table; AbortController integration |
| `slice-05-absolute-time-range-and-permalink.test.ts` | `lib/url-state/codec` absolute timestamps; from-before-to validation; refresh-disabled invariant |
| `invariant-fidelity.test.ts` | `lib/echarts/buildOption.series[i]` flags (`smooth`, `connectNulls`, `sampling`); NaN preservation |
| `invariant-public-api.test.ts` | (no behaviour assertions; locks public surface against rename) |
| `invariant-licence-headers.test.ts` | (no implementation mutants; locks AGPL header presence) |

Stryker's HTML report at `apps/prism/reports/mutation/` per Gate 10.

---

## 7. Coverage gap audit

| AC | Covered? | Notes |
|---|---|---|
| AC-4.4 (no saved-queries / shared-dashboard surface) | structurally absent; not tested | The absence is asserted at the architectural level; testing the absence of a surface is anti-pattern |

All other 29 ACs are covered. Coverage 29/30 = 0.97 > 0.95 threshold.
