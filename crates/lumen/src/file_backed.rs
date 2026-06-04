// Kaleidoscope Lumen — file-backed durable LogStore adapter (v1)
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

//! Lumen v1 — `FileBackedLogStore`.
//!
//! Third v1 adapter anywhere in the platform plane after Cinder v1
//! and Sluice v1. Same shape, same trait carry-forward, same
//! additive error-variant cost. Pattern is now a settled property
//! of the methodology.

use std::collections::HashMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use wal_recovery::{FsyncBackend, RealFsyncBackend};

use aegis::TenantId;
use serde::{Deserialize, Serialize};

use crate::metrics::MetricsRecorder;
use crate::predicate::Predicate;
use crate::record::{LogBatch, LogRecord, TimeRange};
use crate::store::{IngestReceipt, LogStore, LogStoreError};

// --------------------------------------------------------------------
// WAL record
// --------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum WalRecord {
    Ingest {
        tenant: TenantId,
        records: Vec<LogRecord>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    per_tenant: Vec<TenantBucket>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TenantBucket {
    tenant: TenantId,
    records: Vec<LogRecord>,
}

// --------------------------------------------------------------------
// Adapter
// --------------------------------------------------------------------

/// Durable file-backed `LogStore` adapter. Implements the v0
/// trait verbatim.
pub struct FileBackedLogStore {
    base_path: PathBuf,
    recorder: Box<dyn MetricsRecorder + Send + Sync>,
    fsync_backend: Arc<dyn FsyncBackend + Send + Sync>,
    state: Mutex<Inner>,
}

struct Inner {
    per_tenant: HashMap<TenantId, Vec<LogRecord>>,
    wal: BufWriter<File>,
}

impl FileBackedLogStore {
    /// Open or create a `FileBackedLogStore` rooted at
    /// `base_path`. Loads the snapshot if present then replays
    /// the WAL on top. Records inside each tenant bucket are
    /// re-sorted on observed_time after recovery to preserve
    /// the v0 query-ordering contract.
    pub fn open<P: AsRef<Path>>(
        base_path: P,
        recorder: Box<dyn MetricsRecorder + Send + Sync>,
    ) -> Result<Self, LogStoreError> {
        // The production path uses the honest backend: per-record
        // `sync_all` on append and tmp+fsync+rename+fsync-dir on
        // snapshot. The wal-fsync acceptance suite injects a lying
        // substrate through `open_with_fsync_backend` (mechanism (b)).
        Self::open_with_fsync_backend(base_path, recorder, Arc::new(RealFsyncBackend))
    }

    /// Open with an explicit [`FsyncBackend`] (ADR-0060 §3). The public
    /// [`FileBackedLogStore::open`] delegates here with a
    /// [`RealFsyncBackend`]; the wal-fsync acceptance suite injects a
    /// `LyingFsyncBackend` to make the durability AC falsifiable
    /// in-suite (mechanism (b)). Inherent constructor, NOT a trait
    /// member — preserves the `LogStore` byte-identical surface (C1).
    pub fn open_with_fsync_backend<P: AsRef<Path>>(
        base_path: P,
        recorder: Box<dyn MetricsRecorder + Send + Sync>,
        fsync_backend: Arc<dyn FsyncBackend + Send + Sync>,
    ) -> Result<Self, LogStoreError> {
        let base_path = base_path.as_ref().to_path_buf();
        let snapshot_path = snapshot_path_of(&base_path);
        let wal_path = wal_path_of(&base_path);

        let mut per_tenant: HashMap<TenantId, Vec<LogRecord>> = HashMap::new();

        if snapshot_path.exists() {
            let f = File::open(&snapshot_path).map_err(io)?;
            let snap: Snapshot = serde_json::from_reader(f).map_err(parse)?;
            for b in snap.per_tenant {
                per_tenant.insert(b.tenant, b.records);
            }
        }

        if wal_path.exists() {
            let wal_bytes = std::fs::read(&wal_path).map_err(io)?;
            wal_recovery::replay_wal_tolerating_torn_tail::<WalRecord, LogStoreError>(
                &wal_bytes,
                "lumen",
                |record| {
                    let WalRecord::Ingest { tenant, records } = record;
                    per_tenant.entry(tenant).or_default().extend(records);
                    Ok(())
                },
                |line, error| LogStoreError::PersistenceFailed {
                    reason: format!("WAL parse error at line {line}: {error}"),
                },
            )?;
        }

        // Re-sort every tenant bucket so query ordering holds.
        for bucket in per_tenant.values_mut() {
            bucket.sort_by_key(|r| r.observed_time_unix_nano);
        }

        let wal_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&wal_path)
            .map_err(io)?;
        let wal = BufWriter::new(wal_file);

        Ok(Self {
            base_path,
            recorder,
            fsync_backend,
            state: Mutex::new(Inner { per_tenant, wal }),
        })
    }

    /// Write current state to a snapshot file and truncate the WAL.
    /// The snapshot is written atomically (ADR-0060 §2): serialise to a
    /// same-directory temp, fsync the temp, rename onto the canonical
    /// path, then fsync the parent directory — whole-or-absent at the
    /// canonical path across a crash at ANY point.
    pub fn snapshot(&self) -> Result<(), LogStoreError> {
        let mut state = self.state.lock().expect("poisoned");
        let snapshot_path = snapshot_path_of(&self.base_path);
        let wal_path = wal_path_of(&self.base_path);

        let buckets: Vec<TenantBucket> = state
            .per_tenant
            .iter()
            .map(|(tenant, records)| TenantBucket {
                tenant: tenant.clone(),
                records: records.clone(),
            })
            .collect();
        let snap = Snapshot {
            per_tenant: buckets,
        };

        state.wal.flush().map_err(io)?;

        wal_recovery::atomic_write_snapshot(
            &snapshot_path,
            self.fsync_backend.as_ref(),
            |writer| {
                serde_json::to_writer(&mut *writer, &snap)
                    .map_err(|e| std::io::Error::other(e.to_string()))
            },
        )
        .map_err(io)?;

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

impl fmt::Debug for FileBackedLogStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileBackedLogStore")
            .field("base_path", &self.base_path)
            .field("recorder", &"<opaque>")
            .finish()
    }
}

impl LogStore for FileBackedLogStore {
    fn ingest(&self, tenant: &TenantId, batch: LogBatch) -> Result<IngestReceipt, LogStoreError> {
        if batch.is_empty() {
            self.recorder.record_ingest(tenant, 0);
            return Ok(IngestReceipt { count: 0 });
        }
        let count = batch.records.len();
        let record = WalRecord::Ingest {
            tenant: tenant.clone(),
            records: batch.records.clone(),
        };
        let mut state = self.state.lock().expect("poisoned");
        append_wal(&mut state.wal, &record, self.fsync_backend.as_ref())?;
        let bucket = state.per_tenant.entry(tenant.clone()).or_default();
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

// --------------------------------------------------------------------
// helpers
// --------------------------------------------------------------------

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

fn io(e: std::io::Error) -> LogStoreError {
    LogStoreError::PersistenceFailed {
        reason: format!("io: {e}"),
    }
}

fn parse(e: serde_json::Error) -> LogStoreError {
    LogStoreError::PersistenceFailed {
        reason: format!("parse: {e}"),
    }
}

fn append_wal(
    wal: &mut BufWriter<File>,
    record: &WalRecord,
    fsync_backend: &(dyn FsyncBackend + Send + Sync),
) -> Result<(), LogStoreError> {
    let line = serde_json::to_string(record).map_err(parse)?;
    wal.write_all(line.as_bytes()).map_err(io)?;
    wal.write_all(b"\n").map_err(io)?;
    wal.flush().map_err(io)?;
    // sync_all per record (ADR-0049 §4 / ADR-0060 §3): flush only empties
    // the user-space buffer into the kernel page cache; fsync_file
    // instructs the kernel to put the bytes on stable storage so an acked
    // write survives a power cut.
    fsync_backend.fsync_file(wal.get_ref()).map_err(io)?;
    Ok(())
}
