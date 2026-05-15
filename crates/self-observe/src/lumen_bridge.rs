// Kaleidoscope self-observe — Lumen → Pulse bridge
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

//! `LumenToPulseRecorder` — bridge that turns Lumen's
//! `record_ingest` and `record_query` events into Pulse metric
//! points.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use lumen::MetricsRecorder as LumenRecorder;
use pulse::{Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore};

/// Bridge: implements `lumen::MetricsRecorder`. Every event
/// becomes a single-point `MetricBatch` ingested into the
/// configured Pulse store under the same tenant.
///
/// Metric names follow the convention `lumen.<event>.count`:
///
/// - `lumen.ingest.count` — value = `record_count`
/// - `lumen.query.count` — value = `matched_count`
///
/// The bridge ignores Pulse ingest errors (best-effort
/// observability). The `MetricStore` returned `MetricStoreError`
/// is an empty enum at v0, so no error path actually exists;
/// the explicit `let _ =` is forward-compatible.
pub struct LumenToPulseRecorder {
    pulse: Arc<dyn MetricStore + Send + Sync>,
}

impl LumenToPulseRecorder {
    /// Construct a bridge backed by the given Pulse store.
    pub fn new(pulse: Arc<dyn MetricStore + Send + Sync>) -> Self {
        Self { pulse }
    }

    fn emit(&self, tenant: &TenantId, metric_name: &str, value: f64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let metric = Metric {
            name: MetricName::new(metric_name),
            description: String::new(),
            unit: "1".to_string(),
            kind: MetricKind::Sum,
            points: vec![MetricPoint {
                time_unix_nano: now,
                start_time_unix_nano: 0,
                attributes: BTreeMap::new(),
                value,
            }],
            resource_attributes: BTreeMap::new(),
        };
        // Best-effort. v0 MetricStoreError is empty so this
        // cannot fail; v1+ may grow real failure modes and the
        // bridge will swallow them deliberately.
        let _ = self
            .pulse
            .ingest(tenant, MetricBatch::with_metrics(vec![metric]));
    }
}

impl LumenRecorder for LumenToPulseRecorder {
    fn record_ingest(&self, tenant: &TenantId, record_count: usize) {
        self.emit(tenant, "lumen.ingest.count", record_count as f64);
    }

    fn record_query(&self, tenant: &TenantId, matched_count: usize) {
        self.emit(tenant, "lumen.query.count", matched_count as f64);
    }
}
