// Kaleidoscope Augur — z-score observer (Welford's algorithm)
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

//! z-score observer using Welford's online algorithm.
//!
//! Welford's algorithm maintains running mean and M2 (sum
//! of squared deviations) in O(1) per sample without
//! storing the full history. The variance is `M2 / (n-1)`
//! once `n >= 2`.
//!
//! Reference: B. P. Welford (1962),
//! [Note on a method for calculating corrected sums of
//! squares and products](https://doi.org/10.1080/00401706.1962.10490022).

use std::sync::Arc;
use std::time::SystemTime;

use aegis::TenantId;

use crate::anomaly::{Anomaly, AnomalyObserver};
use crate::metrics::{MetricsRecorder, NoopRecorder};

/// Online z-score observer. After warm-up, an observation
/// whose absolute z-score reaches `threshold` is emitted as
/// an `Anomaly`. The baseline still updates on every
/// observation — sustained anomalies eventually move the
/// baseline towards the new regime, so the detector adapts
/// over time.
///
/// The optional `recorder` field self-instruments the
/// observer: `record_observation(tenant)` fires on every
/// `observe()` call, and `record_anomaly(tenant, score)` fires
/// when the threshold is crossed. Default is `NoopRecorder`,
/// so observers built with `new` alone behave exactly as
/// before. Wire a real recorder via `with_recorder` to feed
/// the operator's Pulse store or OTLP-JSON stream.
#[derive(Clone)]
pub struct ZScoreObserver {
    threshold: f64,
    min_samples: usize,
    samples: usize,
    mean: f64,
    m2: f64,
    recorder: Arc<dyn MetricsRecorder + Send + Sync>,
}

impl std::fmt::Debug for ZScoreObserver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZScoreObserver")
            .field("threshold", &self.threshold)
            .field("min_samples", &self.min_samples)
            .field("samples", &self.samples)
            .field("mean", &self.mean)
            .field("m2", &self.m2)
            .field("recorder", &"<opaque>")
            .finish()
    }
}

impl ZScoreObserver {
    /// Build a z-score observer.
    ///
    /// - `threshold` — absolute z-score at which an
    ///   observation fires an anomaly (typical: 3.0).
    /// - `min_samples` — warm-up window during which no
    ///   anomalies fire even if `|z| > threshold`. Must be
    ///   ≥ 2 for the variance to be defined.
    pub fn new(threshold: f64, min_samples: usize) -> Self {
        Self {
            threshold,
            min_samples: min_samples.max(2),
            samples: 0,
            mean: 0.0,
            m2: 0.0,
            recorder: Arc::new(NoopRecorder),
        }
    }

    /// Wire a recorder that receives every observation +
    /// anomaly event. Builder-style so that existing callers
    /// using `ZScoreObserver::new(...)` are unaffected.
    pub fn with_recorder(mut self, recorder: Arc<dyn MetricsRecorder + Send + Sync>) -> Self {
        self.recorder = recorder;
        self
    }

    /// Current running mean. `0.0` before any observations.
    pub fn mean(&self) -> f64 {
        self.mean
    }

    /// Current sample standard deviation. `0.0` if
    /// `samples < 2`.
    pub fn stddev(&self) -> f64 {
        if self.samples < 2 {
            0.0
        } else {
            (self.m2 / (self.samples as f64 - 1.0)).sqrt()
        }
    }
}

impl AnomalyObserver<f64> for ZScoreObserver {
    fn observe(
        &mut self,
        tenant: &TenantId,
        value: f64,
        observed_at: SystemTime,
    ) -> Option<Anomaly<f64>> {
        // Compute the z-score against the *current*
        // baseline BEFORE we incorporate this value. That
        // gives us "this value is anomalous relative to
        // what we have seen so far", which matches
        // intuition.
        let z = if self.samples >= 2 {
            let sd = self.stddev();
            if sd > 0.0 {
                (value - self.mean) / sd
            } else {
                0.0
            }
        } else {
            0.0
        };

        // Welford's update.
        self.samples += 1;
        let delta = value - self.mean;
        self.mean += delta / (self.samples as f64);
        let delta2 = value - self.mean;
        self.m2 += delta * delta2;

        // Self-instrumentation: every observation is recorded
        // regardless of whether it crosses the threshold,
        // including those still in the warm-up window. Operators
        // therefore see the observation rate continuously, even
        // before the detector has decided anything.
        self.recorder.record_observation(tenant);

        // Warm-up gate.
        if self.samples < self.min_samples {
            return None;
        }
        if z.abs() >= self.threshold {
            // Self-instrumentation: record the anomaly with its
            // continuous score. The bridge maps this to
            // augur.anomaly.score (Gauge, asDouble).
            self.recorder.record_anomaly(tenant, z);
            Some(Anomaly {
                tenant: tenant.clone(),
                value,
                score: z,
                observed_at,
                reason: "z-score >= threshold",
            })
        } else {
            None
        }
    }

    fn samples_seen(&self) -> usize {
        self.samples
    }

    fn reset(&mut self) {
        self.samples = 0;
        self.mean = 0.0;
        self.m2 = 0.0;
    }
}
