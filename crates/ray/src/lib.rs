// Kaleidoscope Ray — first-party trace storage engine
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

//! # Ray — first-party trace storage engine
//!
//! Ray v0 ships the [`TraceStore`] trait + the in-memory adapter
//! [`InMemoryTraceStore`]. The v1 columnar adapter
//! (`trace_id`-partitioned Iceberg-on-Parquet) lives behind the
//! same trait.
//!
//! ## Public surface
//!
//! - [`TraceStore`] — the trait every adapter implements
//! - [`InMemoryTraceStore`] — v0 in-process adapter
//! - [`Span`], [`SpanKind`], [`SpanStatus`], [`StatusCode`],
//!   [`SpanEvent`], [`SpanLink`], [`SpanBatch`] — OTLP-shaped types
//! - [`TraceId`], [`SpanId`], [`ServiceName`], [`TimeRange`]
//! - [`Predicate`] — query inputs for slice 02
//! - [`IngestReceipt`], [`TraceStoreError`] — typed responses
//! - [`MetricsRecorder`], [`NoopRecorder`], [`CapturingRecorder`] —
//!   observability seam mirroring Lumen + Pulse + Sluice
//!
//! ## Architectural posture
//!
//! - Library only at v0. No daemon, no network.
//! - Per-tenant isolation keyed by `aegis::TenantId`.
//! - Dual index: by `(tenant, trace_id)` for `get_trace`, by
//!   `(tenant, service)` for `query`. Spans are cloned on
//!   ingest to populate both indices.
//! - OTLP-shaped types at the boundary.
//! - Time-range query at v0; predicates (span_name / kind /
//!   status) at slice 02.
//! - In-memory only at v0; restart loses spans.
//! - AGPL-3.0-or-later.

#![forbid(unsafe_code)]

mod file_backed;
mod metrics;
mod predicate;
mod span;
mod store;

pub use file_backed::FileBackedTraceStore;
// SCAFFOLD: true — store-fsync-durability-v0 DISTILL (Mandate 7).
// Re-export the durability seam (ADR-0060 §4 home: `wal-recovery`).
pub use metrics::{CapturingRecorder, MetricsRecorder, NoopRecorder, RecordedEvent};
pub use predicate::Predicate;
pub use span::{
    ServiceName, Span, SpanBatch, SpanEvent, SpanId, SpanKind, SpanLink, SpanStatus, StatusCode,
    TimeRange, TraceId,
};
pub use store::{InMemoryTraceStore, IngestReceipt, TraceStore, TraceStoreError};
pub use wal_recovery::{
    fsync_probe, FsyncBackend, FsyncProbeError, LyingFsyncBackend, RealFsyncBackend,
};
