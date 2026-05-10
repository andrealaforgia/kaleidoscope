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

// ADR-0030 — Pure buildOption: takes a QueryOutcome plus a
// BuildOptionContext (palette, range) and returns an EChartsOption.
// No React, no DOM, no I/O. KPI 3 fidelity invariants enforced
// here: smooth=false, connectNulls=false, no auto-downsampling.

import type { QueryOutcome } from '../promql/types';
import type { TimeRange } from '../url-state/types';

export type Palette = 'okabe-ito' | 'tableau10';

export interface BuildOptionContext {
  readonly palette: Palette;
  readonly range: TimeRange;
  readonly prefersReducedMotion: boolean;
}

/**
 * EChartsOption is a structural type defined by ECharts. We do NOT
 * import the upstream type at the module boundary so this file
 * stays pure (no DOM-touching transitive imports). The crafter
 * narrows the return type when implementing the body.
 */
export type EChartsOption = Record<string, unknown>;

export function buildOption(_outcome: QueryOutcome, _ctx: BuildOptionContext): EChartsOption {
  throw new Error('UNIMPLEMENTED — Slice 01 DELIVER (buildOption body)');
}
