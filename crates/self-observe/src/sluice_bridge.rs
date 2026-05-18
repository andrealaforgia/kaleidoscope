// Kaleidoscope self-observe — Sluice → Pulse bridge
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

//! `SluiceToPulseRecorder` — bridge that turns Sluice's queue
//! events into Pulse metric points. Parity with
//! [`crate::LumenToPulseRecorder`] and
//! [`crate::CinderToPulseRecorder`].

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use pulse::{Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore};
use sluice::MetricsRecorder as SluiceRecorder;

/// Bridge: implements `sluice::MetricsRecorder`. Every event
/// becomes a single-point `MetricBatch` ingested into the
/// configured Pulse store under the same tenant.
///
/// Metric names follow the convention `sluice.<event>.count`,
/// matching the existing `lumen.<event>.count` and
/// `cinder.<event>.count` shapes:
///
/// - `sluice.enqueue.count` — value = 1, attribute
///   `accepted=true|false` distinguishes successful enqueues
///   from `EnqueueError::Full` rejections (capacity-based
///   back-pressure visible per-tenant)
/// - `sluice.dequeue.count` — value = 1
/// - `sluice.ack.count` — value = 1
/// - `sluice.nack.count` — value = 1 (consumer-side redelivery
///   signal; spikes indicate downstream processing trouble)
///
/// The bridge ignores Pulse ingest errors (best-effort
/// observability). `MetricStoreError` is empty at v0; the
/// explicit `let _ =` is forward-compatible.
pub struct SluiceToPulseRecorder {
    pulse: Arc<dyn MetricStore + Send + Sync>,
}

impl SluiceToPulseRecorder {
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

impl SluiceRecorder for SluiceToPulseRecorder {
    fn record_enqueue(&self, tenant: &TenantId, accepted: bool) {
        let mut attrs = BTreeMap::new();
        attrs.insert("accepted".to_string(), accepted.to_string());
        self.emit(tenant, "sluice.enqueue.count", 1.0, attrs);
    }

    fn record_dequeue(&self, tenant: &TenantId) {
        self.emit(tenant, "sluice.dequeue.count", 1.0, BTreeMap::new());
    }

    fn record_ack(&self, tenant: &TenantId) {
        self.emit(tenant, "sluice.ack.count", 1.0, BTreeMap::new());
    }

    fn record_nack(&self, tenant: &TenantId) {
        self.emit(tenant, "sluice.nack.count", 1.0, BTreeMap::new());
    }
}
