// Kaleidoscope Pulse — MetricStore trait + in-memory adapter
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

//! `MetricStore` trait + in-memory adapter.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use aegis::TenantId;

use crate::metric::{Metric, MetricBatch, MetricName, MetricPoint, SeriesKey, TimeRange};
use crate::metrics::MetricsRecorder;
use crate::predicate::Predicate;
use crate::MAX_SERIES_PER_TENANT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IngestReceipt {
    pub count: usize,
    /// Number of NEW `SeriesKey`s refused in this ingest call because
    /// the tenant's distinct-series count was already at or above
    /// [`crate::MAX_SERIES_PER_TENANT`]. Counted per-metric, not
    /// per-point. ADR-0051.
    pub series_refused: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetricStoreError {
    /// A durable adapter (v1 `FileBackedMetricStore`) failed to
    /// persist or recover state. The v0 in-memory adapter never
    /// produces this variant.
    PersistenceFailed { reason: String },
}

impl fmt::Display for MetricStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetricStoreError::PersistenceFailed { reason } => {
                write!(f, "persistence failed: {reason}")
            }
        }
    }
}

impl std::error::Error for MetricStoreError {}

/// The metric-store port. v0 ships [`InMemoryMetricStore`] as the
/// only adapter; the disk-backed adapter lands at v1.
///
/// Semantics:
///
/// - **Per-tenant + per-metric isolation.** Lookup is keyed by
///   `(TenantId, MetricName)`; the scan is over the per-metric
///   point list.
/// - **Ascending-time ordering** within a metric.
/// - **OTLP-shaped types.** Field set mirrors
///   `opentelemetry-proto::metrics::v1`.
/// - **Half-open time range.** `[start, end)`.
pub trait MetricStore {
    fn ingest(
        &self,
        tenant: &TenantId,
        batch: MetricBatch,
    ) -> Result<IngestReceipt, MetricStoreError>;

    /// Query every point under `(tenant, metric_name)` whose
    /// `time_unix_nano` falls within `range`. The owning metric
    /// is returned alongside each point so callers can inspect
    /// resource attributes. Empty result is `Ok(Vec::new())`.
    fn query(
        &self,
        tenant: &TenantId,
        metric_name: &MetricName,
        range: TimeRange,
    ) -> Result<Vec<(Metric, MetricPoint)>, MetricStoreError>;

    /// Query with a predicate. The predicate composes with the
    /// time range as `range AND predicate`.
    fn query_with(
        &self,
        tenant: &TenantId,
        metric_name: &MetricName,
        range: TimeRange,
        predicate: &Predicate,
    ) -> Result<Vec<(Metric, MetricPoint)>, MetricStoreError>;
}

/// v0 in-process adapter. `HashMap<(TenantId, MetricName),
/// SeriesEntry>` keyed lookup, plus a per-series point vector
/// sorted-on-ingest by `time_unix_nano`.
pub struct InMemoryMetricStore {
    recorder: Box<dyn MetricsRecorder + Send + Sync>,
    state: Mutex<InnerState>,
}

#[derive(Default)]
struct InnerState {
    /// Indexed by `(tenant, SeriesKey)`, where `SeriesKey` is the
    /// metric name plus its full resource-attribute label set. Each
    /// entry holds the canonical `Metric` (sans `points`) and a sorted
    /// point vector — separating metadata from data so the v1 adapter
    /// can hoist resource attributes to the batch level without
    /// touching the trait shape.
    series: HashMap<(TenantId, SeriesKey), SeriesEntry>,
    /// Shadow per-tenant counter of distinct `SeriesKey`s held in
    /// `series` (ADR-0051 §5). Maintained atomically with `series`
    /// under the same `Mutex` so the cap-check, the increment, and
    /// the insert are atomic per metric. O(1) per check.
    tenant_counts: HashMap<TenantId, usize>,
}

struct SeriesEntry {
    metric: Metric, // `metric.points` stays empty; points live in `points` below
    points: Vec<MetricPoint>,
}

impl InMemoryMetricStore {
    pub fn new(recorder: Box<dyn MetricsRecorder + Send + Sync>) -> Self {
        Self {
            recorder,
            state: Mutex::new(InnerState::default()),
        }
    }
}

impl fmt::Debug for InMemoryMetricStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryMetricStore")
            .field("recorder", &"<opaque>")
            .finish()
    }
}

impl MetricStore for InMemoryMetricStore {
    fn ingest(
        &self,
        tenant: &TenantId,
        batch: MetricBatch,
    ) -> Result<IngestReceipt, MetricStoreError> {
        let mut state = self.state.lock().expect("poisoned");
        let mut count = 0usize;
        let mut series_refused = 0usize;
        for mut metric in batch.metrics {
            let key = (tenant.clone(), SeriesKey::of(&metric));
            let is_existing = state.series.contains_key(&key);
            if !is_existing {
                // Per ADR-0051 §1, the boundary is `>=`: a per-tenant
                // count of exactly MAX_SERIES_PER_TENANT refuses the
                // next new key. The in-memory adapter always enforces
                // the cap (it has no WAL-replay path).
                let tenant_count = state.tenant_counts.get(tenant).copied().unwrap_or(0);
                if tenant_count >= MAX_SERIES_PER_TENANT {
                    series_refused += 1;
                    continue;
                }
                *state.tenant_counts.entry(tenant.clone()).or_default() += 1;
            }
            let points = std::mem::take(&mut metric.points);
            count += points.len();
            let entry = state.series.entry(key).or_insert_with(|| SeriesEntry {
                metric: Metric {
                    points: Vec::new(),
                    ..metric.clone()
                },
                points: Vec::new(),
            });
            // Refresh the canonical metric metadata in case the
            // operator has updated description / unit between
            // ingests. v1 will probably want a "first write wins"
            // policy with conflict warnings; v0 is permissive.
            // `resource_attributes` is NOT refreshed: it is part of
            // the series key (ADR-0045), so a differing label set
            // lands in a different entry and an identical one already
            // matches the stored attributes.
            entry.metric.description = metric.description;
            entry.metric.unit = metric.unit;
            entry.metric.kind = metric.kind;
            entry.points.extend(points);
            entry.points.sort_by_key(|p| p.time_unix_nano);
        }
        self.recorder.record_ingest(tenant, count);
        if series_refused > 0 {
            self.recorder.record_series_refused(tenant, series_refused);
        }
        Ok(IngestReceipt {
            count,
            series_refused,
        })
    }

    fn query(
        &self,
        tenant: &TenantId,
        metric_name: &MetricName,
        range: TimeRange,
    ) -> Result<Vec<(Metric, MetricPoint)>, MetricStoreError> {
        let state = self.state.lock().expect("poisoned");
        // Fan out across every series whose name matches within the
        // tenant; each carries its own resource_attributes.
        let matches: Vec<(Metric, MetricPoint)> = state
            .series
            .iter()
            .filter(|((entry_tenant, key), _)| entry_tenant == tenant && key.name == *metric_name)
            .flat_map(|(_, entry)| {
                entry
                    .points
                    .iter()
                    .filter(|p| range.contains(p.time_unix_nano))
                    .cloned()
                    .map(|p| (entry.metric.clone(), p))
            })
            .collect();
        self.recorder.record_query(tenant, matches.len());
        Ok(matches)
    }

    fn query_with(
        &self,
        tenant: &TenantId,
        metric_name: &MetricName,
        range: TimeRange,
        predicate: &Predicate,
    ) -> Result<Vec<(Metric, MetricPoint)>, MetricStoreError> {
        let state = self.state.lock().expect("poisoned");
        // Fan out across every series whose name matches within the
        // tenant, then apply the predicate per row.
        let matches: Vec<(Metric, MetricPoint)> = state
            .series
            .iter()
            .filter(|((entry_tenant, key), _)| entry_tenant == tenant && key.name == *metric_name)
            .flat_map(|(_, entry)| {
                entry
                    .points
                    .iter()
                    .filter(|p| {
                        range.contains(p.time_unix_nano) && predicate.matches(&entry.metric, p)
                    })
                    .cloned()
                    .map(|p| (entry.metric.clone(), p))
            })
            .collect();
        self.recorder.record_query(tenant, matches.len());
        Ok(matches)
    }
}
