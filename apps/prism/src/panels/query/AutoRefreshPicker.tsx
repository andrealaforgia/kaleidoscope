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

// ADR-0029 / Slice 06a — Auto-refresh picker.
//
// Closed enum of five intervals: off, 5s, 10s, 30s, 1m. The picker
// is disabled when the active range is absolute (ADR-0029 §6
// double-lock at the UI layer; the codec is the second lock).

import type { ChangeEvent, JSX } from 'react';
import type { RefreshInterval } from '../../lib/url-state/types';

interface Option {
  readonly value: RefreshInterval;
  readonly label: string;
}

const OPTIONS: ReadonlyArray<Option> = [
  { value: 'off', label: 'Off' },
  { value: '5s', label: '5 s' },
  { value: '10s', label: '10 s' },
  { value: '30s', label: '30 s' },
  { value: '1m', label: '1 min' },
];

export interface AutoRefreshPickerProps {
  readonly value: RefreshInterval;
  readonly disabled: boolean;
  readonly onChange: (value: RefreshInterval) => void;
}

export function AutoRefreshPicker({
  value,
  disabled,
  onChange,
}: AutoRefreshPickerProps): JSX.Element {
  function onSelect(event: ChangeEvent<HTMLSelectElement>): void {
    onChange(event.target.value as RefreshInterval);
  }

  return (
    <label className="prism-refresh-picker">
      <span className="prism-refresh-label">Auto-refresh</span>
      <select
        className="prism-refresh-select"
        value={value}
        disabled={disabled}
        onChange={onSelect}
        aria-label="Auto-refresh interval"
        data-testid="refresh-picker"
        title={
          disabled
            ? 'Auto-refresh is disabled for absolute time ranges.'
            : 'Pick how often Prism re-fetches.'
        }
      >
        {OPTIONS.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </label>
  );
}
