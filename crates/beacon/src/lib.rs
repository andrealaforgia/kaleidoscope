// Kaleidoscope Beacon — CUE-defined rule evaluation + alerting engine
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

//! # Beacon — Kaleidoscope's alerting engine
//!
//! Beacon evaluates CUE-defined alert rules and SLO burn-rate rules
//! against any OTel-compatible PromQL backend and emits incidents to
//! standard sinks (webhook, SMTP, Mattermost, Zulip, Grafana OnCall).
//!
//! ## Public surface (locked by ADR-0033)
//!
//! The library exposes a pure-function evaluator plus the `Sink`
//! trait. The binary (`beacon-server`) wires the evaluator to a real
//! HTTP client, a Tokio-based scheduler, and a `SIGHUP` reload
//! handler.
//!
//! At workspace-membership commit time this crate is intentionally
//! empty. The DISTILL skeleton (public types, method stubs that panic
//! with `unimplemented!()`) and the five acceptance test files land in
//! a follow-up commit alongside the CI workflow exclude rules.
//!
//! ## Architectural posture
//!
//! - **Library plus binary** (ADR-0033). The library has no Tokio
//!   runtime types in its public API; the binary owns the runtime.
//! - **Pure evaluator** (ADR-0037). `(rules, fetch_fn, now, state)
//!   -> EvaluationResult`. Property-testable without a runtime.
//! - **CUE schema with file + line + field diagnostics** (ADR-0034).
//! - **`Sink` trait with five adapter implementations** (ADR-0035).
//!   Header-redaction invariant shared with Prism's `queryRange`.
//! - **MWMBR synthesis from Google SRE workbook** (ADR-0036).
//! - **AGPL-3.0-or-later.** Symmetric with Aperture, Sieve, Codex.

#![forbid(unsafe_code)]

/// Placeholder marker function. Replaced by the DISTILL skeleton's
/// public API surface (CUE loader, evaluator, sink trait) in a
/// follow-up commit. Kept at workspace-membership commit time so the
/// crate compiles and the workspace `cargo check` is GREEN.
#[doc(hidden)]
pub fn __workspace_membership_marker() {}
