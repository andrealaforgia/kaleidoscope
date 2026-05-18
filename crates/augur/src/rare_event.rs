// Kaleidoscope Augur — rare-event observer (frequency baseline)
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

//! Rare-event observer over categorical streams.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::SystemTime;

use aegis::TenantId;

use crate::anomaly::{Anomaly, AnomalyObserver};
use crate::metrics::{MetricsRecorder, NoopRecorder};

/// Frequency-baseline rare-event detector. An event is
/// emitted as an anomaly when its observed frequency
/// (`count / total`) is below `rarity_threshold`. v0 fires
/// only on the *first* crossing per event — subsequent
/// observations of the same event do not re-emit.
///
/// Like [`crate::ZScoreObserver`], the optional `recorder`
/// self-instruments the observer. `record_observation(tenant)`
/// fires on every `observe()` call; `record_anomaly(tenant,
/// fraction)` fires on the first crossing. Default is
/// `NoopRecorder`; wire a real one via `with_recorder`.
#[derive(Clone)]
pub struct RareEventObserver {
    rarity_threshold: f64,
    min_samples: usize,
    total: u64,
    counts: HashMap<String, u64>,
    /// Events that have already fired an anomaly. Cleared
    /// on `reset`.
    already_fired: HashSet<String>,
    recorder: Arc<dyn MetricsRecorder + Send + Sync>,
}

impl std::fmt::Debug for RareEventObserver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RareEventObserver")
            .field("rarity_threshold", &self.rarity_threshold)
            .field("min_samples", &self.min_samples)
            .field("total", &self.total)
            .field("counts_size", &self.counts.len())
            .field("already_fired_size", &self.already_fired.len())
            .field("recorder", &"<opaque>")
            .finish()
    }
}

impl RareEventObserver {
    /// Build a rare-event observer.
    ///
    /// - `rarity_threshold` — fraction of total
    ///   observations below which an event is considered
    ///   rare (typical: 0.001 = 0.1%).
    /// - `min_samples` — warm-up window during which no
    ///   anomalies fire even if an event is rare.
    pub fn new(rarity_threshold: f64, min_samples: usize) -> Self {
        Self {
            rarity_threshold,
            min_samples,
            total: 0,
            counts: HashMap::new(),
            already_fired: HashSet::new(),
            recorder: Arc::new(NoopRecorder),
        }
    }

    /// Wire a recorder that receives every observation +
    /// anomaly event. Builder-style so existing callers using
    /// `RareEventObserver::new(...)` are unaffected.
    pub fn with_recorder(mut self, recorder: Arc<dyn MetricsRecorder + Send + Sync>) -> Self {
        self.recorder = recorder;
        self
    }

    /// Distinct events seen so far.
    pub fn vocabulary_size(&self) -> usize {
        self.counts.len()
    }
}

impl AnomalyObserver<String> for RareEventObserver {
    fn observe(
        &mut self,
        tenant: &TenantId,
        value: String,
        observed_at: SystemTime,
    ) -> Option<Anomaly<String>> {
        self.total += 1;
        // Increment the per-event count.
        let count = self.counts.entry(value.clone()).or_insert(0);
        *count += 1;
        let count_after = *count;

        // Self-instrumentation: every observation, including
        // those during warm-up and repeat observations of
        // already-fired events, lands in the recorder. Operators
        // see observation rate continuously.
        self.recorder.record_observation(tenant);

        // Warm-up gate.
        if (self.total as usize) < self.min_samples {
            return None;
        }

        // First-crossing semantics — once an event has
        // fired, ignore further observations of the same
        // event for v0.
        if self.already_fired.contains(&value) {
            return None;
        }

        let fraction = (count_after as f64) / (self.total as f64);
        if fraction <= self.rarity_threshold {
            self.already_fired.insert(value.clone());
            // Self-instrumentation: anomaly score is the
            // observed frequency fraction (0..1).
            self.recorder.record_anomaly(tenant, fraction);
            Some(Anomaly {
                tenant: tenant.clone(),
                value,
                score: fraction,
                observed_at,
                reason: "frequency below rarity_threshold",
            })
        } else {
            None
        }
    }

    fn samples_seen(&self) -> usize {
        self.total as usize
    }

    fn reset(&mut self) {
        self.total = 0;
        self.counts.clear();
        self.already_fired.clear();
    }
}
