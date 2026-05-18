// Kaleidoscope self-observability
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

//! # Kaleidoscope self-observability
//!
//! Bridges that turn one crate's `MetricsRecorder` events into
//! another crate's storage. Kaleidoscope observes itself using
//! its own primitives.
//!
//! ## In-workspace bridge
//!
//! [`LumenToPulseRecorder`] implements `lumen::MetricsRecorder`
//! and writes each `record_ingest` / `record_query` call as a
//! point into a `pulse::MetricStore`. Operators wire it as the
//! recorder Lumen uses; the events become queryable metric
//! points in a (typically separate) Pulse instance.
//!
//! ## Cross-process bridge
//!
//! [`LumenToOtlpJsonWriter`] implements the same trait but
//! writes one line of OTLP-JSON `ResourceMetrics` per event to
//! a generic `Write`. A sidecar process that consumes the
//! stream can wrap it in a `MetricsData` envelope and POST it
//! to any OTLP/HTTP collector. The writer is sync, has no
//! tokio dependency, and pulls in only `serde` + `serde_json`
//! beyond what the workspace already carried.
//!
//! ## In-workspace bridge: Cinder
//!
//! [`CinderToPulseRecorder`] implements `cinder::MetricsRecorder`
//! and writes each `record_place`, `record_migrate`, and
//! `record_evaluate` call as a single-point `MetricBatch` into a
//! `pulse::MetricStore`. Metric names follow `cinder.<event>.count`
//! mirroring the Lumen bridge's `lumen.<event>.count` shape.
//! Tier names appear as point attributes (`tier`, `from`, `to`)
//! so an operator dashboard can break tier-migration rate down
//! per transition.
//!
//! ## In-workspace bridge: Sluice
//!
//! [`SluiceToPulseRecorder`] and [`SluiceToOtlpJsonWriter`]
//! implement `sluice::MetricsRecorder` and follow the same
//! template. Metric names: `sluice.enqueue.count` (with
//! `accepted=true|false` attribute distinguishing successful
//! enqueues from `EnqueueError::Full` rejections),
//! `sluice.dequeue.count`, `sluice.ack.count`,
//! `sluice.nack.count`. The `accepted` attribute makes
//! capacity-based back-pressure visible per-tenant in the
//! same OTLP stream as Lumen and Cinder events.
//!
//! ## Future
//!
//! The same trait pattern fits every other crate's
//! `MetricsRecorder`. Augur, Ray, Strata bridges follow
//! `XxxToPulseRecorder` / `XxxToOtlpJsonWriter` naming.
//! A full `opentelemetry-otlp` push exporter with tokio + tonic
//! lands at v2 when a real deployment needs it.

#![forbid(unsafe_code)]

mod cinder_bridge;
mod cinder_otlp_json;
mod lumen_bridge;
mod lumen_otlp_json;
mod sluice_bridge;
mod sluice_otlp_json;

pub use cinder_bridge::CinderToPulseRecorder;
pub use cinder_otlp_json::CinderToOtlpJsonWriter;
pub use lumen_bridge::LumenToPulseRecorder;
pub use lumen_otlp_json::LumenToOtlpJsonWriter;
pub use sluice_bridge::SluiceToPulseRecorder;
pub use sluice_otlp_json::SluiceToOtlpJsonWriter;
