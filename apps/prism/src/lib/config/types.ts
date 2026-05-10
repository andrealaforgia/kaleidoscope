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

// ADR-0030 — RuntimeConfig shape consumed by the App composition
// root. Loaded from /config.json at startup; the App refuses to
// mount on ConfigError per the wire-then-probe-then-use posture.

export interface RuntimeConfig {
  readonly backend: {
    readonly url: string;
    readonly label: string;
  };
  readonly prism: {
    readonly version: string;
  };
}

export interface ConfigError {
  readonly kind: 'fetch-failed' | 'parse-failed' | 'shape-failed';
  readonly message: string;
}
