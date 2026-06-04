// Kaleidoscope Cinder — tiering port
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

//! # Cinder — tiering port
//!
//! Cinder v0 ships the [`TieringStore`] trait + the in-memory
//! adapter [`InMemoryTieringStore`]. The v1 adapter (S3 +
//! OpenDAL + Iceberg manifests) lives behind the same trait.
//!
//! ## Public surface
//!
//! - [`TieringStore`] — the trait every adapter implements
//! - [`InMemoryTieringStore`] — v0 in-process adapter
//! - [`Tier`] — `Hot` / `Warm` / `Cold`
//! - [`ItemId`] — generic string-keyed item identifier
//! - [`TierEntry`] — current tier + placed_at + migrated_at
//! - [`TierPolicy`] — slice 02: age-based lifecycle policy
//! - [`MigrateError`] — typed failure modes
//! - [`MetricsRecorder`], [`NoopRecorder`],
//!   [`CapturingRecorder`] — observability seam mirroring the
//!   storage engines
//!
//! ## Architectural posture
//!
//! - Library only at v0. No daemon, no network, no timer.
//!   The operator binary owns the periodic invocation of
//!   `evaluate_at` at v1.
//! - Cinder stores **tier metadata**, not payloads. The
//!   storage engines own the payloads.
//! - Per-tenant + per-item isolation keyed by
//!   `aegis::TenantId × ItemId`.
//! - Three tiers (`Hot`, `Warm`, `Cold`). The trait does
//!   not assume a physical substrate.
//! - In-memory only at v0; restart loses tier metadata.
//! - AGPL-3.0-or-later.

#![forbid(unsafe_code)]

mod file_backed;
mod metrics;
mod policy;
mod store;
mod tier;

pub use file_backed::FileBackedTieringStore;
// SCAFFOLD: true — store-fsync-durability-v0 DISTILL (Mandate 7).
// Re-export the durability seam (ADR-0060 §4 home: `wal-recovery`).
pub use metrics::{CapturingRecorder, MetricsRecorder, NoopRecorder, RecordedEvent};
pub use policy::TierPolicy;
pub use store::{InMemoryTieringStore, MigrateError, TieringStore};
pub use tier::{ItemId, Tier, TierEntry};
pub use wal_recovery::{
    fsync_probe, CountingFsyncBackend, FsyncBackend, FsyncProbeError, LyingFsyncBackend,
    RealFsyncBackend,
};
