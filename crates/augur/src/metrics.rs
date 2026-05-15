// Kaleidoscope Augur — metrics seam
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

//! Observability seam mirroring every prior storage / port
//! crate.

use std::sync::{Arc, Mutex};

use aegis::TenantId;

pub trait MetricsRecorder: Send + Sync {
    fn record_observation(&self, tenant: &TenantId);
    fn record_anomaly(&self, tenant: &TenantId, score: f64);
}

#[derive(Debug, Clone, Default)]
pub struct NoopRecorder;

impl MetricsRecorder for NoopRecorder {
    fn record_observation(&self, _tenant: &TenantId) {}
    fn record_anomaly(&self, _tenant: &TenantId, _score: f64) {}
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecordedEvent {
    Observation { tenant: TenantId },
    Anomaly { tenant: TenantId, score: f64 },
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
    fn record_observation(&self, tenant: &TenantId) {
        self.events
            .lock()
            .expect("poisoned")
            .push(RecordedEvent::Observation {
                tenant: tenant.clone(),
            });
    }

    fn record_anomaly(&self, tenant: &TenantId, score: f64) {
        self.events
            .lock()
            .expect("poisoned")
            .push(RecordedEvent::Anomaly {
                tenant: tenant.clone(),
                score,
            });
    }
}
