// Kaleidoscope Pulse — OTLP-shaped metric types
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

//! OTLP-shaped metric types at the trait boundary.
//!
//! The field set mirrors `opentelemetry-proto::metrics::v1` for
//! the gauge + sum number-point case. Histogram and summary land
//! at v1 with their own point types behind the same trait.

use std::collections::BTreeMap;

/// Metric name. Stable hash key for the per-(tenant, name)
/// point list.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MetricName(pub String);

impl MetricName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The point shape v0 supports. v1 adds Histogram,
/// ExponentialHistogram, Summary as new variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetricKind {
    /// Snapshot at a point in time (e.g. `process.cpu.utilization`).
    Gauge,
    /// Cumulative or delta sum (e.g. `http.server.duration.count`).
    Sum,
}

/// One OTLP metric definition. Carries the points belonging to it.
#[derive(Debug, Clone, PartialEq)]
pub struct Metric {
    pub name: MetricName,
    pub description: String,
    pub unit: String,
    pub kind: MetricKind,
    pub points: Vec<MetricPoint>,
    /// Resource attributes (e.g. `service.name`). Carried per
    /// metric to keep the v0 adapter simple; v1 will hoist common
    /// resource attributes to the batch level.
    pub resource_attributes: BTreeMap<String, String>,
}

/// One observation. Field set matches the OTLP NumberDataPoint
/// for gauge + sum. v1 will add `exemplars: Vec<Exemplar>`
/// non-breakingly.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricPoint {
    /// Nanoseconds since Unix epoch when the observation was
    /// recorded. Sort key for time-range queries.
    pub time_unix_nano: u64,
    /// Nanoseconds since Unix epoch when the cumulative window
    /// started (zero for delta-temporality sums and gauges).
    pub start_time_unix_nano: u64,
    /// Point-level attributes (e.g. `http.route`,
    /// `http.status_code`).
    pub attributes: BTreeMap<String, String>,
    /// Observation value. `f64` matches the OTLP
    /// `NumberDataPoint.as_double` shape; integer points encode
    /// as exact `f64`.
    pub value: f64,
}

/// A batch of metrics, all belonging to one tenant.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MetricBatch {
    pub metrics: Vec<Metric>,
}

impl MetricBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_metrics(metrics: Vec<Metric>) -> Self {
        Self { metrics }
    }

    pub fn push(&mut self, metric: Metric) {
        self.metrics.push(metric);
    }

    pub fn is_empty(&self) -> bool {
        self.metrics.is_empty()
    }

    /// Total point count across every metric in the batch.
    pub fn total_points(&self) -> usize {
        self.metrics.iter().map(|m| m.points.len()).sum()
    }
}

/// Half-open time range `[start, end)` in nanoseconds since the
/// Unix epoch.
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

    pub fn all() -> Self {
        Self::new(0, u64::MAX)
    }

    pub fn contains(&self, time_unix_nano: u64) -> bool {
        time_unix_nano >= self.start_unix_nano && time_unix_nano < self.end_unix_nano
    }
}
