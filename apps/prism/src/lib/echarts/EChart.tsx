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
// instance via useRef, calls setOption({notMerge: true}) on data
// change without re-mounting. Slice 01 implements the body.

import type { EChartsOption } from './buildOption';

export interface EChartProps {
  readonly option: EChartsOption;
  readonly className?: string;
}

export function EChart(_props: EChartProps): JSX.Element {
  throw new Error('UNIMPLEMENTED — Slice 01 DELIVER (EChart wrapper)');
}
