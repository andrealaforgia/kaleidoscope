// Kaleidoscope Strata — ProfileStore trait + in-memory adapter
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

//! `ProfileStore` trait + in-memory adapter.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Mutex;

use aegis::TenantId;

use crate::metrics::MetricsRecorder;
use crate::predicate::Predicate;
use crate::profile::{Profile, ProfileBatch, ServiceName, TimeRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IngestReceipt {
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileStoreError {
    /// The underlying storage adapter failed to persist an
    /// operation. Only emitted by adapters with side effects
    /// (e.g. `FileBackedProfileStore`); the v0 `InMemoryProfileStore`
    /// never returns this. Same `PersistenceFailed` shape as the
    /// other v1 adapters.
    PersistenceFailed { reason: String },
}

impl fmt::Display for ProfileStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfileStoreError::PersistenceFailed { reason } => {
                write!(f, "persistence failed: {reason}")
            }
        }
    }
}

impl std::error::Error for ProfileStoreError {}

/// The profile-store port.
///
/// Semantics:
///
/// - **Per-tenant + per-service isolation.** Keyed by
///   `(TenantId, ServiceName)`.
/// - **Ascending-time ordering** within a service bucket.
/// - **pprof-shaped types.** Field set mirrors
///   `profile.proto`.
/// - **Half-open time range.** `[start, end)`.
pub trait ProfileStore {
    fn ingest(
        &self,
        tenant: &TenantId,
        batch: ProfileBatch,
    ) -> Result<IngestReceipt, ProfileStoreError>;

    /// Every profile for `(tenant, service)` whose
    /// `time_unix_nano` falls within `range`.
    fn query(
        &self,
        tenant: &TenantId,
        service: &ServiceName,
        range: TimeRange,
    ) -> Result<Vec<Profile>, ProfileStoreError>;

    /// Predicate-narrowed query.
    fn query_with(
        &self,
        tenant: &TenantId,
        service: &ServiceName,
        range: TimeRange,
        predicate: &Predicate,
    ) -> Result<Vec<Profile>, ProfileStoreError>;
}

/// v0 in-process adapter.
pub struct InMemoryProfileStore {
    recorder: Box<dyn MetricsRecorder + Send + Sync>,
    state: Mutex<InnerState>,
}

#[derive(Default)]
struct InnerState {
    per_service: HashMap<(TenantId, ServiceName), Vec<Profile>>,
}

impl InMemoryProfileStore {
    pub fn new(recorder: Box<dyn MetricsRecorder + Send + Sync>) -> Self {
        Self {
            recorder,
            state: Mutex::new(InnerState::default()),
        }
    }
}

impl fmt::Debug for InMemoryProfileStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryProfileStore")
            .field("recorder", &"<opaque>")
            .finish()
    }
}

impl ProfileStore for InMemoryProfileStore {
    fn ingest(
        &self,
        tenant: &TenantId,
        batch: ProfileBatch,
    ) -> Result<IngestReceipt, ProfileStoreError> {
        let mut state = self.state.lock().expect("poisoned");
        let count = batch.profiles.len();
        // Track which buckets we touched; sort each exactly
        // once at the end.
        let mut touched: HashSet<ServiceName> = HashSet::new();
        for profile in batch.profiles {
            // Profiles without service.name are dropped from
            // the by-service index at v0. v1 will keep them
            // under a synthetic service identifier.
            if profile.service_name().is_empty() {
                continue;
            }
            let service = ServiceName::new(profile.service_name());
            touched.insert(service.clone());
            let key = (tenant.clone(), service);
            state.per_service.entry(key).or_default().push(profile);
        }
        for service in touched {
            let key = (tenant.clone(), service);
            if let Some(bucket) = state.per_service.get_mut(&key) {
                bucket.sort_by_key(|p| p.time_unix_nano);
            }
        }
        self.recorder.record_ingest(tenant, count);
        Ok(IngestReceipt { count })
    }

    fn query(
        &self,
        tenant: &TenantId,
        service: &ServiceName,
        range: TimeRange,
    ) -> Result<Vec<Profile>, ProfileStoreError> {
        let state = self.state.lock().expect("poisoned");
        let key = (tenant.clone(), service.clone());
        let bucket = match state.per_service.get(&key) {
            Some(b) => b,
            None => {
                self.recorder.record_query(tenant, 0);
                return Ok(Vec::new());
            }
        };
        let matches: Vec<Profile> = bucket
            .iter()
            .filter(|p| range.contains(p.time_unix_nano))
            .cloned()
            .collect();
        self.recorder.record_query(tenant, matches.len());
        Ok(matches)
    }

    fn query_with(
        &self,
        tenant: &TenantId,
        service: &ServiceName,
        range: TimeRange,
        predicate: &Predicate,
    ) -> Result<Vec<Profile>, ProfileStoreError> {
        let state = self.state.lock().expect("poisoned");
        let key = (tenant.clone(), service.clone());
        let bucket = match state.per_service.get(&key) {
            Some(b) => b,
            None => {
                self.recorder.record_query(tenant, 0);
                return Ok(Vec::new());
            }
        };
        let matches: Vec<Profile> = bucket
            .iter()
            .filter(|p| range.contains(p.time_unix_nano) && predicate.matches(p))
            .cloned()
            .collect();
        self.recorder.record_query(tenant, matches.len());
        Ok(matches)
    }
}
