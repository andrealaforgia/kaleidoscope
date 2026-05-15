// Kaleidoscope Augur — AnomalyObserver trait + Anomaly event
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

//! Generic anomaly trait + emitted event type.

use std::time::SystemTime;

use aegis::TenantId;

/// One anomaly event. `T` is the observed value type.
/// `score` is detector-specific (z-score for numeric,
/// frequency-fraction for categorical) and lets downstream
/// consumers sort or rank.
#[derive(Debug, Clone, PartialEq)]
pub struct Anomaly<T> {
    pub tenant: TenantId,
    pub value: T,
    pub score: f64,
    pub observed_at: SystemTime,
    pub reason: &'static str,
}

/// Streaming online anomaly observer. Generic over the
/// observed signal type.
///
/// `observe` updates the observer's baseline and returns
/// `Some(Anomaly)` if the observation crosses the
/// detector's threshold, `None` otherwise.
pub trait AnomalyObserver<T> {
    fn observe(
        &mut self,
        tenant: &TenantId,
        value: T,
        observed_at: SystemTime,
    ) -> Option<Anomaly<T>>;

    /// Number of observations seen so far. Useful for
    /// warm-up assertions in tests.
    fn samples_seen(&self) -> usize;

    /// Reset the observer to its pre-warm-up state.
    fn reset(&mut self);
}
