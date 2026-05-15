// Kaleidoscope integration suite
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

//! Kaleidoscope cross-crate integration suite.
//!
//! This crate intentionally exposes no public surface. Its only
//! purpose is to host integration tests that prove multiple
//! platform crates compose. The tests live under `tests/`.
//!
//! The first integration test
//! (`v1_three_adapters_compose_under_restart`) proves that:
//!
//! - Cinder v1 (`FileBackedTieringStore`)
//! - Sluice v1 (`FileBackedQueue`)
//! - Lumen v1 (`FileBackedLogStore`)
//!
//! all share `aegis::TenantId` as the cross-crate tenant identity
//! contract, and all survive a process restart together with
//! consistent state across the three adapters.

#![forbid(unsafe_code)]

// Re-export TenantId so downstream test code can write
// `integration_suite::TenantId` if that ever helps readability.
// Otherwise this crate has no public API.
pub use aegis::TenantId;
