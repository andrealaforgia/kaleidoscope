// Kaleidoscope Lumen — metrics seam
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

//! Observability seam mirroring Sluice's `MetricsRecorder`.
//!
//! Lumen itself does not depend on a specific OTLP SDK. The
//! [`MetricsRecorder`] trait lets the operator's binary wire its
//! own counter / histogram emission (e.g. via
//! `opentelemetry-otlp`). v0 ships [`NoopRecorder`] (no-op) and
//! [`CapturingRecorder`] (test-only, captures every event into a
//! thread-safe vector).

use std::sync::{Arc, Mutex};

use aegis::TenantId;

/// Recorder callback. Implementations should be cheap; the methods
/// are called on the hot paths (`ingest`, `query`).
pub trait MetricsRecorder: Send + Sync {
    /// Called on every ingest. `record_count` is the number of
    /// records in the batch.
    fn record_ingest(&self, tenant: &TenantId, record_count: usize);

    /// Called on every query. `matched_count` is the number of
    /// records returned to the caller.
    fn record_query(&self, tenant: &TenantId, matched_count: usize);
}

/// Default no-op recorder for production deployments that have not
/// yet wired OTLP metrics, or for benchmarks measuring pure store
/// overhead.
#[derive(Debug, Clone, Default)]
pub struct NoopRecorder;

impl MetricsRecorder for NoopRecorder {
    fn record_ingest(&self, _tenant: &TenantId, _record_count: usize) {}
    fn record_query(&self, _tenant: &TenantId, _matched_count: usize) {}
}

/// One recorded event for test inspection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedEvent {
    Ingest {
        tenant: TenantId,
        record_count: usize,
    },
    Query {
        tenant: TenantId,
        matched_count: usize,
    },
}

/// Test-only recorder that captures every event into a
/// thread-safe vector.
#[derive(Debug, Clone, Default)]
pub struct CapturingRecorder {
    events: Arc<Mutex<Vec<RecordedEvent>>>,
}

impl CapturingRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the captured events. Returns a clone; the
    /// recorder continues to capture after this call.
    pub fn snapshot(&self) -> Vec<RecordedEvent> {
        self.events.lock().expect("poisoned").clone()
    }

    pub fn len(&self) -> usize {
        self.events.lock().expect("poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl MetricsRecorder for CapturingRecorder {
    fn record_ingest(&self, tenant: &TenantId, record_count: usize) {
        self.events
            .lock()
            .expect("poisoned")
            .push(RecordedEvent::Ingest {
                tenant: tenant.clone(),
                record_count,
            });
    }

    fn record_query(&self, tenant: &TenantId, matched_count: usize) {
        self.events
            .lock()
            .expect("poisoned")
            .push(RecordedEvent::Query {
                tenant: tenant.clone(),
                matched_count,
            });
    }
}
