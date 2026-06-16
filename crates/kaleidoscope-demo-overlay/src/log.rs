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
//! SYNTHESISES a noisy set of about a dozen logs at query time with now-relative
//! observed timestamps — mostly non-error (INFO/WARN) across several customers
//! and request types, with EXACTLY ONE declined ERROR cause log. That one cause
//! log carries the SAME demo trace id and failed-checkout error span id the
//! trace overlay uses — so the linked view's `with_logs` correlation (logs
//! filtered by trace id) shows the failing span and its cause log together; the
//! noise logs carry neither. Every other read short-circuits straight to the
//! wrapped store. The demo has **no write path** — `ingest` only delegates real
//! writes, so the synthesised logs can never be persisted.

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
    is_demo_tenant, DEMO_CUSTOMER_ID_KEY, DEMO_SERVICE_NAME, FAILED_CHECKOUT_CUSTOMER_ID,
    FAILED_CHECKOUT_ERROR_MESSAGE, FAILED_CHECKOUT_SPAN_ID_BYTES, FAILED_CHECKOUT_TRACE_ID_BYTES,
};

/// How far before "now" the synthesised cause log is observed (30s), matching
/// the failed-checkout span's start offset in the trace overlay so the cause log
/// sits alongside the failing span inside any rolling window.
const CAUSE_LOG_OFFSET_NANOS: u64 = 30_000_000_000;

/// One synthesised demo log's fixed shape + now-relative offset. Timestamps are
/// computed `now - start_offset_nanos` at read time so the demo always lands
/// inside a rolling window.
struct DemoLogSpec {
    /// `observed_time = now - start_offset_nanos` (saturating).
    start_offset_nanos: u64,
    severity_number: SeverityNumber,
    severity_text: &'static str,
    body: &'static str,
    /// The customer this log belongs to, emitted as the `customer.id` attribute.
    customer_id: &'static str,
    /// When true, this is the one declined ERROR cause log: it carries the demo
    /// trace id + failed-checkout error span id so the `with_logs` view
    /// correlates it with the failing span (WHERE + WHY together).
    is_failed_checkout_cause: bool,
}

/// The noisy synthesised demo log set (ADR-0079): about a dozen logs across
/// FOUR customers (`alice`, `bob`, `carol`, `dave`) and THREE request types
/// (checkout, products, cart), MOSTLY non-error (INFO/WARN), with EXACTLY ONE
/// declined ERROR cause log. So a body search for "declined" returns exactly
/// one, and a min-severity=ERROR floor returns exactly that one out of the
/// noise. All offsets are within ~70s of "now" so every log falls inside any
/// reasonable rolling window.
const DEMO_LOGS: [DemoLogSpec; 12] = [
    DemoLogSpec {
        start_offset_nanos: 70_000_000_000,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO",
        body: "GET /api/v1/products returned 200 in 34ms",
        customer_id: "bob",
        is_failed_checkout_cause: false,
    },
    DemoLogSpec {
        start_offset_nanos: 68_000_000_000,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO",
        body: "GET /api/v1/cart returned 200 in 12ms",
        customer_id: "carol",
        is_failed_checkout_cause: false,
    },
    DemoLogSpec {
        start_offset_nanos: 66_000_000_000,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO",
        body: "POST /api/v1/checkout returned 200 in 142ms",
        customer_id: "alice",
        is_failed_checkout_cause: false,
    },
    DemoLogSpec {
        start_offset_nanos: 64_000_000_000,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO",
        body: "GET /api/v1/products returned 200 in 28ms",
        customer_id: "alice",
        is_failed_checkout_cause: false,
    },
    DemoLogSpec {
        start_offset_nanos: 62_000_000_000,
        severity_number: SeverityNumber::WARN,
        severity_text: "WARN",
        body: "GET /api/v1/cart slow response 512ms",
        customer_id: "bob",
        is_failed_checkout_cause: false,
    },
    DemoLogSpec {
        start_offset_nanos: 60_000_000_000,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO",
        body: "POST /api/v1/checkout returned 200 in 119ms",
        customer_id: "bob",
        is_failed_checkout_cause: false,
    },
    DemoLogSpec {
        start_offset_nanos: 58_000_000_000,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO",
        body: "GET /api/v1/products returned 200 in 41ms",
        customer_id: "carol",
        is_failed_checkout_cause: false,
    },
    DemoLogSpec {
        start_offset_nanos: 56_000_000_000,
        severity_number: SeverityNumber::WARN,
        severity_text: "WARN",
        body: "POST /api/v1/checkout retry after upstream timeout",
        customer_id: "carol",
        is_failed_checkout_cause: false,
    },
    DemoLogSpec {
        start_offset_nanos: 54_000_000_000,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO",
        body: "GET /api/v1/cart returned 200 in 18ms",
        customer_id: "alice",
        is_failed_checkout_cause: false,
    },
    DemoLogSpec {
        start_offset_nanos: 52_000_000_000,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO",
        body: "user session started",
        customer_id: "dave",
        is_failed_checkout_cause: false,
    },
    DemoLogSpec {
        start_offset_nanos: 50_000_000_000,
        severity_number: SeverityNumber::INFO,
        severity_text: "INFO",
        body: "GET /api/v1/products returned 200 in 36ms",
        customer_id: "dave",
        is_failed_checkout_cause: false,
    },
    // The one declined ERROR cause log — owned by `alice`, the failed-checkout
    // customer, carrying the demo trace id + failed-checkout error span id.
    DemoLogSpec {
        start_offset_nanos: CAUSE_LOG_OFFSET_NANOS,
        severity_number: SeverityNumber::ERROR,
        severity_text: "ERROR",
        body: FAILED_CHECKOUT_ERROR_MESSAGE,
        customer_id: FAILED_CHECKOUT_CUSTOMER_ID,
        is_failed_checkout_cause: true,
    },
];

/// Build one synthesised demo log, anchored to `now`. Every log carries the demo
/// `service.name` (so a service-scoped read engages and a foreign-service read
/// excludes it) and a `customer.id` attribute. Only the cause log carries the
/// demo trace id + failed-checkout error span id (so a by-trace_id read
/// correlates it with the failing span); the noise logs carry neither.
fn synthesize_log(spec: &DemoLogSpec, now_unix_nano: u64) -> LogRecord {
    let observed_time_unix_nano = now_unix_nano.saturating_sub(spec.start_offset_nanos);

    let mut resource_attributes = BTreeMap::new();
    resource_attributes.insert("service.name".to_string(), DEMO_SERVICE_NAME.to_string());

    let mut attributes = BTreeMap::new();
    attributes.insert(
        DEMO_CUSTOMER_ID_KEY.to_string(),
        spec.customer_id.to_string(),
    );

    let (trace_id, span_id) = if spec.is_failed_checkout_cause {
        (
            Some(FAILED_CHECKOUT_TRACE_ID_BYTES),
            Some(FAILED_CHECKOUT_SPAN_ID_BYTES),
        )
    } else {
        (None, None)
    };

    LogRecord {
        observed_time_unix_nano,
        severity_number: spec.severity_number,
        severity_text: spec.severity_text.to_string(),
        body: spec.body.to_string(),
        attributes,
        resource_attributes,
        trace_id,
        span_id,
    }
}

/// All synthesised demo logs, anchored to `now`.
fn synthesize_all(now_unix_nano: u64) -> Vec<LogRecord> {
    DEMO_LOGS
        .iter()
        .map(|spec| synthesize_log(spec, now_unix_nano))
        .collect()
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
        records.extend(
            synthesize_all(self.clock.now_unix_nano())
                .into_iter()
                .filter(|record| range.contains(record.observed_time_unix_nano)),
        );
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
        records.extend(
            synthesize_all(self.clock.now_unix_nano())
                .into_iter()
                .filter(|record| {
                    range.contains(record.observed_time_unix_nano) && predicate.matches(record)
                }),
        );
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

    /// The one declined ERROR cause log among the synthesised records.
    fn declined_cause_log(records: &[LogRecord]) -> &LogRecord {
        let declined: Vec<&LogRecord> = records
            .iter()
            .filter(|record| record.body == FAILED_CHECKOUT_ERROR_MESSAGE)
            .collect();
        assert_eq!(
            declined.len(),
            1,
            "exactly one declined cause log in the synthesised set"
        );
        declined[0]
    }

    // ---- Behavior 1: by-service+window read returns the noisy synthesised set
    //      (~12 logs), each now-relative, with the one cause log among them. --

    #[test]
    fn demo_service_window_query_returns_the_noisy_set_with_the_cause_log_now_relative() {
        let overlay = DemoLogOverlay::new(empty_inner(), FixedClock::at(NOW));
        let window = TimeRange::new(NOW - FIFTEEN_MINUTES_NANOS, NOW + 1);
        let demo_service = Predicate::new().service(DEMO_SERVICE_NAME);

        let records = overlay
            .query_with(&acme(), window, &demo_service)
            .expect("demo service+window read succeeds");

        assert_eq!(records.len(), 12, "the dozen synthesised demo logs");
        for record in &records {
            assert!(
                window.contains(record.observed_time_unix_nano),
                "every synthesised log lands inside the rolling window"
            );
        }

        let cause = declined_cause_log(&records);
        assert_eq!(cause.body, FAILED_CHECKOUT_ERROR_MESSAGE);
        assert_eq!(cause.severity_number, SeverityNumber::ERROR);
        assert_eq!(
            cause.observed_time_unix_nano,
            NOW - CAUSE_LOG_OFFSET_NANOS,
            "the cause log is observed now-relative, 30s before now"
        );
    }

    // ---- LOGSEARCH: the iteration-2 search journeys on the noisy set. The set
    //      is MOSTLY non-error (INFO/WARN) across >=3 customers and >=2 request
    //      types, with EXACTLY ONE declined ERROR log — so a body search for
    //      "declined" returns exactly one, and a min-severity=ERROR floor
    //      returns exactly that one out of the noise. -------------------------

    #[test]
    fn synthesised_logs_are_mostly_non_error_across_several_customers() {
        let overlay = DemoLogOverlay::new(empty_inner(), FixedClock::at(NOW));

        let records = overlay
            .query_with(
                &acme(),
                TimeRange::all(),
                &Predicate::new().service(DEMO_SERVICE_NAME),
            )
            .expect("demo read succeeds");

        assert_eq!(records.len(), 12, "about a dozen synthesised logs");
        let errors = records
            .iter()
            .filter(|record| record.severity_number >= SeverityNumber::ERROR)
            .count();
        assert_eq!(
            errors, 1,
            "exactly one ERROR log; the rest are non-error noise"
        );

        let distinct_customers: std::collections::BTreeSet<&str> = records
            .iter()
            .filter_map(|record| record.attributes.get(DEMO_CUSTOMER_ID_KEY))
            .map(String::as_str)
            .collect();
        assert!(
            distinct_customers.len() >= 3,
            "logs span at least three customers, got {distinct_customers:?}"
        );
    }

    #[test]
    fn logsearch_body_match_for_declined_returns_exactly_the_one_error_log() {
        let overlay = DemoLogOverlay::new(empty_inner(), FixedClock::at(NOW));

        let records = overlay
            .query_with(
                &acme(),
                TimeRange::all(),
                &Predicate::new()
                    .service(DEMO_SERVICE_NAME)
                    .body_contains("declined"),
            )
            .expect("demo body search succeeds");

        assert_eq!(
            records.len(),
            1,
            "a body search for 'declined' yields exactly one"
        );
        assert_eq!(records[0].body, FAILED_CHECKOUT_ERROR_MESSAGE);
        assert_eq!(records[0].severity_number, SeverityNumber::ERROR);
        assert_eq!(
            records[0].trace_id,
            Some(FAILED_CHECKOUT_TRACE_ID_BYTES),
            "the declined log is the cause log carrying the failed-checkout trace id"
        );
    }

    #[test]
    fn logsearch_min_severity_error_floor_returns_exactly_the_one_declined_log() {
        let overlay = DemoLogOverlay::new(empty_inner(), FixedClock::at(NOW));

        let records = overlay
            .query_with(
                &acme(),
                TimeRange::all(),
                &Predicate::new()
                    .service(DEMO_SERVICE_NAME)
                    .min_severity(SeverityNumber::ERROR),
            )
            .expect("demo severity-floor search succeeds");

        assert_eq!(
            records.len(),
            1,
            "a min-severity=ERROR floor returns exactly the one declined log out of the noise"
        );
        assert_eq!(records[0].body, FAILED_CHECKOUT_ERROR_MESSAGE);
    }

    #[test]
    fn cause_log_lands_inside_a_rolling_window_for_any_now() {
        // Whatever "now" is, the whole synthesised set lands inside a typical
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

            assert_eq!(
                records.len(),
                12,
                "every synthesised log is inside the window"
            );
            for record in &records {
                assert!(
                    record.observed_time_unix_nano < now,
                    "every log is observed at or before now"
                );
            }
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

        assert_eq!(
            first.len(),
            12,
            "first read synthesises the dozen demo logs"
        );
        assert_eq!(
            second.len(),
            12,
            "second read still twelve — the logs were never persisted (no accumulation)"
        );
        assert_eq!(first, second, "synthesis is stable read-to-read");
    }
}
