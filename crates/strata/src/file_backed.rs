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
//! Sixth and last v1 adapter on the platform plane. Every
//! storage engine in the architecture document now has a
//! durable file-backed adapter behind the same v0 trait. Same
//! NDJSON WAL + JSON snapshot template, same additive error
//! variant, same trait carry-forward.
//!
//! On-disk shape: per-`(tenant, service)` bucket of profiles.
//! Matches the in-memory layout — Strata has only one index
//! at v0, so no rebuild is needed on `open`.

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
use crate::profile::{Profile, ProfileBatch, ServiceName, TimeRange};
use crate::store::{IngestReceipt, ProfileStore, ProfileStoreError};

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

pub struct FileBackedProfileStore {
    base_path: PathBuf,
    recorder: Box<dyn MetricsRecorder + Send + Sync>,
    state: Mutex<Inner>,
}

struct Inner {
    per_service: HashMap<(TenantId, ServiceName), Vec<Profile>>,
    wal: BufWriter<File>,
}

impl FileBackedProfileStore {
    pub fn open<P: AsRef<Path>>(
        base_path: P,
        recorder: Box<dyn MetricsRecorder + Send + Sync>,
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
            state: Mutex::new(Inner { per_service, wal }),
        })
    }

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
        if batch.profiles.is_empty() {
            self.recorder.record_ingest(tenant, 0);
            return Ok(IngestReceipt { count: 0 });
        }
        let count = batch.profiles.len();
        let record = WalRecord::Ingest {
            tenant: tenant.clone(),
            profiles: batch.profiles.clone(),
        };
        let mut state = self.state.lock().expect("poisoned");
        append_wal(&mut state.wal, &record)?;
        apply_ingest(&mut state.per_service, tenant, batch.profiles);
        for bucket in state.per_service.values_mut() {
            bucket.sort_by_key(|p| p.time_unix_nano);
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

fn apply_ingest(
    per_service: &mut HashMap<(TenantId, ServiceName), Vec<Profile>>,
    tenant: &TenantId,
    profiles: Vec<Profile>,
) {
    let _touched: HashSet<ServiceName> = HashSet::new();
    for profile in profiles {
        // Profiles without service.name are dropped from the
        // by-service index at v0/v1, matching the in-memory
        // adapter's contract.
        if profile.service_name().is_empty() {
            continue;
        }
        let service = ServiceName::new(profile.service_name());
        let key = (tenant.clone(), service);
        per_service.entry(key).or_default().push(profile);
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

fn append_wal(wal: &mut BufWriter<File>, record: &WalRecord) -> Result<(), ProfileStoreError> {
    let line = serde_json::to_string(record).map_err(parse)?;
    wal.write_all(line.as_bytes()).map_err(io)?;
    wal.write_all(b"\n").map_err(io)?;
    wal.flush().map_err(io)?;
    Ok(())
}
