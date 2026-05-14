// Kaleidoscope Cinder — lifecycle policy
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

//! Lifecycle policy. v0 ships an age-based policy with two
//! configurable thresholds. Size-based / query-rate-based /
//! cost-based policies land at v1.

use std::time::Duration;

use crate::tier::Tier;

/// Age-based migration policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TierPolicy {
    hot_to_warm: Duration,
    warm_to_cold: Duration,
}

impl TierPolicy {
    /// Build an age-based policy. Items in `Hot` move to
    /// `Warm` once their age (relative to `migrated_at`)
    /// reaches `hot_to_warm`; items in `Warm` move to
    /// `Cold` once their age reaches `warm_to_cold`.
    pub fn age_based(hot_to_warm: Duration, warm_to_cold: Duration) -> Self {
        Self {
            hot_to_warm,
            warm_to_cold,
        }
    }

    /// The threshold for `from -> next_forward(from)`, or
    /// `None` if no automatic migration applies from this
    /// tier.
    pub fn threshold_from(&self, from: Tier) -> Option<Duration> {
        match from {
            Tier::Hot => Some(self.hot_to_warm),
            Tier::Warm => Some(self.warm_to_cold),
            Tier::Cold => None,
        }
    }
}
