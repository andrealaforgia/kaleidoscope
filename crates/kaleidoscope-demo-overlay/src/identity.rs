// Kaleidoscope demo overlay — the shared demo identity
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

//! The ONE demo identity every overlay shares (ADR-0079).
//!
//! The trace overlay (slice A), the log overlay (slice B) and the metric
//! overlay (slice C) must agree byte-for-byte on WHO the demo is — the same
//! tenant, the same `service.name`, and crucially the SAME failed-checkout
//! trace id and error span id — so the linked view's `with_logs` correlation
//! (logs filtered by trace id) shows the failing span and its cause log
//! together. These constants are the single source of truth for that identity;
//! every overlay references them rather than re-spelling divergent literals.

use aegis::TenantId;
use ray::{SpanId, TraceId};

/// The single local tenant the managed instance runs under (W3, ADR-0077). The
/// demo is scoped by SERVICE identity within this tenant, not by a separate
/// tenant (the auth-off read path is pinned to one query tenant; ADR-0078/0079).
pub const DEMO_TENANT: &str = "acme";

/// The `service.name` the demo telemetry is filed under (matches the
/// `telemetrygen` seed's `DEFAULT_SERVICE_NAME`). Synthesised demo records
/// carry this as their `service.name` resource attribute so a service-scoped
/// read engages, and a foreign-service read excludes them.
pub const DEMO_SERVICE_NAME: &str = "kaleidoscope-demo";

/// The pinned failed-checkout demo trace id `4bf92f3577b34da6a3ce929d0e0e4736`
/// (ADR-0077 F3, reused verbatim). The trace overlay's failed-checkout span,
/// the log overlay's cause log, and the seed all carry this exact id, so a
/// by-trace_id read correlates the span with its cause log.
pub const FAILED_CHECKOUT_TRACE_ID_BYTES: [u8; 16] = [
    0x4b, 0xf9, 0x2f, 0x35, 0x77, 0xb3, 0x4d, 0xa6, 0xa3, 0xce, 0x92, 0x9d, 0x0e, 0x0e, 0x47, 0x36,
];

/// The span id of the failed-checkout ERROR span the trace overlay synthesises
/// (`00f067aa0ba902b8`, the parent context id + 1). The log overlay's cause log
/// carries this same span id so the `with_logs` view attaches the cause log to
/// exactly the failing span, mirroring the seed's "log emitted inside the demo
/// span" correlation.
pub const FAILED_CHECKOUT_SPAN_ID_BYTES: [u8; 8] = [0x00, 0xf0, 0x67, 0xaa, 0x0b, 0xa9, 0x02, 0xb8];

/// The ray-typed view of [`FAILED_CHECKOUT_TRACE_ID_BYTES`] for the trace overlay.
pub const FAILED_CHECKOUT_TRACE_ID: TraceId = TraceId(FAILED_CHECKOUT_TRACE_ID_BYTES);

/// The ray-typed view of [`FAILED_CHECKOUT_SPAN_ID_BYTES`] for the trace overlay.
pub const FAILED_CHECKOUT_SPAN_ID: SpanId = SpanId(FAILED_CHECKOUT_SPAN_ID_BYTES);

/// The human-readable Error status message on the failed-checkout demo span and
/// the body of its cause log — the WHERE and the WHY tell one story, reused
/// verbatim from the seed vocabulary (ADR-0077).
pub const FAILED_CHECKOUT_ERROR_MESSAGE: &str = "checkout failed: card declined";

/// True when `tenant` is the demo tenant — the cheap half of every overlay's
/// O(1) demo identity short-circuit. Shared so all three overlays gate on the
/// SAME tenant test.
pub fn is_demo_tenant(tenant: &TenantId) -> bool {
    tenant.0 == DEMO_TENANT
}
