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

// ADR-0028 / Slice 02 + 05 — Relative + absolute time-range picker.
//
// The operator-canonical five presets per the JTBD analysis (Last
// 5 min through Last 24 h) plus a Custom mode (slice 05) that
// reveals two ISO-8601 inputs for an absolute window.

import { useState, type ChangeEvent, type JSX } from 'react';
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

function toIsoInput(d: Date): string {
  // datetime-local input format: YYYY-MM-DDTHH:MM (no seconds, no Z).
  // We round-trip through the Date object so user edits land as UTC.
  const pad = (n: number): string => n.toString().padStart(2, '0');
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}T${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}`;
}

function fromIsoInput(value: string): Date | null {
  // Parse YYYY-MM-DDTHH:MM as UTC.
  if (value.length === 0) return null;
  const d = new Date(`${value}:00Z`);
  if (Number.isNaN(d.getTime())) return null;
  return d;
}

export function TimeRangePicker({ range, onChange }: TimeRangePickerProps): JSX.Element {
  const selected = range.kind === 'absolute' ? CUSTOM_VALUE : range.from;

  // Local draft state for the Custom inputs. Committed to the parent
  // only when both inputs parse and from <= to. The picker stays in
  // Custom mode regardless of draft validity so the operator can
  // keep editing.
  const initialFrom =
    range.kind === 'absolute' ? toIsoInput(range.from) : toIsoInput(new Date(Date.now() - 900_000));
  const initialTo = range.kind === 'absolute' ? toIsoInput(range.to) : toIsoInput(new Date());
  const [fromDraft, setFromDraft] = useState(initialFrom);
  const [toDraft, setToDraft] = useState(initialTo);
  const [error, setError] = useState<string | null>(null);

  function onSelect(event: ChangeEvent<HTMLSelectElement>): void {
    const value = event.target.value;
    if (value === CUSTOM_VALUE) {
      // Switch into Custom mode using the current draft (or sensible
      // defaults). The parent only sees the change if the draft is
      // valid; otherwise the picker stays "Custom" with the
      // inline error visible.
      const from = fromIsoInput(fromDraft);
      const to = fromIsoInput(toDraft);
      if (from !== null && to !== null && from.getTime() <= to.getTime()) {
        onChange({ kind: 'absolute', from, to });
      }
      return;
    }
    setError(null);
    onChange({ kind: 'relative', from: value as RelativeOffset });
  }

  function commitDraft(nextFrom: string, nextTo: string): void {
    const from = fromIsoInput(nextFrom);
    const to = fromIsoInput(nextTo);
    if (from === null) {
      setError('Enter an ISO-8601 timestamp like 2026-05-07T03:00.');
      return;
    }
    if (to === null) {
      setError('Enter an ISO-8601 timestamp like 2026-05-07T03:15.');
      return;
    }
    if (from.getTime() > to.getTime()) {
      setError('Time range start must be before end.');
      return;
    }
    setError(null);
    onChange({ kind: 'absolute', from, to });
  }

  function onFromChange(e: ChangeEvent<HTMLInputElement>): void {
    const value = e.target.value;
    setFromDraft(value);
    commitDraft(value, toDraft);
  }

  function onToChange(e: ChangeEvent<HTMLInputElement>): void {
    const value = e.target.value;
    setToDraft(value);
    commitDraft(fromDraft, value);
  }

  const showCustomInputs = range.kind === 'absolute';

  return (
    <div className="prism-time-range">
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
          <option value={CUSTOM_VALUE}>Custom</option>
        </select>
      </label>

      {showCustomInputs && (
        <div className="prism-time-range-custom" data-testid="time-range-custom">
          <label className="prism-time-range-input-label">
            <span>From</span>
            <input
              type="datetime-local"
              value={fromDraft}
              onChange={onFromChange}
              aria-label="From timestamp"
              data-testid="time-range-from"
            />
          </label>
          <label className="prism-time-range-input-label">
            <span>To</span>
            <input
              type="datetime-local"
              value={toDraft}
              onChange={onToChange}
              aria-label="To timestamp"
              data-testid="time-range-to"
            />
          </label>
          {error !== null && (
            <span className="prism-time-range-error" role="alert" data-testid="time-range-error">
              {error}
            </span>
          )}
        </div>
      )}
    </div>
  );
}
