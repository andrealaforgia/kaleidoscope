// Kaleidoscope Aegis — tenancy + auth + audit
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

//! # Aegis — tenancy + auth + audit
//!
//! Aegis v0 takes a JWT, validates it against a configured issuer +
//! JWKS, looks the carried `tenant_id` claim up in an
//! operator-authored TOML catalogue, and returns a typed
//! [`TenantContext`] with the tenant id + role. Every validation
//! attempt emits a stable [`tracing`] audit event.
//!
//! ## Public surface
//!
//! - [`TenantId`] — newtype wrapper around `String`
//! - [`Role`] — `Viewer | Operator`
//! - [`TenantContext`] — what consumers receive on success
//! - [`ValidationError`] — typed failure modes
//! - [`Validator`] — pre-loaded with issuer + audience + key +
//!   catalogue; the `validate` method is the slice 01 entry point
//! - [`TenantCatalogue`] — slice 02's typed catalogue
//! - [`load_catalogue`] — slice 02's TOML loader
//!
//! ## Architectural posture
//!
//! - Library only at v0. SPIFFE/SPIRE control plane is v1.
//! - JWT validation against a configured issuer + JWKS pre-loaded
//!   at construction time. No network at validation time.
//! - Tenant catalogue is a TOML file at v0. FoundationDB swap is v1.
//! - Two roles at v0: `viewer` + `operator`. Full OPA RBAC is v1.
//! - Audit log via `tracing::info!` (allow) and `tracing::warn!`
//!   (deny) with stable field names.
//! - AGPL-3.0-or-later.

#![forbid(unsafe_code)]

mod catalogue;
mod validator;

pub use catalogue::{load_catalogue, CatalogueError, TenantCatalogue, TenantRecord};
pub use validator::{Role, TenantContext, TenantId, ValidationError, Validator, ValidatorConfig};
