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
//! Fifth v1 adapter in the platform plane after Cinder, Sluice,
//! Lumen, and Pulse. Same NDJSON WAL + JSON snapshot template,
//! same trait carry-forward.
//!
//! The on-disk shape stores spans canonically per
//! `(tenant, trace_id)`; the secondary `(tenant, service)`
//! index is rebuilt in-memory on `open` by iterating the
//! canonical buckets. That keeps the snapshot small (no
//! duplication) and the recovery cost modest (O(N) iteration).

use std::collections::HashMap;
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum WalRecord {
    Ingest { tenant: TenantId, spans: Vec<Span> },
}

#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    buckets: Vec<TraceBucket>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TraceBucket {
    tenant: TenantId,
    trace_id: TraceId,
    spans: Vec<Span>,
}

pub struct FileBackedTraceStore {
    base_path: PathBuf,
    recorder: Box<dyn MetricsRecorder + Send + Sync>,
    state: Mutex<Inner>,
}

struct Inner {
    by_trace: HashMap<(TenantId, TraceId), Vec<Span>>,
    by_service: HashMap<(TenantId, ServiceName), Vec<Span>>,
    wal: BufWriter<File>,
}

impl FileBackedTraceStore {
    pub fn open<P: AsRef<Path>>(
        base_path: P,
        recorder: Box<dyn MetricsRecorder + Send + Sync>,
    ) -> Result<Self, TraceStoreError> {
        let base_path = base_path.as_ref().to_path_buf();
        let snapshot_path = snapshot_path_of(&base_path);
        let wal_path = wal_path_of(&base_path);

        let mut by_trace: HashMap<(TenantId, TraceId), Vec<Span>> = HashMap::new();

        if snapshot_path.exists() {
            let f = File::open(&snapshot_path).map_err(io)?;
            let snap: Snapshot = serde_json::from_reader(f).map_err(parse)?;
            for b in snap.buckets {
                by_trace.insert((b.tenant, b.trace_id), b.spans);
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
                        for span in spans {
                            let key = (tenant.clone(), span.trace_id);
                            by_trace.entry(key).or_default().push(span);
                        }
                    }
                }
            }
        }

        // Re-sort canonical buckets, then rebuild the service
        // index by iterating them. Same sort order as the
        // in-memory adapter (`start_time_unix_nano`).
        for bucket in by_trace.values_mut() {
            bucket.sort_by_key(|s| s.start_time_unix_nano);
        }
        let by_service = rebuild_service_index(&by_trace);

        let wal_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&wal_path)
            .map_err(io)?;
        let wal = BufWriter::new(wal_file);

        Ok(Self {
            base_path,
            recorder,
            state: Mutex::new(Inner {
                by_trace,
                by_service,
                wal,
            }),
        })
    }

    pub fn snapshot(&self) -> Result<(), TraceStoreError> {
        let mut state = self.state.lock().expect("poisoned");
        let snapshot_path = snapshot_path_of(&self.base_path);
        let wal_path = wal_path_of(&self.base_path);

        let buckets: Vec<TraceBucket> = state
            .by_trace
            .iter()
            .map(|((tenant, trace_id), spans)| TraceBucket {
                tenant: tenant.clone(),
                trace_id: *trace_id,
                spans: spans.clone(),
            })
            .collect();
        let snap = Snapshot { buckets };

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
        use std::collections::HashSet;

        if batch.spans.is_empty() {
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

        let mut touched_traces: HashSet<TraceId> = HashSet::new();
        let mut touched_services: HashSet<ServiceName> = HashSet::new();
        for span in batch.spans {
            let trace_key = (tenant.clone(), span.trace_id);
            touched_traces.insert(span.trace_id);
            state
                .by_trace
                .entry(trace_key)
                .or_default()
                .push(span.clone());
            if !span.service_name().is_empty() {
                let service = ServiceName::new(span.service_name());
                let service_key = (tenant.clone(), service.clone());
                touched_services.insert(service);
                state.by_service.entry(service_key).or_default().push(span);
            }
        }
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

fn rebuild_service_index(
    by_trace: &HashMap<(TenantId, TraceId), Vec<Span>>,
) -> HashMap<(TenantId, ServiceName), Vec<Span>> {
    let mut by_service: HashMap<(TenantId, ServiceName), Vec<Span>> = HashMap::new();
    for ((tenant, _trace_id), spans) in by_trace.iter() {
        for span in spans {
            if span.service_name().is_empty() {
                continue;
            }
            let key = (tenant.clone(), ServiceName::new(span.service_name()));
            by_service.entry(key).or_default().push(span.clone());
        }
    }
    for bucket in by_service.values_mut() {
        bucket.sort_by_key(|s| s.start_time_unix_nano);
    }
    by_service
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
