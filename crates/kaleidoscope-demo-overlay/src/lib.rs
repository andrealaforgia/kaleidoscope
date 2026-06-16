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
//! [`ray::TraceStore`]. Slice B adds the **log** half: [`DemoLogOverlay`] over
//! lumen's [`lumen::LogStore`]. Slice C adds the **metric** half:
//! [`DemoMetricOverlay`] over pulse's [`pulse::MetricStore`]. All three reuse the
//! same [`Clock`] seam and the same demo identity (the log overlay's cause log
//! carries the IDENTICAL trace id + failed-checkout error span id the trace
//! overlay synthesises, so `with_logs` correlates the failing span with its
//! cause log).
//!
//! Determinism: time enters through the injected [`Clock`] seam, never through
//! ambient `SystemTime`, so synthesis is fully testable.

#![forbid(unsafe_code)]

mod clock;
mod identity;
mod log;
mod metric;
mod trace;

pub use clock::{Clock, SystemClock};
pub use log::DemoLogOverlay;
pub use metric::DemoMetricOverlay;
pub use trace::{DemoTraceOverlay, DEMO_SERVICE_NAME, DEMO_TENANT};

#[cfg(test)]
mod cross_overlay_coherence {
    //! The demo trace overlay and the demo log overlay must agree on the demo
    //! identity: the cause log the log overlay synthesises must carry the SAME
    //! trace id AND span id as the failed-checkout ERROR span the trace overlay
    //! synthesises. This is what lets the linked `with_logs` view (logs filtered
    //! by trace id, attached to a span by span id) correlate the failing span
    //! with its cause log. This test reads through BOTH overlays' driving ports
    //! and asserts the ids line up — purely behavioural, no constant peeking.

    use aegis::TenantId;

    use crate::{Clock, DemoLogOverlay, DemoTraceOverlay, DEMO_SERVICE_NAME, DEMO_TENANT};

    const NOW: u64 = 1_700_000_000_000_000_000;

    struct FixedClock {
        now_unix_nano: u64,
    }

    impl Clock for FixedClock {
        fn now_unix_nano(&self) -> u64 {
            self.now_unix_nano
        }
    }

    fn acme() -> TenantId {
        TenantId(DEMO_TENANT.to_string())
    }

    #[test]
    fn cause_log_trace_id_and_span_id_match_the_failed_checkout_error_span() {
        // Read the failed-checkout ERROR span through the trace overlay.
        let trace_overlay = DemoTraceOverlay::new(
            ray::InMemoryTraceStore::new(Box::new(ray::NoopRecorder)),
            FixedClock { now_unix_nano: NOW },
        );
        let spans = ray::TraceStore::query(
            &trace_overlay,
            &acme(),
            &ray::ServiceName::new(DEMO_SERVICE_NAME),
            ray::TimeRange::all(),
        )
        .expect("trace overlay demo query succeeds");
        let error_span = spans
            .iter()
            .find(|span| span.status.code == ray::StatusCode::Error)
            .expect("the failed-checkout error span is present");

        // Read the cause log through the log overlay (the by-trace_id read path
        // is `query(tenant, all)` then a trace_id filter).
        let log_overlay = DemoLogOverlay::new(
            lumen::InMemoryLogStore::new(Box::new(lumen::NoopRecorder)),
            FixedClock { now_unix_nano: NOW },
        );
        let records = lumen::LogStore::query(&log_overlay, &acme(), lumen::TimeRange::all())
            .expect("log overlay demo query succeeds");
        let cause_log = records
            .iter()
            .find(|record| record.trace_id == Some(error_span.trace_id.0))
            .expect("a cause log carrying the failed-checkout trace id is present");

        assert_eq!(
            cause_log.trace_id,
            Some(error_span.trace_id.0),
            "the cause log carries the failed-checkout trace id (with_logs filters on this)"
        );
        assert_eq!(
            cause_log.span_id,
            Some(error_span.span_id.0),
            "the cause log carries the failed-checkout ERROR span id (with_logs attaches on this)"
        );
    }
}
