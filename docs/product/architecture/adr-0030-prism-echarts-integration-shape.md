# ADR-0030 — Prism ECharts integration shape

- **Status**: Accepted
- **Date**: 2026-05-07
- **Author**: `nw-solution-architect` (Morgan, dispatched by Bea)
- **Feature**: `prism` v0
- **Supersedes**: none
- **Superseded by**: none
- **Related**: ADR-0026 (`lib/echarts/` module), ADR-0027 (`QueryOutcome`
  feeds the option builder), ADR-0028 (URL state determines axis range)

## Context

Apache ECharts is the chosen charting library (pre-locked). It is the
largest dependency in Prism's bundle (~200 KB gzipped of the 300 KB
budget). Two responsibilities sit on top of it:

1. **A pure option builder**: `QueryOutcome::Success → EChartsOption`.
   This is the function KPI 3 (data-fidelity invariant) tests against:
   given a five-point fixture (1, NaN, 3, NaN, 5), the option's series
   must contain exactly those five points, in order, with no
   smoothing, no interpolation across NaN, no aggregation.
2. **An imperative React wrapper**: a `<EChart>` component that holds
   the ECharts instance via `useRef`, calls `chart.setOption(...)` on
   data change without re-instantiating the chart (which is what
   prevents the flicker Slice 04 § AC-5.3 forbids).

Slice 06 (accessibility) layers two further concerns: a colour-blind-safe
palette swap (US-PR-07 § AC-7.3), and `prefers-reduced-motion` honoured
across animations (§ AC-7.5). Both land at the option-builder boundary
(palette is part of the option; animation is a flag in the option).

The pre-locked decision rules out `echarts-for-react` (a wrapper
library) on bundle and indirection grounds. This ADR locks the
direct-import shape, the option-builder contract, the React lifecycle
binding, the palette mechanism, and the screen-reader fallback.

## Decision

### 1. Direct ECharts import

Slice 01 imports the ECharts modules directly:

```ts
// apps/prism/src/lib/echarts/instance.ts
import * as echarts from 'echarts/core';
import { LineChart } from 'echarts/charts';
import {
  GridComponent,
  TooltipComponent,
  LegendComponent,
  AriaComponent,
  TitleComponent,
} from 'echarts/components';
import { CanvasRenderer } from 'echarts/renderers';

echarts.use([
  LineChart,
  GridComponent,
  TooltipComponent,
  LegendComponent,
  AriaComponent,
  TitleComponent,
  CanvasRenderer,
]);
```

The tree-shaken import surface is the smallest set that satisfies
Slice 01's chart shape (line chart with axis, tooltip, legend) plus
Slice 06's accessibility component (`AriaComponent` for ECharts'
built-in screen-reader summary).

The chosen renderer is `CanvasRenderer`. Slice 06's screen-reader
fallback is provided by `AriaComponent` plus a hand-rolled SR-only
`<table>` (see § 6); `SVGRenderer` is rejected because the rasterisation
behaviour for large datasets (over a few thousand points) degrades
faster on SVG.

### 2. Option-builder contract

```ts
// apps/prism/src/lib/echarts/buildOption.ts
import type { EChartsOption } from 'echarts/types/dist/shared';
import type { PromqlSeries } from '../promql/types';

export function buildOption(
  outcome: QueryOutcomeSuccess,
  context: BuildOptionContext,
): EChartsOption;

export type BuildOptionContext = {
  palette: 'okabe-ito' | 'tableau-10';   // Slice 06 swap
  reducedMotion: boolean;                 // honour prefers-reduced-motion
  range: TimeRange;                       // x-axis bounds
};
```

`buildOption` is a **pure function**. It has no React imports, no
ECharts imports beyond the type definitions, no side effects. It is
testable as a pure function under Vitest with no JSdom. KPI 3's data
fidelity test asserts byte-identical option output for a known
fixture.

The option carries the following invariants set by `buildOption` and
NEVER overridden downstream:

| Field | Locked value | Rationale |
|---|---|---|
| `series[i].smooth` | `false` | KPI 3: no client-side smoothing |
| `series[i].connectNulls` | `false` | KPI 3: no interpolation across gaps |
| `series[i].sampling` | `undefined` | KPI 3: no auto-downsampling |
| `series[i].symbolSize` | `0` (unless reduced-motion is off and a small dataset) | reduces visual noise on dense series |
| `animation` | `!reducedMotion` | Slice 06 § AC-7.5 |
| `animationDuration` | `0 if reducedMotion else 200` | calm at incident time |
| `aria.enabled` | `true` | Slice 06 § AC-7.2 |
| `aria.label.description` | computed from `outcome` (series count + bounds) | screen-reader announcement |

These invariants are part of the option-builder's contract surface;
the QueryPanel cannot reach into the option and override them. Tests
assert the invariants on every code path that produces an option.

### 3. The `<EChart>` React wrapper

```ts
// apps/prism/src/lib/echarts/EChart.tsx (sketch)

export type EChartProps = {
  option: EChartsOption;
  fallback: SeriesSummary;   // Slice 06 SR-only <table>
};

export function EChart({ option, fallback }: EChartProps): JSX.Element {
  // Holds the ECharts instance across renders via useRef.
  // Mounts on first render; calls chart.setOption(option) on prop change;
  // resizes on window resize; disposes on unmount.
}
```

The wrapper:

- Uses `useRef<echarts.ECharts | null>(null)` for the instance.
- Uses `useRef<HTMLDivElement | null>(null)` for the container.
- On mount: `echarts.init(containerRef.current!)`.
- On `option` change: `chartRef.current?.setOption(option, { notMerge: true })`.
  `notMerge: true` is critical: a merge-mode update would conflate stale
  series with fresh series and break the no-cache contract.
- On `ResizeObserver` callback: `chartRef.current?.resize()`.
- On unmount: `chartRef.current?.dispose()`; clear refs.

The SR-only `<table>` (Slice 06 fallback) is rendered as a sibling of
the canvas container, visually hidden via `clip: rect(0 0 0 0)`. It is
read from `fallback` (a typed summary the QueryPanel computes alongside
the option). ECharts' built-in `aria.label` carries the textual chart
summary (highest, lowest, latest); the `<table>` carries the full point
list so a screen-reader user can navigate to a specific point.

### 4. Lifecycle correctness

The flicker invariant (Slice 04 § AC-5.3, US-PR-05) requires that
auto-refresh ticks update the chart without re-mounting. The wrapper's
`useEffect` dependency is `[option]`; React's referential identity
makes this exact: a new option object triggers `setOption`, the same
object does not. The QueryPanel produces a fresh option object on
every successful fetch (via `buildOption`), so every fetch updates
the chart and a re-render with the same outcome (e.g. parent re-render
for an unrelated reason) does not.

The `notMerge: true` flag means stale series data does not linger.
Combined with KPI 3's no-cache invariant in `lib/promql/`, the chart
shows exactly what the most recent successful fetch returned, and
nothing else.

### 5. Palette mechanism

CSS custom properties are the single source of truth for colour:

```css
/* apps/prism/src/styles/theme.module.css */

[data-palette="okabe-ito"] {
  --series-1: #E69F00; --series-2: #56B4E9; --series-3: #009E73;
  --series-4: #F0E442; --series-5: #0072B2; --series-6: #D55E00;
  --series-7: #CC79A7; --series-8: #000000;
}
[data-palette="tableau-10"] {
  --series-1: #4E79A7; --series-2: #F28E2B; --series-3: #E15759;
  --series-4: #76B7B2; --series-5: #59A14F; --series-6: #EDC948;
  --series-7: #B07AA1; --series-8: #FF9DA7; --series-9: #9C755F;
  --series-10: #BAB0AC;
}
```

`buildOption` does NOT bake colour values into the option directly;
it reads them from `getComputedStyle(document.documentElement)` via a
small helper at the time the option is built. The QueryPanel sets
`data-palette="okabe-ito"` on a parent element. The Slice 06 picker
toggles `data-palette` between the two presets; ECharts re-reads the
colours on the next `setOption` call.

The two palettes are colour-blind-safe by design (Okabe-Ito is the
canonical eight-colour palette for deuteranopia / protanopia /
tritanopia; Tableau 10 is an alternative with broader contrast for
non-colour-vision-impaired operators).

### 6. Reduced-motion honouring

`prefers-reduced-motion: reduce` is read via
`window.matchMedia('(prefers-reduced-motion: reduce)').matches`.
The QueryPanel reads it once on mount, listens for changes, and passes
the boolean to `buildOption` via `BuildOptionContext`. `buildOption`
disables ECharts' animation and sets `animationDuration: 0`.

Other animations in the SPA (skeleton fade-in, banner slide-in) are
similarly gated by a CSS rule:

```css
@media (prefers-reduced-motion: reduce) {
  * { animation-duration: 0 !important; transition-duration: 0 !important; }
}
```

This is the WCAG 2.2 SC 2.3.3 (AAA, but trivially included at AA in
this case) and AC-7.5.

### 7. Bundle-size escape hatch

If the bundle approaches the 300 KB gzipped gate (DEVOPS' CI gate)
at any point during DELIVER, the escape hatch is to lazy-import the
ECharts module:

```ts
const { initEcharts, buildOption } =
  await import('./lib/echarts/index.js');
```

The QueryPanel renders a skeleton loader while the chunk loads; the
chart appears on resolve. The lazy boundary is the `lib/echarts/` folder
boundary; nothing outside it changes.

The escape hatch is a DELIVER-time decision contingent on the actual
bundle measurement; the v0 default is direct import (synchronous
ECharts available on first render). Vite's tree-shaking is well-tuned
for ECharts' modular API; the escape hatch is not expected to be
needed at v0.

## Alternatives considered

### Option A (rejected): `echarts-for-react` wrapper

The wrapper provides a `<ReactEChartsCore>` component that handles
mount, update, dispose. Argument for: less code in `lib/echarts/`.
Argument against (and the reason the pre-locked decision rejects
it): the wrapper bundles a slightly older ECharts version (lagging
upstream by weeks-to-months), adds ~3 KB to the bundle, and removes
the lifecycle control needed for the AbortController-driven update
shape. The hand-written wrapper is ~50 lines of TypeScript and gives
full control.

### Option B (rejected): Plotly.js

Plotly is more accessibility-friendly out of the box (built-in screen
reader support, better keyboard navigation). Argument for: less
hand-rolling on Slice 06. Argument against: bundle weight is
~700 KB minified (well over the 300 KB gate even with tree-shaking);
ECharts has the AriaComponent and a textual fallback path that
covers Slice 06's needs.

### Option C (rejected): Recharts

Recharts is the React-idiomatic charting library; it composes via JSX.
Argument for: no imperative wrapper needed; React all the way down.
Argument against: bundle weight is acceptable but rendering is SVG
(slow on dense datasets, and Prometheus result sets are routinely
hundreds-of-points-per-series); animation control is less granular
than ECharts'; the JSX composition style hides the option-builder
contract that KPI 3's tests want to exercise.

### Option D (rejected): D3 from scratch

Argument for: complete control, smallest possible bundle (~30-40 KB
gzipped). Argument against: hand-rolling axis ticks, legend, tooltip,
zoom, accessibility is far more code than the bundle savings justify.
Slice 01 is already cumbering with the first-frontend-feature work;
adding "implement a chart library" is out of scope.

## Consequences

### Positive

- **The option builder is pure and testable**. KPI 3's data-fidelity
  invariant lives as a Vitest test with a five-point NaN-bearing
  fixture. The mutation-test surface is the option fields.
- **The wrapper is thin**. Mount/update/resize/dispose lifecycle in
  ~50 lines of React. Reviewers can read it in one sitting.
- **The flicker invariant is structurally enforced**. `setOption` with
  `notMerge: true` plus the same instance across renders means the
  chart never re-mounts.
- **Palette swap is a runtime CSS property change**. The Slice 06
  picker is a button that toggles `data-palette`; no rebuild, no
  rerun, no re-fetch.
- **Bundle escape hatch is documented**. If the bundle gate trips,
  the lazy-import path is one folder boundary away.

### Negative

- **The imperative wrapper is non-idiomatic React**. `useRef` plus
  `useEffect` plus mutation-via-setOption is exactly the shape React
  documentation cautions against. Mitigation: it is a single component
  in `lib/echarts/`; the rest of the SPA is fully declarative.
- **ECharts' API surface drift is a maintenance hazard**. ECharts'
  type definitions move between minor versions. Pin the exact minor
  (per the project's `=0.27` style for Codex/Spark) and update via a
  dedicated PR with the visual regression baseline as the gate.
- **Two palette presets is intentionally limited**. Operators with
  more specific needs (e.g. tritanopia) are not served at v0. Revisit
  at v0.1 if user testing surfaces a need.

### Trade-off summary

The shape (pure option builder + thin imperative wrapper) is the
smallest division that gives KPI 3 a testable surface and Slice 04 a
flicker-free update path. The alternatives either bloat the bundle
(Plotly), regress on dense datasets (Recharts SVG), or rewrite the
chart from scratch (D3). The chosen shape composes cleanly with every
slice 01-06.

## Verification

- Vitest test: `buildOption(fixture).series[0].data` is byte-identical
  to the fixture's `[ts, value]` tuples (KPI 3 structural enforcement).
- Vitest test: `buildOption` with `reducedMotion: true` produces
  `animation: false` and `animationDuration: 0`.
- Vitest test: `buildOption` is a pure function (calling it twice with
  the same input produces equal output; no global state mutation).
- Playwright E2E (Slice 01): chart renders within 1 s of Run press
  on the local-Prometheus fixture (US-PR-01 § AC-1.4).
- Playwright E2E (Slice 04): chart updates without re-mounting
  (DOM node identity is preserved across ticks).
- Playwright E2E (Slice 06): switching palettes does not trigger a
  re-fetch; ECharts re-renders with new colours.
- Visual-regression baseline (Playwright `toHaveScreenshot`): the
  five-point fixture renders to a known reference image; ECharts
  version bumps require an explicit baseline refresh.
