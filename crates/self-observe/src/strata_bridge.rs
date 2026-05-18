// Kaleidoscope self-observe — Strata → Pulse bridge
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

//! `StrataToPulseRecorder` — bridge that turns Strata's
//! continuous-profile-store events into Pulse metric points.
//! Identical shape to `LumenToPulseRecorder` and
//! `RayToPulseRecorder`; the metric names are
//! `strata.<event>.count`.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use pulse::{Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore};
use strata::MetricsRecorder as StrataRecorder;

pub struct StrataToPulseRecorder {
    pulse: Arc<dyn MetricStore + Send + Sync>,
}

impl StrataToPulseRecorder {
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
        let _ = self
            .pulse
            .ingest(tenant, MetricBatch::with_metrics(vec![metric]));
    }
}

impl StrataRecorder for StrataToPulseRecorder {
    fn record_ingest(&self, tenant: &TenantId, profile_count: usize) {
        self.emit(tenant, "strata.ingest.count", profile_count as f64);
    }

    fn record_query(&self, tenant: &TenantId, matched_count: usize) {
        self.emit(tenant, "strata.query.count", matched_count as f64);
    }
}
