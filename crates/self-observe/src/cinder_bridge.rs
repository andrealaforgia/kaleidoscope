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
//! `record_place`, `record_migrate` and `record_evaluate` events
//! into Pulse metric points.
//!
//! Per ADR-0038 §1 the locked public surface is:
//!
//! ```text
//! pub struct CinderToPulseRecorder {
//!     pulse: Arc<dyn MetricStore + Send + Sync>,
//! }
//!
//! impl CinderToPulseRecorder {
//!     pub fn new(pulse: Arc<dyn MetricStore + Send + Sync>) -> Self;
//! }
//!
//! impl cinder::MetricsRecorder for CinderToPulseRecorder { ... }
//! ```
//!
//! Per ADR-0038 §2 the per-event emission contract is:
//!
//! - `record_place(tenant, tier)`     → `cinder.place.count`, value=1,
//!   attrs={tier}
//! - `record_migrate(tenant, f, t)`   → `cinder.migrate.count`, value=1,
//!   attrs={from, to}
//! - `record_evaluate(tenant, n)`     → `cinder.evaluate.migrated.count`,
//!   value=n, attrs={}
//!
//! Per DISCUSS D4 the `Tier` → lowercase-string serialisation is
//! enforced from one location ([`tier_lowercase`]).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{MetricsRecorder as CinderRecorder, Tier};
use pulse::{Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore};

/// Bridge: implements `cinder::MetricsRecorder`. Every Cinder
/// tier event becomes a single-point Pulse `MetricBatch` ingested
/// into the configured store under the same tenant.
///
/// Pulse ingest errors are swallowed (best-effort observability,
/// DISCUSS D5). The `MetricStoreError` returned by
/// `MetricStore::ingest` is an empty enum at v0, so no error
/// path actually exists; the explicit `let _ =` is forward-
/// compatible.
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

/// Single point-of-truth for `Tier` → wire-format string
/// (DISCUSS D4). Both the `from`/`to` attributes on migrate
/// points and the `tier` attribute on place points must agree;
/// centralising the mapping here makes that impossible to break
/// by accident.
fn tier_lowercase(tier: Tier) -> &'static str {
    match tier {
        Tier::Hot => "hot",
        Tier::Warm => "warm",
        Tier::Cold => "cold",
    }
}

impl CinderRecorder for CinderToPulseRecorder {
    fn record_place(&self, tenant: &TenantId, tier: Tier) {
        let mut attributes = BTreeMap::new();
        attributes.insert("tier".to_string(), tier_lowercase(tier).to_string());
        self.emit(tenant, "cinder.place.count", 1.0, attributes);
    }

    fn record_migrate(&self, tenant: &TenantId, from: Tier, to: Tier) {
        let mut attributes = BTreeMap::new();
        attributes.insert("from".to_string(), tier_lowercase(from).to_string());
        attributes.insert("to".to_string(), tier_lowercase(to).to_string());
        self.emit(tenant, "cinder.migrate.count", 1.0, attributes);
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
