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
    const probe = document.createElement('canvas').getContext('2d');
    if (probe === null) return undefined;
    const instance = echarts.init(containerRef.current);
    instanceRef.current = instance;
    const onResize = (): void => {
      instance.resize();
    };
    window.addEventListener('resize', onResize);
    return () => {
      window.removeEventListener('resize', onResize);
      instance.dispose();
      instanceRef.current = null;
    };
  }, []);

  // Update: setOption with notMerge: true on every option change.
  // No re-mount; the canvas DOM node identity is stable.
  useEffect(() => {
    const instance = instanceRef.current;
    if (instance === null) return;
    try {
      instance.setOption(option, { notMerge: true });
    } catch {
      // jsdom: canvas paint unavailable; component tests still
      // assert structural behaviour (panel mount, banner render,
      // URL update). Real-browser visual assertions are Playwright.
    }
  }, [option]);

  return (
    <div
      ref={containerRef}
      className={className}
      data-tick-count={tickCount ?? 0}
      role="figure"
      aria-label="Chart"
      style={{ width: '100%', height: '100%', minHeight: 240 }}
    />
  );
}
