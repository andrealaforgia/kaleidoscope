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

// ADR-0026 §3 — Public consumer surface for the PromQL adapter.
// Consumers (panels, integration tests) import from `client`; the
// queryRange body lives in queryRange.ts so unit tests can pin it
// directly through queryRange's own export.

export { queryRange } from './queryRange';
export type {
  QueryOutcome,
  TransportCause,
  Series,
  LabelSet,
  QueryRangeRequest,
  QueryRangeContext,
} from './types';
