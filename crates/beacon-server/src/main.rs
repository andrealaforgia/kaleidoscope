// Kaleidoscope Beacon — server binary
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

//! # beacon-server — the deployable form of `beacon`
//!
//! At workspace-membership commit time this is a `cargo check`-shaped
//! shell. Slice 01 DELIVER (per ADR-0037) wires:
//!
//! - A `tokio` runtime
//! - A `reqwest` client honouring the PromQL HTTP API contract
//! - A `RealScheduler` ticking the evaluator at each rule's interval
//! - A `SIGHUP` handler that triggers rule-set reload
//! - Optional OTLP telemetry exporter (env-gated)
//!
//! The library (`crates/beacon`) holds the load-bearing logic; this
//! binary owns the runtime concerns.

fn main() {
    // Reserved by ADR-0037. Slice 01 DELIVER replaces this with the
    // CLI argument parser plus the orchestrator loop.
    beacon::__workspace_membership_marker();
    eprintln!(
        "beacon-server placeholder. Implementation arrives at slice 01 DELIVER per ADR-0037."
    );
    std::process::exit(2);
}
