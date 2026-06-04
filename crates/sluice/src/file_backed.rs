// Kaleidoscope Sluice — file-backed durable Queue adapter (v1)
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

//! Sluice v1 — `FileBackedQueue`.
//!
//! Second v1 adapter anywhere in the platform plane after Cinder
//! v1. Validates that the v0→v1 carry-forward pattern is
//! repeatable across independent crates.
//!
//! ## Storage layout
//!
//! Given a base `path`:
//!
//! - `{path}.wal` — append-only NDJSON WAL of operations.
//! - `{path}.snapshot` — optional full-state snapshot.
//!
//! Payloads are hex-encoded so they survive a JSON round-trip
//! without dragging in a base64 dependency.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use aegis::TenantId;
use serde::{Deserialize, Serialize};

use crate::metrics::MetricsRecorder;
use crate::queue::{EnqueueError, Message, MessageId, Queue};

// --------------------------------------------------------------------
// WAL record
// --------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum WalRecord {
    Enqueue {
        id: u64,
        tenant: TenantId,
        /// Hex-encoded payload bytes.
        payload_hex: String,
    },
    Dequeue {
        id: u64,
    },
    Ack {
        id: u64,
    },
    Nack {
        id: u64,
    },
}

// --------------------------------------------------------------------
// Snapshot
// --------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    next_id: u64,
    pending: Vec<TenantPending>,
    in_flight: Vec<MessageSnapshot>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TenantPending {
    tenant: TenantId,
    messages: Vec<MessageSnapshot>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MessageSnapshot {
    id: u64,
    tenant: TenantId,
    payload_hex: String,
}

// --------------------------------------------------------------------
// Public adapter
// --------------------------------------------------------------------

/// Durable WAL-backed queue adapter. Implements the v0 `Queue`
/// trait verbatim.
pub struct FileBackedQueue {
    base_path: PathBuf,
    cap: usize,
    recorder: Box<dyn MetricsRecorder + Send + Sync>,
    state: Mutex<Inner>,
}

struct Inner {
    next_id: u64,
    pending: HashMap<TenantId, VecDeque<Message>>,
    in_flight: HashMap<MessageId, Message>,
    total: usize,
    wal: BufWriter<File>,
}

impl FileBackedQueue {
    /// Open or create a `FileBackedQueue` rooted at `base_path`.
    /// Loads the snapshot if present then replays the WAL on
    /// top. The per-tenant capacity must match across restarts;
    /// the caller is responsible for passing it.
    pub fn open<P: AsRef<Path>>(
        base_path: P,
        cap: usize,
        recorder: Box<dyn MetricsRecorder + Send + Sync>,
    ) -> Result<Self, EnqueueError> {
        let base_path = base_path.as_ref().to_path_buf();
        let snapshot_path = snapshot_path_of(&base_path);
        let wal_path = wal_path_of(&base_path);

        let mut next_id: u64 = 0;
        let mut pending: HashMap<TenantId, VecDeque<Message>> = HashMap::new();
        let mut in_flight: HashMap<MessageId, Message> = HashMap::new();

        // 1. Load snapshot if present.
        if snapshot_path.exists() {
            let f = File::open(&snapshot_path).map_err(io)?;
            let snap: Snapshot = serde_json::from_reader(f).map_err(parse)?;
            next_id = snap.next_id;
            for t in snap.pending {
                let mut q: VecDeque<Message> = VecDeque::new();
                for m in t.messages {
                    let payload = decode_hex(&m.payload_hex)?;
                    q.push_back(Message {
                        id: MessageId(m.id),
                        tenant: m.tenant,
                        payload,
                    });
                }
                pending.insert(t.tenant, q);
            }
            for m in snap.in_flight {
                let payload = decode_hex(&m.payload_hex)?;
                in_flight.insert(
                    MessageId(m.id),
                    Message {
                        id: MessageId(m.id),
                        tenant: m.tenant,
                        payload,
                    },
                );
            }
        }

        // 2. Replay WAL.
        if wal_path.exists() {
            let f = File::open(&wal_path).map_err(io)?;
            let reader = BufReader::new(f);
            for (idx, line) in reader.lines().enumerate() {
                let line = line.map_err(io)?;
                if line.is_empty() {
                    continue;
                }
                let record: WalRecord =
                    serde_json::from_str(&line).map_err(|e| EnqueueError::PersistenceFailed {
                        reason: format!("WAL parse error at line {}: {e}; raw={line:?}", idx + 1),
                    })?;
                apply_record(&mut next_id, &mut pending, &mut in_flight, record)?;
            }
        }

        let total: usize = pending.values().map(|q| q.len()).sum();

        // 3. Open WAL for append.
        let wal_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&wal_path)
            .map_err(io)?;
        let wal = BufWriter::new(wal_file);

        Ok(Self {
            base_path,
            cap,
            recorder,
            state: Mutex::new(Inner {
                next_id,
                pending,
                in_flight,
                total,
                wal,
            }),
        })
    }

    /// SCAFFOLD: true — store-fsync-durability-v0 DISTILL (Mandate 7).
    /// Open with an explicit [`FsyncBackend`] (ADR-0060 §3); the wal-fsync
    /// acceptance suite injects a `LyingFsyncBackend` (mechanism (b)).
    /// Mirrors `open`'s `(base_path, cap, recorder)` plus the backend.
    /// Inherent constructor, NOT a trait member — preserves C1. DELIVER
    /// replaces this RED scaffold and makes `open` delegate to it.
    pub fn open_with_fsync_backend<P: AsRef<Path>>(
        _base_path: P,
        _cap: usize,
        _recorder: Box<dyn MetricsRecorder + Send + Sync>,
        _fsync_backend: std::sync::Arc<dyn wal_recovery::FsyncBackend + Send + Sync>,
    ) -> Result<Self, EnqueueError> {
        panic!("__SCAFFOLD__ sluice::FileBackedQueue::open_with_fsync_backend RED scaffold (store-fsync-durability-v0 slice 05)")
    }

    /// Write current state to a snapshot file and truncate the
    /// WAL.
    pub fn snapshot(&self) -> Result<(), EnqueueError> {
        let mut state = self.state.lock().expect("poisoned");
        let snapshot_path = snapshot_path_of(&self.base_path);
        let wal_path = wal_path_of(&self.base_path);

        let pending_snap: Vec<TenantPending> = state
            .pending
            .iter()
            .map(|(tenant, queue)| TenantPending {
                tenant: tenant.clone(),
                messages: queue
                    .iter()
                    .map(|m| MessageSnapshot {
                        id: m.id.0,
                        tenant: m.tenant.clone(),
                        payload_hex: encode_hex(&m.payload),
                    })
                    .collect(),
            })
            .collect();
        let in_flight_snap: Vec<MessageSnapshot> = state
            .in_flight
            .values()
            .map(|m| MessageSnapshot {
                id: m.id.0,
                tenant: m.tenant.clone(),
                payload_hex: encode_hex(&m.payload),
            })
            .collect();
        let snap = Snapshot {
            next_id: state.next_id,
            pending: pending_snap,
            in_flight: in_flight_snap,
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

impl fmt::Debug for FileBackedQueue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileBackedQueue")
            .field("base_path", &self.base_path)
            .field("cap", &self.cap)
            .field("recorder", &"<opaque>")
            .finish()
    }
}

impl Queue for FileBackedQueue {
    fn enqueue(&self, tenant: &TenantId, payload: Vec<u8>) -> Result<MessageId, EnqueueError> {
        let mut state = self.state.lock().expect("poisoned");
        let queue = state.pending.entry(tenant.clone()).or_default();
        if queue.len() >= self.cap {
            self.recorder.record_enqueue(tenant, false);
            return Err(EnqueueError::Full {
                tenant: tenant.clone(),
                cap: self.cap,
            });
        }
        state.next_id += 1;
        let id = MessageId(state.next_id);
        let record = WalRecord::Enqueue {
            id: id.0,
            tenant: tenant.clone(),
            payload_hex: encode_hex(&payload),
        };
        append_wal(&mut state.wal, &record)?;
        let message = Message {
            id,
            tenant: tenant.clone(),
            payload,
        };
        state
            .pending
            .get_mut(tenant)
            .expect("inserted above")
            .push_back(message);
        state.total += 1;
        self.recorder.record_enqueue(tenant, true);
        Ok(id)
    }

    fn dequeue(&self, tenant: &TenantId) -> Option<Message> {
        let mut state = self.state.lock().expect("poisoned");
        let queue = state.pending.get_mut(tenant)?;
        let message = queue.pop_front()?;
        if queue.is_empty() {
            state.pending.remove(tenant);
        }
        state.total -= 1;
        let id = message.id;
        let record = WalRecord::Dequeue { id: id.0 };
        // Best-effort WAL write for state-mutating ops where the
        // trait has no error channel (dequeue / ack / nack).
        let _ = append_wal(&mut state.wal, &record);
        state.in_flight.insert(id, message.clone());
        self.recorder.record_dequeue(tenant);
        Some(message)
    }

    fn ack(&self, id: MessageId) {
        let mut state = self.state.lock().expect("poisoned");
        if let Some(msg) = state.in_flight.remove(&id) {
            let record = WalRecord::Ack { id: id.0 };
            let _ = append_wal(&mut state.wal, &record);
            self.recorder.record_ack(&msg.tenant);
        }
    }

    fn nack(&self, id: MessageId) {
        let mut state = self.state.lock().expect("poisoned");
        if let Some(message) = state.in_flight.remove(&id) {
            let tenant = message.tenant.clone();
            let record = WalRecord::Nack { id: id.0 };
            let _ = append_wal(&mut state.wal, &record);
            state
                .pending
                .entry(tenant.clone())
                .or_default()
                .push_front(message);
            state.total += 1;
            self.recorder.record_nack(&tenant);
        }
    }

    fn depth(&self, tenant: &TenantId) -> usize {
        let state = self.state.lock().expect("poisoned");
        state.pending.get(tenant).map(|q| q.len()).unwrap_or(0)
    }

    fn total_depth(&self) -> usize {
        let state = self.state.lock().expect("poisoned");
        state.total
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

fn io(e: std::io::Error) -> EnqueueError {
    EnqueueError::PersistenceFailed {
        reason: format!("io: {e}"),
    }
}

fn parse(e: serde_json::Error) -> EnqueueError {
    EnqueueError::PersistenceFailed {
        reason: format!("parse: {e}"),
    }
}

fn append_wal(wal: &mut BufWriter<File>, record: &WalRecord) -> Result<(), EnqueueError> {
    let line = serde_json::to_string(record).map_err(parse)?;
    wal.write_all(line.as_bytes()).map_err(io)?;
    wal.write_all(b"\n").map_err(io)?;
    wal.flush().map_err(io)?;
    Ok(())
}

fn apply_record(
    next_id: &mut u64,
    pending: &mut HashMap<TenantId, VecDeque<Message>>,
    in_flight: &mut HashMap<MessageId, Message>,
    record: WalRecord,
) -> Result<(), EnqueueError> {
    match record {
        WalRecord::Enqueue {
            id,
            tenant,
            payload_hex,
        } => {
            if id > *next_id {
                *next_id = id;
            }
            let payload = decode_hex(&payload_hex)?;
            let msg_id = MessageId(id);
            pending
                .entry(tenant.clone())
                .or_default()
                .push_back(Message {
                    id: msg_id,
                    tenant,
                    payload,
                });
        }
        WalRecord::Dequeue { id } => {
            // Find the message in pending by id and move to
            // in_flight. Scanning all tenants is fine at replay
            // time because each id appears exactly once.
            let msg_id = MessageId(id);
            let mut found: Option<(TenantId, Message)> = None;
            for (tenant, q) in pending.iter_mut() {
                if let Some(pos) = q.iter().position(|m| m.id == msg_id) {
                    let m = q.remove(pos).expect("just located");
                    found = Some((tenant.clone(), m));
                    break;
                }
            }
            if let Some((tenant, m)) = found {
                if let Some(q) = pending.get(&tenant) {
                    if q.is_empty() {
                        pending.remove(&tenant);
                    }
                }
                in_flight.insert(msg_id, m);
            }
        }
        WalRecord::Ack { id } => {
            in_flight.remove(&MessageId(id));
        }
        WalRecord::Nack { id } => {
            let msg_id = MessageId(id);
            if let Some(m) = in_flight.remove(&msg_id) {
                let tenant = m.tenant.clone();
                pending.entry(tenant).or_default().push_front(m);
            }
        }
    }
    Ok(())
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let hi = (b >> 4) & 0x0f;
        let lo = b & 0x0f;
        s.push(hex_char(hi));
        s.push(hex_char(lo));
    }
    s
}

fn hex_char(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + nibble - 10) as char,
        _ => unreachable!(),
    }
}

fn decode_hex(s: &str) -> Result<Vec<u8>, EnqueueError> {
    if s.len() % 2 != 0 {
        return Err(EnqueueError::PersistenceFailed {
            reason: format!("odd hex length: {}", s.len()),
        });
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_value(bytes[i])?;
        let lo = hex_value(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

fn hex_value(b: u8) -> Result<u8, EnqueueError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(EnqueueError::PersistenceFailed {
            reason: format!("bad hex digit: {:?}", b as char),
        }),
    }
}
