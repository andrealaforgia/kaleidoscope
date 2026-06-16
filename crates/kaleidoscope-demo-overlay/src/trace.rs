// Kaleidoscope demo overlay — the trace overlay (slice A)
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

//! The trace half of the always-current demo overlay (ADR-0079 slice A).
//!
//! [`DemoTraceOverlay`] decorates any [`TraceStore`]. For the demo service
//! identity under the demo tenant it SYNTHESISES the failed-checkout trace plus
//! the three healthy traces at query time with now-relative timestamps, merging
//! them into the result; every other read short-circuits straight to the wrapped
//! store. The demo has **no write path** — `ingest` only delegates real writes
//! to the inner store, and the synthesis is read-time only, so the demo can
//! never be persisted.

use std::collections::BTreeMap;

use aegis::TenantId;
use ray::{
    IngestReceipt, Predicate, ServiceName, Span, SpanBatch, SpanId, SpanKind, SpanStatus,
    StatusCode, TimeRange, TraceId, TraceStore, TraceStoreError,
};

use crate::clock::{Clock, SystemClock};

/// The single local tenant the managed instance runs under (W3, ADR-0077). The
/// demo is scoped by SERVICE identity within this tenant, not by a separate
/// tenant (the auth-off read path is pinned to one query tenant; ADR-0078/0079).
pub const DEMO_TENANT: &str = "acme";

/// The `service.name` the demo telemetry is filed under (matches the
/// `telemetrygen` seed's `DEFAULT_SERVICE_NAME`). The traces `service` query
/// parameter must equal this for synthesis to engage.
pub const DEMO_SERVICE_NAME: &str = "kaleidoscope-demo";

/// The human-readable Error status message on the failed-checkout demo span —
/// the WHERE of the failure, reused verbatim from the seed vocabulary
/// (ADR-0077). A by-id read shows the failing span marked failed with this.
const FAILED_CHECKOUT_ERROR_MESSAGE: &str = "checkout failed: card declined";

/// One synthesised demo span's fixed identity + now-relative time shape. The
/// trace/span ids and names are the ADR-0077 sample vocabulary, reused verbatim
/// so the synthesised demo is byte-compatible with what the seed/linked view
/// established. Timestamps are NOT stored here — they are computed `now - offset`
/// at read time so the demo always lands inside a rolling window.
struct DemoSpanSpec {
    trace_id: TraceId,
    span_id: SpanId,
    parent_span_id: SpanId,
    name: &'static str,
    status_code: StatusCode,
    status_message: &'static str,
    /// `start_time = now - start_offset_nanos` (saturating).
    start_offset_nanos: u64,
    /// `end_time = start_time + duration_nanos`.
    duration_nanos: u64,
}

/// The hardcoded demo dataset (ADR-0079: identity hardcoded to the ADR-0077
/// vocabulary; extensibility deferred): the one failed checkout (Error) plus the
/// three healthy traces (Ok) — a successful checkout, a products listing, and a
/// cart view — so filtering the demo service+window to `error=true` is
/// non-vacuous. All offsets are within ~75s of "now" so every span falls inside
/// any reasonable rolling window.
const DEMO_SPANS: [DemoSpanSpec; 4] = [
    // The failed checkout — the pinned demo trace id `4bf92f…`, status Error.
    DemoSpanSpec {
        trace_id: TraceId([
            0x4b, 0xf9, 0x2f, 0x35, 0x77, 0xb3, 0x4d, 0xa6, 0xa3, 0xce, 0x92, 0x9d, 0x0e, 0x0e,
            0x47, 0x36,
        ]),
        span_id: SpanId([0x00, 0xf0, 0x67, 0xaa, 0x0b, 0xa9, 0x02, 0xb8]),
        parent_span_id: SpanId([0x00, 0xf0, 0x67, 0xaa, 0x0b, 0xa9, 0x02, 0xb7]),
        name: "POST /api/v1/checkout",
        status_code: StatusCode::Error,
        status_message: FAILED_CHECKOUT_ERROR_MESSAGE,
        start_offset_nanos: 30_000_000_000,
        duration_nanos: 250_000_000,
    },
    // Healthy: a successful checkout.
    DemoSpanSpec {
        trace_id: TraceId([0xa1; 16]),
        span_id: SpanId([0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xb1]),
        parent_span_id: SpanId([0xa1; 8]),
        name: "POST /api/v1/checkout",
        status_code: StatusCode::Ok,
        status_message: "",
        start_offset_nanos: 45_000_000_000,
        duration_nanos: 120_000_000,
    },
    // Healthy: a products listing.
    DemoSpanSpec {
        trace_id: TraceId([0xb2; 16]),
        span_id: SpanId([0xb2, 0xb2, 0xb2, 0xb2, 0xb2, 0xb2, 0xb2, 0xc2]),
        parent_span_id: SpanId([0xb2; 8]),
        name: "GET /api/v1/products",
        status_code: StatusCode::Ok,
        status_message: "",
        start_offset_nanos: 60_000_000_000,
        duration_nanos: 35_000_000,
    },
    // Healthy: a cart view.
    DemoSpanSpec {
        trace_id: TraceId([0xc3; 16]),
        span_id: SpanId([0xc3, 0xc3, 0xc3, 0xc3, 0xc3, 0xc3, 0xc3, 0xd3]),
        parent_span_id: SpanId([0xc3; 8]),
        name: "GET /api/v1/cart",
        status_code: StatusCode::Ok,
        status_message: "",
        start_offset_nanos: 75_000_000_000,
        duration_nanos: 20_000_000,
    },
];

/// A read-side decorator over a [`TraceStore`] that synthesises the always-current
/// demo at query time for the demo service identity, and delegates every other
/// read straight through to the wrapped store (ADR-0079).
///
/// Generic over the inner store `S` (zero-cost monomorphisation, no `dyn`) and
/// the [`Clock`] seam `C` (deterministic synthesis in tests).
pub struct DemoTraceOverlay<S, C = SystemClock> {
    inner: S,
    clock: C,
}

impl<S> DemoTraceOverlay<S, SystemClock> {
    /// Wrap `inner` with the production [`SystemClock`].
    pub fn with_system_clock(inner: S) -> Self {
        Self {
            inner,
            clock: SystemClock,
        }
    }
}

impl<S, C> DemoTraceOverlay<S, C> {
    /// Wrap `inner`, anchoring the demo's now-relative timestamps to `clock`.
    pub fn new(inner: S, clock: C) -> Self {
        Self { inner, clock }
    }
}

/// True when `tenant` is the demo tenant — the cheap half of the O(1) demo
/// identity short-circuit.
fn is_demo_tenant(tenant: &TenantId) -> bool {
    tenant.0 == DEMO_TENANT
}

/// True when `service` is the demo service.
fn is_demo_service(service: &ServiceName) -> bool {
    service.as_str() == DEMO_SERVICE_NAME
}

/// The demo span spec for `trace_id`, or `None` when it is not a demo trace —
/// the by-id arm of the demo identity short-circuit. A single lookup decides
/// both "is this a demo id" and "which span to synthesise", so there is no
/// redundant second filter to mask a defect.
fn demo_spec_for_trace(trace_id: &TraceId) -> Option<&'static DemoSpanSpec> {
    DEMO_SPANS.iter().find(|spec| &spec.trace_id == trace_id)
}

/// Build the OTLP-shaped [`Span`] for one demo spec, anchored to `now`.
fn synthesize_span(spec: &DemoSpanSpec, now_unix_nano: u64) -> Span {
    let start_time_unix_nano = now_unix_nano.saturating_sub(spec.start_offset_nanos);
    let end_time_unix_nano = start_time_unix_nano.saturating_add(spec.duration_nanos);

    let mut resource_attributes = BTreeMap::new();
    resource_attributes.insert("service.name".to_string(), DEMO_SERVICE_NAME.to_string());

    Span {
        trace_id: spec.trace_id,
        span_id: spec.span_id,
        parent_span_id: Some(spec.parent_span_id),
        name: spec.name.to_string(),
        kind: SpanKind::Server,
        start_time_unix_nano,
        end_time_unix_nano,
        status: SpanStatus {
            code: spec.status_code,
            message: spec.status_message.to_string(),
        },
        attributes: BTreeMap::new(),
        resource_attributes,
        events: Vec::new(),
        links: Vec::new(),
    }
}

/// All four demo spans, anchored to `now`.
fn synthesize_all(now_unix_nano: u64) -> Vec<Span> {
    DEMO_SPANS
        .iter()
        .map(|spec| synthesize_span(spec, now_unix_nano))
        .collect()
}

impl<S: TraceStore, C: Clock> TraceStore for DemoTraceOverlay<S, C> {
    /// Read-only overlay: the demo is NEVER written. Real writes delegate
    /// straight through to the inner store; there is no synthesis branch here,
    /// so the demo data has no write path (ADR-0079 read-only invariant).
    fn ingest(
        &self,
        tenant: &TenantId,
        batch: SpanBatch,
    ) -> Result<IngestReceipt, TraceStoreError> {
        self.inner.ingest(tenant, batch)
    }

    fn get_trace(
        &self,
        tenant: &TenantId,
        trace_id: &TraceId,
    ) -> Result<Vec<Span>, TraceStoreError> {
        if !is_demo_tenant(tenant) {
            return self.inner.get_trace(tenant, trace_id);
        }
        let spec = match demo_spec_for_trace(trace_id) {
            Some(spec) => spec,
            None => return self.inner.get_trace(tenant, trace_id),
        };
        let mut spans = self.inner.get_trace(tenant, trace_id)?;
        spans.push(synthesize_span(spec, self.clock.now_unix_nano()));
        spans.sort_by_key(|span| span.start_time_unix_nano);
        Ok(spans)
    }

    fn query(
        &self,
        tenant: &TenantId,
        service: &ServiceName,
        range: TimeRange,
    ) -> Result<Vec<Span>, TraceStoreError> {
        if !is_demo_tenant(tenant) || !is_demo_service(service) {
            return self.inner.query(tenant, service, range);
        }
        let mut spans = self.inner.query(tenant, service, range)?;
        spans.extend(
            synthesize_all(self.clock.now_unix_nano())
                .into_iter()
                .filter(|span| range.contains(span.start_time_unix_nano)),
        );
        spans.sort_by_key(|span| span.start_time_unix_nano);
        Ok(spans)
    }

    fn query_with(
        &self,
        tenant: &TenantId,
        service: &ServiceName,
        range: TimeRange,
        predicate: &Predicate,
    ) -> Result<Vec<Span>, TraceStoreError> {
        if !is_demo_tenant(tenant) || !is_demo_service(service) {
            return self.inner.query_with(tenant, service, range, predicate);
        }
        let mut spans = self.inner.query_with(tenant, service, range, predicate)?;
        spans.extend(
            synthesize_all(self.clock.now_unix_nano())
                .into_iter()
                .filter(|span| {
                    range.contains(span.start_time_unix_nano) && predicate.matches(span)
                }),
        );
        spans.sort_by_key(|span| span.start_time_unix_nano);
        Ok(spans)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ray::{InMemoryTraceStore, NoopRecorder};

    /// A realistic, large "now" (~2023) so `now - offset` never underflows and
    /// the demo lands in any plausible window.
    const NOW: u64 = 1_700_000_000_000_000_000;

    /// A 15-minute rolling window in nanoseconds — a typical traces query span.
    const FIFTEEN_MINUTES_NANOS: u64 = 15 * 60 * 1_000_000_000;

    /// The pinned failed-checkout demo trace id `4bf92f3577b34da6a3ce929d0e0e4736`.
    fn failed_checkout_trace_id() -> TraceId {
        TraceId([
            0x4b, 0xf9, 0x2f, 0x35, 0x77, 0xb3, 0x4d, 0xa6, 0xa3, 0xce, 0x92, 0x9d, 0x0e, 0x0e,
            0x47, 0x36,
        ])
    }

    /// A clock frozen at a caller-chosen instant — the deterministic seam.
    struct FixedClock {
        now_unix_nano: u64,
    }

    impl FixedClock {
        fn at(now_unix_nano: u64) -> Self {
            Self { now_unix_nano }
        }
    }

    impl Clock for FixedClock {
        fn now_unix_nano(&self) -> u64 {
            self.now_unix_nano
        }
    }

    fn empty_inner() -> InMemoryTraceStore {
        InMemoryTraceStore::new(Box::new(NoopRecorder))
    }

    fn acme() -> TenantId {
        TenantId(DEMO_TENANT.to_string())
    }

    fn demo_service() -> ServiceName {
        ServiceName::new(DEMO_SERVICE_NAME)
    }

    /// A real (non-demo) span under another service, for the pass-through tests.
    fn real_non_demo_span(start_time_unix_nano: u64) -> Span {
        let mut resource_attributes = BTreeMap::new();
        resource_attributes.insert("service.name".to_string(), "checkout-service".to_string());
        Span {
            trace_id: TraceId([0x11; 16]),
            span_id: SpanId([0x22; 8]),
            parent_span_id: None,
            name: "POST /orders".to_string(),
            kind: SpanKind::Server,
            start_time_unix_nano,
            end_time_unix_nano: start_time_unix_nano + 1_000_000,
            status: SpanStatus {
                code: StatusCode::Ok,
                message: String::new(),
            },
            attributes: BTreeMap::new(),
            resource_attributes,
            events: Vec::new(),
            links: Vec::new(),
        }
    }

    // ---- Behavior 1: demo service+window query returns the four demo traces,
    //      with now-relative timestamps. ------------------------------------

    #[test]
    fn demo_service_window_query_returns_all_four_demo_traces() {
        let overlay = DemoTraceOverlay::new(empty_inner(), FixedClock::at(NOW));

        let spans = overlay
            .query(&acme(), &demo_service(), TimeRange::all())
            .expect("demo query succeeds");

        assert_eq!(
            spans.len(),
            4,
            "the failed checkout plus three healthy traces"
        );

        let mut trace_ids: Vec<TraceId> = spans.iter().map(|s| s.trace_id).collect();
        trace_ids.sort();
        trace_ids.dedup();
        assert_eq!(trace_ids.len(), 4, "four distinct demo traces");
        assert!(
            trace_ids.contains(&failed_checkout_trace_id()),
            "the pinned failed-checkout trace must be present"
        );

        // The failed checkout span is anchored 30s before now with a 250ms
        // duration — pins the now-relative arithmetic exactly.
        let failed = spans
            .iter()
            .find(|s| s.trace_id == failed_checkout_trace_id())
            .expect("failed checkout present");
        assert_eq!(failed.start_time_unix_nano, NOW - 30_000_000_000);
        assert_eq!(
            failed.end_time_unix_nano,
            NOW - 30_000_000_000 + 250_000_000
        );
        assert_eq!(failed.status.code, StatusCode::Error);
    }

    #[test]
    fn demo_traces_have_now_relative_timestamps_inside_a_rolling_window_for_any_now() {
        // Whatever "now" is, every synthesised demo span lands inside a typical
        // 15-minute window ending at now — this is the currency property.
        for now in [
            1_700_000_000_000_000_000u64,
            1_800_000_000_000_000_000u64,
            2_000_000_000_000_000_000u64,
        ] {
            let overlay = DemoTraceOverlay::new(empty_inner(), FixedClock::at(now));
            let window = TimeRange::new(now - FIFTEEN_MINUTES_NANOS, now + 1);

            let spans = overlay
                .query(&acme(), &demo_service(), window)
                .expect("demo query succeeds");

            assert_eq!(
                spans.len(),
                4,
                "all four demo traces fall inside the window"
            );
            for span in &spans {
                assert!(
                    window.contains(span.start_time_unix_nano),
                    "span start {} must be inside [{}, {})",
                    span.start_time_unix_nano,
                    window.start_unix_nano,
                    window.end_unix_nano
                );
                assert!(
                    span.start_time_unix_nano < span.end_time_unix_nano,
                    "a synthesised span must have positive duration"
                );
                assert!(
                    span.end_time_unix_nano <= now,
                    "a synthesised span must end at or before now"
                );
            }
        }
    }

    // ---- Behavior 2: the error path returns exactly the failed checkout. ----

    #[test]
    fn error_path_returns_only_the_failed_checkout() {
        let overlay = DemoTraceOverlay::new(empty_inner(), FixedClock::at(NOW));
        let only_errors = Predicate::new().status(StatusCode::Error);

        let spans = overlay
            .query_with(&acme(), &demo_service(), TimeRange::all(), &only_errors)
            .expect("demo error query succeeds");

        assert_eq!(spans.len(), 1, "exactly the one failed checkout");
        assert_eq!(spans[0].trace_id, failed_checkout_trace_id());
        assert_eq!(spans[0].status.code, StatusCode::Error);
        assert_eq!(spans[0].status.message, FAILED_CHECKOUT_ERROR_MESSAGE);
    }

    // ---- Behavior 3: by-id returns the demo spans (failed + healthy). -------

    #[test]
    fn by_id_returns_the_failed_checkout_span_with_readable_error_message() {
        let overlay = DemoTraceOverlay::new(empty_inner(), FixedClock::at(NOW));

        let spans = overlay
            .get_trace(&acme(), &failed_checkout_trace_id())
            .expect("by-id demo query succeeds");

        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "POST /api/v1/checkout");
        assert_eq!(spans[0].status.code, StatusCode::Error);
        assert_eq!(
            spans[0].status.message, FAILED_CHECKOUT_ERROR_MESSAGE,
            "the by-id view shows the readable failure message"
        );
    }

    #[test]
    fn by_id_returns_each_healthy_demo_trace_as_ok() {
        let overlay = DemoTraceOverlay::new(empty_inner(), FixedClock::at(NOW));

        for (trace_id, expected_name) in [
            (TraceId([0xa1; 16]), "POST /api/v1/checkout"),
            (TraceId([0xb2; 16]), "GET /api/v1/products"),
            (TraceId([0xc3; 16]), "GET /api/v1/cart"),
        ] {
            let spans = overlay
                .get_trace(&acme(), &trace_id)
                .expect("by-id demo query succeeds");

            assert_eq!(spans.len(), 1, "one span for healthy trace {expected_name}");
            assert_eq!(spans[0].name, expected_name);
            assert_eq!(
                spans[0].status.code,
                StatusCode::Ok,
                "healthy demo traces are Ok"
            );
        }
    }

    // ---- Behavior 4: non-demo reads delegate to the wrapped store unchanged. -

    #[test]
    fn non_demo_service_query_delegates_unchanged() {
        let inner = empty_inner();
        let real = real_non_demo_span(NOW - 5_000_000_000);
        inner
            .ingest(&acme(), SpanBatch::with_spans(vec![real.clone()]))
            .expect("seed a real span");
        let overlay = DemoTraceOverlay::new(inner, FixedClock::at(NOW));

        let spans = overlay
            .query(
                &acme(),
                &ServiceName::new("checkout-service"),
                TimeRange::all(),
            )
            .expect("non-demo query succeeds");

        assert_eq!(
            spans,
            vec![real],
            "a non-demo service read is byte-identical to the inner store's result, no demo injected"
        );
    }

    #[test]
    fn non_demo_trace_id_get_trace_delegates_unchanged() {
        let inner = empty_inner();
        let real = real_non_demo_span(NOW - 5_000_000_000);
        inner
            .ingest(&acme(), SpanBatch::with_spans(vec![real.clone()]))
            .expect("seed a real span");
        let overlay = DemoTraceOverlay::new(inner, FixedClock::at(NOW));

        let spans = overlay
            .get_trace(&acme(), &TraceId([0x11; 16]))
            .expect("non-demo by-id succeeds");

        assert_eq!(
            spans,
            vec![real],
            "a non-demo by-id read passes straight through"
        );
    }

    #[test]
    fn non_demo_query_with_delegates_unchanged() {
        let inner = empty_inner();
        let real = real_non_demo_span(NOW - 5_000_000_000);
        inner
            .ingest(&acme(), SpanBatch::with_spans(vec![real.clone()]))
            .expect("seed a real span");
        let overlay = DemoTraceOverlay::new(inner, FixedClock::at(NOW));

        let spans = overlay
            .query_with(
                &acme(),
                &ServiceName::new("checkout-service"),
                TimeRange::all(),
                &Predicate::new().status(StatusCode::Ok),
            )
            .expect("non-demo predicate query succeeds");

        assert_eq!(
            spans,
            vec![real],
            "a non-demo predicate read passes straight through, no demo injected"
        );
    }

    #[test]
    fn non_demo_tenant_query_delegates_even_for_the_demo_service() {
        // The demo is scoped to the demo service AND the demo tenant. A query
        // for the demo SERVICE under a DIFFERENT tenant must NOT synthesise —
        // the tenant guard is load-bearing, not just the service guard.
        let overlay = DemoTraceOverlay::new(empty_inner(), FixedClock::at(NOW));

        let spans = overlay
            .query(
                &TenantId("someone-else".to_string()),
                &demo_service(),
                TimeRange::all(),
            )
            .expect("foreign-tenant query succeeds");

        assert!(
            spans.is_empty(),
            "no demo is synthesised for the demo service under a foreign tenant"
        );
    }

    #[test]
    fn non_demo_tenant_get_trace_delegates_even_for_a_demo_trace_id() {
        // A by-id read of a demo TRACE ID under a foreign tenant must delegate,
        // not synthesise — the tenant guard gates the by-id path too.
        let overlay = DemoTraceOverlay::new(empty_inner(), FixedClock::at(NOW));

        let spans = overlay
            .get_trace(
                &TenantId("someone-else".to_string()),
                &failed_checkout_trace_id(),
            )
            .expect("foreign-tenant by-id succeeds");

        assert!(
            spans.is_empty(),
            "no demo is synthesised for a demo trace id under a foreign tenant"
        );
    }

    // ---- Behavior 5/6: read-only invariant — the demo is never written. -----

    #[test]
    fn demo_read_does_not_write_to_the_wrapped_store() {
        // The stores append on ingest with NO dedup (ADR-0079). So if a demo
        // read wrote its synthesis into the wrapped store, a second demo read
        // would merge inner(4) + synth(4) = 8. Two reads each returning exactly
        // four proves the demo synthesis never mutates the store — it is
        // read-only and store-free for the demo identity.
        let overlay = DemoTraceOverlay::new(empty_inner(), FixedClock::at(NOW));

        let first = overlay
            .query(&acme(), &demo_service(), TimeRange::all())
            .expect("first demo read");
        let second = overlay
            .query(&acme(), &demo_service(), TimeRange::all())
            .expect("second demo read");

        assert_eq!(first.len(), 4, "first read synthesises four demo traces");
        assert_eq!(
            second.len(),
            4,
            "second read still four — the demo was never persisted (no accumulation)"
        );
        assert_eq!(first, second, "synthesis is stable read-to-read");
    }
}
