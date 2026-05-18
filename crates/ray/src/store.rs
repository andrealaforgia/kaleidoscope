// Kaleidoscope Ray — TraceStore trait + in-memory adapter
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

//! `TraceStore` trait + in-memory adapter.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use aegis::TenantId;

use crate::metrics::MetricsRecorder;
use crate::predicate::Predicate;
use crate::span::{ServiceName, Span, SpanBatch, TimeRange, TraceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IngestReceipt {
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceStoreError {}

impl fmt::Display for TraceStoreError {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {}
    }
}

impl std::error::Error for TraceStoreError {}

/// The trace-store port. v0 ships [`InMemoryTraceStore`] as the
/// only adapter; the v1 columnar (trace_id-partitioned
/// Iceberg-on-Parquet) adapter lands behind this trait.
///
/// Semantics:
///
/// - **Per-tenant isolation.**
/// - **`get_trace` returns the full trace** in
///   `start_time_unix_nano` ascending order.
/// - **`query` returns spans for a service in a time range**,
///   also start-time ascending.
/// - **Half-open time range.** `[start, end)`.
pub trait TraceStore {
    fn ingest(&self, tenant: &TenantId, batch: SpanBatch)
        -> Result<IngestReceipt, TraceStoreError>;

    /// Return every span sharing `trace_id` for this tenant.
    /// Empty trace returns `Ok(Vec::new())`.
    fn get_trace(
        &self,
        tenant: &TenantId,
        trace_id: &TraceId,
    ) -> Result<Vec<Span>, TraceStoreError>;

    /// Return every span belonging to `service` whose
    /// `start_time_unix_nano` falls within `range`.
    fn query(
        &self,
        tenant: &TenantId,
        service: &ServiceName,
        range: TimeRange,
    ) -> Result<Vec<Span>, TraceStoreError>;

    /// Query with a predicate. `range AND predicate`.
    fn query_with(
        &self,
        tenant: &TenantId,
        service: &ServiceName,
        range: TimeRange,
        predicate: &Predicate,
    ) -> Result<Vec<Span>, TraceStoreError>;
}

/// v0 in-process adapter. Dual index:
/// `HashMap<(TenantId, TraceId), Vec<Span>>` for `get_trace`,
/// `HashMap<(TenantId, ServiceName), Vec<Span>>` for service +
/// range query. Spans are cloned on ingest into both maps —
/// O(1) lookup, 2× memory cost. v1's columnar adapter will
/// merge these into a single trace-id-partitioned layout.
pub struct InMemoryTraceStore {
    recorder: Box<dyn MetricsRecorder + Send + Sync>,
    state: Mutex<InnerState>,
}

#[derive(Default)]
struct InnerState {
    by_trace: HashMap<(TenantId, TraceId), Vec<Span>>,
    by_service: HashMap<(TenantId, ServiceName), Vec<Span>>,
}

impl InMemoryTraceStore {
    pub fn new(recorder: Box<dyn MetricsRecorder + Send + Sync>) -> Self {
        Self {
            recorder,
            state: Mutex::new(InnerState::default()),
        }
    }
}

impl fmt::Debug for InMemoryTraceStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryTraceStore")
            .field("recorder", &"<opaque>")
            .finish()
    }
}

impl TraceStore for InMemoryTraceStore {
    fn ingest(
        &self,
        tenant: &TenantId,
        batch: SpanBatch,
    ) -> Result<IngestReceipt, TraceStoreError> {
        use std::collections::HashSet;

        let mut state = self.state.lock().expect("poisoned");
        let count = batch.spans.len();
        // Track which buckets we touched so we can sort each
        // exactly once at the end of the batch.
        let mut touched_traces: HashSet<TraceId> = HashSet::new();
        let mut touched_services: HashSet<ServiceName> = HashSet::new();

        for span in batch.spans {
            let trace_key = (tenant.clone(), span.trace_id);
            let trace_bucket = state.by_trace.entry(trace_key).or_default();
            touched_traces.insert(span.trace_id);
            trace_bucket.push(span.clone());

            if !span.service_name().is_empty() {
                let service = ServiceName::new(span.service_name());
                let service_key = (tenant.clone(), service.clone());
                let svc_bucket = state.by_service.entry(service_key).or_default();
                touched_services.insert(service);
                svc_bucket.push(span);
            }
        }

        // One sort per touched bucket — O(N log N) per batch
        // rather than O(N²) of per-span sorting. Worst case sort
        // size is the existing bucket length plus this batch's
        // contribution; v0's adapter accepts that ceiling.
        for trace_id in touched_traces {
            let key = (tenant.clone(), trace_id);
            if let Some(b) = state.by_trace.get_mut(&key) {
                b.sort_by_key(|s| s.start_time_unix_nano);
            }
        }
        for service in touched_services {
            let key = (tenant.clone(), service);
            if let Some(b) = state.by_service.get_mut(&key) {
                b.sort_by_key(|s| s.start_time_unix_nano);
            }
        }

        self.recorder.record_ingest(tenant, count);
        Ok(IngestReceipt { count })
    }

    fn get_trace(
        &self,
        tenant: &TenantId,
        trace_id: &TraceId,
    ) -> Result<Vec<Span>, TraceStoreError> {
        let state = self.state.lock().expect("poisoned");
        let key = (tenant.clone(), *trace_id);
        let spans = state.by_trace.get(&key).cloned().unwrap_or_default();
        self.recorder.record_query(tenant, spans.len());
        Ok(spans)
    }

    fn query(
        &self,
        tenant: &TenantId,
        service: &ServiceName,
        range: TimeRange,
    ) -> Result<Vec<Span>, TraceStoreError> {
        let state = self.state.lock().expect("poisoned");
        let key = (tenant.clone(), service.clone());
        let bucket = match state.by_service.get(&key) {
            Some(b) => b,
            None => {
                self.recorder.record_query(tenant, 0);
                return Ok(Vec::new());
            }
        };
        let matches: Vec<Span> = bucket
            .iter()
            .filter(|s| range.contains(s.start_time_unix_nano))
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
    ) -> Result<Vec<Span>, TraceStoreError> {
        let state = self.state.lock().expect("poisoned");
        let key = (tenant.clone(), service.clone());
        let bucket = match state.by_service.get(&key) {
            Some(b) => b,
            None => {
                self.recorder.record_query(tenant, 0);
                return Ok(Vec::new());
            }
        };
        let matches: Vec<Span> = bucket
            .iter()
            .filter(|s| range.contains(s.start_time_unix_nano) && predicate.matches(s))
            .cloned()
            .collect();
        self.recorder.record_query(tenant, matches.len());
        Ok(matches)
    }
}
