// Kaleidoscope demo overlay — always-current, store-free, read-side
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

//! # `kaleidoscope-demo-overlay` — the always-current demo, synthesised at read time.
//!
//! Per ADR-0079, the managed instance's demo must be **current on any day**,
//! **never accumulate**, and **never put the Customer's real data at risk**. The
//! overlay satisfies all three *by construction*: it decorates the per-signal
//! store **read** traits and, for the hardcoded demo service identity only,
//! SYNTHESISES the demo telemetry at query time with **now-relative**
//! timestamps; for every other read it delegates straight through to the wrapped
//! store (an O(1) identity short-circuit). Nothing is ever written — the demo
//! has **no write path**, so it cannot physically reach the durable stores.
//!
//! Slice A ships the **trace** half: [`DemoTraceOverlay`] over ray's
//! [`ray::TraceStore`]. The log and metric overlays (slices B/C) reuse the same
//! [`Clock`] seam.
//!
//! Determinism: time enters through the injected [`Clock`] seam, never through
//! ambient `SystemTime`, so synthesis is fully testable.

#![forbid(unsafe_code)]

mod clock;
mod trace;

pub use clock::{Clock, SystemClock};
pub use trace::{DemoTraceOverlay, DEMO_SERVICE_NAME, DEMO_TENANT};
