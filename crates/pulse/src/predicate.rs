// Kaleidoscope Pulse — query predicate
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

//! Query predicate. v0 supports service-name match + label-eq
//! filters. Regex / glob label match and PromQL operators land
//! at v1.

use std::collections::BTreeMap;

use crate::metric::{Metric, MetricPoint};

/// Composable predicate. Empty predicate accepts every point.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Predicate {
    service: Option<String>,
    label_eq: BTreeMap<String, String>,
}

impl Predicate {
    /// Empty predicate.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter to points whose owning metric's resource attribute
    /// `service.name` equals `name`. Metrics without a
    /// `service.name` resource attribute never match.
    pub fn service(mut self, name: impl Into<String>) -> Self {
        self.service = Some(name.into());
        self
    }

    /// Filter to points whose point-level attribute `key` equals
    /// `value`. Multiple `label_eq` calls compose as
    /// intersection (`AND`).
    pub fn label_eq(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.label_eq.insert(key.into(), value.into());
        self
    }

    /// True if the point matches every set filter. The `Metric`
    /// is needed to check the resource-attribute scoped
    /// `service` filter.
    pub fn matches(&self, metric: &Metric, point: &MetricPoint) -> bool {
        if let Some(target) = self.service.as_deref() {
            match metric.resource_attributes.get("service.name") {
                Some(name) if name == target => {}
                _ => return false,
            }
        }
        for (key, value) in &self.label_eq {
            match point.attributes.get(key) {
                Some(actual) if actual == value => {}
                _ => return false,
            }
        }
        true
    }

    /// True if no filters are set.
    pub fn is_empty(&self) -> bool {
        self.service.is_none() && self.label_eq.is_empty()
    }
}
