// Kaleidoscope Strata — pprof-shaped profile types
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

//! pprof-shaped profile types at the trait boundary.
//!
//! Field set mirrors the public `profile.proto`
//! ([github.com/google/pprof](https://github.com/google/pprof)).
//! v1 will align with the OpenTelemetry Profiles signal as
//! that stabilises upstream.

use std::collections::BTreeMap;

/// Service identity (stable key for the per-service index).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ServiceName(pub String);

impl ServiceName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Half-open time range `[start, end)` in nanoseconds since
/// the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRange {
    pub start_unix_nano: u64,
    pub end_unix_nano: u64,
}

impl TimeRange {
    pub fn new(start_unix_nano: u64, end_unix_nano: u64) -> Self {
        Self {
            start_unix_nano,
            end_unix_nano,
        }
    }

    pub fn all() -> Self {
        Self::new(0, u64::MAX)
    }

    pub fn contains(&self, t: u64) -> bool {
        t >= self.start_unix_nano && t < self.end_unix_nano
    }
}

/// pprof `ValueType` — (type, unit) pair indexing into the
/// string table.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ValueType {
    pub type_index: u32,
    pub unit_index: u32,
}

/// pprof `SampleType` — describes one column of sample values.
/// Same shape as `ValueType` plus the aggregation type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SampleType {
    pub value_type: ValueType,
    pub aggregation_temporality: u32,
}

/// pprof `Function` entry — function metadata indexed into
/// the string table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub id: u64,
    pub name_index: u32,
    pub system_name_index: u32,
    pub filename_index: u32,
    pub start_line: i64,
}

/// pprof `Mapping` entry — a loaded binary segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapping {
    pub id: u64,
    pub memory_start: u64,
    pub memory_limit: u64,
    pub file_offset: u64,
    pub filename_index: u32,
    pub build_id_index: u32,
}

/// pprof `Location` entry — an address inside a mapping plus
/// the function(s) that address resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub id: u64,
    pub mapping_id: u64,
    pub address: u64,
    /// Function ids inlined at this address (innermost last).
    pub function_ids: Vec<u64>,
}

/// One sample (a stack with its measured values).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    /// Location ids from innermost frame outward.
    pub location_ids: Vec<u64>,
    /// One value per `sample_type` column.
    pub values: Vec<i64>,
    /// Optional sample-level attributes (e.g. `thread.id`,
    /// `process.id`).
    pub attributes: BTreeMap<String, String>,
}

/// One pprof profile. Field set mirrors `profile.proto`. The
/// `profile_type` field is a Kaleidoscope-side hint (e.g.
/// `"cpu"`, `"heap"`, `"goroutine"`) used for the
/// `query_with` predicate; the underlying pprof's
/// `sample_type` column array is the authoritative source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub time_unix_nano: u64,
    pub duration_nanos: u64,
    pub profile_type: String,
    pub sample_type: Vec<SampleType>,
    pub samples: Vec<Sample>,
    pub locations: Vec<Location>,
    pub functions: Vec<Function>,
    pub mappings: Vec<Mapping>,
    /// pprof string table — every name / unit / filename / build
    /// id is indexed here.
    pub string_table: Vec<String>,
    pub resource_attributes: BTreeMap<String, String>,
    pub attributes: BTreeMap<String, String>,
}

impl Profile {
    /// `service.name` resource attribute, or empty string if
    /// missing.
    pub fn service_name(&self) -> &str {
        self.resource_attributes
            .get("service.name")
            .map(String::as_str)
            .unwrap_or("")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProfileBatch {
    pub profiles: Vec<Profile>,
}

impl ProfileBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_profiles(profiles: Vec<Profile>) -> Self {
        Self { profiles }
    }

    pub fn push(&mut self, profile: Profile) {
        self.profiles.push(profile);
    }

    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }
}
