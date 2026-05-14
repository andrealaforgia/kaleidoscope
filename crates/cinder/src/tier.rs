// Kaleidoscope Cinder — tier + item identity types
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

//! Tier + item identity types at the trait boundary.

use std::time::SystemTime;

/// Storage tier. The trait makes no assumption about the
/// physical substrate behind each tier — at v1 hot is
/// in-memory / RocksDB, warm is local Parquet, cold is
/// S3-via-OpenDAL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tier {
    Hot,
    Warm,
    Cold,
}

impl Tier {
    /// The next forward tier (for the age-based lifecycle).
    /// `Cold.next_forward()` is `None` — cold is the
    /// terminal tier under forward-only migration.
    pub fn next_forward(self) -> Option<Tier> {
        match self {
            Tier::Hot => Some(Tier::Warm),
            Tier::Warm => Some(Tier::Cold),
            Tier::Cold => None,
        }
    }
}

/// Generic item identifier. Storage engines pick their own
/// id scheme — Pulse can pass
/// `format!("{}/{}", metric_name, time_bucket)`, Ray can
/// pass `hex(trace_id)`, etc.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemId(pub String);

impl ItemId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One tier-metadata entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TierEntry {
    pub tier: Tier,
    pub placed_at: SystemTime,
    pub migrated_at: SystemTime,
}
