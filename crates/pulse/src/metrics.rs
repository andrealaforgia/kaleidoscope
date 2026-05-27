// Kaleidoscope Pulse — metrics seam
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

//! Observability seam mirroring Lumen + Sluice.

use std::sync::{Arc, Mutex};

use aegis::TenantId;

/// Recorder callback. Cheap; called on hot paths.
pub trait MetricsRecorder: Send + Sync {
    fn record_ingest(&self, tenant: &TenantId, point_count: usize);
    fn record_query(&self, tenant: &TenantId, matched_count: usize);

    /// Fired once per ingest call in which one or more NEW
    /// `SeriesKey`s were refused over the per-tenant cardinality
    /// watermark (ADR-0051). `count` is the number of NEW
    /// `SeriesKey`s refused in this call. Default body is a no-op so
    /// existing implementors continue to compile and behave
    /// identically.
    fn record_series_refused(&self, _tenant: &TenantId, _count: usize) {}
}

#[derive(Debug, Clone, Default)]
pub struct NoopRecorder;

impl MetricsRecorder for NoopRecorder {
    fn record_ingest(&self, _tenant: &TenantId, _point_count: usize) {}
    fn record_query(&self, _tenant: &TenantId, _matched_count: usize) {}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedEvent {
    Ingest {
        tenant: TenantId,
        point_count: usize,
    },
    Query {
        tenant: TenantId,
        matched_count: usize,
    },
    /// Pushed by `CapturingRecorder` for each
    /// `record_series_refused` call (ADR-0051). `count` is the
    /// number of NEW `SeriesKey`s refused in the originating ingest
    /// call.
    SeriesRefused { tenant: TenantId, count: usize },
}

#[derive(Debug, Clone, Default)]
pub struct CapturingRecorder {
    events: Arc<Mutex<Vec<RecordedEvent>>>,
}

impl CapturingRecorder {
    pub fn new() -> Self {
        Self::default()
    }

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
    fn record_ingest(&self, tenant: &TenantId, point_count: usize) {
        self.events
            .lock()
            .expect("poisoned")
            .push(RecordedEvent::Ingest {
                tenant: tenant.clone(),
                point_count,
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

    fn record_series_refused(&self, tenant: &TenantId, count: usize) {
        self.events
            .lock()
            .expect("poisoned")
            .push(RecordedEvent::SeriesRefused {
                tenant: tenant.clone(),
                count,
            });
    }
}
