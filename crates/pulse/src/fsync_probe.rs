// Kaleidoscope Pulse — Earned-Trust fsync-honesty probe (re-export shim)
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

//! Earned-Trust fsync-honesty probe (ADR-0049) — re-export shim.
//!
//! Under ADR-0060 §4 the `FsyncBackend` family + `fsync_probe` MOVED into
//! the shared leaf crate `crates/wal-recovery` so all seven file-backed
//! pillars reuse one durability seam INWARD (lumen, ray, … must not
//! depend on the metrics pillar to fsync). The move is mechanical and
//! behaviour-preserving: the lie modes (`no_op`/`truncating`/
//! `byte_flipping`) and the `substrate_descriptor` mapping are carried
//! verbatim. pulse re-exports the same names from here so its public
//! surface and the gateway's `pulse::{fsync_probe, FsyncBackend, …}`
//! imports stay byte-identical (Gate 2 `cargo public-api`).

pub use wal_recovery::{
    fsync_probe, FsyncBackend, FsyncProbeError, LyingFsyncBackend, RealFsyncBackend,
};
