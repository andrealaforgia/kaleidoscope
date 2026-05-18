// Kaleidoscope self-observe — Augur → Pulse bridge
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

//! `AugurToPulseRecorder` — bridge that turns Augur's
//! anomaly-observer events into Pulse metric points.
//!
//! Augur's `MetricsRecorder` trait has two events:
//!
//! - `record_observation(tenant)` — one observation seen
//! - `record_anomaly(tenant, score)` — observer flagged an
//!   anomaly with the given continuous score
//!
//! Pulse natively carries `f64` point values, so the anomaly
//! score lands directly as the metric value of
//! `augur.anomaly.score`. A separate `augur.anomaly.count`
//! metric with value=1 lets operators rate-alert on anomaly
//! occurrences without aggregating over the variable score.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use augur::MetricsRecorder as AugurRecorder;
use pulse::{Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore};

pub struct AugurToPulseRecorder {
    pulse: Arc<dyn MetricStore + Send + Sync>,
}

impl AugurToPulseRecorder {
    pub fn new(pulse: Arc<dyn MetricStore + Send + Sync>) -> Self {
        Self { pulse }
    }

    fn emit(&self, tenant: &TenantId, metric_name: &str, kind: MetricKind, value: f64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let metric = Metric {
            name: MetricName::new(metric_name),
            description: String::new(),
            unit: "1".to_string(),
            kind,
            points: vec![MetricPoint {
                time_unix_nano: now,
                start_time_unix_nano: 0,
                attributes: BTreeMap::new(),
                value,
            }],
            resource_attributes: BTreeMap::new(),
        };
        let _ = self
            .pulse
            .ingest(tenant, MetricBatch::with_metrics(vec![metric]));
    }
}

impl AugurRecorder for AugurToPulseRecorder {
    fn record_observation(&self, tenant: &TenantId) {
        self.emit(tenant, "augur.observation.count", MetricKind::Sum, 1.0);
    }

    fn record_anomaly(&self, tenant: &TenantId, score: f64) {
        // Two metrics per anomaly event: a monotonic counter so
        // operators can rate-alert on occurrences, and a gauge
        // carrying the score so dashboards can plot it
        // directly without aggregating over a high-cardinality
        // attribute.
        self.emit(tenant, "augur.anomaly.count", MetricKind::Sum, 1.0);
        self.emit(tenant, "augur.anomaly.score", MetricKind::Gauge, score);
    }
}
