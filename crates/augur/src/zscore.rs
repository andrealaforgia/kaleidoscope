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

use std::time::SystemTime;

use aegis::TenantId;

use crate::anomaly::{Anomaly, AnomalyObserver};

/// Online z-score observer. After warm-up, an observation
/// whose absolute z-score reaches `threshold` is emitted as
/// an `Anomaly`. The baseline still updates on every
/// observation — sustained anomalies eventually move the
/// baseline towards the new regime, so the detector adapts
/// over time.
#[derive(Debug, Clone)]
pub struct ZScoreObserver {
    threshold: f64,
    min_samples: usize,
    samples: usize,
    mean: f64,
    m2: f64,
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
        }
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

        // Warm-up gate.
        if self.samples < self.min_samples {
            return None;
        }
        if z.abs() >= self.threshold {
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
