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
//! Fourth v1 adapter in the platform plane after Cinder v1, Sluice
//! v1, and Lumen v1. Same shape, same trait carry-forward, same
//! additive error-variant cost. The metrics model differs from logs:
//! Pulse indexes by `(tenant, metric_name)` series rather than by a
//! flat per-tenant record list, so the WAL replays through the same
//! split-into-series logic the in-memory adapter uses on ingest.

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
// WAL record + snapshot shapes
// --------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum WalRecord {
    Ingest {
        tenant: TenantId,
        metrics: Vec<Metric>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    series: Vec<SeriesBucket>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SeriesBucket {
    tenant: TenantId,
    /// Canonical metric metadata (its `points` field stays empty;
    /// the points live in `points` below, mirroring the in-memory
    /// adapter's `SeriesEntry` split of metadata from data).
    metric: Metric,
    points: Vec<MetricPoint>,
}

// --------------------------------------------------------------------
// Adapter
// --------------------------------------------------------------------

/// Durable file-backed `MetricStore` adapter. Implements the v0
/// trait verbatim.
pub struct FileBackedMetricStore {
    base_path: PathBuf,
    recorder: Box<dyn MetricsRecorder + Send + Sync>,
    state: Mutex<Inner>,
}

struct Inner {
    series: HashMap<(TenantId, MetricName), SeriesEntry>,
    wal: BufWriter<File>,
}

struct SeriesEntry {
    metric: Metric,
    points: Vec<MetricPoint>,
}

impl FileBackedMetricStore {
    /// Open or create a `FileBackedMetricStore` rooted at
    /// `base_path`. Loads the snapshot if present then replays the
    /// WAL on top. Each series's point vector is re-sorted on
    /// `time_unix_nano` after recovery to preserve the v0
    /// query-ordering contract.
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
            for b in snap.series {
                let key = (b.tenant, b.metric.name.clone());
                series.insert(
                    key,
                    SeriesEntry {
                        metric: b.metric,
                        points: b.points,
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
                    WalRecord::Ingest { tenant, metrics } => {
                        apply_ingest(&mut series, &tenant, metrics);
                    }
                }
            }
        }

        // Re-sort every series so query ordering holds.
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

    /// Write current state to a snapshot file and truncate the WAL.
    pub fn snapshot(&self) -> Result<(), MetricStoreError> {
        let mut state = self.state.lock().expect("poisoned");
        let snapshot_path = snapshot_path_of(&self.base_path);
        let wal_path = wal_path_of(&self.base_path);

        let buckets: Vec<SeriesBucket> = state
            .series
            .iter()
            .map(|((tenant, _name), entry)| SeriesBucket {
                tenant: tenant.clone(),
                metric: entry.metric.clone(),
                points: entry.points.clone(),
            })
            .collect();
        let snap = Snapshot { series: buckets };

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
        if batch.is_empty() {
            self.recorder.record_ingest(tenant, 0);
            return Ok(IngestReceipt { count: 0 });
        }
        let count = batch.total_points();
        let record = WalRecord::Ingest {
            tenant: tenant.clone(),
            metrics: batch.metrics.clone(),
        };
        let mut state = self.state.lock().expect("poisoned");
        append_wal(&mut state.wal, &record)?;
        apply_ingest(&mut state.series, tenant, batch.metrics);
        // Keep each touched series sorted (apply_ingest extends; we
        // sort here so the in-memory read path matches recovery).
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

/// Splits a batch's metrics into the per-`(tenant, metric_name)`
/// series index, mirroring `InMemoryMetricStore::ingest`. Points are
/// extended onto the series; the caller sorts. Used by both the live
/// ingest path and WAL recovery so the two cannot drift.
fn apply_ingest(
    series: &mut HashMap<(TenantId, MetricName), SeriesEntry>,
    tenant: &TenantId,
    metrics: Vec<Metric>,
) {
    for mut metric in metrics {
        let key = (tenant.clone(), metric.name.clone());
        // Take the points out first; the canonical metric metadata
        // stored in the series keeps an empty points vector (points
        // live in the parallel `points` vector below). Because the
        // take has already emptied `metric.points`, `metric.clone()`
        // here carries no points, so no explicit `points: Vec::new()`
        // override is needed.
        let points = std::mem::take(&mut metric.points);
        let entry = series.entry(key).or_insert_with(|| SeriesEntry {
            metric: metric.clone(),
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

// --------------------------------------------------------------------
// Inline white-box tests.
//
// The acceptance suites under tests/v1_slice_0{1,2}_*.rs cover the
// trait ingest + plain query + WAL/snapshot recovery paths. These
// inline tests close the mutation-coverage gaps the acceptance suite
// leaves: the predicate query path (query_with), the Debug impl, and
// the canonical-metric-points invariant inside apply_ingest. They
// exist to discharge cargo mutants at 100% on crates/pulse/src/
// file_backed.rs.
// --------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::metric::MetricKind;
    use crate::metrics::NoopRecorder;
    use std::collections::BTreeMap;

    fn temp_base(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!(
            "pulse-fb-unit-{name}-{}-{nanos}",
            std::process::id()
        ));
        p
    }

    fn point(time: u64, value: f64, attrs: &[(&str, &str)]) -> MetricPoint {
        let mut attributes = BTreeMap::new();
        for (k, v) in attrs {
            attributes.insert((*k).to_string(), (*v).to_string());
        }
        MetricPoint {
            time_unix_nano: time,
            start_time_unix_nano: 0,
            attributes,
            value,
        }
    }

    fn gauge(name: &str, service: &str, points: Vec<MetricPoint>) -> Metric {
        let mut resource = BTreeMap::new();
        resource.insert("service.name".to_string(), service.to_string());
        Metric {
            name: MetricName::new(name),
            description: String::new(),
            unit: "1".to_string(),
            kind: MetricKind::Gauge,
            points,
            resource_attributes: resource,
        }
    }

    // Kills the query_with -> Ok(vec![]) mutant and the && -> ||
    // mutant: the predicate must filter to exactly the matching
    // point, and the AND of range + predicate must both apply.
    #[test]
    fn query_with_applies_range_and_predicate_conjunction() {
        let base = temp_base("query_with");
        let recorder: Box<dyn MetricsRecorder + Send + Sync> = Box::new(NoopRecorder);
        let store = FileBackedMetricStore::open(&base, recorder).expect("open");
        let t = TenantId("acme".to_string());
        store
            .ingest(
                &t,
                MetricBatch::with_metrics(vec![gauge(
                    "rps",
                    "checkout",
                    vec![
                        point(100, 1.0, &[("route", "/a")]),
                        point(200, 2.0, &[("route", "/b")]),
                    ],
                )]),
            )
            .expect("ingest");

        // Predicate matches route=/a; range [0, 300) covers both
        // points. Only the /a point should survive the conjunction.
        let by_route = store
            .query_with(
                &t,
                &MetricName::new("rps"),
                TimeRange::new(0, 300),
                &Predicate::new().label_eq("route", "/a"),
            )
            .expect("query_with");
        assert_eq!(by_route.len(), 1, "predicate narrows to one point");
        assert_eq!(by_route[0].1.value, 1.0);

        // Range [0, 150) excludes the /b point; predicate matches
        // route=/b. The conjunction (range AND predicate) yields
        // zero: if && were ||, the /b point would wrongly appear.
        let conjunction = store
            .query_with(
                &t,
                &MetricName::new("rps"),
                TimeRange::new(0, 150),
                &Predicate::new().label_eq("route", "/b"),
            )
            .expect("query_with");
        assert!(
            conjunction.is_empty(),
            "range AND predicate excludes the out-of-range /b point"
        );

        let _ = std::fs::remove_file(wal_path_of(&base));
        let _ = std::fs::remove_file(snapshot_path_of(&base));
    }

    // Kills the Debug::fmt -> Ok(Default::default()) mutant: the
    // formatted output must name the struct.
    #[test]
    fn debug_impl_names_the_struct() {
        let base = temp_base("debug");
        let recorder: Box<dyn MetricsRecorder + Send + Sync> = Box::new(NoopRecorder);
        let store = FileBackedMetricStore::open(&base, recorder).expect("open");
        let rendered = format!("{store:?}");
        assert!(
            rendered.contains("FileBackedMetricStore"),
            "Debug output names the struct; got {rendered}"
        );
        let _ = std::fs::remove_file(wal_path_of(&base));
    }

    // Kills the "delete field points from struct Metric" mutant in
    // apply_ingest: the canonical metric stored in the series MUST
    // carry an empty points vector (points live in the parallel
    // points vector). If the field initializer is deleted, the
    // canonical metric would inherit the ingested metric's points,
    // doubling them on the read path.
    #[test]
    fn apply_ingest_keeps_canonical_metric_points_empty() {
        let mut series: HashMap<(TenantId, MetricName), SeriesEntry> = HashMap::new();
        let t = TenantId("acme".to_string());
        apply_ingest(
            &mut series,
            &t,
            vec![gauge("rps", "checkout", vec![point(100, 1.0, &[])])],
        );
        let entry = series
            .get(&(t.clone(), MetricName::new("rps")))
            .expect("series present");
        assert!(
            entry.metric.points.is_empty(),
            "canonical metric carries no points; they live in the series points vec"
        );
        assert_eq!(entry.points.len(), 1, "the point is in the series vec");
    }
}
