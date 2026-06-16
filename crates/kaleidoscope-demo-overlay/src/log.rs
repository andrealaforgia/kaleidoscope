// Kaleidoscope demo overlay — the log overlay (slice B)
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

//! The log half of the always-current demo overlay (ADR-0079 slice B).
//!
//! [`DemoLogOverlay`] decorates any [`LogStore`]. For the demo tenant it
//! SYNTHESISES the failed-checkout cause log at query time with a now-relative
//! observed timestamp, carrying the SAME demo trace id and failed-checkout error
//! span id the trace overlay uses — so the linked view's `with_logs`
//! correlation (logs filtered by trace id) shows the failing span and its cause
//! log together. Every other read short-circuits straight to the wrapped store.
//! The demo has **no write path** — `ingest` only delegates real writes, so the
//! synthesised cause log can never be persisted.

use std::collections::BTreeMap;

use aegis::TenantId;
use lumen::{
    IngestReceipt, LogBatch, LogRecord, LogStore, LogStoreError, Predicate, SeverityNumber,
    TimeRange,
};

use crate::clock::{Clock, SystemClock};
#[cfg(test)]
use crate::identity::DEMO_TENANT;
use crate::identity::{
    is_demo_tenant, DEMO_SERVICE_NAME, FAILED_CHECKOUT_ERROR_MESSAGE,
    FAILED_CHECKOUT_SPAN_ID_BYTES, FAILED_CHECKOUT_TRACE_ID_BYTES,
};

/// How far before "now" the synthesised cause log is observed (30s), matching
/// the failed-checkout span's start offset in the trace overlay so the cause log
/// sits alongside the failing span inside any rolling window.
const CAUSE_LOG_OFFSET_NANOS: u64 = 30_000_000_000;

/// Build the failed-checkout cause log, anchored to `now`. It carries the demo
/// `service.name` (so a service-scoped read engages and a foreign-service read
/// excludes it) and the demo trace id + failed-checkout error span id (so a
/// by-trace_id read correlates it with the failing span).
fn synthesize_cause_log(now_unix_nano: u64) -> LogRecord {
    let observed_time_unix_nano = now_unix_nano.saturating_sub(CAUSE_LOG_OFFSET_NANOS);

    let mut resource_attributes = BTreeMap::new();
    resource_attributes.insert("service.name".to_string(), DEMO_SERVICE_NAME.to_string());

    LogRecord {
        observed_time_unix_nano,
        severity_number: SeverityNumber::ERROR,
        severity_text: "ERROR".to_string(),
        body: FAILED_CHECKOUT_ERROR_MESSAGE.to_string(),
        attributes: BTreeMap::new(),
        resource_attributes,
        trace_id: Some(FAILED_CHECKOUT_TRACE_ID_BYTES),
        span_id: Some(FAILED_CHECKOUT_SPAN_ID_BYTES),
    }
}

/// A read-side decorator over a [`LogStore`] that synthesises the always-current
/// demo cause log at query time for the demo tenant, and delegates every other
/// read straight through to the wrapped store (ADR-0079).
///
/// Generic over the inner store `S` (zero-cost monomorphisation, no `dyn`) and
/// the [`Clock`] seam `C` (deterministic synthesis in tests).
pub struct DemoLogOverlay<S, C = SystemClock> {
    inner: S,
    clock: C,
}

impl<S> DemoLogOverlay<S, SystemClock> {
    /// Wrap `inner` with the production [`SystemClock`].
    pub fn with_system_clock(inner: S) -> Self {
        Self {
            inner,
            clock: SystemClock,
        }
    }
}

impl<S, C> DemoLogOverlay<S, C> {
    /// Wrap `inner`, anchoring the demo's now-relative timestamp to `clock`.
    pub fn new(inner: S, clock: C) -> Self {
        Self { inner, clock }
    }
}

impl<S: LogStore, C: Clock> LogStore for DemoLogOverlay<S, C> {
    /// Read-only overlay: the demo cause log is NEVER written. Real writes
    /// delegate straight through; there is no synthesis branch here, so the demo
    /// data has no write path (ADR-0079 read-only invariant).
    fn ingest(&self, tenant: &TenantId, batch: LogBatch) -> Result<IngestReceipt, LogStoreError> {
        self.inner.ingest(tenant, batch)
    }

    /// The by-trace_id read path (`query(tenant, TimeRange::all())`, then the
    /// router filters `record.trace_id == Some(id)`) flows through here: for the
    /// demo tenant the cause log — carrying the demo trace id — is injected when
    /// its now-relative timestamp falls inside `range`.
    fn query(&self, tenant: &TenantId, range: TimeRange) -> Result<Vec<LogRecord>, LogStoreError> {
        if !is_demo_tenant(tenant) {
            return self.inner.query(tenant, range);
        }
        let mut records = self.inner.query(tenant, range)?;
        let cause_log = synthesize_cause_log(self.clock.now_unix_nano());
        if range.contains(cause_log.observed_time_unix_nano) {
            records.push(cause_log);
        }
        records.sort_by_key(|record| record.observed_time_unix_nano);
        Ok(records)
    }

    /// The by-service+window read path (`query_with` with a `service` predicate)
    /// flows through here: for the demo tenant the cause log is injected when it
    /// falls inside `range` AND matches the predicate. The predicate carries the
    /// service scoping — a foreign-service read never matches the demo log.
    fn query_with(
        &self,
        tenant: &TenantId,
        range: TimeRange,
        predicate: &Predicate,
    ) -> Result<Vec<LogRecord>, LogStoreError> {
        if !is_demo_tenant(tenant) {
            return self.inner.query_with(tenant, range, predicate);
        }
        let mut records = self.inner.query_with(tenant, range, predicate)?;
        let cause_log = synthesize_cause_log(self.clock.now_unix_nano());
        if range.contains(cause_log.observed_time_unix_nano) && predicate.matches(&cause_log) {
            records.push(cause_log);
        }
        records.sort_by_key(|record| record.observed_time_unix_nano);
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumen::{InMemoryLogStore, NoopRecorder};

    /// A realistic, large "now" (~2023) so `now - offset` never underflows.
    const NOW: u64 = 1_700_000_000_000_000_000;

    /// A 15-minute rolling window in nanoseconds — a typical logs query span.
    const FIFTEEN_MINUTES_NANOS: u64 = 15 * 60 * 1_000_000_000;

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

    fn empty_inner() -> InMemoryLogStore {
        InMemoryLogStore::new(Box::new(NoopRecorder))
    }

    fn acme() -> TenantId {
        TenantId(DEMO_TENANT.to_string())
    }

    /// A real (non-demo) log under another service, for the pass-through tests.
    fn real_non_demo_log(observed_time_unix_nano: u64) -> LogRecord {
        let mut resource_attributes = BTreeMap::new();
        resource_attributes.insert("service.name".to_string(), "checkout-service".to_string());
        LogRecord {
            observed_time_unix_nano,
            severity_number: SeverityNumber::INFO,
            severity_text: "INFO".to_string(),
            body: "order accepted".to_string(),
            attributes: BTreeMap::new(),
            resource_attributes,
            trace_id: Some([0x11; 16]),
            span_id: Some([0x22; 8]),
        }
    }

    // ---- Behavior 1: by-service+window read returns the cause log with a
    //      now-relative observed timestamp. ----------------------------------

    #[test]
    fn demo_service_window_query_returns_the_cause_log_with_now_relative_timestamp() {
        let overlay = DemoLogOverlay::new(empty_inner(), FixedClock::at(NOW));
        let window = TimeRange::new(NOW - FIFTEEN_MINUTES_NANOS, NOW + 1);
        let demo_service = Predicate::new().service(DEMO_SERVICE_NAME);

        let records = overlay
            .query_with(&acme(), window, &demo_service)
            .expect("demo service+window read succeeds");

        assert_eq!(records.len(), 1, "the one synthesised cause log");
        let cause = &records[0];
        assert_eq!(cause.body, FAILED_CHECKOUT_ERROR_MESSAGE);
        assert_eq!(cause.severity_number, SeverityNumber::ERROR);
        assert_eq!(
            cause.observed_time_unix_nano,
            NOW - CAUSE_LOG_OFFSET_NANOS,
            "the cause log is observed now-relative, 30s before now"
        );
        assert!(
            window.contains(cause.observed_time_unix_nano),
            "the cause log lands inside the rolling window"
        );
    }

    #[test]
    fn cause_log_lands_inside_a_rolling_window_for_any_now() {
        // Whatever "now" is, the synthesised cause log lands inside a typical
        // 15-minute window ending at now — the currency property.
        for now in [
            1_700_000_000_000_000_000u64,
            1_800_000_000_000_000_000u64,
            2_000_000_000_000_000_000u64,
        ] {
            let overlay = DemoLogOverlay::new(empty_inner(), FixedClock::at(now));
            let window = TimeRange::new(now - FIFTEEN_MINUTES_NANOS, now + 1);

            let records = overlay
                .query_with(
                    &acme(),
                    window,
                    &Predicate::new().service(DEMO_SERVICE_NAME),
                )
                .expect("demo read succeeds");

            assert_eq!(records.len(), 1, "the cause log is inside the window");
            assert!(
                records[0].observed_time_unix_nano < now,
                "the cause log is observed at or before now"
            );
        }
    }

    // ---- Behavior 2: by-trace_id read (no window) returns the cause log
    //      carrying the demo trace id and the failed-checkout error span id. --

    #[test]
    fn by_trace_id_read_returns_the_cause_log_carrying_the_demo_trace_and_span_ids() {
        // `logs_for_trace` reads `query(tenant, TimeRange::all())` then filters
        // by `record.trace_id == Some(id)`. The cause log must be present in the
        // all-time read and carry the demo trace id AND the failed-checkout
        // error span id so `with_logs` attaches it to the failing span.
        let overlay = DemoLogOverlay::new(empty_inner(), FixedClock::at(NOW));

        let records = overlay
            .query(&acme(), TimeRange::all())
            .expect("by-trace_id all-time read succeeds");

        let cause = records
            .iter()
            .find(|record| record.trace_id == Some(FAILED_CHECKOUT_TRACE_ID_BYTES))
            .expect("the cause log carrying the demo trace id is present");
        assert_eq!(
            cause.span_id,
            Some(FAILED_CHECKOUT_SPAN_ID_BYTES),
            "the cause log carries the failed-checkout error span id"
        );
        assert_eq!(cause.body, FAILED_CHECKOUT_ERROR_MESSAGE);
    }

    // ---- Behavior 3: non-demo (foreign-tenant) reads delegate unchanged. ----

    #[test]
    fn foreign_tenant_query_delegates_unchanged() {
        let inner = empty_inner();
        let real = real_non_demo_log(NOW - 5_000_000_000);
        inner
            .ingest(
                &TenantId("someone-else".to_string()),
                LogBatch::with_records(vec![real.clone()]),
            )
            .expect("seed a real log under a foreign tenant");
        let overlay = DemoLogOverlay::new(inner, FixedClock::at(NOW));

        let records = overlay
            .query(&TenantId("someone-else".to_string()), TimeRange::all())
            .expect("foreign-tenant read succeeds");

        assert_eq!(
            records,
            vec![real],
            "a foreign-tenant read is byte-identical to the inner store's result, no demo injected"
        );
    }

    #[test]
    fn foreign_tenant_query_with_delegates_even_for_the_demo_service() {
        // The demo is scoped to the demo tenant. A query for the demo SERVICE
        // under a DIFFERENT tenant must NOT synthesise — the tenant guard is
        // load-bearing.
        let overlay = DemoLogOverlay::new(empty_inner(), FixedClock::at(NOW));

        let records = overlay
            .query_with(
                &TenantId("someone-else".to_string()),
                TimeRange::all(),
                &Predicate::new().service(DEMO_SERVICE_NAME),
            )
            .expect("foreign-tenant read succeeds");

        assert!(
            records.is_empty(),
            "no demo cause log is synthesised for the demo service under a foreign tenant"
        );
    }

    // ---- Behavior 4: a demo-tenant read scoped to a FOREIGN service excludes
    //      the demo cause log (service scoping rides the predicate). ---------

    #[test]
    fn demo_tenant_foreign_service_query_with_excludes_the_demo_cause_log() {
        let overlay = DemoLogOverlay::new(empty_inner(), FixedClock::at(NOW));

        let records = overlay
            .query_with(
                &acme(),
                TimeRange::all(),
                &Predicate::new().service("checkout-service"),
            )
            .expect("demo-tenant foreign-service read succeeds");

        assert!(
            records.is_empty(),
            "the demo cause log carries the demo service.name, so a foreign-service predicate excludes it"
        );
    }

    // ---- Behavior 5: read-only invariant — the demo is never written. -------

    #[test]
    fn demo_read_does_not_write_to_the_wrapped_store() {
        // The store appends on ingest with no dedup. If a demo read wrote its
        // synthesis into the wrapped store, a second read would accumulate. Two
        // reads each returning exactly one cause log proves the synthesis never
        // mutates the store — read-only and store-free for the demo identity.
        let overlay = DemoLogOverlay::new(empty_inner(), FixedClock::at(NOW));

        let first = overlay
            .query(&acme(), TimeRange::all())
            .expect("first demo read");
        let second = overlay
            .query(&acme(), TimeRange::all())
            .expect("second demo read");

        assert_eq!(first.len(), 1, "first read synthesises one cause log");
        assert_eq!(
            second.len(),
            1,
            "second read still one — the cause log was never persisted (no accumulation)"
        );
        assert_eq!(first, second, "synthesis is stable read-to-read");
    }
}
