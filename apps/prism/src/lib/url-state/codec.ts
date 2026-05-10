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

// ADR-0028 §2 — Pure URL codec. encode produces a query-string
// representation; decode parses it into a Result<UrlState,
// UrlParseError>. No I/O, no Date.now(), no React.

import type { UrlState, UrlParseError } from './types';

export type DecodeResult =
  | { readonly kind: 'ok'; readonly value: UrlState }
  | { readonly kind: 'error'; readonly error: UrlParseError };

export function encode(_state: UrlState): string {
  throw new Error('UNIMPLEMENTED — Slice 02 DELIVER (codec.encode)');
}

export function decode(_search: string | URLSearchParams): DecodeResult {
  throw new Error('UNIMPLEMENTED — Slice 02 DELIVER (codec.decode)');
}
