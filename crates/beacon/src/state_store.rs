// Kaleidoscope Beacon — durable rule-state store seam
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

//! The `RuleStateStore` seam (ADR-0040).
//!
//! Holds each rule's [`RuleState`](crate::RuleState) durably so a
//! beacon-server restart does not forget a firing alert and re-page
//! on-call. The store sits BESIDE the pure
//! [`transition`](crate::transition) (ADR-0037): it loads, persists and
//! recovers values, and contains no transition logic.
//!
//! Two adapters mirror the platform's six storage pillars:
//!
//! - [`InMemoryRuleStateStore`] — the v0 test seam. A
//!   `Mutex<HashMap<String, RuleState>>`. Loses state on a fresh
//!   process, so a restart still loses state (the durability defect
//!   this feature fixes lands with the file-backed adapter, not here).
//! - [`FileBackedRuleStateStore`] — the v1 durable adapter. WAL NDJSON
//!   (`<base>.wal`, one [`WalRecord::Put`] per line) + JSON snapshot
//!   (`<base>.snapshot`, the current map). `open()` loads the snapshot
//!   then replays the WAL; `snapshot()` truncates the WAL; `put()`
//!   appends one NDJSON line and updates the in-memory map.
//!
//! ## Keyed-latest-wins (ADR-0040 decision 2, DD4)
//!
//! The single load-bearing difference from the storage pillars: a rule
//! has exactly one CURRENT state, not a history. Recovery replays Put
//! records in file order doing `map.insert(rule_name, state)`, so the
//! LAST Put per rule name wins. There is NO sort and no time-ordering
//! of values, unlike the pillars' append-and-sort recovery. A reader
//! who copied a pillar's "push then sort" recovery into beacon would
//! introduce a latent ordering bug; this is why the ADR exists.
//!
//! British English throughout, no em dashes.

use std::collections::HashMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::state_machine::RuleState;

// --------------------------------------------------------------------
// Error
// --------------------------------------------------------------------

/// Failure surface for the durable adapter. A single additive variant
/// mirroring every storage pillar's `*StoreError` (DD6). The in-memory
/// adapter never returns it; it exists for the file-backed adapter's
/// WAL/snapshot IO and parse failures. A corrupt snapshot on `open()`
/// surfaces here so the composition root can refuse to start rather
/// than silently reset (ADR-0040 decision 3, DD8).
#[derive(Debug)]
pub enum RuleStateStoreError {
    /// Persisting or recovering durable state failed. `reason` names
    /// the cause so the operator knows what happened.
    PersistenceFailed { reason: String },
}

impl fmt::Display for RuleStateStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleStateStoreError::PersistenceFailed { reason } => {
                write!(f, "rule-state persistence failed: {reason}")
            }
        }
    }
}

impl std::error::Error for RuleStateStoreError {}

// --------------------------------------------------------------------
// Port
// --------------------------------------------------------------------

/// The durable rule-state port. Two methods, mirroring how `run_rule`
/// uses the state (DD3): recover the whole map once at startup, then
/// upsert one rule's state per transition. Object-safe so the
/// orchestrator can hold `Box<dyn RuleStateStore>` and swap the durable
/// adapter for the in-memory test double without touching the loop.
pub trait RuleStateStore: Send + Sync {
    /// Recover every persisted rule state. Called once at startup to
    /// seed the per-rule loops. The in-memory adapter returns an empty
    /// map on a fresh process; the file-backed adapter returns the
    /// recovered map (snapshot + WAL replay, keyed-latest-wins).
    fn load_all(&self) -> Result<HashMap<String, RuleState>, RuleStateStoreError>;

    /// Persist the latest state for one rule. Called by the per-rule
    /// loop only when `state != next` (latest-wins, DD4). The in-memory
    /// adapter updates its map; the file-backed adapter appends a
    /// [`WalRecord::Put`] and updates its map.
    fn put(&self, rule_name: &str, state: RuleState) -> Result<(), RuleStateStoreError>;
}

// --------------------------------------------------------------------
// In-memory adapter (v0 test seam)
// --------------------------------------------------------------------

/// In-memory `RuleStateStore`. A `Mutex<HashMap<String, RuleState>>`.
/// Behaviour-preserving: holds state within a process lifetime and
/// isolates rules by name, identically to the local-variable behaviour
/// it replaces, but loses everything on a fresh process. Never returns
/// [`RuleStateStoreError::PersistenceFailed`].
#[derive(Default)]
pub struct InMemoryRuleStateStore {
    states: Mutex<HashMap<String, RuleState>>,
}

impl InMemoryRuleStateStore {
    /// Construct a fresh, empty in-memory store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl fmt::Debug for InMemoryRuleStateStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryRuleStateStore").finish()
    }
}

impl RuleStateStore for InMemoryRuleStateStore {
    fn load_all(&self) -> Result<HashMap<String, RuleState>, RuleStateStoreError> {
        Ok(self.states.lock().expect("poisoned").clone())
    }

    fn put(&self, rule_name: &str, state: RuleState) -> Result<(), RuleStateStoreError> {
        self.states
            .lock()
            .expect("poisoned")
            .insert(rule_name.to_string(), state);
        Ok(())
    }
}

// --------------------------------------------------------------------
// WAL record + snapshot shapes
// --------------------------------------------------------------------

/// One durable mutation. Tagged `{"op": "put", ...}` so future record
/// kinds can be added without breaking the NDJSON wire shape.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum WalRecord {
    Put { rule_name: String, state: RuleState },
}

/// The snapshot is just the current map serialised. No buckets, no
/// ordering: a rule has one current state.
#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    rules: HashMap<String, RuleState>,
}

// --------------------------------------------------------------------
// File-backed adapter (v1 durable)
// --------------------------------------------------------------------

/// Durable file-backed `RuleStateStore`. WAL NDJSON + JSON snapshot +
/// keyed-latest-wins recovery (ADR-0040). Mirrors the storage pillars'
/// `open` / `snapshot` / append skeleton, but the replay rule is
/// `map.insert` (latest-wins, NO sort) and `open()` refuses to start on
/// a corrupt snapshot.
pub struct FileBackedRuleStateStore {
    base_path: PathBuf,
    state: Mutex<Inner>,
}

struct Inner {
    rules: HashMap<String, RuleState>,
    wal: BufWriter<File>,
}

impl FileBackedRuleStateStore {
    /// Open or create a store rooted at `base_path`. Loads the snapshot
    /// if present then replays the WAL on top with keyed-latest-wins.
    /// A snapshot file that is present but unreadable or unparseable
    /// returns [`RuleStateStoreError::PersistenceFailed`]: a lying or
    /// truncated state file is refused, never silently reset
    /// (ADR-0040 decision 3, DD8).
    pub fn open<P: AsRef<Path>>(base_path: P) -> Result<Self, RuleStateStoreError> {
        let base_path = base_path.as_ref().to_path_buf();
        let snapshot_path = snapshot_path_of(&base_path);
        let wal_path = wal_path_of(&base_path);

        let mut rules: HashMap<String, RuleState> = HashMap::new();

        if snapshot_path.exists() {
            let f = File::open(&snapshot_path).map_err(io)?;
            let snap: Snapshot = serde_json::from_reader(f).map_err(parse)?;
            rules = snap.rules;
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
                    RuleStateStoreError::PersistenceFailed {
                        reason: format!("WAL parse error at line {}: {e}", idx + 1),
                    }
                })?;
                // Keyed-latest-wins: the last Put per rule name wins.
                // No sort, no accumulation (ADR-0040 decision 2).
                match record {
                    WalRecord::Put { rule_name, state } => {
                        rules.insert(rule_name, state);
                    }
                }
            }
        }

        let wal_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&wal_path)
            .map_err(io)?;
        let wal = BufWriter::new(wal_file);

        Ok(Self {
            base_path,
            state: Mutex::new(Inner { rules, wal }),
        })
    }

    /// SCAFFOLD: true — store-fsync-durability-v0 DISTILL (Mandate 7).
    /// Open with an explicit [`FsyncBackend`] (ADR-0060 §3); the wal-fsync
    /// acceptance suite injects a `LyingFsyncBackend` (mechanism (b)).
    /// Inherent constructor, NOT a trait member — preserves
    /// `RuleStateStore` byte-identical (C1). DELIVER replaces this RED
    /// scaffold and makes `open` delegate to it.
    pub fn open_with_fsync_backend<P: AsRef<Path>>(
        _base_path: P,
        _fsync_backend: std::sync::Arc<dyn wal_recovery::FsyncBackend + Send + Sync>,
    ) -> Result<Self, RuleStateStoreError> {
        panic!("__SCAFFOLD__ beacon::FileBackedRuleStateStore::open_with_fsync_backend RED scaffold (store-fsync-durability-v0 slice 06)")
    }

    /// Write the current map to the snapshot file and truncate the WAL.
    pub fn snapshot(&self) -> Result<(), RuleStateStoreError> {
        let mut state = self.state.lock().expect("poisoned");
        let snapshot_path = snapshot_path_of(&self.base_path);
        let wal_path = wal_path_of(&self.base_path);

        let snap = Snapshot {
            rules: state.rules.clone(),
        };

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

impl fmt::Debug for FileBackedRuleStateStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileBackedRuleStateStore")
            .field("base_path", &self.base_path)
            .finish()
    }
}

impl RuleStateStore for FileBackedRuleStateStore {
    fn load_all(&self) -> Result<HashMap<String, RuleState>, RuleStateStoreError> {
        Ok(self.state.lock().expect("poisoned").rules.clone())
    }

    fn put(&self, rule_name: &str, state: RuleState) -> Result<(), RuleStateStoreError> {
        let record = WalRecord::Put {
            rule_name: rule_name.to_string(),
            state,
        };
        let mut guard = self.state.lock().expect("poisoned");
        append_wal(&mut guard.wal, &record)?;
        guard.rules.insert(rule_name.to_string(), state);
        Ok(())
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

fn io(e: std::io::Error) -> RuleStateStoreError {
    RuleStateStoreError::PersistenceFailed {
        reason: format!("io: {e}"),
    }
}

fn parse(e: serde_json::Error) -> RuleStateStoreError {
    RuleStateStoreError::PersistenceFailed {
        reason: format!("parse: {e}"),
    }
}

fn append_wal(wal: &mut BufWriter<File>, record: &WalRecord) -> Result<(), RuleStateStoreError> {
    let line = serde_json::to_string(record).map_err(parse)?;
    wal.write_all(line.as_bytes()).map_err(io)?;
    wal.write_all(b"\n").map_err(io)?;
    wal.flush().map_err(io)?;
    Ok(())
}

// --------------------------------------------------------------------
// Inline white-box tests.
//
// The acceptance suites under tests/v0_slice_01_*.rs and
// tests/v1_slice_02_*.rs drive the trait through its public port and
// cover the durable round-trip, keyed-latest-wins, snapshot replay,
// since round-trip, the corrupt-snapshot refusal, and the KPIs. These
// inline tests close the mutation-coverage gaps that port-only tests
// leave: the in-memory empty-load path, both Debug impls, the
// in-memory overwrite, and the Display impl. They exist to discharge
// cargo mutants at 100% on crates/beacon/src/state_store.rs.
// --------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn at(secs: u64) -> std::time::SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    // Kills the load_all -> Ok(Default::default()) mutant for the
    // in-memory adapter on the empty path AND the put-then-overwrite
    // path: a fresh store loads empty, and a second put overwrites the
    // first rather than accumulating.
    #[test]
    fn in_memory_loads_empty_then_overwrites_on_repeat_put() {
        let store = InMemoryRuleStateStore::new();
        assert!(
            store.load_all().expect("load empty").is_empty(),
            "a fresh in-memory store recovers nothing"
        );

        store
            .put(
                "pay-latency",
                RuleState::Pending {
                    since: at(1_700_000_000),
                },
            )
            .expect("put 1");
        store
            .put(
                "pay-latency",
                RuleState::Firing {
                    since: at(1_700_000_120),
                },
            )
            .expect("put 2");

        let recovered = store.load_all().expect("reload");
        assert_eq!(recovered.len(), 1, "one key, latest-wins not accumulate");
        assert_eq!(
            recovered.get("pay-latency"),
            Some(&RuleState::Firing {
                since: at(1_700_000_120)
            }),
            "the last put wins in memory too"
        );
    }

    // Kills the Debug::fmt -> Ok(()) mutant on both adapters: the
    // formatted output must name the struct.
    #[test]
    fn debug_impls_name_their_structs() {
        let in_memory = format!("{:?}", InMemoryRuleStateStore::new());
        assert!(
            in_memory.contains("InMemoryRuleStateStore"),
            "in-memory Debug names the struct; got {in_memory}"
        );

        let base = {
            let mut p = std::env::temp_dir();
            p.push(format!(
                "beacon-state-unit-debug-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&p).expect("mkdir");
            p.push("store");
            p
        };
        let file_backed = FileBackedRuleStateStore::open(&base).expect("open");
        let rendered = format!("{file_backed:?}");
        assert!(
            rendered.contains("FileBackedRuleStateStore"),
            "file-backed Debug names the struct; got {rendered}"
        );
        let _ = std::fs::remove_file(wal_path_of(&base));
        let _ = std::fs::remove_file(snapshot_path_of(&base));
    }

    // Kills the Display::fmt mutant: the error message must carry the
    // reason so the operator knows what happened (DD6).
    #[test]
    fn persistence_error_display_carries_the_reason() {
        let err = RuleStateStoreError::PersistenceFailed {
            reason: "snapshot truncated".to_string(),
        };
        let rendered = err.to_string();
        assert!(
            rendered.contains("snapshot truncated"),
            "the Display impl surfaces the cause; got {rendered}"
        );
    }
}
