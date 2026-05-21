// Kaleidoscope Ray — file-backed durable TraceStore adapter (v1)
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

//! Ray v1 — `FileBackedTraceStore`.
//!
//! Fifth v1 adapter in the platform plane after Cinder v1, Sluice
//! v1, Lumen v1, and Pulse v1. The WAL+snapshot pattern is settled;
//! Ray's wrinkle is its dual index. The v0 in-memory adapter keeps
//! `by_trace` keyed on `(tenant, trace_id)` and `by_service` keyed
//! on `(tenant, service_name)`, cloning each span into both (a span
//! with no `service.name` goes only into `by_trace`). v1 persists
//! the spans ONCE (the `by_trace` buckets) and rebuilds the
//! `by_service` index on recovery. The single `apply_ingest`
//! routine inserts into both maps and is called by the live ingest
//! path AND by WAL replay, so the two indices cannot drift.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use aegis::TenantId;
use serde::{Deserialize, Serialize};

use crate::metrics::MetricsRecorder;
use crate::predicate::Predicate;
use crate::span::{ServiceName, Span, SpanBatch, TimeRange, TraceId};
use crate::store::{IngestReceipt, TraceStore, TraceStoreError};

// --------------------------------------------------------------------
// WAL record + snapshot shapes
// --------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum WalRecord {
    Ingest { tenant: TenantId, spans: Vec<Span> },
}

#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    /// One bucket per `(tenant, trace_id)`. The by_service index is
    /// NOT persisted; it is rebuilt from these spans on recovery.
    traces: Vec<TraceBucket>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TraceBucket {
    tenant: TenantId,
    trace_id: TraceId,
    spans: Vec<Span>,
}

// --------------------------------------------------------------------
// Adapter
// --------------------------------------------------------------------

/// Durable file-backed `TraceStore` adapter. Implements the v0
/// trait verbatim.
pub struct FileBackedTraceStore {
    base_path: PathBuf,
    recorder: Box<dyn MetricsRecorder + Send + Sync>,
    state: Mutex<Inner>,
}

#[derive(Default)]
struct Indices {
    by_trace: HashMap<(TenantId, TraceId), Vec<Span>>,
    by_service: HashMap<(TenantId, ServiceName), Vec<Span>>,
}

struct Inner {
    indices: Indices,
    wal: BufWriter<File>,
}

impl FileBackedTraceStore {
    /// Open or create a `FileBackedTraceStore` rooted at
    /// `base_path`. Loads the snapshot if present then replays the
    /// WAL on top, rebuilding BOTH indices via `apply_ingest`. Each
    /// bucket is re-sorted on `start_time_unix_nano` after recovery
    /// to preserve the v0 query-ordering contract.
    pub fn open<P: AsRef<Path>>(
        base_path: P,
        recorder: Box<dyn MetricsRecorder + Send + Sync>,
    ) -> Result<Self, TraceStoreError> {
        let base_path = base_path.as_ref().to_path_buf();
        let snapshot_path = snapshot_path_of(&base_path);
        let wal_path = wal_path_of(&base_path);

        let mut indices = Indices::default();

        if snapshot_path.exists() {
            let f = File::open(&snapshot_path).map_err(io)?;
            let snap: Snapshot = serde_json::from_reader(f).map_err(parse)?;
            for bucket in snap.traces {
                // Rebuild both indices from the persisted spans.
                apply_ingest(&mut indices, &bucket.tenant, bucket.spans);
            }
        }

        if wal_path.exists() {
            let f = File::open(&wal_path).map_err(io)?;
            let reader = BufReader::new(f);
            for (idx, line) in reader.lines().enumerate() {
                let line = line.map_err(io)?;
                if line.is_empty() {
                    continue;
                }
                let record: WalRecord = serde_json::from_str(&line).map_err(|e| {
                    TraceStoreError::PersistenceFailed {
                        reason: format!("WAL parse error at line {}: {e}", idx + 1),
                    }
                })?;
                match record {
                    WalRecord::Ingest { tenant, spans } => {
                        apply_ingest(&mut indices, &tenant, spans);
                    }
                }
            }
        }

        sort_all(&mut indices);

        let wal_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&wal_path)
            .map_err(io)?;
        let wal = BufWriter::new(wal_file);

        Ok(Self {
            base_path,
            recorder,
            state: Mutex::new(Inner { indices, wal }),
        })
    }

    /// Write current state to a snapshot file and truncate the WAL.
    /// Only the `by_trace` buckets are persisted; `by_service` is
    /// derived on the next open.
    pub fn snapshot(&self) -> Result<(), TraceStoreError> {
        let mut state = self.state.lock().expect("poisoned");
        let snapshot_path = snapshot_path_of(&self.base_path);
        let wal_path = wal_path_of(&self.base_path);

        let traces: Vec<TraceBucket> = state
            .indices
            .by_trace
            .iter()
            .map(|((tenant, trace_id), spans)| TraceBucket {
                tenant: tenant.clone(),
                trace_id: *trace_id,
                spans: spans.clone(),
            })
            .collect();
        let snap = Snapshot { traces };

        state.wal.flush().map_err(io)?;

        let f = File::create(&snapshot_path).map_err(io)?;
        let mut writer = BufWriter::new(f);
        serde_json::to_writer(&mut writer, &snap).map_err(parse)?;
        writer.flush().map_err(io)?;
        drop(writer);

        let wal_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&wal_path)
            .map_err(io)?;
        state.wal = BufWriter::new(wal_file);

        Ok(())
    }
}

impl fmt::Debug for FileBackedTraceStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileBackedTraceStore")
            .field("base_path", &self.base_path)
            .field("recorder", &"<opaque>")
            .finish()
    }
}

impl TraceStore for FileBackedTraceStore {
    fn ingest(
        &self,
        tenant: &TenantId,
        batch: SpanBatch,
    ) -> Result<IngestReceipt, TraceStoreError> {
        if batch.is_empty() {
            self.recorder.record_ingest(tenant, 0);
            return Ok(IngestReceipt { count: 0 });
        }
        let count = batch.spans.len();
        let record = WalRecord::Ingest {
            tenant: tenant.clone(),
            spans: batch.spans.clone(),
        };
        let mut state = self.state.lock().expect("poisoned");
        append_wal(&mut state.wal, &record)?;
        let touched = apply_ingest(&mut state.indices, tenant, batch.spans);
        sort_touched(&mut state.indices, tenant, touched);
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
        let spans = state
            .indices
            .by_trace
            .get(&key)
            .cloned()
            .unwrap_or_default();
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
        let bucket = match state.indices.by_service.get(&key) {
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
        let bucket = match state.indices.by_service.get(&key) {
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

// --------------------------------------------------------------------
// helpers
// --------------------------------------------------------------------

/// The buckets a single `apply_ingest` call touched, so the live
/// ingest path can sort exactly those (and no others). Tenant is
/// fixed per call so only the trace-id and service halves vary.
#[derive(Default)]
struct Touched {
    traces: HashSet<TraceId>,
    services: HashSet<ServiceName>,
}

/// Inserts each span into BOTH the by_trace and by_service indices
/// (a span with an empty `service.name` goes only into by_trace).
/// Shared by the live ingest path and WAL/snapshot recovery so the
/// two indices cannot drift — this is the no-drift guarantee that
/// ray-v1's mutation gate enforces. Mirrors the v0
/// InMemoryTraceStore::ingest fan-out. Returns the touched buckets
/// so the caller can sort exactly those; recovery ignores the
/// return and sorts everything once via sort_all.
fn apply_ingest(indices: &mut Indices, tenant: &TenantId, spans: Vec<Span>) -> Touched {
    let mut touched = Touched::default();
    for span in spans {
        let trace_key = (tenant.clone(), span.trace_id);
        touched.traces.insert(span.trace_id);
        indices
            .by_trace
            .entry(trace_key)
            .or_default()
            .push(span.clone());

        if !span.service_name().is_empty() {
            let service = ServiceName::new(span.service_name());
            touched.services.insert(service.clone());
            let service_key = (tenant.clone(), service);
            indices
                .by_service
                .entry(service_key)
                .or_default()
                .push(span);
        }
    }
    touched
}

/// Sorts only the buckets `apply_ingest` touched on this batch, on
/// `start_time_unix_nano`. O(touched) rather than O(all buckets) —
/// the v0 InMemoryTraceStore does the same to keep ingest off the
/// quadratic path.
fn sort_touched(indices: &mut Indices, tenant: &TenantId, touched: Touched) {
    for trace_id in touched.traces {
        if let Some(b) = indices.by_trace.get_mut(&(tenant.clone(), trace_id)) {
            b.sort_by_key(|s| s.start_time_unix_nano);
        }
    }
    for service in touched.services {
        if let Some(b) = indices.by_service.get_mut(&(tenant.clone(), service)) {
            b.sort_by_key(|s| s.start_time_unix_nano);
        }
    }
}

/// Sorts every bucket in both indices on `start_time_unix_nano`,
/// preserving the v0 ascending-time query contract. Used once on
/// recovery (the WAL/snapshot replay order is not guaranteed
/// sorted across batches).
fn sort_all(indices: &mut Indices) {
    for bucket in indices.by_trace.values_mut() {
        bucket.sort_by_key(|s| s.start_time_unix_nano);
    }
    for bucket in indices.by_service.values_mut() {
        bucket.sort_by_key(|s| s.start_time_unix_nano);
    }
}

fn wal_path_of(base: &Path) -> PathBuf {
    let mut p = base.as_os_str().to_owned();
    p.push(".wal");
    PathBuf::from(p)
}

fn snapshot_path_of(base: &Path) -> PathBuf {
    let mut p = base.as_os_str().to_owned();
    p.push(".snapshot");
    PathBuf::from(p)
}

fn io(e: std::io::Error) -> TraceStoreError {
    TraceStoreError::PersistenceFailed {
        reason: format!("io: {e}"),
    }
}

fn parse(e: serde_json::Error) -> TraceStoreError {
    TraceStoreError::PersistenceFailed {
        reason: format!("parse: {e}"),
    }
}

fn append_wal(wal: &mut BufWriter<File>, record: &WalRecord) -> Result<(), TraceStoreError> {
    let line = serde_json::to_string(record).map_err(parse)?;
    wal.write_all(line.as_bytes()).map_err(io)?;
    wal.write_all(b"\n").map_err(io)?;
    wal.flush().map_err(io)?;
    Ok(())
}

// --------------------------------------------------------------------
// Inline white-box tests.
//
// The acceptance suites under tests/v1_slice_0{1,2}_*.rs cover the
// trait ingest + get_trace + WAL/snapshot recovery paths, all of
// which reopen the store before querying (so recovery's sort_all
// masks any missing live-path sort). These inline tests close the
// mutation-coverage gaps that leaves: the predicate query path
// (query_with), the Debug impl, and the live-ingest sort_touched
// (queried WITHOUT a reopen so sort_all cannot mask it). They
// discharge cargo mutants at 100% on crates/ray/src/file_backed.rs.
// --------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::NoopRecorder;
    use crate::predicate::Predicate;
    use crate::span::{SpanId, SpanKind, SpanStatus};
    use std::collections::BTreeMap;

    fn temp_base(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!("ray-fb-unit-{name}-{}-{nanos}", std::process::id()));
        p
    }

    fn span(trace: u8, span_byte: u8, service: &str, name: &str, start: u64) -> Span {
        let mut resource = BTreeMap::new();
        resource.insert("service.name".to_string(), service.to_string());
        Span {
            trace_id: TraceId([trace; 16]),
            span_id: SpanId([span_byte; 8]),
            parent_span_id: None,
            name: name.to_string(),
            kind: SpanKind::Server,
            start_time_unix_nano: start,
            end_time_unix_nano: start + 10,
            status: SpanStatus::default(),
            attributes: BTreeMap::new(),
            resource_attributes: resource,
            events: Vec::new(),
            links: Vec::new(),
        }
    }

    fn cleanup(base: &Path) {
        let _ = std::fs::remove_file(wal_path_of(base));
        let _ = std::fs::remove_file(snapshot_path_of(base));
    }

    // Kills query_with -> Ok(vec![]) and the && -> || mutant: the
    // predicate must filter, and range AND predicate must both apply.
    #[test]
    fn query_with_applies_range_and_predicate_conjunction() {
        let base = temp_base("query_with");
        let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open");
        let t = TenantId("acme".to_string());
        store
            .ingest(
                &t,
                SpanBatch::with_spans(vec![
                    span(0xAA, 0x01, "checkout", "alpha", 100),
                    span(0xAA, 0x02, "checkout", "beta", 200),
                ]),
            )
            .expect("ingest");

        // Predicate matches span_name=alpha; range [0,300) covers both.
        let by_name = store
            .query_with(
                &t,
                &ServiceName::new("checkout"),
                TimeRange::new(0, 300),
                &Predicate::new().span_name("alpha"),
            )
            .expect("query_with");
        assert_eq!(by_name.len(), 1, "predicate narrows to one span");
        assert_eq!(by_name[0].name, "alpha");

        // Range [0,150) excludes beta; predicate matches beta. The
        // conjunction yields zero; if && were ||, beta would appear.
        let conj = store
            .query_with(
                &t,
                &ServiceName::new("checkout"),
                TimeRange::new(0, 150),
                &Predicate::new().span_name("beta"),
            )
            .expect("query_with");
        assert!(conj.is_empty(), "range AND predicate excludes beta");
        cleanup(&base);
    }

    // Kills the Debug::fmt -> Ok(Default::default()) mutant.
    #[test]
    fn debug_impl_names_the_struct() {
        let base = temp_base("debug");
        let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open");
        let rendered = format!("{store:?}");
        assert!(
            rendered.contains("FileBackedTraceStore"),
            "Debug names the struct; got {rendered}"
        );
        cleanup(&base);
    }

    // Kills the sort_touched -> () mutant. The acceptance suite always
    // reopens before querying, so sort_all masks a missing live sort.
    // Here we ingest out-of-order spans and query in the SAME process
    // (no reopen) via BOTH indices, so only sort_touched can produce
    // the sorted result.
    #[test]
    fn live_ingest_sorts_touched_buckets_without_reopen() {
        let base = temp_base("sort_touched");
        let store = FileBackedTraceStore::open(&base, Box::new(NoopRecorder)).expect("open");
        let t = TenantId("acme".to_string());
        // Out-of-order within a single batch.
        store
            .ingest(
                &t,
                SpanBatch::with_spans(vec![
                    span(0xCC, 0x03, "gateway", "third", 300),
                    span(0xCC, 0x01, "gateway", "first", 100),
                    span(0xCC, 0x02, "gateway", "second", 200),
                ]),
            )
            .expect("ingest");

        // No reopen. by_trace must be sorted by live sort_touched.
        let by_trace = store
            .get_trace(&t, &TraceId([0xCC; 16]))
            .expect("get_trace");
        let trace_names: Vec<&str> = by_trace.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(trace_names, vec!["first", "second", "third"]);

        // by_service must be sorted too (the other half of sort_touched).
        let by_service = store
            .query(&t, &ServiceName::new("gateway"), TimeRange::all())
            .expect("query");
        let svc_names: Vec<&str> = by_service.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(svc_names, vec!["first", "second", "third"]);
        cleanup(&base);
    }
}
