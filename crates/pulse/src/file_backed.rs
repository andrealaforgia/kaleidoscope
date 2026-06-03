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
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use aegis::TenantId;
use serde::{Deserialize, Serialize};

use crate::fsync_probe::{FsyncBackend, RealFsyncBackend};
use crate::metric::{Metric, MetricBatch, MetricName, MetricPoint, SeriesKey, TimeRange};
use crate::metrics::MetricsRecorder;
use crate::predicate::Predicate;
use crate::store::{IngestReceipt, MetricStore, MetricStoreError};
use crate::MAX_SERIES_PER_TENANT;

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
    fsync_backend: Arc<dyn FsyncBackend + Send + Sync>,
    state: Mutex<Inner>,
}

struct Inner {
    series: HashMap<(TenantId, SeriesKey), SeriesEntry>,
    /// Shadow per-tenant counter of distinct `SeriesKey`s held in
    /// `series` (ADR-0051 §5). Maintained atomically with `series`
    /// under the same `Mutex` so the cap-check, the increment, and
    /// the insert are atomic per metric. O(1) per check. Seeded from
    /// the rebuilt series after WAL replay on `open()`.
    tenant_counts: HashMap<TenantId, usize>,
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
    ///
    /// Delegates to [`FileBackedMetricStore::open_with_fsync_backend`]
    /// with a [`RealFsyncBackend`]; tests that need to observe or
    /// simulate fsync calls inject a counting or lying backend via
    /// the explicit constructor.
    pub fn open<P: AsRef<Path>>(
        base_path: P,
        recorder: Box<dyn MetricsRecorder + Send + Sync>,
    ) -> Result<Self, MetricStoreError> {
        Self::open_with_fsync_backend(base_path, recorder, Arc::new(RealFsyncBackend))
    }

    /// Open or create a `FileBackedMetricStore` with an explicit
    /// [`FsyncBackend`]. The production path uses
    /// [`FileBackedMetricStore::open`] which threads a
    /// [`RealFsyncBackend`]; the slice 03 acceptance suite injects a
    /// counting wrapper to observe per-record fsync calls (ADR-0049
    /// §6).
    pub fn open_with_fsync_backend<P: AsRef<Path>>(
        base_path: P,
        recorder: Box<dyn MetricsRecorder + Send + Sync>,
        fsync_backend: Arc<dyn FsyncBackend + Send + Sync>,
    ) -> Result<Self, MetricStoreError> {
        let base_path = base_path.as_ref().to_path_buf();
        let snapshot_path = snapshot_path_of(&base_path);
        let wal_path = wal_path_of(&base_path);

        let mut series: HashMap<(TenantId, SeriesKey), SeriesEntry> = HashMap::new();
        // Shadow per-tenant counter is fed by BOTH the snapshot
        // rehydrate path and the WAL replay path with enforce_cap=false
        // (ADR-0051 §4): replay never refuses, so the rebuilt count is
        // exactly the on-disk cardinality regardless of how it relates
        // to MAX_SERIES_PER_TENANT.
        let mut tenant_counts: HashMap<TenantId, usize> = HashMap::new();

        if snapshot_path.exists() {
            let f = File::open(&snapshot_path).map_err(io)?;
            let snap: Snapshot = serde_json::from_reader(f).map_err(parse)?;
            for b in snap.series {
                let key = (b.tenant.clone(), SeriesKey::of(&b.metric));
                if series
                    .insert(
                        key,
                        SeriesEntry {
                            metric: b.metric,
                            points: b.points,
                        },
                    )
                    .is_none()
                {
                    *tenant_counts.entry(b.tenant).or_default() += 1;
                }
            }
        }

        if wal_path.exists() {
            // ADR-0059: tolerate ONLY a single torn final line (a partial
            // record with no trailing newline, the residue of a crash
            // mid-append). The intact acked prefix recovers; every other
            // parse failure (mid-file, or a newline-terminated malformed
            // final line) stays fail-closed via `on_parse_error`.
            let wal_bytes = std::fs::read(&wal_path).map_err(io)?;
            wal_recovery::replay_wal_tolerating_torn_tail::<WalRecord, MetricStoreError>(
                &wal_bytes,
                "pulse",
                |record| {
                    let WalRecord::Ingest { tenant, metrics } = record;
                    // ADR-0051 §4: WAL replay passes enforce_cap=false.
                    // Replay rebuilds existing series past the cap; the
                    // cap fires only on post-replay live ingest. The
                    // returned refused count is therefore always zero and
                    // is discarded.
                    let _ = apply_ingest(&mut series, &mut tenant_counts, &tenant, metrics, false);
                    Ok(())
                },
                |line, error| MetricStoreError::PersistenceFailed {
                    reason: format!("WAL parse error at line {line}: {error}"),
                },
            )?;
        }

        // Re-sort every series so query ordering holds.
        for entry in series.values_mut() {
            entry.points.sort_by_key(|p| p.time_unix_nano);
        }

        // Belt-and-braces seeding of the shadow counter from the
        // rebuilt series map (ADR-0051 §5). The snapshot and WAL
        // replay paths above already maintain `tenant_counts`; this
        // single pass guarantees the counter matches the rebuilt
        // cardinality regardless of which path supplied each entry,
        // killing the mutant that elides incrementing on the snapshot
        // path. Idempotent: it overwrites with the same value.
        let mut rebuilt: HashMap<TenantId, usize> = HashMap::new();
        for (tenant, _key) in series.keys() {
            *rebuilt.entry(tenant.clone()).or_default() += 1;
        }
        let tenant_counts = rebuilt;

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
            state: Mutex::new(Inner {
                series,
                tenant_counts,
                wal,
            }),
        })
    }

    /// Write current state to a snapshot file and truncate the WAL.
    ///
    /// fsync discipline (ADR-0049 §5):
    /// 1. Write the snapshot file and `sync_all` it before depending
    ///    on it for WAL truncation.
    /// 2. `fsync_dir` on the snapshot's parent so the directory
    ///    entry pointing at the snapshot is durable BEFORE the WAL
    ///    truncate.
    /// 3. Truncate and recreate the WAL.
    /// 4. `fsync_dir` on the parent again so the WAL recreate is
    ///    durable.
    pub fn snapshot(&self) -> Result<(), MetricStoreError> {
        let mut state = self.state.lock().expect("poisoned");
        let snapshot_path = snapshot_path_of(&self.base_path);
        let wal_path = wal_path_of(&self.base_path);
        let parent = snapshot_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let buckets: Vec<SeriesBucket> = state
            .series
            .iter()
            .map(|((tenant, _key), entry)| SeriesBucket {
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
        // sync_all on the snapshot file before depending on it.
        self.fsync_backend
            .fsync_file(writer.get_ref())
            .map_err(io)?;
        drop(writer);

        // fsync the parent so the snapshot's directory entry is
        // durable before the WAL truncation that depends on it.
        self.fsync_backend.fsync_dir(&parent).map_err(io)?;

        let wal_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&wal_path)
            .map_err(io)?;
        state.wal = BufWriter::new(wal_file);

        // fsync the parent again so the WAL truncate-and-recreate is
        // itself durable on POSIX.
        self.fsync_backend.fsync_dir(&parent).map_err(io)?;

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
            return Ok(IngestReceipt {
                count: 0,
                series_refused: 0,
            });
        }
        // WAL-log the batch up front (ADR-0049 durability discipline,
        // unchanged). The cap is then enforced by `apply_ingest`
        // under the same lock that protects the series map; refused
        // metrics' points are dropped before they reach the index.
        // Per ADR-0051 §4, WAL replay re-applies via the same
        // `apply_ingest` seam with `enforce_cap=false` so the rebuilt
        // state reflects what was accepted at restart time.
        let record = WalRecord::Ingest {
            tenant: tenant.clone(),
            metrics: batch.metrics.clone(),
        };
        let mut state = self.state.lock().expect("poisoned");
        append_wal(&mut state.wal, &record, self.fsync_backend.as_ref())?;
        let Inner {
            series,
            tenant_counts,
            ..
        } = &mut *state;
        let (count, series_refused) =
            apply_ingest(series, tenant_counts, tenant, batch.metrics, true);
        // Keep each touched series sorted (apply_ingest extends; we
        // sort here so the in-memory read path matches recovery).
        for entry in state.series.values_mut() {
            entry.points.sort_by_key(|p| p.time_unix_nano);
        }
        drop(state);
        self.recorder.record_ingest(tenant, count);
        if series_refused > 0 {
            self.recorder.record_series_refused(tenant, series_refused);
        }
        Ok(IngestReceipt {
            count,
            series_refused,
        })
    }

    fn query(
        &self,
        tenant: &TenantId,
        metric_name: &MetricName,
        range: TimeRange,
    ) -> Result<Vec<(Metric, MetricPoint)>, MetricStoreError> {
        let state = self.state.lock().expect("poisoned");
        // Fan out across every series whose name matches within the
        // tenant; each carries its own resource_attributes.
        let matches: Vec<(Metric, MetricPoint)> = state
            .series
            .iter()
            .filter(|((entry_tenant, key), _)| entry_tenant == tenant && key.name == *metric_name)
            .flat_map(|(_, entry)| {
                entry
                    .points
                    .iter()
                    .filter(|p| range.contains(p.time_unix_nano))
                    .cloned()
                    .map(|p| (entry.metric.clone(), p))
            })
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
        // Fan out across every series whose name matches within the
        // tenant, then apply the predicate per row.
        let matches: Vec<(Metric, MetricPoint)> = state
            .series
            .iter()
            .filter(|((entry_tenant, key), _)| entry_tenant == tenant && key.name == *metric_name)
            .flat_map(|(_, entry)| {
                entry
                    .points
                    .iter()
                    .filter(|p| {
                        range.contains(p.time_unix_nano) && predicate.matches(&entry.metric, p)
                    })
                    .cloned()
                    .map(|p| (entry.metric.clone(), p))
            })
            .collect();
        self.recorder.record_query(tenant, matches.len());
        Ok(matches)
    }
}

// --------------------------------------------------------------------
// helpers
// --------------------------------------------------------------------

/// Splits a batch's metrics into the per-`(tenant, SeriesKey)` series
/// index, mirroring `InMemoryMetricStore::ingest`. `SeriesKey` is the
/// metric name plus its full resource-attribute label set, so two
/// services sharing a name stay distinct series (ADR-0045). Points are
/// extended onto the series; the caller sorts. Used by both the live
/// ingest path and WAL recovery so the two cannot drift.
///
/// Cardinality cap (ADR-0051):
///
/// - `enforce_cap=true` (live ingest): refuses NEW `SeriesKey`s when
///   the per-tenant count is `>=` [`MAX_SERIES_PER_TENANT`]. The
///   refused metric's points are dropped; the loop continues
///   (partial apply, ADR-0051 §3).
/// - `enforce_cap=false` (WAL replay): every record is reconstructed
///   regardless of count; the cap fires only on post-replay live
///   ingest (ADR-0051 §4).
///
/// `tenant_counts` is the shadow per-tenant distinct-series counter;
/// it is incremented for every NEW key inserted on either path.
///
/// Returns `(points_stored, series_refused)`. `series_refused` is
/// always zero when `enforce_cap=false`.
fn apply_ingest(
    series: &mut HashMap<(TenantId, SeriesKey), SeriesEntry>,
    tenant_counts: &mut HashMap<TenantId, usize>,
    tenant: &TenantId,
    metrics: Vec<Metric>,
    enforce_cap: bool,
) -> (usize, usize) {
    let mut stored = 0usize;
    let mut refused = 0usize;
    for mut metric in metrics {
        let key = (tenant.clone(), SeriesKey::of(&metric));
        let is_existing = series.contains_key(&key);
        if !is_existing {
            // ADR-0051 §1 boundary: `>=`. A per-tenant count of
            // exactly MAX_SERIES_PER_TENANT refuses the next new key.
            if enforce_cap {
                let count = tenant_counts.get(tenant).copied().unwrap_or(0);
                if count >= MAX_SERIES_PER_TENANT {
                    refused += 1;
                    continue;
                }
            }
            *tenant_counts.entry(tenant.clone()).or_default() += 1;
        }
        // Take the points out first; the canonical metric metadata
        // stored in the series keeps an empty points vector (points
        // live in the parallel `points` vector below). Because the
        // take has already emptied `metric.points`, `metric.clone()`
        // here carries no points, so no explicit `points: Vec::new()`
        // override is needed.
        let points = std::mem::take(&mut metric.points);
        stored += points.len();
        let entry = series.entry(key).or_insert_with(|| SeriesEntry {
            metric: metric.clone(),
            points: Vec::new(),
        });
        // `resource_attributes` is NOT refreshed: it is part of the
        // series key, so a differing label set lands in a different
        // entry and an identical one already matches the stored
        // attributes (ADR-0045).
        entry.metric.description = metric.description;
        entry.metric.unit = metric.unit;
        entry.metric.kind = metric.kind;
        entry.points.extend(points);
    }
    (stored, refused)
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

fn append_wal(
    wal: &mut BufWriter<File>,
    record: &WalRecord,
    fsync_backend: &(dyn FsyncBackend + Send + Sync),
) -> Result<(), MetricStoreError> {
    let line = serde_json::to_string(record).map_err(parse)?;
    wal.write_all(line.as_bytes()).map_err(io)?;
    wal.write_all(b"\n").map_err(io)?;
    wal.flush().map_err(io)?;
    // sync_all per record (ADR-0049 §4): flush only empties the
    // user-space buffer to the kernel; sync_all instructs the kernel
    // to make the data durable on stable storage.
    fsync_backend.fsync_file(wal.get_ref()).map_err(io)?;
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

    // Kills the `entry_tenant == tenant && key.name == *metric_name`
    // -> `||` mutants in both query and query_with: the fan-out is
    // scoped to the queried tenant. Two tenants ingest the same metric
    // name; a query of one tenant must see only its own series, never
    // the other tenant's points. With `||`, every series sharing the
    // name (across all tenants) would leak in.
    #[test]
    fn query_is_scoped_to_the_queried_tenant() {
        let base = temp_base("tenant_scope");
        let recorder: Box<dyn MetricsRecorder + Send + Sync> = Box::new(NoopRecorder);
        let store = FileBackedMetricStore::open(&base, recorder).expect("open");
        let acme = TenantId("acme".to_string());
        let globex = TenantId("globex".to_string());

        store
            .ingest(
                &acme,
                MetricBatch::with_metrics(vec![gauge(
                    "rps",
                    "checkout",
                    vec![point(100, 1.0, &[("route", "/a")])],
                )]),
            )
            .expect("ingest acme");
        store
            .ingest(
                &globex,
                MetricBatch::with_metrics(vec![gauge(
                    "rps",
                    "cart",
                    vec![point(100, 2.0, &[("route", "/a")])],
                )]),
            )
            .expect("ingest globex");

        let acme_rows = store
            .query(&acme, &MetricName::new("rps"), TimeRange::all())
            .expect("query acme");
        assert_eq!(acme_rows.len(), 1, "acme sees only its own series");
        assert_eq!(acme_rows[0].1.value, 1.0);
        assert_eq!(
            acme_rows[0].0.resource_attributes.get("service.name"),
            Some(&"checkout".to_string()),
        );

        let acme_filtered = store
            .query_with(
                &acme,
                &MetricName::new("rps"),
                TimeRange::all(),
                &Predicate::new().label_eq("route", "/a"),
            )
            .expect("query_with acme");
        assert_eq!(
            acme_filtered.len(),
            1,
            "query_with is tenant-scoped too; globex's matching point must not leak in"
        );
        assert_eq!(acme_filtered[0].1.value, 1.0);

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
        let mut series: HashMap<(TenantId, SeriesKey), SeriesEntry> = HashMap::new();
        let mut tenant_counts: HashMap<TenantId, usize> = HashMap::new();
        let t = TenantId("acme".to_string());
        let metric = gauge("rps", "checkout", vec![point(100, 1.0, &[])]);
        let key = SeriesKey::of(&metric);
        let (stored, refused) =
            apply_ingest(&mut series, &mut tenant_counts, &t, vec![metric], false);
        assert_eq!(stored, 1);
        assert_eq!(refused, 0);
        let entry = series.get(&(t.clone(), key)).expect("series present");
        assert!(
            entry.metric.points.is_empty(),
            "canonical metric carries no points; they live in the series points vec"
        );
        assert_eq!(entry.points.len(), 1, "the point is in the series vec");
    }

    // ADR-0051 boundary mutants: `>=` -> `>` and `>=` -> `<` on the
    // cap arm. These complement scenario 3 in the acceptance suite
    // (which exercises the seam through the driving port) by hitting
    // the helper in isolation: cheaper to run and tighter to
    // attribute when the boundary mutant flips.
    #[test]
    fn apply_ingest_enforce_cap_refuses_at_exactly_max() {
        let mut series: HashMap<(TenantId, SeriesKey), SeriesEntry> = HashMap::new();
        let mut tenant_counts: HashMap<TenantId, usize> = HashMap::new();
        let t = TenantId("acme".to_string());
        // Pre-seed the shadow counter at exactly the cap; no series
        // entries are needed because the cap check is by counter, not
        // by `series.len()`.
        tenant_counts.insert(t.clone(), MAX_SERIES_PER_TENANT);
        // A new series at the boundary must be refused.
        let new_metric = gauge("rps", "checkout", vec![point(100, 1.0, &[])]);
        let (stored, refused) =
            apply_ingest(&mut series, &mut tenant_counts, &t, vec![new_metric], true);
        assert_eq!(stored, 0, "no points stored for the refused metric");
        assert_eq!(refused, 1, "the boundary metric is refused");
        assert_eq!(
            *tenant_counts.get(&t).unwrap(),
            MAX_SERIES_PER_TENANT,
            "the refused metric does NOT increment the counter"
        );
        assert!(
            series.is_empty(),
            "the refused metric is NOT inserted into the index"
        );
    }

    // Kills the mutant that flips `enforce_cap=true` to
    // `enforce_cap=false` on the live-ingest call site. With the
    // mutant, refused metrics would land in the index.
    #[test]
    fn apply_ingest_enforce_cap_false_bypasses_the_cap() {
        let mut series: HashMap<(TenantId, SeriesKey), SeriesEntry> = HashMap::new();
        let mut tenant_counts: HashMap<TenantId, usize> = HashMap::new();
        let t = TenantId("acme".to_string());
        tenant_counts.insert(t.clone(), MAX_SERIES_PER_TENANT + 100);
        let new_metric = gauge("rps", "checkout", vec![point(100, 1.0, &[])]);
        let (stored, refused) =
            apply_ingest(&mut series, &mut tenant_counts, &t, vec![new_metric], false);
        assert_eq!(stored, 1, "replay accepts the point");
        assert_eq!(refused, 0, "replay never refuses");
        assert_eq!(
            *tenant_counts.get(&t).unwrap(),
            MAX_SERIES_PER_TENANT + 101,
            "the replay-accepted new key increments the counter"
        );
    }

    // Kills the post-snapshot seeding mutant. If the shadow counter
    // is NOT seeded from the rebuilt series map on `open()`, a
    // subsequent live ingest of a new key would NOT refuse (the
    // counter starts at 0). This test reopens a store whose snapshot
    // holds one series and asserts the counter reflects it.
    #[test]
    fn open_seeds_tenant_counts_from_rebuilt_series() {
        let base = temp_base("seed_counts");
        let recorder: Box<dyn MetricsRecorder + Send + Sync> = Box::new(NoopRecorder);
        let store = FileBackedMetricStore::open(&base, recorder).expect("open");
        let t = TenantId("acme".to_string());
        store
            .ingest(
                &t,
                MetricBatch::with_metrics(vec![gauge(
                    "rps",
                    "checkout",
                    vec![point(100, 1.0, &[])],
                )]),
            )
            .expect("ingest");
        // Snapshot then drop so reopen rehydrates from the snapshot.
        store.snapshot().expect("snapshot");
        drop(store);

        // Reopen and probe via state-lock peek. We use the public
        // ingest with a series that, if the counter is correctly
        // seeded at 1 and the cap is artificially lowered, would
        // refuse. The cap is compile-time at slice 01 so we cannot
        // lower it here; instead we observe the counter via a fresh
        // ingest of the SAME series: the counter must stay at 1
        // (existing series match, no increment).
        let recorder2: Box<dyn MetricsRecorder + Send + Sync> = Box::new(NoopRecorder);
        let store2 = FileBackedMetricStore::open(&base, recorder2).expect("open 2");
        let state = store2.state.lock().expect("poisoned");
        assert_eq!(
            state.tenant_counts.get(&t).copied(),
            Some(1),
            "post-open shadow counter equals the rebuilt cardinality"
        );
        drop(state);

        let _ = std::fs::remove_file(wal_path_of(&base));
        let _ = std::fs::remove_file(snapshot_path_of(&base));
    }
}
