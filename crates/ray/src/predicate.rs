// Kaleidoscope Ray — query predicate
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

//! Query predicate. v0 supports span_name + kind + status
//! filters. Span-event / link predicates and attribute-path
//! match land at v1 alongside TraceQL.

use crate::span::{Span, SpanKind, StatusCode};

/// Composable predicate. Empty predicate accepts every span.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Predicate {
    span_name: Option<String>,
    kind: Option<SpanKind>,
    status: Option<StatusCode>,
}

impl Predicate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn span_name(mut self, name: impl Into<String>) -> Self {
        self.span_name = Some(name.into());
        self
    }

    pub fn kind(mut self, kind: SpanKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn status(mut self, code: StatusCode) -> Self {
        self.status = Some(code);
        self
    }

    /// True if every set filter passes for this span.
    /// Composition is conjunctive.
    pub fn matches(&self, span: &Span) -> bool {
        if let Some(target) = self.span_name.as_deref() {
            if span.name != target {
                return false;
            }
        }
        if let Some(k) = self.kind {
            if span.kind != k {
                return false;
            }
        }
        if let Some(s) = self.status {
            if span.status.code != s {
                return false;
            }
        }
        true
    }

    pub fn is_empty(&self) -> bool {
        self.span_name.is_none() && self.kind.is_none() && self.status.is_none()
    }
}
