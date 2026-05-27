// Kaleidoscope self-observe — Pulse cardinality watermark bridge
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

//! `PulseCardinalityToPulseRecorder` — bridge that turns Pulse's
//! `record_series_refused` events into Pulse metric points.
//!
//! Per ADR-0051 the per-event emission contract is:
//!
//! - `record_series_refused(tenant, count)` -> `pulse.series.refused.count`,
//!   value=count, kind Sum, point attribute `{tenant}`.
//!
//! The tenant rides as a POINT-level attribute (not a series-level
//! resource attribute) so the bridge does not multiply self-observe
//! cardinality by the number of tenants under the cap: one series for
//! all tenants, the tenant carried on each point.
//!
//! `record_ingest` and `record_query` are no-ops in this bridge:
//! pulse-on-pulse for ingest and query would loop. The Lumen and
//! Cinder bridges cover the upstream pillars.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use pulse::{
    Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore,
    MetricsRecorder as PulseRecorder,
};

/// Bridge: implements `pulse::MetricsRecorder`. A
/// `record_series_refused` event becomes a single-point
/// `MetricBatch` ingested into the configured Pulse store under
/// the same tenant.
///
/// Pulse ingest errors are swallowed (best-effort observability,
/// mirroring the Lumen and Cinder bridges). The `MetricStoreError`
/// returned by `MetricStore::ingest` is an empty enum at v0, so no
/// error path actually exists; the explicit `let _ =` is
/// forward-compatible.
pub struct PulseCardinalityToPulseRecorder {
    pulse: Arc<dyn MetricStore + Send + Sync>,
}

impl PulseCardinalityToPulseRecorder {
    /// Construct a bridge backed by the given Pulse store.
    pub fn new(pulse: Arc<dyn MetricStore + Send + Sync>) -> Self {
        Self { pulse }
    }
}

impl PulseRecorder for PulseCardinalityToPulseRecorder {
    fn record_ingest(&self, _tenant: &TenantId, _point_count: usize) {
        // No-op: pulse-on-pulse would loop on the ingest path.
    }

    fn record_query(&self, _tenant: &TenantId, _matched_count: usize) {
        // No-op: pulse-on-pulse would loop on the query path.
    }

    fn record_series_refused(&self, tenant: &TenantId, count: usize) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        // The tenant is a POINT-level attribute (ADR-0051 §7): one
        // series for all tenants, the tenant carried on each point.
        // Resource attributes are empty for the same reason: a
        // distinct resource_attributes set per tenant would land each
        // tenant in its own SeriesKey and re-introduce the very
        // cardinality blow-up this slice defends against.
        let mut attributes = BTreeMap::new();
        attributes.insert("tenant".to_string(), tenant.0.clone());
        let metric = Metric {
            name: MetricName::new("pulse.series.refused.count"),
            description: String::new(),
            unit: "1".to_string(),
            kind: MetricKind::Sum,
            points: vec![MetricPoint {
                time_unix_nano: now,
                start_time_unix_nano: 0,
                attributes,
                value: count as f64,
            }],
            resource_attributes: BTreeMap::new(),
        };
        let _ = self
            .pulse
            .ingest(tenant, MetricBatch::with_metrics(vec![metric]));
    }
}
