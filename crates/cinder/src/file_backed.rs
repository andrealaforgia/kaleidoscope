// Kaleidoscope Cinder — file-backed durable TieringStore adapter (v1)
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

//! Cinder v1 — `FileBackedTieringStore`.
//!
//! First v1 adapter anywhere in the platform plane. Validates
//! that the v0 `TieringStore` trait shape carries forward to a
//! durable implementation without retrofit.
//!
//! ## Storage layout
//!
//! Given a base `path`:
//!
//! - `{path}.wal` — append-only NDJSON write-ahead log of
//!   `Place` and `Migrate` operations.
//! - `{path}.snapshot` — optional full-state snapshot in JSON.
//!
//! On `open` the snapshot is loaded first (if present) into
//! in-memory state, then the WAL is replayed on top.
//!
//! ## Recovery
//!
//! WAL replay delegates to the shared
//! [`wal_recovery::replay_wal_tolerating_torn_tail`] routine
//! (ADR-0059). A torn FINAL line — the last line of the WAL,
//! with no trailing newline, that fails to parse — is the
//! residue of a crash mid-write (ADR-0049 made the write side
//! crash-honest, so a partial record is the only residue). That
//! single torn tail is dropped, the intact acked prefix is
//! recovered, and one structured
//! `event="wal.recovery.torn_tail_dropped"` WARN is emitted
//! (naming `pillar="cinder"`, the 1-based line, and the dropped
//! byte length).
//!
//! Every OTHER parse failure stays fail-closed and is surfaced
//! as `MigrateError::PersistenceFailed`: a malformed line that is
//! NOT the last line (mid-file corruption), and a malformed final
//! line that DOES end in a trailing newline (a complete-but-bad
//! write, not a tear). The trailing newline is the discriminator.
//! The tolerance is intentionally narrow — swallowing mid-file
//! corruption would be strictly worse than refusing to open.

use std::collections::HashMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use aegis::TenantId;
use serde::{Deserialize, Serialize};
use wal_recovery::{FsyncBackend, RealFsyncBackend};

use crate::metrics::MetricsRecorder;
use crate::policy::TierPolicy;
use crate::store::{MigrateError, TieringStore};
use crate::tier::{ItemId, Tier, TierEntry};

/// One serialised WAL record. NDJSON one record per line.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum WalRecord {
    Place {
        tenant: TenantId,
        item: ItemId,
        tier: Tier,
        placed_at: SystemTime,
    },
    Migrate {
        tenant: TenantId,
        item: ItemId,
        to_tier: Tier,
        migrated_at: SystemTime,
    },
}

/// Full snapshot — list of `(tenant, item, entry)` tuples.
#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    entries: Vec<SnapshotEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SnapshotEntry {
    tenant: TenantId,
    item: ItemId,
    entry: TierEntry,
}

/// Durable WAL-backed adapter. Implements the v0
/// `TieringStore` trait verbatim.
pub struct FileBackedTieringStore {
    base_path: PathBuf,
    recorder: Box<dyn MetricsRecorder + Send + Sync>,
    fsync_backend: Arc<dyn FsyncBackend + Send + Sync>,
    state: Mutex<Inner>,
}

struct Inner {
    entries: HashMap<(TenantId, ItemId), TierEntry>,
    wal: BufWriter<File>,
}

impl FileBackedTieringStore {
    /// Open or create a `FileBackedTieringStore` rooted at
    /// `base_path`. Loads the snapshot if present, then
    /// replays the WAL on top. Returns an error if any
    /// file I/O fails, or if the WAL contains invalid JSON
    /// other than a single torn final line (last line, no
    /// trailing newline) — that torn tail is dropped, the
    /// intact prefix recovered, and a
    /// `wal.recovery.torn_tail_dropped` WARN emitted. See the
    /// module-level `## Recovery` docs (ADR-0059).
    pub fn open<P: AsRef<Path>>(
        base_path: P,
        recorder: Box<dyn MetricsRecorder + Send + Sync>,
    ) -> Result<Self, MigrateError> {
        // The production path uses the honest backend: per-record
        // `sync_all` on append and tmp+fsync+rename+fsync-dir on
        // snapshot. The wal-fsync acceptance suite injects a counting
        // substrate through `open_with_fsync_backend` (mechanism (b)).
        Self::open_with_fsync_backend(base_path, recorder, Arc::new(RealFsyncBackend))
    }

    /// Open with an explicit [`FsyncBackend`] (ADR-0060 §3). The public
    /// [`FileBackedTieringStore::open`] delegates here with a
    /// [`RealFsyncBackend`]; the wal-fsync acceptance suite injects a
    /// `CountingFsyncBackend` to make the durability AC falsifiable
    /// in-suite (mechanism (b)). Inherent constructor, NOT a trait
    /// member — preserves the `TieringStore` byte-identical surface (C1).
    pub fn open_with_fsync_backend<P: AsRef<Path>>(
        base_path: P,
        recorder: Box<dyn MetricsRecorder + Send + Sync>,
        fsync_backend: Arc<dyn FsyncBackend + Send + Sync>,
    ) -> Result<Self, MigrateError> {
        let base_path = base_path.as_ref().to_path_buf();
        let snapshot_path = snapshot_path_of(&base_path);
        let wal_path = wal_path_of(&base_path);

        let mut entries: HashMap<(TenantId, ItemId), TierEntry> = HashMap::new();

        // 1. Load snapshot if present.
        if snapshot_path.exists() {
            let f = File::open(&snapshot_path).map_err(io)?;
            let snapshot: Snapshot = serde_json::from_reader(f).map_err(parse)?;
            for SnapshotEntry {
                tenant,
                item,
                entry,
            } in snapshot.entries
            {
                entries.insert((tenant, item), entry);
            }
        }

        // 2. Replay WAL, recovering the intact acked prefix past a
        //    single torn final line (ADR-0059, shared wal-recovery).
        if wal_path.exists() {
            let wal_bytes = std::fs::read(&wal_path).map_err(io)?;
            wal_recovery::replay_wal_tolerating_torn_tail::<WalRecord, MigrateError>(
                &wal_bytes,
                "cinder",
                |record| {
                    apply_to_entries(&mut entries, record);
                    Ok(())
                },
                |line, error| MigrateError::PersistenceFailed {
                    reason: format!("WAL parse error at line {line}: {error}"),
                },
            )?;
        }

        // 3. Open WAL for append.
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
            state: Mutex::new(Inner { entries, wal }),
        })
    }

    /// Write the current in-memory state to a snapshot
    /// file then truncate the WAL. Idempotent under
    /// repeated invocation with no intervening writes.
    pub fn snapshot(&self) -> Result<(), MigrateError> {
        let mut state = self.state.lock().expect("poisoned");
        let snapshot_path = snapshot_path_of(&self.base_path);
        let wal_path = wal_path_of(&self.base_path);

        // Build the snapshot from current entries.
        let entries: Vec<SnapshotEntry> = state
            .entries
            .iter()
            .map(|((tenant, item), entry)| SnapshotEntry {
                tenant: tenant.clone(),
                item: item.clone(),
                entry: entry.clone(),
            })
            .collect();
        let snapshot = Snapshot { entries };

        // Flush any pending WAL writes before truncating —
        // we don't want to silently drop unwritten records.
        state.wal.flush().map_err(io)?;

        // Write snapshot atomically (tmp+fsync+rename+fsync-dir) so the
        // canonical path is whole-or-absent across a crash at ANY point.
        wal_recovery::atomic_write_snapshot(
            &snapshot_path,
            self.fsync_backend.as_ref(),
            |writer| {
                serde_json::to_writer(&mut *writer, &snapshot)
                    .map_err(|e| std::io::Error::other(e.to_string()))
            },
        )
        .map_err(io)?;

        // Truncate WAL by reopening with truncate.
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

impl fmt::Debug for FileBackedTieringStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileBackedTieringStore")
            .field("base_path", &self.base_path)
            .field("recorder", &"<opaque>")
            .finish()
    }
}

impl TieringStore for FileBackedTieringStore {
    fn place(&self, tenant: &TenantId, item: &ItemId, tier: Tier, placed_at: SystemTime) {
        let record = WalRecord::Place {
            tenant: tenant.clone(),
            item: item.clone(),
            tier,
            placed_at,
        };
        let mut state = self.state.lock().expect("poisoned");
        if let Err(_e) = append_wal(&mut state.wal, &record, self.fsync_backend.as_ref()) {
            // v1 contract: `place` returns no error; logging
            // the WAL failure is the operator's job at v2.
            // The in-memory state is updated optimistically
            // so subsequent reads stay consistent until
            // restart, when the WAL replay will pick up
            // wherever it can.
        }
        apply_to_entries(&mut state.entries, record);
        self.recorder.record_place(tenant, tier);
    }

    fn get_tier(&self, tenant: &TenantId, item: &ItemId) -> Option<Tier> {
        let state = self.state.lock().expect("poisoned");
        state
            .entries
            .get(&(tenant.clone(), item.clone()))
            .map(|e| e.tier)
    }

    fn get_entry(&self, tenant: &TenantId, item: &ItemId) -> Option<TierEntry> {
        let state = self.state.lock().expect("poisoned");
        state.entries.get(&(tenant.clone(), item.clone())).cloned()
    }

    fn migrate(
        &self,
        tenant: &TenantId,
        item: &ItemId,
        to_tier: Tier,
        migrated_at: SystemTime,
    ) -> Result<(), MigrateError> {
        let mut state = self.state.lock().expect("poisoned");
        let key = (tenant.clone(), item.clone());
        if !state.entries.contains_key(&key) {
            return Err(MigrateError::UnknownItem {
                tenant: tenant.clone(),
                item: item.clone(),
            });
        }
        let record = WalRecord::Migrate {
            tenant: tenant.clone(),
            item: item.clone(),
            to_tier,
            migrated_at,
        };
        append_wal(&mut state.wal, &record, self.fsync_backend.as_ref())?;
        let from = state.entries[&key].tier;
        apply_to_entries(&mut state.entries, record);
        self.recorder.record_migrate(tenant, from, to_tier);
        Ok(())
    }

    fn list_by_tier(&self, tenant: &TenantId, tier: Tier) -> Vec<ItemId> {
        let state = self.state.lock().expect("poisoned");
        state
            .entries
            .iter()
            .filter(|((t, _), e)| t == tenant && e.tier == tier)
            .map(|((_, id), _)| id.clone())
            .collect()
    }

    fn evaluate_at(&self, now: SystemTime, policy: &TierPolicy) -> usize {
        // Same algorithm as InMemoryTieringStore::evaluate_at,
        // plus WAL writes for every migration.
        let mut state = self.state.lock().expect("poisoned");
        let mut to_migrate: Vec<((TenantId, ItemId), Tier, Tier)> = Vec::new();
        for ((tenant, item), entry) in state.entries.iter() {
            let Some(threshold) = policy.threshold_from(entry.tier) else {
                continue;
            };
            let Some(next) = entry.tier.next_forward() else {
                continue;
            };
            let Ok(age) = now.duration_since(entry.migrated_at) else {
                continue;
            };
            if age >= threshold {
                to_migrate.push(((tenant.clone(), item.clone()), entry.tier, next));
            }
        }
        let migrated_count = to_migrate.len();
        let mut per_tenant: HashMap<TenantId, usize> = HashMap::new();
        for ((tenant, item), from, to) in to_migrate {
            let record = WalRecord::Migrate {
                tenant: tenant.clone(),
                item: item.clone(),
                to_tier: to,
                migrated_at: now,
            };
            // Best-effort WAL write; if it fails we still
            // update in-memory state so the verdict stays
            // consistent for the rest of this evaluation.
            let _ = append_wal(&mut state.wal, &record, self.fsync_backend.as_ref());
            if let Some(entry) = state.entries.get_mut(&(tenant.clone(), item)) {
                entry.tier = to;
                entry.migrated_at = now;
            }
            self.recorder.record_migrate(&tenant, from, to);
            *per_tenant.entry(tenant).or_insert(0) += 1;
        }
        for (tenant, count) in per_tenant {
            self.recorder.record_evaluate(&tenant, count);
        }
        migrated_count
    }
}

// ----- helpers -----

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

fn io(e: std::io::Error) -> MigrateError {
    MigrateError::PersistenceFailed {
        reason: format!("io: {e}"),
    }
}

fn parse(e: serde_json::Error) -> MigrateError {
    MigrateError::PersistenceFailed {
        reason: format!("parse: {e}"),
    }
}

fn append_wal(
    wal: &mut BufWriter<File>,
    record: &WalRecord,
    fsync_backend: &(dyn FsyncBackend + Send + Sync),
) -> Result<(), MigrateError> {
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

fn apply_to_entries(entries: &mut HashMap<(TenantId, ItemId), TierEntry>, record: WalRecord) {
    match record {
        WalRecord::Place {
            tenant,
            item,
            tier,
            placed_at,
        } => {
            entries.insert(
                (tenant, item),
                TierEntry {
                    tier,
                    placed_at,
                    migrated_at: placed_at,
                },
            );
        }
        WalRecord::Migrate {
            tenant,
            item,
            to_tier,
            migrated_at,
        } => {
            if let Some(entry) = entries.get_mut(&(tenant, item)) {
                entry.tier = to_tier;
                entry.migrated_at = migrated_at;
            }
            // If the migrate references an unknown item
            // (shouldn't happen with a correctly-written
            // log, but if it does we silently skip rather
            // than poison recovery).
        }
    }
}
