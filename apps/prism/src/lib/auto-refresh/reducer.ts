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

// ADR-0029 §2 — Pure reducer for the auto-refresh state machine.
// Slice 04 implements the body; idle/running/backoff/hidden
// transitions and the 5/10/20/30s capped backoff curve.

import type { AutoRefreshState, AutoRefreshEvent, AutoRefreshEffect } from './events';

export interface ReduceResult {
  readonly next: AutoRefreshState;
  readonly effects: ReadonlyArray<AutoRefreshEffect>;
}

export function reduce(_state: AutoRefreshState, _event: AutoRefreshEvent): ReduceResult {
  throw new Error('UNIMPLEMENTED — Slice 04 DELIVER (auto-refresh reducer)');
}
