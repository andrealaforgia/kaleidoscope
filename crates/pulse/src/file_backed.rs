// Kaleidoscope Pulse — file-backed durable MetricStore adapter (v1)
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

//! Pulse v1 — `FileBackedMetricStore`.
//!
//! Fourth v1 adapter on the platform plane after Cinder v1,
//! Sluice v1, and Lumen v1. Same shape: NDJSON WAL append per
//! ingest, JSON snapshot on `snapshot()`, recovery loads
//! snapshot then replays WAL on top. The trait carry-forward
//! is verbatim — no `MetricStore` method changes.
//!
//! Storage layout (given `<base>`):
//!
//! - `<base>.wal` — NDJSON ingest records, one line per
//!   `MetricBatch`
//! - `<base>.snapshot` — JSON dump of the per-(tenant, metric)
//!   series, written on `snapshot()`, truncates the WAL

use std::collections::HashMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use aegis::TenantId;
use serde::{Deserialize, Serialize};

use crate::metric::{Metric, MetricBatch, MetricName, MetricPoint, TimeRange};
use crate::metrics::MetricsRecorder;
use crate::predicate::Predicate;
use crate::store::{IngestReceipt, MetricStore, MetricStoreError};

// --------------------------------------------------------------------
// WAL + snapshot records
// --------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum WalRecord {
    Ingest {
        tenant: TenantId,
        batch: MetricBatch,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    series: Vec<SerialisedSeries>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SerialisedSeries {
    tenant: TenantId,
    metric: Metric, // metric.points stays empty here; points live below
    points: Vec<MetricPoint>,
}

// --------------------------------------------------------------------
// Adapter
// --------------------------------------------------------------------

pub struct FileBackedMetricStore {
    base_path: PathBuf,
    recorder: Box<dyn MetricsRecorder + Send + Sync>,
    state: Mutex<Inner>,
}

struct Inner {
    /// Indexed by `(tenant, metric_name)`. The canonical
    /// `Metric` keeps its `points` field empty; points live in
    /// the parallel `points` field of the series entry so the
    /// snapshot can serialise them separately.
    series: HashMap<(TenantId, MetricName), SeriesEntry>,
    wal: BufWriter<File>,
}

struct SeriesEntry {
    metric: Metric,
    points: Vec<MetricPoint>,
}

impl FileBackedMetricStore {
    /// Open or create a `FileBackedMetricStore` rooted at
    /// `base_path`. Loads the snapshot if present, then
    /// replays the WAL on top. Each series's point vector is
    /// re-sorted on `time_unix_nano` after recovery to preserve
    /// the v0 query-ordering contract.
    pub fn open<P: AsRef<Path>>(
        base_path: P,
        recorder: Box<dyn MetricsRecorder + Send + Sync>,
    ) -> Result<Self, MetricStoreError> {
        let base_path = base_path.as_ref().to_path_buf();
        let snapshot_path = snapshot_path_of(&base_path);
        let wal_path = wal_path_of(&base_path);

        let mut series: HashMap<(TenantId, MetricName), SeriesEntry> = HashMap::new();

        if snapshot_path.exists() {
            let f = File::open(&snapshot_path).map_err(io)?;
            let snap: Snapshot = serde_json::from_reader(f).map_err(parse)?;
            for s in snap.series {
                let key = (s.tenant.clone(), s.metric.name.clone());
                series.insert(
                    key,
                    SeriesEntry {
                        metric: s.metric,
                        points: s.points,
                    },
                );
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
                    MetricStoreError::PersistenceFailed {
                        reason: format!("WAL parse error at line {}: {e}", idx + 1),
                    }
                })?;
                match record {
                    WalRecord::Ingest { tenant, batch } => {
                        apply_ingest(&mut series, &tenant, batch);
                    }
                }
            }
        }

        for entry in series.values_mut() {
            entry.points.sort_by_key(|p| p.time_unix_nano);
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
            state: Mutex::new(Inner { series, wal }),
        })
    }

    /// Write current state to a snapshot file and truncate the
    /// WAL. Whole-store operation, not per-tenant.
    pub fn snapshot(&self) -> Result<(), MetricStoreError> {
        let mut state = self.state.lock().expect("poisoned");
        let snapshot_path = snapshot_path_of(&self.base_path);
        let wal_path = wal_path_of(&self.base_path);

        let series: Vec<SerialisedSeries> = state
            .series
            .iter()
            .map(|((tenant, _name), entry)| SerialisedSeries {
                tenant: tenant.clone(),
                metric: entry.metric.clone(),
                points: entry.points.clone(),
            })
            .collect();
        let snap = Snapshot { series };

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

impl fmt::Debug for FileBackedMetricStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileBackedMetricStore")
            .field("base_path", &self.base_path)
            .field("recorder", &"<opaque>")
            .finish()
    }
}

impl MetricStore for FileBackedMetricStore {
    fn ingest(
        &self,
        tenant: &TenantId,
        batch: MetricBatch,
    ) -> Result<IngestReceipt, MetricStoreError> {
        let count: usize = batch.metrics.iter().map(|m| m.points.len()).sum();
        if count == 0 {
            // Mirror the InMemoryMetricStore contract: record an
            // ingest event even for a no-op batch so observers
            // can see that the call happened.
            self.recorder.record_ingest(tenant, 0);
            return Ok(IngestReceipt { count: 0 });
        }
        let record = WalRecord::Ingest {
            tenant: tenant.clone(),
            batch: batch.clone(),
        };
        let mut state = self.state.lock().expect("poisoned");
        append_wal(&mut state.wal, &record)?;
        apply_ingest(&mut state.series, tenant, batch);
        for entry in state.series.values_mut() {
            entry.points.sort_by_key(|p| p.time_unix_nano);
        }
        self.recorder.record_ingest(tenant, count);
        Ok(IngestReceipt { count })
    }

    fn query(
        &self,
        tenant: &TenantId,
        metric_name: &MetricName,
        range: TimeRange,
    ) -> Result<Vec<(Metric, MetricPoint)>, MetricStoreError> {
        let state = self.state.lock().expect("poisoned");
        let key = (tenant.clone(), metric_name.clone());
        let entry = match state.series.get(&key) {
            Some(e) => e,
            None => {
                self.recorder.record_query(tenant, 0);
                return Ok(Vec::new());
            }
        };
        let matches: Vec<(Metric, MetricPoint)> = entry
            .points
            .iter()
            .filter(|p| range.contains(p.time_unix_nano))
            .cloned()
            .map(|p| (entry.metric.clone(), p))
            .collect();
        self.recorder.record_query(tenant, matches.len());
        Ok(matches)
    }

    fn query_with(
        &self,
        tenant: &TenantId,
        metric_name: &MetricName,
        range: TimeRange,
        predicate: &Predicate,
    ) -> Result<Vec<(Metric, MetricPoint)>, MetricStoreError> {
        let state = self.state.lock().expect("poisoned");
        let key = (tenant.clone(), metric_name.clone());
        let entry = match state.series.get(&key) {
            Some(e) => e,
            None => {
                self.recorder.record_query(tenant, 0);
                return Ok(Vec::new());
            }
        };
        let matches: Vec<(Metric, MetricPoint)> = entry
            .points
            .iter()
            .filter(|p| range.contains(p.time_unix_nano) && predicate.matches(&entry.metric, p))
            .cloned()
            .map(|p| (entry.metric.clone(), p))
            .collect();
        self.recorder.record_query(tenant, matches.len());
        Ok(matches)
    }
}

// --------------------------------------------------------------------
// helpers
// --------------------------------------------------------------------

fn apply_ingest(
    series: &mut HashMap<(TenantId, MetricName), SeriesEntry>,
    tenant: &TenantId,
    batch: MetricBatch,
) {
    for mut metric in batch.metrics {
        let key = (tenant.clone(), metric.name.clone());
        let points = std::mem::take(&mut metric.points);
        let entry = series.entry(key).or_insert_with(|| SeriesEntry {
            metric: Metric {
                points: Vec::new(),
                ..metric.clone()
            },
            points: Vec::new(),
        });
        entry.metric.description = metric.description;
        entry.metric.unit = metric.unit;
        entry.metric.kind = metric.kind;
        entry.metric.resource_attributes = metric.resource_attributes;
        entry.points.extend(points);
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

fn io(e: std::io::Error) -> MetricStoreError {
    MetricStoreError::PersistenceFailed {
        reason: format!("io: {e}"),
    }
}

fn parse(e: serde_json::Error) -> MetricStoreError {
    MetricStoreError::PersistenceFailed {
        reason: format!("parse: {e}"),
    }
}

fn append_wal(wal: &mut BufWriter<File>, record: &WalRecord) -> Result<(), MetricStoreError> {
    let line = serde_json::to_string(record).map_err(parse)?;
    wal.write_all(line.as_bytes()).map_err(io)?;
    wal.write_all(b"\n").map_err(io)?;
    wal.flush().map_err(io)?;
    Ok(())
}
