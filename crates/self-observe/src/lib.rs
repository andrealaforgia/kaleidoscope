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
//! v0 ships [`LumenToPulseRecorder`]: implements
//! `lumen::MetricsRecorder` and writes each `record_ingest` /
//! `record_query` call as a point into a `pulse::MetricStore`.
//! Operators wire it as the recorder Lumen uses; the events
//! become queryable metric points in a (typically separate)
//! Pulse instance.
//!
//! The same pattern fits every other crate's `MetricsRecorder`.
//! Future bridges follow the naming convention
//! `XxxToPulseRecorder`. v2 may add an `OtelOtlpRecorder` family
//! that exports to a real OTLP collector via
//! `opentelemetry-otlp`; v1 stays inside the workspace because
//! the in-workspace bridge teaches the contract clearly without
//! a heavy dependency.

#![forbid(unsafe_code)]

mod lumen_bridge;

pub use lumen_bridge::LumenToPulseRecorder;
