// Kaleidoscope Lumen — OTLP-shaped log record types
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

//! OTLP-shaped log record types at the trait boundary.
//!
//! The field set mirrors `opentelemetry-proto::logs::v1::LogRecord`
//! exactly. The v1 disk-backed adapter must round-trip every field
//! byte-stable.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Severity number per the OpenTelemetry Logs specification.
/// The numeric value matches the OTLP proto encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SeverityNumber(pub i32);

impl SeverityNumber {
    pub const UNSPECIFIED: SeverityNumber = SeverityNumber(0);
    pub const TRACE: SeverityNumber = SeverityNumber(1);
    pub const DEBUG: SeverityNumber = SeverityNumber(5);
    pub const INFO: SeverityNumber = SeverityNumber(9);
    pub const WARN: SeverityNumber = SeverityNumber(13);
    pub const ERROR: SeverityNumber = SeverityNumber(17);
    pub const FATAL: SeverityNumber = SeverityNumber(21);
}

/// One OTLP log record. Field set mirrors
/// `opentelemetry-proto::logs::v1::LogRecord` exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogRecord {
    /// Nanoseconds since Unix epoch when the event was observed.
    /// Sort key for time-range queries.
    pub observed_time_unix_nano: u64,
    /// Severity per OTel logs spec.
    pub severity_number: SeverityNumber,
    /// Free-form severity label (`"INFO"`, `"ERROR"`, etc.).
    pub severity_text: String,
    /// The log message body.
    pub body: String,
    /// Record-level attributes (e.g. `http.status_code`).
    pub attributes: BTreeMap<String, String>,
    /// Resource attributes (e.g. `service.name`). Carried per
    /// record to keep the v0 adapter simple; v1 will hoist common
    /// resource attributes to the batch level.
    pub resource_attributes: BTreeMap<String, String>,
    /// Optional W3C trace context.
    pub trace_id: Option<[u8; 16]>,
    pub span_id: Option<[u8; 8]>,
}

/// A batch of log records, all belonging to one tenant.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LogBatch {
    pub records: Vec<LogRecord>,
}

impl LogBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_records(records: Vec<LogRecord>) -> Self {
        Self { records }
    }

    pub fn push(&mut self, record: LogRecord) {
        self.records.push(record);
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Half-open time range `[start, end)` in nanoseconds since the
/// Unix epoch. A record matches when
/// `start <= observed_time_unix_nano < end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRange {
    pub start_unix_nano: u64,
    pub end_unix_nano: u64,
}

impl TimeRange {
    pub fn new(start_unix_nano: u64, end_unix_nano: u64) -> Self {
        Self {
            start_unix_nano,
            end_unix_nano,
        }
    }

    /// `[0, u64::MAX)`. Useful for "give me everything you have".
    pub fn all() -> Self {
        Self::new(0, u64::MAX)
    }

    pub fn contains(&self, observed_time_unix_nano: u64) -> bool {
        observed_time_unix_nano >= self.start_unix_nano
            && observed_time_unix_nano < self.end_unix_nano
    }
}
