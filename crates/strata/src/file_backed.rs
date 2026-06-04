// Kaleidoscope Strata — file-backed durable ProfileStore adapter (v1)
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

//! Strata v1 — `FileBackedProfileStore`.
//!
//! Sixth and last v1 adapter in the platform plane after Cinder,
//! Sluice, Lumen, Pulse and Ray. Same shape, same trait
//! carry-forward, same additive error-variant cost. Strata indexes
//! by `(tenant, service)` exactly as the in-memory adapter does, so
//! the WAL replays through the same drop-empty-service split logic
//! the live ingest path uses. A `Profile` is the heaviest payload of
//! the six pillars, but the durability machinery is the lightest of
//! the lot: the per-service bucket is a flat `Vec<Profile>` with no
//! metadata-from-data split, and no field needs custom serialisation
//! because the pprof table set is fully structured (DD5).

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use wal_recovery::{FsyncBackend, RealFsyncBackend};

use aegis::TenantId;
use serde::{Deserialize, Serialize};

use crate::metrics::MetricsRecorder;
use crate::predicate::Predicate;
use crate::profile::{Profile, ProfileBatch, ServiceName, TimeRange};
use crate::store::{IngestReceipt, ProfileStore, ProfileStoreError};

// --------------------------------------------------------------------
// WAL record + snapshot shapes
// --------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum WalRecord {
    Ingest {
        tenant: TenantId,
        profiles: Vec<Profile>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    buckets: Vec<ServiceBucket>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ServiceBucket {
    tenant: TenantId,
    service: ServiceName,
    profiles: Vec<Profile>,
}

// --------------------------------------------------------------------
// Adapter
// --------------------------------------------------------------------

/// Durable file-backed `ProfileStore` adapter. Implements the v0
/// trait verbatim.
pub struct FileBackedProfileStore {
    base_path: PathBuf,
    recorder: Box<dyn MetricsRecorder + Send + Sync>,
    fsync_backend: Arc<dyn FsyncBackend + Send + Sync>,
    state: Mutex<Inner>,
}

struct Inner {
    per_service: HashMap<(TenantId, ServiceName), Vec<Profile>>,
    wal: BufWriter<File>,
}

impl FileBackedProfileStore {
    /// Open or create a `FileBackedProfileStore` rooted at
    /// `base_path`. Loads the snapshot if present then replays the
    /// WAL on top. Every service bucket is re-sorted on
    /// `time_unix_nano` after recovery to preserve the v0
    /// query-ordering contract.
    pub fn open<P: AsRef<Path>>(
        base_path: P,
        recorder: Box<dyn MetricsRecorder + Send + Sync>,
    ) -> Result<Self, ProfileStoreError> {
        // The production path uses the honest backend: per-record
        // `sync_all` on append and tmp+fsync+rename+fsync-dir on
        // snapshot. The wal-fsync acceptance suite injects a counting
        // substrate through `open_with_fsync_backend` (mechanism (b)).
        Self::open_with_fsync_backend(base_path, recorder, Arc::new(RealFsyncBackend))
    }

    /// Open with an explicit [`FsyncBackend`] (ADR-0060 §3). The public
    /// [`FileBackedProfileStore::open`] delegates here with a
    /// [`RealFsyncBackend`]; the wal-fsync acceptance suite injects a
    /// `CountingFsyncBackend` to make the durability AC falsifiable
    /// in-suite (mechanism (b)). Inherent constructor, NOT a trait
    /// member — preserves the `ProfileStore` byte-identical surface (C1).
    pub fn open_with_fsync_backend<P: AsRef<Path>>(
        base_path: P,
        recorder: Box<dyn MetricsRecorder + Send + Sync>,
        fsync_backend: Arc<dyn FsyncBackend + Send + Sync>,
    ) -> Result<Self, ProfileStoreError> {
        let base_path = base_path.as_ref().to_path_buf();
        let snapshot_path = snapshot_path_of(&base_path);
        let wal_path = wal_path_of(&base_path);

        let mut per_service: HashMap<(TenantId, ServiceName), Vec<Profile>> = HashMap::new();

        if snapshot_path.exists() {
            let f = File::open(&snapshot_path).map_err(io)?;
            let snap: Snapshot = serde_json::from_reader(f).map_err(parse)?;
            for b in snap.buckets {
                per_service.insert((b.tenant, b.service), b.profiles);
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
                    ProfileStoreError::PersistenceFailed {
                        reason: format!("WAL parse error at line {}: {e}", idx + 1),
                    }
                })?;
                match record {
                    WalRecord::Ingest { tenant, profiles } => {
                        apply_ingest(&mut per_service, &tenant, profiles);
                    }
                }
            }
        }

        // Re-sort every bucket so query ordering holds.
        for bucket in per_service.values_mut() {
            bucket.sort_by_key(|p| p.time_unix_nano);
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
            state: Mutex::new(Inner { per_service, wal }),
        })
    }

    /// Write current state to a snapshot file and truncate the WAL.
    pub fn snapshot(&self) -> Result<(), ProfileStoreError> {
        let mut state = self.state.lock().expect("poisoned");
        let snapshot_path = snapshot_path_of(&self.base_path);
        let wal_path = wal_path_of(&self.base_path);

        let buckets: Vec<ServiceBucket> = state
            .per_service
            .iter()
            .map(|((tenant, service), profiles)| ServiceBucket {
                tenant: tenant.clone(),
                service: service.clone(),
                profiles: profiles.clone(),
            })
            .collect();
        let snap = Snapshot { buckets };

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

impl fmt::Debug for FileBackedProfileStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileBackedProfileStore")
            .field("base_path", &self.base_path)
            .field("recorder", &"<opaque>")
            .finish()
    }
}

impl ProfileStore for FileBackedProfileStore {
    fn ingest(
        &self,
        tenant: &TenantId,
        batch: ProfileBatch,
    ) -> Result<IngestReceipt, ProfileStoreError> {
        if batch.is_empty() {
            self.recorder.record_ingest(tenant, 0);
            return Ok(IngestReceipt { count: 0 });
        }
        let count = batch.profiles.len();
        let record = WalRecord::Ingest {
            tenant: tenant.clone(),
            profiles: batch.profiles.clone(),
        };
        let mut state = self.state.lock().expect("poisoned");
        append_wal(&mut state.wal, &record, self.fsync_backend.as_ref())?;
        let touched = apply_ingest(&mut state.per_service, tenant, batch.profiles);
        // Sort only the buckets this ingest touched, so the in-memory
        // read path matches recovery without re-sorting the whole
        // index on every call.
        for key in touched {
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

// --------------------------------------------------------------------
// helpers
// --------------------------------------------------------------------

/// Splits a batch's profiles into the per-`(tenant, service)` index,
/// mirroring `InMemoryProfileStore::ingest`. Profiles without a
/// `service.name` resource attribute are dropped (the v0 rule, kept
/// at v1). Returns the set of buckets touched so the live path can
/// sort exactly those. Used by both the live ingest path and WAL
/// recovery so the two cannot drift.
fn apply_ingest(
    per_service: &mut HashMap<(TenantId, ServiceName), Vec<Profile>>,
    tenant: &TenantId,
    profiles: Vec<Profile>,
) -> HashSet<(TenantId, ServiceName)> {
    let mut touched: HashSet<(TenantId, ServiceName)> = HashSet::new();
    for profile in profiles {
        if profile.service_name().is_empty() {
            continue;
        }
        let service = ServiceName::new(profile.service_name());
        let key = (tenant.clone(), service);
        per_service.entry(key.clone()).or_default().push(profile);
        touched.insert(key);
    }
    touched
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

fn io(e: std::io::Error) -> ProfileStoreError {
    ProfileStoreError::PersistenceFailed {
        reason: format!("io: {e}"),
    }
}

fn parse(e: serde_json::Error) -> ProfileStoreError {
    ProfileStoreError::PersistenceFailed {
        reason: format!("parse: {e}"),
    }
}

fn append_wal(
    wal: &mut BufWriter<File>,
    record: &WalRecord,
    fsync_backend: &(dyn FsyncBackend + Send + Sync),
) -> Result<(), ProfileStoreError> {
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

// --------------------------------------------------------------------
// Inline white-box tests.
//
// The acceptance suites under tests/v1_slice_0{1,2}_*.rs cover the
// trait ingest + plain query + WAL/snapshot recovery paths. These
// inline tests close the mutation-coverage gaps the acceptance suite
// leaves: the predicate query path (query_with, never exercised by
// the acceptance suite), the Debug impl, the live-path touched-bucket
// sort (the acceptance suite always reopens before querying, so
// recovery's sort-all masks a deleted live sort), and the
// drop-empty-service rule inside apply_ingest. They exist to discharge
// cargo mutants at 100% on crates/strata/src/file_backed.rs.
// --------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::NoopRecorder;
    use std::collections::BTreeMap;

    fn temp_base(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!(
            "strata-fb-unit-{name}-{}-{nanos}",
            std::process::id()
        ));
        p
    }

    fn profile(time: u64, service: &str, profile_type: &str) -> Profile {
        let mut resource = BTreeMap::new();
        if !service.is_empty() {
            resource.insert("service.name".to_string(), service.to_string());
        }
        Profile {
            time_unix_nano: time,
            duration_nanos: 1_000_000,
            profile_type: profile_type.to_string(),
            sample_type: Vec::new(),
            samples: Vec::new(),
            locations: Vec::new(),
            functions: Vec::new(),
            mappings: Vec::new(),
            string_table: Vec::new(),
            resource_attributes: resource,
            attributes: BTreeMap::new(),
        }
    }

    fn cleanup(base: &Path) {
        let _ = std::fs::remove_file(wal_path_of(base));
        let _ = std::fs::remove_file(snapshot_path_of(base));
    }

    // Kills the query_with -> Ok(vec![]) mutant and the && -> ||
    // mutant: the predicate must filter to exactly the matching
    // profile, and the AND of range + predicate must both apply.
    #[test]
    fn query_with_applies_range_and_predicate_conjunction() {
        let base = temp_base("query_with");
        let store = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open");
        let t = TenantId("acme".to_string());
        store
            .ingest(
                &t,
                ProfileBatch::with_profiles(vec![
                    profile(100, "checkout", "cpu"),
                    profile(200, "checkout", "heap"),
                ]),
            )
            .expect("ingest");

        // Predicate matches profile_type=cpu; range [0, 300) covers
        // both profiles. Only the cpu profile should survive.
        let cpu = store
            .query_with(
                &t,
                &ServiceName::new("checkout"),
                TimeRange::new(0, 300),
                &Predicate::new().profile_type("cpu"),
            )
            .expect("query_with");
        assert_eq!(cpu.len(), 1, "predicate narrows to one profile");
        assert_eq!(cpu[0].time_unix_nano, 100);

        // Range [0, 150) excludes the heap profile at t=200; predicate
        // matches profile_type=heap. The conjunction yields zero: if
        // && were ||, the out-of-range heap profile would wrongly
        // appear.
        let conjunction = store
            .query_with(
                &t,
                &ServiceName::new("checkout"),
                TimeRange::new(0, 150),
                &Predicate::new().profile_type("heap"),
            )
            .expect("query_with");
        assert!(
            conjunction.is_empty(),
            "range AND predicate excludes the out-of-range heap profile"
        );
        cleanup(&base);
    }

    // Kills the Debug::fmt -> Ok(Default::default()) mutant: the
    // formatted output must name the struct.
    #[test]
    fn debug_impl_names_the_struct() {
        let base = temp_base("debug");
        let store = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open");
        let rendered = format!("{store:?}");
        assert!(
            rendered.contains("FileBackedProfileStore"),
            "Debug output names the struct; got {rendered}"
        );
        cleanup(&base);
    }

    // Kills the live-path sort mutant: ingesting out of order and
    // querying in the SAME process (no reopen) must return sorted
    // order. Recovery's sort-all would mask a deleted live sort, so
    // this test deliberately never reopens.
    #[test]
    fn live_ingest_sorts_touched_buckets_without_reopen() {
        let base = temp_base("live_sort");
        let store = FileBackedProfileStore::open(&base, Box::new(NoopRecorder)).expect("open");
        let t = TenantId("acme".to_string());
        store
            .ingest(
                &t,
                ProfileBatch::with_profiles(vec![
                    profile(300, "svc", "cpu"),
                    profile(100, "svc", "cpu"),
                    profile(200, "svc", "cpu"),
                ]),
            )
            .expect("ingest");
        let out = store
            .query(&t, &ServiceName::new("svc"), TimeRange::all())
            .expect("query");
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].time_unix_nano, 100);
        assert_eq!(out[1].time_unix_nano, 200);
        assert_eq!(out[2].time_unix_nano, 300);
        cleanup(&base);
    }

    // Kills the drop-empty-service mutant (the `continue` and the
    // is_empty guard) inside apply_ingest: a profile without a
    // service.name must not create a bucket, and the touched set must
    // not include it.
    #[test]
    fn apply_ingest_drops_service_less_profiles() {
        let mut per_service: HashMap<(TenantId, ServiceName), Vec<Profile>> = HashMap::new();
        let t = TenantId("acme".to_string());
        let touched = apply_ingest(
            &mut per_service,
            &t,
            vec![profile(100, "", "cpu"), profile(200, "svc", "cpu")],
        );
        assert_eq!(
            per_service.len(),
            1,
            "only the profile carrying a service.name creates a bucket"
        );
        assert!(per_service.contains_key(&(t.clone(), ServiceName::new("svc"))));
        assert_eq!(touched.len(), 1, "the service-less profile is not touched");
    }
}
