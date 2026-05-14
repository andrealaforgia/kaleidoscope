// Kaleidoscope Sluice — queue port between Sieve and storage
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

//! Observability seam for depth + counter emission.
//!
//! Sluice itself does not depend on a specific OTLP SDK. The
//! [`MetricsRecorder`] trait lets the operator's binary wire its
//! own gauge / counter emission (e.g. via `opentelemetry-otlp`).
//! v0 ships [`NoopRecorder`] (no-op) and [`CapturingRecorder`]
//! (test-only, captures every event into a thread-safe vector).

use std::sync::{Arc, Mutex};

use aegis::TenantId;

/// Recorder callback. Implementations should be cheap; the methods
/// are called on the queue's hot path (enqueue / dequeue / ack /
/// nack), so allocate carefully.
pub trait MetricsRecorder: Send + Sync {
    /// Called on every enqueue. `accepted` is `false` if the queue
    /// was at capacity (i.e. `EnqueueError::Full` returned).
    fn record_enqueue(&self, tenant: &TenantId, accepted: bool);

    /// Called on every successful dequeue.
    fn record_dequeue(&self, tenant: &TenantId);

    /// Called on every ack (consumer processed the message).
    fn record_ack(&self, tenant: &TenantId);

    /// Called on every nack (consumer returned the message for
    /// redelivery).
    fn record_nack(&self, tenant: &TenantId);
}

/// Default no-op recorder. Use in production deployments that do
/// not yet wire OTLP metrics, or in benchmarks that want to
/// measure pure queue overhead.
#[derive(Debug, Clone, Default)]
pub struct NoopRecorder;

impl MetricsRecorder for NoopRecorder {
    fn record_enqueue(&self, _tenant: &TenantId, _accepted: bool) {}
    fn record_dequeue(&self, _tenant: &TenantId) {}
    fn record_ack(&self, _tenant: &TenantId) {}
    fn record_nack(&self, _tenant: &TenantId) {}
}

/// One recorded queue event for test inspection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedEvent {
    Enqueue { tenant: TenantId, accepted: bool },
    Dequeue { tenant: TenantId },
    Ack { tenant: TenantId },
    Nack { tenant: TenantId },
}

/// Test-only recorder that captures every event into a thread-safe
/// vector. Use in acceptance tests to assert on enqueue / dequeue /
/// ack / nack sequencing.
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

    /// Number of events captured.
    pub fn len(&self) -> usize {
        self.events.lock().expect("poisoned").len()
    }

    /// True if no events have been recorded yet.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl MetricsRecorder for CapturingRecorder {
    fn record_enqueue(&self, tenant: &TenantId, accepted: bool) {
        self.events
            .lock()
            .expect("poisoned")
            .push(RecordedEvent::Enqueue {
                tenant: tenant.clone(),
                accepted,
            });
    }

    fn record_dequeue(&self, tenant: &TenantId) {
        self.events
            .lock()
            .expect("poisoned")
            .push(RecordedEvent::Dequeue {
                tenant: tenant.clone(),
            });
    }

    fn record_ack(&self, tenant: &TenantId) {
        self.events
            .lock()
            .expect("poisoned")
            .push(RecordedEvent::Ack {
                tenant: tenant.clone(),
            });
    }

    fn record_nack(&self, tenant: &TenantId) {
        self.events
            .lock()
            .expect("poisoned")
            .push(RecordedEvent::Nack {
                tenant: tenant.clone(),
            });
    }
}
