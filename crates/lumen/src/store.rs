// Kaleidoscope Lumen — LogStore trait + in-memory adapter
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

//! `LogStore` trait + in-memory adapter.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use aegis::TenantId;

use crate::metrics::MetricsRecorder;
use crate::predicate::Predicate;
use crate::record::{LogBatch, LogRecord, TimeRange};

/// Successful ingest response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IngestReceipt {
    pub count: usize,
}

/// Typed store failures.
///
/// **v1 note**: at v0 this enum was empty (the in-memory adapter
/// has no failure modes). v1's `FileBackedLogStore` adds
/// `PersistenceFailed { reason: String }` so I/O errors surface
/// through the same trait. The Display impl gained an arm to
/// match. v0 callers that pattern-matched on `match *err {}` need
/// to add the arm or use a wildcard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogStoreError {
    /// The underlying storage adapter failed to persist an
    /// operation. Only emitted by adapters with side effects
    /// (e.g. `FileBackedLogStore`); the v0 `InMemoryLogStore`
    /// never returns this.
    PersistenceFailed { reason: String },
}

impl fmt::Display for LogStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogStoreError::PersistenceFailed { reason } => {
                write!(f, "persistence failed: {reason}")
            }
        }
    }
}

impl std::error::Error for LogStoreError {}

/// The log-store port. v0 ships [`InMemoryLogStore`] as the only
/// adapter; the disk-backed adapter lands at v1 behind this trait.
///
/// Semantics:
///
/// - **Per-tenant isolation.** `query` on tenant A never returns
///   tenant B's records.
/// - **Observed-time ordering.** `query` returns records in
///   ascending `observed_time_unix_nano` order within a tenant.
/// - **OTLP-shaped types.** No projections; field set mirrors
///   `opentelemetry-proto::logs::v1::LogRecord`.
/// - **Half-open time range.** `[start, end)`.
pub trait LogStore {
    /// Ingest a batch for the given tenant. Returns the receipt
    /// with the count of records persisted.
    fn ingest(&self, tenant: &TenantId, batch: LogBatch) -> Result<IngestReceipt, LogStoreError>;

    /// Query all records for the tenant whose
    /// `observed_time_unix_nano` falls within `range`. Returns an
    /// empty vector (not an error) when nothing matches.
    fn query(&self, tenant: &TenantId, range: TimeRange) -> Result<Vec<LogRecord>, LogStoreError>;

    /// Query with a predicate. The predicate composes with the
    /// time range: `range AND predicate`. An empty predicate is
    /// equivalent to [`LogStore::query`].
    fn query_with(
        &self,
        tenant: &TenantId,
        range: TimeRange,
        predicate: &Predicate,
    ) -> Result<Vec<LogRecord>, LogStoreError>;
}

/// v0 in-process adapter. `HashMap<TenantId, Vec<LogRecord>>`
/// sorted-on-ingest by `observed_time_unix_nano`. Linear scan
/// query. The KPI ceilings assume this shape; v1's columnar
/// substrate will tighten them.
pub struct InMemoryLogStore {
    recorder: Box<dyn MetricsRecorder + Send + Sync>,
    state: Mutex<InnerState>,
}

#[derive(Default)]
struct InnerState {
    per_tenant: HashMap<TenantId, Vec<LogRecord>>,
}

impl InMemoryLogStore {
    /// Construct an in-memory log store with the given metrics
    /// recorder. The recorder is called on every ingest / query.
    pub fn new(recorder: Box<dyn MetricsRecorder + Send + Sync>) -> Self {
        Self {
            recorder,
            state: Mutex::new(InnerState::default()),
        }
    }
}

impl fmt::Debug for InMemoryLogStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryLogStore")
            .field("recorder", &"<opaque>")
            .finish()
    }
}

impl LogStore for InMemoryLogStore {
    fn ingest(&self, tenant: &TenantId, batch: LogBatch) -> Result<IngestReceipt, LogStoreError> {
        let mut state = self.state.lock().expect("poisoned");
        let bucket = state.per_tenant.entry(tenant.clone()).or_default();
        let count = batch.records.len();
        bucket.extend(batch.records);
        bucket.sort_by_key(|r| r.observed_time_unix_nano);
        self.recorder.record_ingest(tenant, count);
        Ok(IngestReceipt { count })
    }

    fn query(&self, tenant: &TenantId, range: TimeRange) -> Result<Vec<LogRecord>, LogStoreError> {
        let state = self.state.lock().expect("poisoned");
        let bucket = match state.per_tenant.get(tenant) {
            Some(b) => b,
            None => {
                self.recorder.record_query(tenant, 0);
                return Ok(Vec::new());
            }
        };
        let matches: Vec<LogRecord> = bucket
            .iter()
            .filter(|r| range.contains(r.observed_time_unix_nano))
            .cloned()
            .collect();
        self.recorder.record_query(tenant, matches.len());
        Ok(matches)
    }

    fn query_with(
        &self,
        tenant: &TenantId,
        range: TimeRange,
        predicate: &Predicate,
    ) -> Result<Vec<LogRecord>, LogStoreError> {
        let state = self.state.lock().expect("poisoned");
        let bucket = match state.per_tenant.get(tenant) {
            Some(b) => b,
            None => {
                self.recorder.record_query(tenant, 0);
                return Ok(Vec::new());
            }
        };
        let matches: Vec<LogRecord> = bucket
            .iter()
            .filter(|r| range.contains(r.observed_time_unix_nano) && predicate.matches(r))
            .cloned()
            .collect();
        self.recorder.record_query(tenant, matches.len());
        Ok(matches)
    }
}
