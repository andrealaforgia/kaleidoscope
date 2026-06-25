// Kaleidoscope consolidated runtime — self-metrics recorder (A2.S1)
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

//! Self-metrics recorder for the log-ingest path (A2.S1).
//!
//! [`SelfMetricsIngestRecorder`] implements `lumen::MetricsRecorder` and
//! writes a **CUMULATIVE** `records_ingested{signal=<signal>}` counter into
//! the supplied Pulse [`MetricStore`]. Because it tracks a running total via
//! an [`AtomicU64`] and writes that total on every ingest call, any single
//! query reading the latest point always sees the exact cumulative count —
//! not a per-batch delta, not an off-by-batching artifact.
//!
//! ## Forward-compatibility seam (E6)
//!
//! The `self_tenant` field decides which Pulse tenant the self-metric lands
//! under:
//!
//! - `None` (S1 default): land under the **ingest tenant** — whatever tenant
//!   the log-ingest call carries. For S1 this is always the customer tenant
//!   (e.g. `acme`).
//! - `Some(t)`: land under `t` regardless of the ingest tenant. This door a
//!   **later slice** walks through to assign a reserved internal identity
//!   (e.g. `__kaleidoscope_internal__`) so self-metrics are separated from
//!   customer data. Do **NOT** pass `Some(_)` in the S1 production wiring.
//!
//! The seam is a single construction-time field so the decision of *which*
//! tenant the self-metric lives under is entirely localised here, reversible
//! in one line, and requires no changes to any recorder logic.
//!
//! [`MetricStore`]: pulse::MetricStore
//! [`AtomicU64`]: std::sync::atomic::AtomicU64

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use lumen::MetricsRecorder as LumenMetricsRecorder;
use pulse::{Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore};

/// Self-metrics recorder: writes a cumulative `records_ingested` counter into
/// a Pulse store so the runtime can observe its own log-ingest throughput on
/// the normal metrics query path.
///
/// See the [module docs](self) for the forward-compat seam description.
pub struct SelfMetricsIngestRecorder {
    pulse: Arc<dyn MetricStore + Send + Sync>,
    signal: &'static str,
    /// Atomic running total. `fetch_add` + the returned previous value gives
    /// the new cumulative total in one lock-free step.
    total: AtomicU64,
    /// Forward-compat seam (A2.S1 E6). `None` → ingest tenant (S1 default);
    /// `Some(t)` → reserved internal identity (future slice only).
    self_tenant: Option<TenantId>,
}

impl SelfMetricsIngestRecorder {
    /// Build a recorder that writes `records_ingested{signal=signal}` into
    /// `pulse` under the resolved tenant.
    ///
    /// - `pulse`: the **same** `Arc<dyn MetricStore>` the metrics query router
    ///   reads from, so the self-metric is immediately visible on the normal
    ///   query path with no separate ingest step.
    /// - `signal`: the `signal` label value, e.g. `"logs"`.
    /// - `self_tenant`: `None` for the S1 default (ingest tenant); `Some(t)`
    ///   for the future reserved-internal-identity seam (E6).
    pub fn new(
        pulse: Arc<dyn MetricStore + Send + Sync>,
        signal: &'static str,
        self_tenant: Option<TenantId>,
    ) -> Self {
        Self {
            pulse,
            signal,
            total: AtomicU64::new(0),
            self_tenant,
        }
    }
}

impl LumenMetricsRecorder for SelfMetricsIngestRecorder {
    /// Increment the cumulative counter by `record_count` and write the new
    /// running total as a `records_ingested` Sum point into Pulse.
    ///
    /// The Pulse ingest is best-effort (`let _ = ...`): a metric-store hiccup
    /// must never propagate into the log-ingest hot path.
    fn record_ingest(&self, tenant: &TenantId, record_count: usize) {
        let added = record_count as u64;
        // Relaxed ordering: the metric point carries its own wall-clock
        // timestamp; the precise memory-order of the counter update relative
        // to concurrent reads does not affect correctness.
        let total = self.total.fetch_add(added, Ordering::Relaxed) + added;

        // Resolve target tenant. S1 wires `None` so the self-metric lands
        // under the same tenant the log records were filed under. A future
        // slice passes `Some(reserved_id)` here to segregate self-metrics.
        let target_tenant = self.self_tenant.clone().unwrap_or_else(|| tenant.clone());

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);

        let mut resource_attributes = BTreeMap::new();
        resource_attributes.insert("signal".to_string(), self.signal.to_string());

        let metric = Metric {
            name: MetricName::new("records_ingested"),
            description: String::new(),
            unit: "1".to_string(),
            kind: MetricKind::Sum,
            points: vec![MetricPoint {
                time_unix_nano: now,
                start_time_unix_nano: 0,
                attributes: BTreeMap::new(),
                value: total as f64,
            }],
            resource_attributes,
        };

        // Best-effort: swallow Pulse ingest errors so the log-ingest path is
        // never blocked by self-observability failures.
        let _ = self
            .pulse
            .ingest(&target_tenant, MetricBatch::with_metrics(vec![metric]));
    }

    /// No-op for this slice. Query-count tracking is deferred to a later A2
    /// slice.
    fn record_query(&self, _tenant: &TenantId, _matched_count: usize) {}
}
