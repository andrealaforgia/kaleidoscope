// Kaleidoscope Cinder — metrics seam
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

//! Observability seam mirroring the storage engines.

use std::sync::{Arc, Mutex};

use aegis::TenantId;

use crate::tier::Tier;

pub trait MetricsRecorder: Send + Sync {
    fn record_place(&self, tenant: &TenantId, tier: Tier);
    fn record_migrate(&self, tenant: &TenantId, from: Tier, to: Tier);
    fn record_evaluate(&self, tenant: &TenantId, migrated: usize);
}

#[derive(Debug, Clone, Default)]
pub struct NoopRecorder;

impl MetricsRecorder for NoopRecorder {
    fn record_place(&self, _tenant: &TenantId, _tier: Tier) {}
    fn record_migrate(&self, _tenant: &TenantId, _from: Tier, _to: Tier) {}
    fn record_evaluate(&self, _tenant: &TenantId, _migrated: usize) {}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedEvent {
    Place {
        tenant: TenantId,
        tier: Tier,
    },
    Migrate {
        tenant: TenantId,
        from: Tier,
        to: Tier,
    },
    Evaluate {
        tenant: TenantId,
        migrated: usize,
    },
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
    fn record_place(&self, tenant: &TenantId, tier: Tier) {
        self.events
            .lock()
            .expect("poisoned")
            .push(RecordedEvent::Place {
                tenant: tenant.clone(),
                tier,
            });
    }

    fn record_migrate(&self, tenant: &TenantId, from: Tier, to: Tier) {
        self.events
            .lock()
            .expect("poisoned")
            .push(RecordedEvent::Migrate {
                tenant: tenant.clone(),
                from,
                to,
            });
    }

    fn record_evaluate(&self, tenant: &TenantId, migrated: usize) {
        self.events
            .lock()
            .expect("poisoned")
            .push(RecordedEvent::Evaluate {
                tenant: tenant.clone(),
                migrated,
            });
    }
}
