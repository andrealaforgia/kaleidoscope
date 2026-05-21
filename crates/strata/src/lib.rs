// Kaleidoscope Strata — first-party profile storage engine
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

//! # Strata — first-party profile storage engine
//!
//! Strata v0 ships the [`ProfileStore`] trait + the in-memory
//! adapter [`InMemoryProfileStore`]. The v1 columnar adapter
//! (Arrow + Parquet + DataFusion + RocksDB + gimli/addr2line
//! symbolisation) lives behind the same trait.
//!
//! ## Public surface
//!
//! - [`ProfileStore`] — the trait every adapter implements
//! - [`InMemoryProfileStore`] — v0 in-process adapter
//! - [`Profile`], [`Sample`], [`Location`], [`Function`],
//!   [`Mapping`], [`SampleType`], [`ValueType`],
//!   [`ProfileBatch`] — pprof-shaped types
//! - [`ServiceName`], [`TimeRange`], [`Predicate`]
//! - [`IngestReceipt`], [`ProfileStoreError`]
//! - [`MetricsRecorder`], [`NoopRecorder`],
//!   [`CapturingRecorder`] — observability seam
//!
//! ## Architectural posture
//!
//! - Library only at v0. No daemon, no network.
//! - Per-tenant + per-service isolation keyed by
//!   `aegis::TenantId` × `ServiceName`.
//! - pprof-shaped types at the boundary; v1 aligns with the
//!   OpenTelemetry Profiles signal once that stabilises.
//! - Single index: `HashMap<(TenantId, ServiceName),
//!   Vec<Profile>>` sorted by `time_unix_nano`.
//! - In-memory only at v0; restart loses profiles.
//! - AGPL-3.0-or-later.

#![forbid(unsafe_code)]

mod file_backed;
mod metrics;
mod predicate;
mod profile;
mod store;

pub use file_backed::FileBackedProfileStore;
pub use metrics::{CapturingRecorder, MetricsRecorder, NoopRecorder, RecordedEvent};
pub use predicate::Predicate;
pub use profile::{
    Function, Location, Mapping, Profile, ProfileBatch, Sample, SampleType, ServiceName, TimeRange,
    ValueType,
};
pub use store::{InMemoryProfileStore, IngestReceipt, ProfileStore, ProfileStoreError};
