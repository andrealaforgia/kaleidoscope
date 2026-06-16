// Kaleidoscope demo overlay — the metric overlay (slice C)
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

//! The metric half of the always-current demo overlay (ADR-0079 slice C).
//!
//! [`DemoMetricOverlay`] decorates any [`MetricStore`]. For the demo tenant's
//! `request_count` metric it SYNTHESISES the seed's demo metric point at query
//! time with a now-relative timestamp, so a metrics query over a normal window
//! returns it; every other read short-circuits straight to the wrapped store.
//! The demo has **no write path** — `ingest` only delegates real writes, so the
//! synthesised point can never be persisted.

use std::collections::BTreeMap;

use aegis::TenantId;
use pulse::{
    IngestReceipt, Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore,
    MetricStoreError, Predicate, TimeRange,
};

use crate::clock::{Clock, SystemClock};
use crate::identity::{is_demo_tenant, DEMO_SERVICE_NAME};

/// The demo metric name the seed pushes (a `u64_counter`, hence a Sum), reused
/// verbatim from `kaleidoscope-telemetrygen`. The metrics query looks this up.
const DEMO_METRIC_NAME: &str = "request_count";

/// How far before "now" the synthesised metric point is timestamped (30s), so it
/// lands inside any reasonable rolling window ending at now.
const METRIC_POINT_OFFSET_NANOS: u64 = 30_000_000_000;

/// The single observation the seed's `request_count` counter pushes.
const DEMO_METRIC_VALUE: f64 = 1.0;

/// True when `metric_name` is the demo metric — the by-name arm of the demo
/// identity short-circuit.
fn is_demo_metric(metric_name: &MetricName) -> bool {
    metric_name.as_str() == DEMO_METRIC_NAME
}

/// Build the demo `request_count` metric + its single now-relative point. The
/// metric carries the demo `service.name` resource attribute so a service-scoped
/// matcher keeps it and a foreign-service matcher excludes it.
fn synthesize_request_count(now_unix_nano: u64) -> (Metric, MetricPoint) {
    let time_unix_nano = now_unix_nano.saturating_sub(METRIC_POINT_OFFSET_NANOS);

    let point = MetricPoint {
        time_unix_nano,
        start_time_unix_nano: 0,
        attributes: BTreeMap::new(),
        value: DEMO_METRIC_VALUE,
    };

    let mut resource_attributes = BTreeMap::new();
    resource_attributes.insert("service.name".to_string(), DEMO_SERVICE_NAME.to_string());

    let metric = Metric {
        name: MetricName::new(DEMO_METRIC_NAME),
        description: String::new(),
        unit: String::new(),
        kind: MetricKind::Sum,
        points: Vec::new(),
        resource_attributes,
    };

    (metric, point)
}

/// A read-side decorator over a [`MetricStore`] that synthesises the demo
/// `request_count` metric at query time for the demo tenant, and delegates every
/// other read straight through to the wrapped store (ADR-0079).
///
/// Generic over the inner store `S` (zero-cost monomorphisation, no `dyn`) and
/// the [`Clock`] seam `C` (deterministic synthesis in tests).
pub struct DemoMetricOverlay<S, C = SystemClock> {
    inner: S,
    clock: C,
}

impl<S> DemoMetricOverlay<S, SystemClock> {
    /// Wrap `inner` with the production [`SystemClock`].
    pub fn with_system_clock(inner: S) -> Self {
        Self {
            inner,
            clock: SystemClock,
        }
    }
}

impl<S, C> DemoMetricOverlay<S, C> {
    /// Wrap `inner`, anchoring the demo's now-relative timestamp to `clock`.
    pub fn new(inner: S, clock: C) -> Self {
        Self { inner, clock }
    }
}

impl<S: MetricStore, C: Clock> MetricStore for DemoMetricOverlay<S, C> {
    /// Read-only overlay: the demo point is NEVER written. Real writes delegate
    /// straight through; there is no synthesis branch here, so the demo data has
    /// no write path (ADR-0079 read-only invariant).
    fn ingest(
        &self,
        tenant: &TenantId,
        batch: MetricBatch,
    ) -> Result<IngestReceipt, MetricStoreError> {
        self.inner.ingest(tenant, batch)
    }

    /// The metrics read path (`query(tenant, metric_name, range)`) flows through
    /// here: for the demo tenant's `request_count` the now-relative point is
    /// injected when its timestamp falls inside `range`.
    fn query(
        &self,
        tenant: &TenantId,
        metric_name: &MetricName,
        range: TimeRange,
    ) -> Result<Vec<(Metric, MetricPoint)>, MetricStoreError> {
        if !is_demo_tenant(tenant) || !is_demo_metric(metric_name) {
            return self.inner.query(tenant, metric_name, range);
        }
        let mut rows = self.inner.query(tenant, metric_name, range)?;
        let (metric, point) = synthesize_request_count(self.clock.now_unix_nano());
        if range.contains(point.time_unix_nano) {
            rows.push((metric, point));
        }
        rows.sort_by_key(|(_, point)| point.time_unix_nano);
        Ok(rows)
    }

    /// The predicate read path: for the demo tenant's `request_count` the point
    /// is injected when it falls inside `range` AND matches the predicate. The
    /// predicate carries the service scoping — a foreign-service read never
    /// matches the demo metric.
    fn query_with(
        &self,
        tenant: &TenantId,
        metric_name: &MetricName,
        range: TimeRange,
        predicate: &Predicate,
    ) -> Result<Vec<(Metric, MetricPoint)>, MetricStoreError> {
        if !is_demo_tenant(tenant) || !is_demo_metric(metric_name) {
            return self.inner.query_with(tenant, metric_name, range, predicate);
        }
        let mut rows = self
            .inner
            .query_with(tenant, metric_name, range, predicate)?;
        let (metric, point) = synthesize_request_count(self.clock.now_unix_nano());
        if range.contains(point.time_unix_nano) && predicate.matches(&metric, &point) {
            rows.push((metric, point));
        }
        rows.sort_by_key(|(_, point)| point.time_unix_nano);
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulse::{InMemoryMetricStore, NoopRecorder};

    /// A realistic, large "now" (~2023) so `now - offset` never underflows.
    const NOW: u64 = 1_700_000_000_000_000_000;

    /// A 15-minute rolling window in nanoseconds — a typical metrics query span.
    const FIFTEEN_MINUTES_NANOS: u64 = 15 * 60 * 1_000_000_000;

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

    fn empty_inner() -> InMemoryMetricStore {
        InMemoryMetricStore::new(Box::new(NoopRecorder))
    }

    fn acme() -> TenantId {
        TenantId(DEMO_TENANT_STR.to_string())
    }

    const DEMO_TENANT_STR: &str = "acme";

    fn request_count() -> MetricName {
        MetricName::new(DEMO_METRIC_NAME)
    }

    /// A real (non-demo) metric under another service, for the pass-through test.
    fn real_non_demo_metric(time_unix_nano: u64) -> Metric {
        let mut resource_attributes = BTreeMap::new();
        resource_attributes.insert("service.name".to_string(), "checkout-service".to_string());
        Metric {
            name: MetricName::new("http_server_duration"),
            description: String::new(),
            unit: "ms".to_string(),
            kind: MetricKind::Gauge,
            points: vec![MetricPoint {
                time_unix_nano,
                start_time_unix_nano: 0,
                attributes: BTreeMap::new(),
                value: 12.0,
            }],
            resource_attributes,
        }
    }

    // ---- Behavior 1: demo request_count window read returns the synthesised
    //      point with a now-relative timestamp. -----------------------------

    #[test]
    fn demo_request_count_window_query_returns_the_point_with_now_relative_timestamp() {
        let overlay = DemoMetricOverlay::new(empty_inner(), FixedClock::at(NOW));
        let window = TimeRange::new(NOW - FIFTEEN_MINUTES_NANOS, NOW + 1);

        let rows = overlay
            .query(&acme(), &request_count(), window)
            .expect("demo metrics window read succeeds");

        assert_eq!(rows.len(), 1, "the one synthesised demo metric point");
        let (metric, point) = &rows[0];
        assert_eq!(metric.name, request_count());
        assert_eq!(metric.kind, MetricKind::Sum);
        assert_eq!(point.value, DEMO_METRIC_VALUE);
        assert_eq!(
            point.time_unix_nano,
            NOW - METRIC_POINT_OFFSET_NANOS,
            "the point is timestamped now-relative, 30s before now"
        );
        assert!(
            window.contains(point.time_unix_nano),
            "the demo point lands inside the rolling window"
        );
        assert_eq!(
            metric
                .resource_attributes
                .get("service.name")
                .map(String::as_str),
            Some(DEMO_SERVICE_NAME),
            "the demo metric carries the demo service.name so a service matcher keeps it"
        );
    }

    #[test]
    fn demo_point_lands_inside_a_rolling_window_for_any_now() {
        for now in [
            1_700_000_000_000_000_000u64,
            1_800_000_000_000_000_000u64,
            2_000_000_000_000_000_000u64,
        ] {
            let overlay = DemoMetricOverlay::new(empty_inner(), FixedClock::at(now));
            let window = TimeRange::new(now - FIFTEEN_MINUTES_NANOS, now + 1);

            let rows = overlay
                .query(&acme(), &request_count(), window)
                .expect("demo read succeeds");

            assert_eq!(rows.len(), 1, "the demo point is inside the window");
            assert!(
                rows[0].1.time_unix_nano < now,
                "the demo point is timestamped at or before now"
            );
        }
    }

    // ---- Behavior 2: a non-demo metric name delegates unchanged. -----------

    #[test]
    fn non_demo_metric_name_query_delegates_unchanged() {
        let inner = empty_inner();
        let real = real_non_demo_metric(NOW - 5_000_000_000);
        inner
            .ingest(&acme(), MetricBatch::with_metrics(vec![real.clone()]))
            .expect("seed a real metric");
        let overlay = DemoMetricOverlay::new(inner, FixedClock::at(NOW));

        let rows = overlay
            .query(
                &acme(),
                &MetricName::new("http_server_duration"),
                TimeRange::all(),
            )
            .expect("non-demo metric read succeeds");

        assert_eq!(rows.len(), 1, "only the real metric, no demo injected");
        assert_eq!(rows[0].0.name, MetricName::new("http_server_duration"));
        assert_eq!(rows[0].1.value, 12.0);
    }

    // ---- Behavior 3: foreign-tenant reads delegate unchanged. --------------

    #[test]
    fn foreign_tenant_request_count_query_delegates_unchanged() {
        // The demo is scoped to the demo tenant. A request_count read under a
        // DIFFERENT tenant must NOT synthesise — the tenant guard is load-bearing.
        let overlay = DemoMetricOverlay::new(empty_inner(), FixedClock::at(NOW));

        let rows = overlay
            .query(
                &TenantId("someone-else".to_string()),
                &request_count(),
                TimeRange::all(),
            )
            .expect("foreign-tenant read succeeds");

        assert!(
            rows.is_empty(),
            "no demo metric is synthesised for request_count under a foreign tenant"
        );
    }

    // ---- Behavior 4: the predicate read path (`query_with`) injects the demo
    //      point for the demo identity and scopes by tenant/metric/predicate. -

    #[test]
    fn demo_request_count_query_with_service_predicate_returns_the_point() {
        let overlay = DemoMetricOverlay::new(empty_inner(), FixedClock::at(NOW));
        let window = TimeRange::new(NOW - FIFTEEN_MINUTES_NANOS, NOW + 1);

        let rows = overlay
            .query_with(
                &acme(),
                &request_count(),
                window,
                &Predicate::new().service(DEMO_SERVICE_NAME),
            )
            .expect("demo predicate read succeeds");

        assert_eq!(rows.len(), 1, "the one synthesised demo point");
        assert_eq!(rows[0].1.value, DEMO_METRIC_VALUE);
        assert_eq!(rows[0].1.time_unix_nano, NOW - METRIC_POINT_OFFSET_NANOS);
    }

    #[test]
    fn foreign_tenant_request_count_query_with_delegates() {
        // The tenant guard gates the predicate path too.
        let overlay = DemoMetricOverlay::new(empty_inner(), FixedClock::at(NOW));

        let rows = overlay
            .query_with(
                &TenantId("someone-else".to_string()),
                &request_count(),
                TimeRange::all(),
                &Predicate::new().service(DEMO_SERVICE_NAME),
            )
            .expect("foreign-tenant predicate read succeeds");

        assert!(
            rows.is_empty(),
            "no demo point under a foreign tenant, even via query_with"
        );
    }

    #[test]
    fn non_demo_metric_name_query_with_delegates() {
        // The metric-name guard gates the predicate path too.
        let overlay = DemoMetricOverlay::new(empty_inner(), FixedClock::at(NOW));

        let rows = overlay
            .query_with(
                &acme(),
                &MetricName::new("http_server_duration"),
                TimeRange::all(),
                &Predicate::new().service(DEMO_SERVICE_NAME),
            )
            .expect("non-demo metric predicate read succeeds");

        assert!(
            rows.is_empty(),
            "no demo point is synthesised for a non-demo metric name"
        );
    }

    #[test]
    fn demo_tenant_foreign_service_query_with_excludes_the_demo_point() {
        // The demo point carries the demo service.name; a foreign-service
        // predicate must exclude it (the predicate carries the service scoping).
        let overlay = DemoMetricOverlay::new(empty_inner(), FixedClock::at(NOW));

        let rows = overlay
            .query_with(
                &acme(),
                &request_count(),
                TimeRange::all(),
                &Predicate::new().service("checkout-service"),
            )
            .expect("demo-tenant foreign-service predicate read succeeds");

        assert!(
            rows.is_empty(),
            "the demo point is excluded when the predicate scopes to a foreign service"
        );
    }

    // ---- Behavior 5: read-only invariant — the demo is never written. ------

    #[test]
    fn demo_read_does_not_write_to_the_wrapped_store() {
        // If a demo read wrote its synthesis into the wrapped store, a second
        // read would accumulate. Two reads each returning exactly one point
        // proves the synthesis never mutates the store — read-only, store-free.
        let overlay = DemoMetricOverlay::new(empty_inner(), FixedClock::at(NOW));

        let first = overlay
            .query(&acme(), &request_count(), TimeRange::all())
            .expect("first demo read");
        let second = overlay
            .query(&acme(), &request_count(), TimeRange::all())
            .expect("second demo read");

        assert_eq!(first.len(), 1, "first read synthesises one point");
        assert_eq!(
            second.len(),
            1,
            "second read still one — the point was never persisted (no accumulation)"
        );
        assert_eq!(first, second, "synthesis is stable read-to-read");
    }
}
