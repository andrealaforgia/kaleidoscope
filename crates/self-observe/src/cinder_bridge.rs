// Kaleidoscope self-observe — Cinder → Pulse bridge
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

//! `CinderToPulseRecorder` — bridge that turns Cinder's
//! `record_place`, `record_migrate`, and `record_evaluate`
//! events into Pulse metric points. Parity with
//! [`crate::LumenToPulseRecorder`].

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{MetricsRecorder as CinderRecorder, Tier};
use pulse::{Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore};

/// Bridge: implements `cinder::MetricsRecorder`. Every event
/// becomes a single-point `MetricBatch` ingested into the
/// configured Pulse store under the same tenant.
///
/// Metric names follow the convention `cinder.<event>.count`,
/// matching the Lumen bridge's `lumen.<event>.count` shape:
///
/// - `cinder.place.count` — value = 1, attributes `{tier=hot|warm|cold}`
/// - `cinder.migrate.count` — value = 1, attributes `{from=…, to=…}`
/// - `cinder.evaluate.migrated.count` — value = number of items
///   migrated in this evaluate pass; the metric fires once per
///   `evaluate()` call (including the zero-migration case, so an
///   operator dashboard can show evaluate-rate independently of
///   migrate-rate).
///
/// The bridge ignores Pulse ingest errors (best-effort
/// observability). The `MetricStore` returned `MetricStoreError`
/// is an empty enum at v0, so no error path actually exists;
/// the explicit `let _ =` is forward-compatible.
pub struct CinderToPulseRecorder {
    pulse: Arc<dyn MetricStore + Send + Sync>,
}

impl CinderToPulseRecorder {
    /// Construct a bridge backed by the given Pulse store.
    pub fn new(pulse: Arc<dyn MetricStore + Send + Sync>) -> Self {
        Self { pulse }
    }

    fn emit(
        &self,
        tenant: &TenantId,
        metric_name: &str,
        value: f64,
        attributes: BTreeMap<String, String>,
    ) {
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
                attributes,
                value,
            }],
            resource_attributes: BTreeMap::new(),
        };
        let _ = self
            .pulse
            .ingest(tenant, MetricBatch::with_metrics(vec![metric]));
    }
}

fn tier_label(tier: Tier) -> &'static str {
    match tier {
        Tier::Hot => "hot",
        Tier::Warm => "warm",
        Tier::Cold => "cold",
    }
}

impl CinderRecorder for CinderToPulseRecorder {
    fn record_place(&self, tenant: &TenantId, tier: Tier) {
        let mut attrs = BTreeMap::new();
        attrs.insert("tier".to_string(), tier_label(tier).to_string());
        self.emit(tenant, "cinder.place.count", 1.0, attrs);
    }

    fn record_migrate(&self, tenant: &TenantId, from: Tier, to: Tier) {
        let mut attrs = BTreeMap::new();
        attrs.insert("from".to_string(), tier_label(from).to_string());
        attrs.insert("to".to_string(), tier_label(to).to_string());
        self.emit(tenant, "cinder.migrate.count", 1.0, attrs);
    }

    fn record_evaluate(&self, tenant: &TenantId, migrated: usize) {
        self.emit(
            tenant,
            "cinder.evaluate.migrated.count",
            migrated as f64,
            BTreeMap::new(),
        );
    }
}
