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

// ADR-0027 §1 — Total-function queryRange. Never throws; every
// failure mode is a QueryOutcome arm. Slice 01 implements the body.

import type { QueryOutcome, QueryRangeRequest, QueryRangeContext } from './types';

export async function queryRange(
  _request: QueryRangeRequest,
  _ctx: QueryRangeContext,
): Promise<QueryOutcome> {
  throw new Error('UNIMPLEMENTED — Slice 01 DELIVER (queryRange body)');
}
