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
//! Pulse v0 ships the [`MetricStore`] trait + the in-memory adapter
//! [`InMemoryMetricStore`]. The v1 columnar + durable adapter
//! (Arrow + Parquet + DataFusion + Prometheus TSDB block) lives
//! behind the same trait.
//!
//! ## Public surface
//!
//! - [`MetricStore`] — the trait every adapter implements
//! - [`InMemoryMetricStore`] — v0 in-process adapter
//! - [`FileBackedMetricStore`] — v1 file-backed adapter (NDJSON
//!   WAL + JSON snapshot, restart-safe). Same trait, additive
//!   error variant. Fourth file-backed v1 adapter in the
//!   platform plane after Cinder, Sluice, and Lumen.
//! - [`Metric`], [`MetricPoint`], [`MetricKind`], [`MetricName`],
//!   [`MetricBatch`] — OTLP-shaped types (Serialize + Deserialize
//!   so v1 can persist them)
//! - [`TimeRange`], [`Predicate`] — query inputs
//! - [`IngestReceipt`], [`MetricStoreError`] — typed responses
//! - [`MetricsRecorder`], [`NoopRecorder`], [`CapturingRecorder`] —
//!   observability seam mirroring Lumen + Sluice
//!
//! ## Architectural posture
//!
//! - Library only at v0/v1. No daemon, no network.
//! - Per-tenant isolation keyed by `aegis::TenantId`.
//! - Gauge + sum (number points) only at v0; histogram /
//!   exponential histogram / summary land at v2 alongside the
//!   columnar substrate (Arrow + Parquet + DataFusion +
//!   Prometheus TSDB block) and PromQL.
//! - OTLP-shaped types at the boundary — no Pulse-specific
//!   projections.
//! - Time-range query at v0; rich predicates (service + label_eq)
//!   at slice 02.
//! - v1 file-backed adapter (NDJSON WAL + JSON snapshot) survives
//!   process restart. v2 columnar adapter behind the same trait.
//! - AGPL-3.0-or-later.

#![forbid(unsafe_code)]

mod file_backed;
mod metric;
mod metrics;
mod predicate;
mod store;

pub use file_backed::FileBackedMetricStore;
pub use metric::{Metric, MetricBatch, MetricKind, MetricName, MetricPoint, TimeRange};
pub use metrics::{CapturingRecorder, MetricsRecorder, NoopRecorder, RecordedEvent};
pub use predicate::Predicate;
pub use store::{InMemoryMetricStore, IngestReceipt, MetricStore, MetricStoreError};
