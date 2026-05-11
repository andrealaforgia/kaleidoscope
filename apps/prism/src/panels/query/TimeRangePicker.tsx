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

// ADR-0028 / Slice 02 — Relative time-range picker.
//
// The operator-canonical five presets per the JTBD analysis (Last
// 5 min through Last 24 h). The "Custom" option is rendered as
// disabled at slice 02; slice 05 enables it to drive the absolute
// timestamp inputs.

import type { ChangeEvent, JSX } from 'react';
import type { RelativeOffset, TimeRange } from '../../lib/url-state/types';

interface Preset {
  readonly value: RelativeOffset;
  readonly label: string;
}

const PRESETS: ReadonlyArray<Preset> = [
  { value: '-5m', label: 'Last 5 min' },
  { value: '-15m', label: 'Last 15 min' },
  { value: '-1h', label: 'Last 1 h' },
  { value: '-6h', label: 'Last 6 h' },
  { value: '-24h', label: 'Last 24 h' },
];

const CUSTOM_VALUE = 'custom';

export interface TimeRangePickerProps {
  readonly range: TimeRange;
  readonly onChange: (range: TimeRange) => void;
}

export function TimeRangePicker({ range, onChange }: TimeRangePickerProps): JSX.Element {
  const selected = range.kind === 'absolute' ? CUSTOM_VALUE : range.from;

  function onSelect(event: ChangeEvent<HTMLSelectElement>): void {
    const value = event.target.value;
    if (value === CUSTOM_VALUE) {
      // Slice 05 enables Custom mode; at slice 02 the option is
      // disabled so the picker UI cannot reach this branch.
      return;
    }
    onChange({ kind: 'relative', from: value as RelativeOffset });
  }

  return (
    <label className="prism-time-range-picker">
      <span className="prism-time-range-label">Range</span>
      <select
        className="prism-time-range-select"
        value={selected}
        onChange={onSelect}
        aria-label="Time range"
        data-testid="time-range-picker"
      >
        {PRESETS.map((p) => (
          <option key={p.value} value={p.value}>
            {p.label}
          </option>
        ))}
        <option value={CUSTOM_VALUE} disabled>
          Custom
        </option>
      </select>
    </label>
  );
}
