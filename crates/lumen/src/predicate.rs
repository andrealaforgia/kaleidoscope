// Kaleidoscope Lumen — query predicate
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

//! Query predicate. v0 supports service-name match + severity
//! floor. Body / attribute-path predicates land at v1 alongside
//! the columnar substrate.

use crate::record::{LogRecord, SeverityNumber};

/// Composable predicate. Empty predicate accepts every record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Predicate {
    service: Option<String>,
    min_severity: Option<SeverityNumber>,
    /// ADR-0055 (log-body-text-search-v0). The `body_contains` filter
    /// narrows the response to records whose `body` field contains
    /// the supplied substring, byte-wise, case-sensitive (via
    /// `str::contains` / `String::contains`).
    body_contains: Option<String>,
}

impl Predicate {
    /// Empty predicate. Equivalent to `query` without a predicate
    /// — accepts every record.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter to records whose resource attribute `service.name`
    /// equals `name`. Records without a `service.name` resource
    /// attribute never match.
    pub fn service(mut self, name: impl Into<String>) -> Self {
        self.service = Some(name.into());
        self
    }

    /// Filter to records whose `severity_number >= sev`.
    pub fn min_severity(mut self, sev: SeverityNumber) -> Self {
        self.min_severity = Some(sev);
        self
    }

    /// Filter to records whose `body` field contains the supplied
    /// substring (byte-wise, case-sensitive via `String::contains`).
    /// ADR-0055 (log-body-text-search-v0).
    pub fn body_contains(mut self, s: impl Into<String>) -> Self {
        self.body_contains = Some(s.into());
        self
    }

    /// True if every set filter passes for this record.
    /// Composition is conjunctive (`AND`).
    pub fn matches(&self, record: &LogRecord) -> bool {
        if let Some(target) = self.service.as_deref() {
            match record.resource_attributes.get("service.name") {
                Some(name) if name == target => {}
                _ => return false,
            }
        }
        if let Some(floor) = self.min_severity {
            if record.severity_number < floor {
                return false;
            }
        }
        if let Some(target) = self.body_contains.as_deref() {
            if !record.body.contains(target) {
                return false;
            }
        }
        true
    }

    /// True if this predicate has no filters set (every record
    /// matches).
    pub fn is_empty(&self) -> bool {
        self.service.is_none() && self.min_severity.is_none() && self.body_contains.is_none()
    }
}
