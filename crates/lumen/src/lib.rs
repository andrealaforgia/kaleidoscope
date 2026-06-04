// Kaleidoscope Lumen — first-party log storage engine
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

//! # Lumen — first-party log storage engine
//!
//! Lumen v0 ships the [`LogStore`] trait + the in-memory adapter
//! [`InMemoryLogStore`]. The v1 columnar + durable adapter
//! (Arrow + Parquet + DataFusion + Tantivy + RocksDB) lives behind
//! the same trait.
//!
//! ## Public surface
//!
//! - [`LogStore`] — the trait every adapter implements
//! - [`InMemoryLogStore`] — v0 in-process adapter
//! - [`LogRecord`], [`LogBatch`], [`SeverityNumber`] — OTLP-shaped
//!   types at the trait boundary
//! - [`TimeRange`], [`Predicate`] — query inputs
//! - [`IngestReceipt`], [`LogStoreError`] — typed responses
//! - [`MetricsRecorder`], [`NoopRecorder`], [`CapturingRecorder`] —
//!   observability seam mirroring Sluice
//!
//! ## Architectural posture
//!
//! - Library only at v0. No daemon, no network.
//! - Per-tenant isolation keyed by `aegis::TenantId`.
//! - OTLP-shaped types at the boundary — no Lumen-specific
//!   projections.
//! - Time-range query at v0; rich predicates (service + severity)
//!   at slice 02.
//! - In-memory only at v0; restart loses data. The v1 adapter
//!   implements durability behind the same trait.
//! - AGPL-3.0-or-later.

#![forbid(unsafe_code)]

mod file_backed;
mod metrics;
mod predicate;
mod record;
mod store;

pub use file_backed::FileBackedLogStore;
// Re-export the durability seam (ADR-0060 §4: the family lives in
// `wal-recovery`; each store re-exports it so the acceptance suite drives
// `lumen::{FsyncBackend, LyingFsyncBackend, ...}`).
pub use metrics::{CapturingRecorder, MetricsRecorder, NoopRecorder, RecordedEvent};
pub use predicate::Predicate;
pub use record::{LogBatch, LogRecord, SeverityNumber, TimeRange};
pub use store::{InMemoryLogStore, IngestReceipt, LogStore, LogStoreError};
pub use wal_recovery::{
    fsync_probe, CountingFsyncBackend, FsyncBackend, FsyncProbeError, LyingFsyncBackend,
    RealFsyncBackend,
};
