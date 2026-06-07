// Kaleidoscope Pulse — first-party metric storage engine
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

//! # Pulse — first-party metric storage engine
//!
//! Pulse v0 ships the [`MetricStore`] trait, the in-memory adapter
//! [`InMemoryMetricStore`], **and** the durable JSON-over-WAL
//! [`FileBackedMetricStore`] (line-delimited JSON over a write-ahead
//! log + atomic snapshot, fsync-durable). A columnar substrate
//! (Arrow / Parquet / DataFusion / Prometheus TSDB block) **is a future
//! direction**, not yet built, behind the same trait.
//!
//! ## Public surface
//!
//! - [`MetricStore`] — the trait every adapter implements
//! - [`InMemoryMetricStore`] — v0 in-process (volatile) adapter
//! - [`FileBackedMetricStore`] — v0 durable JSON-over-WAL adapter
//! - [`Metric`], [`MetricPoint`], [`MetricKind`], [`MetricName`],
//!   [`MetricBatch`] — OTLP-shaped types
//! - [`TimeRange`], [`Predicate`] — query inputs
//! - [`IngestReceipt`], [`MetricStoreError`] — typed responses
//! - [`MetricsRecorder`], [`NoopRecorder`], [`CapturingRecorder`] —
//!   observability seam mirroring Lumen + Sluice
//!
//! ## Architectural posture
//!
//! - Library only at v0. No daemon, no network.
//! - Per-tenant isolation keyed by `aegis::TenantId`.
//! - Gauge + sum (number points) only at v0; histogram /
//!   exponential histogram / summary land at v1 alongside the
//!   columnar substrate and PromQL.
//! - OTLP-shaped types at the boundary — no Pulse-specific
//!   projections.
//! - Time-range query at v0; rich predicates (service + label_eq)
//!   at slice 02.
//! - v0 ships both the volatile [`InMemoryMetricStore`] **and** the
//!   durable [`FileBackedMetricStore`] (JSON-over-WAL + atomic
//!   snapshot) that **survives process restart** (fsync-durable).
//!   `InMemoryMetricStore` is volatile — it loses points on restart;
//!   `FileBackedMetricStore` does not.
//! - AGPL-3.0-or-later.

#![forbid(unsafe_code)]

mod file_backed;
mod fsync_probe;
mod metric;
mod metrics;
mod predicate;
mod store;

/// Per-tenant cardinality watermark (ADR-0051). The maximum number of
/// distinct `SeriesKey`s a single tenant may hold in any pulse store
/// instance. A NEW `SeriesKey` insertion above this ceiling is refused
/// at the shared `apply_ingest` seam on the live ingest path; WAL
/// replay is never gated.
pub const MAX_SERIES_PER_TENANT: usize = 10_000;

pub use file_backed::FileBackedMetricStore;
pub use fsync_probe::{
    fsync_probe, FsyncBackend, FsyncProbeError, LyingFsyncBackend, RealFsyncBackend,
};
pub use metric::{Metric, MetricBatch, MetricKind, MetricName, MetricPoint, TimeRange};
pub use metrics::{CapturingRecorder, MetricsRecorder, NoopRecorder, RecordedEvent};
pub use predicate::Predicate;
pub use store::{InMemoryMetricStore, IngestReceipt, MetricStore, MetricStoreError};
