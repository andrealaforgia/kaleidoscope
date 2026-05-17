// Kaleidoscope CLI — library
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

//! # Kaleidoscope CLI library
//!
//! The CLI is split into a thin binary (`src/main.rs`) and this
//! library (`src/lib.rs`). The binary parses arguments and
//! dispatches to two operations; the library owns the actual
//! work and is exercised by tests that pipe controlled
//! readers/writers through it.
//!
//! ## Operations
//!
//! - [`ingest`] reads NDJSON `lumen::LogRecord` from a reader,
//!   batches them, ingests into a `FileBackedLogStore`, and
//!   places one `Cinder` tier entry per batch under the Hot
//!   tier. The Lumen `MetricsRecorder` is the
//!   `self_observe::LumenToPulseRecorder`, so the platform
//!   observes its own ingest activity via Pulse.
//! - [`read`] queries every record for the tenant from the
//!   Lumen store and writes them back as NDJSON to a writer.
//!
//! ## Storage layout
//!
//! Given `--data-dir <dir>`:
//!
//! - `<dir>/lumen.*` — Lumen v1 WAL + snapshot
//! - `<dir>/cinder.*` — Cinder v1 WAL + snapshot
//!
//! Both adapters survive process restarts. A second invocation
//! of `read` reads back data written by an earlier invocation
//! of `ingest`.

#![forbid(unsafe_code)]

use std::fmt;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use aegis::TenantId;
use cinder::{
    FileBackedTieringStore, ItemId, MigrateError, NoopRecorder as CinderRecorder, Tier,
    TieringStore,
};
use lumen::{
    FileBackedLogStore, LogBatch, LogRecord, LogStore, LogStoreError, MetricsRecorder as LumenRec,
    Predicate, SeverityNumber, TimeRange,
};
use pulse::{InMemoryMetricStore, MetricStore, NoopRecorder as PulseRecorder};
use self_observe::{LumenToOtlpJsonWriter, LumenToPulseRecorder};

/// Configurable batch flush size. Smaller for tests; larger for
/// production. The default chosen here matches the KPI batch
/// shape used in Lumen v1's acceptance suite.
pub const DEFAULT_BATCH_SIZE: usize = 100;

#[derive(Debug)]
pub enum Error {
    LumenOpen(LogStoreError),
    LumenIngest(LogStoreError),
    LumenQuery(LogStoreError),
    LumenSnapshot(LogStoreError),
    CinderOpen(MigrateError),
    CinderSnapshot(MigrateError),
    Io(std::io::Error),
    ParseRecord {
        line: usize,
        source: serde_json::Error,
    },
    SerialiseRecord(serde_json::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::LumenOpen(e) => write!(f, "lumen open: {e}"),
            Error::LumenIngest(e) => write!(f, "lumen ingest: {e}"),
            Error::LumenQuery(e) => write!(f, "lumen query: {e}"),
            Error::LumenSnapshot(e) => write!(f, "lumen snapshot: {e}"),
            Error::CinderOpen(e) => write!(f, "cinder open: {e}"),
            Error::CinderSnapshot(e) => write!(f, "cinder snapshot: {e}"),
            Error::Io(e) => write!(f, "io: {e}"),
            Error::ParseRecord { line, source } => {
                write!(f, "parse record at line {line}: {source}")
            }
            Error::SerialiseRecord(e) => write!(f, "serialise record: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// Statistics emitted after a successful `ingest`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IngestStats {
    pub records_ingested: usize,
    pub batches_flushed: usize,
    pub tier_items_placed: usize,
}

fn lumen_base(data_dir: &Path) -> PathBuf {
    data_dir.join("lumen")
}

fn cinder_base(data_dir: &Path) -> PathBuf {
    data_dir.join("cinder")
}

/// Reads NDJSON `LogRecord` from `reader`, batches them in
/// groups of `batch_size`, ingests into Lumen, and places one
/// Cinder Hot-tier entry per batch.
///
/// The Lumen `MetricsRecorder` is wired to a fresh in-process
/// Pulse store via `LumenToPulseRecorder` so the binary's own
/// observability is available for inspection (currently
/// dropped at end of call). If `otlp_log_path` is `Some`, the
/// recorder is replaced by `LumenToOtlpJsonWriter` which
/// appends NDJSON OTLP-JSON metrics lines to that file. An
/// operator can then `tail -f <path>` to watch the metric
/// stream, or a sidecar process can read the file and forward
/// to a real OTLP/HTTP collector.
pub fn ingest(
    tenant: &TenantId,
    data_dir: &Path,
    batch_size: usize,
    reader: impl BufRead,
    otlp_log_path: Option<&Path>,
) -> Result<IngestStats, Error> {
    std::fs::create_dir_all(data_dir)?;
    let recorder: Box<dyn LumenRec + Send + Sync> = match otlp_log_path {
        Some(path) => {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            Box::new(LumenToOtlpJsonWriter::new(file))
        }
        None => {
            let pulse: Arc<dyn MetricStore + Send + Sync> =
                Arc::new(InMemoryMetricStore::new(Box::new(PulseRecorder)));
            Box::new(LumenToPulseRecorder::new(pulse))
        }
    };
    let lumen =
        FileBackedLogStore::open(lumen_base(data_dir), recorder).map_err(Error::LumenOpen)?;
    let cinder = FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))
        .map_err(Error::CinderOpen)?;

    let mut buffer: Vec<LogRecord> = Vec::with_capacity(batch_size);
    let mut records_ingested = 0usize;
    let mut batches_flushed = 0usize;
    let mut tier_items_placed = 0usize;

    for (idx, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let record: LogRecord = serde_json::from_str(&line).map_err(|e| Error::ParseRecord {
            line: idx + 1,
            source: e,
        })?;
        buffer.push(record);
        if buffer.len() >= batch_size {
            flush(
                tenant,
                &lumen,
                &cinder,
                &mut buffer,
                batches_flushed,
                &mut tier_items_placed,
                &mut records_ingested,
            )?;
            batches_flushed += 1;
        }
    }
    if !buffer.is_empty() {
        flush(
            tenant,
            &lumen,
            &cinder,
            &mut buffer,
            batches_flushed,
            &mut tier_items_placed,
            &mut records_ingested,
        )?;
        batches_flushed += 1;
    }

    Ok(IngestStats {
        records_ingested,
        batches_flushed,
        tier_items_placed,
    })
}

fn flush(
    tenant: &TenantId,
    lumen: &FileBackedLogStore,
    cinder: &FileBackedTieringStore,
    buffer: &mut Vec<LogRecord>,
    batch_seq: usize,
    tier_items_placed: &mut usize,
    records_ingested: &mut usize,
) -> Result<(), Error> {
    let count = buffer.len();
    let batch = LogBatch::with_records(std::mem::take(buffer));
    let receipt = lumen.ingest(tenant, batch).map_err(Error::LumenIngest)?;
    *records_ingested += receipt.count;
    let item = ItemId::new(format!("{}/batch-{:05}", tenant.0, batch_seq));
    cinder.place(tenant, &item, Tier::Hot, SystemTime::now());
    *tier_items_placed += 1;
    debug_assert_eq!(receipt.count, count);
    Ok(())
}

/// Queries every record for the tenant from Lumen and writes
/// them as NDJSON to `writer`, one record per line.
pub fn read(tenant: &TenantId, data_dir: &Path, writer: impl Write) -> Result<usize, Error> {
    read_filtered(tenant, data_dir, &Predicate::new(), writer)
}

/// Queries records for the tenant filtered by `predicate` and
/// writes them as NDJSON to `writer`. An empty predicate is
/// equivalent to [`read`]. The predicate composes with the
/// time range internally (currently `TimeRange::all()`).
pub fn read_filtered(
    tenant: &TenantId,
    data_dir: &Path,
    predicate: &Predicate,
    mut writer: impl Write,
) -> Result<usize, Error> {
    let pulse: Arc<dyn MetricStore + Send + Sync> =
        Arc::new(InMemoryMetricStore::new(Box::new(PulseRecorder)));
    let recorder = Box::new(LumenToPulseRecorder::new(pulse));
    let lumen =
        FileBackedLogStore::open(lumen_base(data_dir), recorder).map_err(Error::LumenOpen)?;
    let records = lumen
        .query_with(tenant, TimeRange::all(), predicate)
        .map_err(Error::LumenQuery)?;
    let count = records.len();
    for record in records {
        let line = serde_json::to_string(&record).map_err(Error::SerialiseRecord)?;
        writer.write_all(line.as_bytes())?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;
    Ok(count)
}

/// Parses a severity name (case-insensitive) into a Lumen
/// [`SeverityNumber`]. Accepts the six OTLP severity names:
/// `TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`, `FATAL`. Returns
/// `None` for any other input — the CLI maps `None` to a usage
/// error.
pub fn parse_severity(s: &str) -> Option<SeverityNumber> {
    match s.to_ascii_uppercase().as_str() {
        "TRACE" => Some(SeverityNumber::TRACE),
        "DEBUG" => Some(SeverityNumber::DEBUG),
        "INFO" => Some(SeverityNumber::INFO),
        "WARN" | "WARNING" => Some(SeverityNumber::WARN),
        "ERROR" => Some(SeverityNumber::ERROR),
        "FATAL" => Some(SeverityNumber::FATAL),
        _ => None,
    }
}

/// Statistics emitted after a successful `compact`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactStats {
    pub lumen_snapshotted: bool,
    pub cinder_snapshotted: bool,
}

/// Triggers `snapshot()` on the file-backed Lumen and Cinder
/// stores in `data_dir`. Each call writes the current state to
/// `{store}.snapshot` and truncates the corresponding WAL,
/// bounding the next `open()`'s replay time.
///
/// `compact` is a whole-store operation, not per-tenant. The
/// snapshot file captures every tenant's records at once.
///
/// Operators run this on a cadence (cron, timer) appropriate
/// for their write volume. The library exposes the API; the
/// CLI exposes the trigger; the operator chooses when.
pub fn compact(data_dir: &Path) -> Result<CompactStats, Error> {
    // Lumen does not need a real recorder for a snapshot-only
    // operation. The NoopRecorder via the in-process Pulse
    // bridge is the cheapest available wiring.
    let pulse: Arc<dyn MetricStore + Send + Sync> =
        Arc::new(InMemoryMetricStore::new(Box::new(PulseRecorder)));
    let recorder = Box::new(LumenToPulseRecorder::new(pulse));
    let lumen =
        FileBackedLogStore::open(lumen_base(data_dir), recorder).map_err(Error::LumenOpen)?;
    lumen.snapshot().map_err(Error::LumenSnapshot)?;

    let cinder = FileBackedTieringStore::open(cinder_base(data_dir), Box::new(CinderRecorder))
        .map_err(Error::CinderOpen)?;
    cinder.snapshot().map_err(Error::CinderSnapshot)?;

    Ok(CompactStats {
        lumen_snapshotted: true,
        cinder_snapshotted: true,
    })
}
