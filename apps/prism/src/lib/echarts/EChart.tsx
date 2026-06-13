// Kaleidoscope Prism — operator-facing observability SPA
// Copyright (C) 2026 The Kaleidoscope authors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with this program. If not, see <https://www.gnu.org/licenses/>.

// ADR-0030 §3 — Imperative ECharts wrapper. Holds the chart
// instance via useRef across renders; calls setOption({notMerge: true})
// on data change without re-mounting. Direct ECharts modular import;
// no echarts-for-react wrapper.

import { useEffect, useRef, type JSX } from 'react';
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

import type { EChartsOption } from './buildOption';
import { seriesHasInk } from './paintSignal';

echarts.use([
  LineChart,
  GridComponent,
  TooltipComponent,
  LegendComponent,
  AriaComponent,
  TitleComponent,
  CanvasRenderer,
]);

export interface EChartProps {
  readonly option: EChartsOption;
  readonly className?: string;
  /** Doc-hidden test attribute: increments on every option update. */
  readonly tickCount?: number;
}

/**
 * Mount an ECharts canvas. The instance is held in a ref across
 * renders; setOption({notMerge: true}) drives updates without
 * re-mounting (KPI 2 latency budget; AC-5.3 no-flicker invariant).
 */
export function EChart({ option, className, tickCount }: EChartProps): JSX.Element {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const instanceRef = useRef<echarts.ECharts | null>(null);

  // Mount: initialise once on first render; tear down on unmount.
  // In jsdom (Vitest integration tests) HTMLCanvasElement.getContext
  // returns null and ECharts' init + paint chain crashes. We probe
  // for a working canvas-2D context before init; if absent we skip
  // the entire ECharts lifecycle. Real-browser visual assertions
  // are Playwright (Gate 7); jsdom tests assert component graph
  // mount + URL state + banner rendering only.
  useEffect(() => {
    if (containerRef.current === null) return undefined;
    const container = containerRef.current;
    const probe = document.createElement('canvas').getContext('2d');
    if (probe === null) return undefined;
    const instance = echarts.init(container);
    instanceRef.current = instance;
    // ADR-0075 D1 — paint signal. Flip `data-prism-chart-painted` to
    // "true" ONLY once ECharts' `finished` event reports a genuinely
    // non-empty rendered series (settle-once semantics; `rendered`
    // fires every animation frame and would flap). `finished` also
    // fires immediately when animation is off (prefers-reduced-motion).
    // Browser-only: jsdom returned null from the canvas probe above, so
    // this subscription is never made there and the signal can never
    // reach "true" under jsdom (the narrow skip is preserved).
    /* Stryker disable all: browser-only paint-signal event wiring; unreachable under jsdom (probe === null guard above), covered by the Playwright slice-01/slice-03 specs (Gate 7, ADR-0075 D1). The non-empty-series decision is mutation-tested via seriesHasInk (paint-signal.test.tsx). */
    const onFinished = (): void => {
      if (seriesHasInk(instance.getOption().series)) {
        container.setAttribute('data-prism-chart-painted', 'true');
      }
    };
    instance.on('finished', onFinished);
    /* Stryker restore all */
    const onResize = (): void => {
      instance.resize();
    };
    window.addEventListener('resize', onResize);
    return () => {
      window.removeEventListener('resize', onResize);
      /* Stryker disable next-line all: browser-only cleanup paired with the finished subscription above; see Gate 7 / ADR-0075 D1. */
      instance.off('finished', onFinished);
      instance.dispose();
      instanceRef.current = null;
    };
  }, []);

  // Update: setOption with notMerge: true on every option change.
  // No re-mount; the canvas DOM node identity is stable.
  useEffect(() => {
    const instance = instanceRef.current;
    if (instance === null) return;
    const container = containerRef.current;
    // ADR-0075 D1 — reset the paint signal to "false" BEFORE applying a
    // new option, so a stale "true" from the prior query's render is
    // never observed across queries; the next `finished` re-flips it.
    /* Stryker disable next-line all: browser-only signal reset; unreachable under jsdom (instance === null guard above), covered by the Playwright slice-01/slice-03 specs (Gate 7, ADR-0075 D1). */
    container?.setAttribute('data-prism-chart-painted', 'false');
    try {
      instance.setOption(option, { notMerge: true });
    } catch (err) {
      // ADR-0075 D3 — catch-and-surface, NOT catch-and-swallow. On a
      // real-browser paint failure: leave the signal "false" (so the
      // walking-skeleton wait reds) AND emit a console.error (so the
      // slice-03 zero-uncaught-error invariant reds). Do NOT re-throw:
      // a throw inside this effect would unmount the subtree and blank
      // the page, violating US-PE-03 "the page stays interactive".
      // Browser-only: jsdom never reaches setOption (instance === null
      // above), so the Vitest suite stays green (ADR-0075 C2).
      /* Stryker disable next-line all: browser-only catch-and-surface; unreachable under jsdom (instance === null guard above); no in-scope spec forces a setOption throw, so this defensive branch is covered by inspection per ADR-0075 D3, not by an executing test. */
      console.error('[prism] ECharts setOption failed', err);
    }
  }, [option]);

  return (
    <div
      ref={containerRef}
      className={className}
      data-tick-count={tickCount ?? 0}
      // ADR-0075 D1 — paint signal, literal "false" on mount/initial
      // render; flipped imperatively to "true" only on a genuine
      // non-empty paint (see the `finished` subscription above).
      data-prism-chart-painted="false"
      role="figure"
      aria-label="Chart"
      style={{ width: '100%', height: '100%', minHeight: 240 }}
    />
  );
}
