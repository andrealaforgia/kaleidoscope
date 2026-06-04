// Kaleidoscope Sluice — queue port between Sieve and storage
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

//! # Sluice — queue port
//!
//! Sluice v0 ships the [`Queue`] trait + the in-memory adapter
//! [`InMemoryQueue`]. Future Kafka / NATS / Redpanda adapters live
//! in separate crates at v1 behind the same trait.
//!
//! ## Public surface
//!
//! - [`Queue`] — the trait every adapter implements
//! - [`InMemoryQueue`] — v0 in-process adapter (HashMap<TenantId, VecDeque<...>>)
//! - [`Message`], [`MessageId`] — message identity
//! - [`EnqueueError`] — typed failure modes
//! - [`MetricsRecorder`], [`NoopRecorder`], [`CapturingRecorder`] —
//!   observability seam for depth + counter emission
//!
//! ## Architectural posture
//!
//! - Library only at v0. No daemon, no network.
//! - Per-tenant FIFO ordering keyed by `aegis::TenantId`.
//! - At-least-once delivery: ack removes; nack returns to queue.
//! - Byte-agnostic payload (`Vec<u8>`); OTLP encode/decode is
//!   upstream/downstream respectively.
//! - Bounded queues with operator-visible `EnqueueError::Full`.
//! - AGPL-3.0-or-later.

#![forbid(unsafe_code)]

mod file_backed;
mod metrics;
mod queue;

pub use file_backed::FileBackedQueue;
// Re-export the durability seam (ADR-0060 §4 home: `wal-recovery`); the
// acceptance suite drives `sluice::{CountingFsyncBackend, FsyncBackend, ...}`.
pub use metrics::{CapturingRecorder, MetricsRecorder, NoopRecorder, RecordedEvent};
pub use queue::{EnqueueError, InMemoryQueue, Message, MessageId, Queue};
pub use wal_recovery::{
    fsync_probe, CountingFsyncBackend, FsyncBackend, FsyncProbeError, LyingFsyncBackend,
    RealFsyncBackend,
};
