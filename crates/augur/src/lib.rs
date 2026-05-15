// Kaleidoscope Augur — anomaly-detection layer
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

//! # Augur — anomaly-detection layer
//!
//! Augur v0 ships the [`AnomalyObserver`] trait + two concrete
//! observers:
//!
//! - [`ZScoreObserver`] — `AnomalyObserver<f64>`, Welford's
//!   online mean/variance, configurable z-score threshold.
//! - [`RareEventObserver`] — `AnomalyObserver<String>`,
//!   frequency-baseline rarity detection.
//!
//! The Phase 9 substrate (BOCPD, sentence-transformer
//! embeddings, vLLM/llama.cpp-served Qwen/Mistral
//! summarisation) lands at v1 behind the same trait.
//!
//! ## Public surface
//!
//! - [`AnomalyObserver`] — the generic trait
//! - [`Anomaly`] — the emitted event
//! - [`ZScoreObserver`] — v0 numeric detector
//! - [`RareEventObserver`] — v0 categorical detector
//! - [`MetricsRecorder`], [`NoopRecorder`],
//!   [`CapturingRecorder`] — observability seam
//!
//! ## Architectural posture
//!
//! - Library only at v0. No daemon, no network, no ML
//!   libraries.
//! - Per-tenant baselines; one observer per
//!   `(tenant, signal)`.
//! - Streaming online algorithm. `observe` is O(1) per
//!   sample for `ZScoreObserver` and amortised O(1) for
//!   `RareEventObserver`.
//! - Hand-rolled numerical methods (Welford, frequency
//!   table) keep the v0 dependency graph tiny.
//! - AGPL-3.0-or-later.

#![forbid(unsafe_code)]

mod anomaly;
mod metrics;
mod rare_event;
mod zscore;

pub use anomaly::{Anomaly, AnomalyObserver};
pub use metrics::{CapturingRecorder, MetricsRecorder, RecordedEvent};
pub use rare_event::RareEventObserver;
pub use zscore::ZScoreObserver;

// Re-export NoopRecorder for use in production / benchmarks
// where the user does not want to wire OTLP metrics yet.
pub use metrics::NoopRecorder;
