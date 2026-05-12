// Kaleidoscope Beacon — rule-evaluation + alerting engine
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

//! # Beacon — alerting engine
//!
//! Slice 01 walking skeleton: the smallest end-to-end pipeline that
//! takes a [`Rule`], a Prometheus HTTP query result, and a [`Sink`],
//! and produces an [`Incident`] emission when the rule's condition
//! has held for the configured `for_duration`.
//!
//! The public surface at slice 01 is intentionally narrow: enough to
//! prove the load → evaluate → emit pipeline works against real
//! HTTP, no more. CUE loading (slice 02), grouping + inhibition
//! (slice 03), multi-sink routing (slice 04), and SLO synthesis
//! (slice 05) each grow the same surface without restructuring it.
//!
//! ## Public surface (locked by ADR-0033)
//!
//! - [`Rule`] — declarative alert rule
//! - [`Severity`] — info / warning / critical
//! - [`Incident`] — operator-visible firing record
//! - [`RuleState`] — Inactive / Pending / Firing / Resolved
//! - [`QueryOutcome`] — what the Prom backend said
//! - [`transition`] — pure state machine transition
//! - [`Sink`] trait + [`WebhookSink`] adapter
//!
//! ## Architectural posture
//!
//! - **Library plus binary** (ADR-0033). Binary (`beacon-server`)
//!   lands in a follow-up commit; library is testable in isolation.
//! - **Pure transition** (ADR-0037). `transition(state, outcome,
//!   rule, now)` is total and side-effect-free.
//! - **Sink trait abstracts protocol** (ADR-0035). Slice 01 ships
//!   the webhook adapter; the other four (SMTP, Mattermost, Zulip,
//!   OnCall) arrive at slice 04.
//! - **AGPL-3.0-or-later.** Symmetric with the rest of the platform.

#![forbid(unsafe_code)]

pub mod inhibition;
pub mod loader;
mod sinks;
pub mod slo;
pub mod state_machine;
mod types;

pub use crate::inhibition::InhibitionResolver;
pub use crate::loader::{load_rules, LoadOutcome, LoaderDiagnostic, LoaderError};
pub use crate::sinks::{Sink, SinkError, SinkKind, WebhookSink};
pub use crate::slo::{synthesise_slo, Slo};
pub use crate::state_machine::{transition, Emission, QueryOutcome, RuleState};
pub use crate::types::{Incident, Rule, Severity, SinkConfig};

/// Slice-01 internal-only re-export. Removed at slice 02.
#[doc(hidden)]
pub fn __workspace_membership_marker() {}
