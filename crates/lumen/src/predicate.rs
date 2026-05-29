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

use regex::Regex;

use crate::record::{LogRecord, SeverityNumber};

/// Composable predicate. Empty predicate accepts every record.
///
/// `PartialEq` and `Eq` are intentionally NOT derived: the
/// `body_regex` field carries a compiled `regex::Regex` (ADR-0056
/// Decision 4 / DD2) which does not implement either trait. The
/// predicate is compared by behaviour (running `matches` against a
/// fixture), never by structural equality; a workspace grep
/// confirms no production caller relies on predicate equality.
#[derive(Debug, Clone, Default)]
pub struct Predicate {
    service: Option<String>,
    min_severity: Option<SeverityNumber>,
    /// ADR-0055 (log-body-text-search-v0). The `body_contains` filter
    /// narrows the response to records whose `body` field contains
    /// the supplied substring, byte-wise, case-sensitive (via
    /// `str::contains` / `String::contains`).
    body_contains: Option<String>,
    /// ADR-0056 (log-body-regex-search-v0). The `body_regex` filter
    /// narrows the response to records whose `body` field is matched
    /// by the supplied compiled regular expression. The handler
    /// compiles the regex ONCE per request in `parse_body_regex`
    /// (fail-fast 400 on invalid syntax) and hands the compiled
    /// `Regex` to the predicate via the [`Predicate::body_regex`]
    /// builder. The per-record match call is `re.is_match(&body)`.
    body_regex: Option<Regex>,
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

    /// Filter to records whose `body` field is matched by the
    /// supplied compiled regular expression. The match is via
    /// `Regex::is_match(&body)` — unanchored (matches anywhere in
    /// the body), byte-wise case-sensitive by default, multiline
    /// off; operators opt into anchoring, case-folding, and
    /// multiline mode via the standard inline flags (`^`, `$`,
    /// `(?i)`, `(?m)`). ADR-0056 (log-body-regex-search-v0).
    pub fn body_regex(mut self, re: Regex) -> Self {
        self.body_regex = Some(re);
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
        // ADR-0056 (log-body-regex-search-v0) Decision 4 / Decision 10.
        // Conjunctive arm placed AFTER body_contains; AND composition
        // is commutative so arm order is for readability, not
        // correctness. The compiled `Regex` is handed in via the
        // [`Predicate::body_regex`] builder; `is_match` is unanchored
        // and byte-wise case-sensitive by default (operators opt into
        // anchoring, case-folding, multiline via the inline `(?i)`,
        // `(?m)`, `^`, `$` flags).
        if let Some(re) = self.body_regex.as_ref() {
            if !re.is_match(&record.body) {
                return false;
            }
        }
        true
    }

    /// True if this predicate has no filters set (every record
    /// matches).
    pub fn is_empty(&self) -> bool {
        self.service.is_none()
            && self.min_severity.is_none()
            && self.body_contains.is_none()
            && self.body_regex.is_none()
    }
}
