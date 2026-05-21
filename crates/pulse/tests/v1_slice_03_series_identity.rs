// Kaleidoscope Pulse v1 — slice 03 series identity acceptance test
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

//! Slice 03 — a metric series is identified by its full label set.
//!
//! Maps to
//! `docs/feature/pulse-series-identity-v0/slices/slice-01-series-identity-by-label-set.md`
//! (US-01, US-02).
//!
//! A series is identified by its FULL label set (metric name +
//! `resource_attributes`), not by name alone. Two metrics sharing a
//! name but differing by `service.name` stay two distinct series, each
//! wearing its own labels, on the live path and across a durable
//! restart.
//!
//! RED is BEHAVIOURAL, not compile-level. The code compiles today: it
//! keys series by `(tenant, MetricName)` and overwrites
//! `resource_attributes` on each ingest, so two services sharing a name
//! collapse into one series wearing the last-ingested service's labels.
//! These tests therefore FAIL on their assertions (collapsed series,
//! wrong labels, wrong point counts), NOT on a missing symbol. No `src`
//! scaffold is needed; only the keying behaviour beneath the unchanged
//! `MetricStore` trait changes.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aegis::TenantId;
use pulse::{
    FileBackedMetricStore, Metric, MetricBatch, MetricKind, MetricName, MetricPoint, MetricStore,
    NoopRecorder, TimeRange,
};

fn tenant(id: &str) -> TenantId {
    TenantId(id.to_string())
}

fn name(s: &str) -> MetricName {
    MetricName::new(s)
}

fn point(time_unix_nano: u64, value: f64) -> MetricPoint {
    MetricPoint {
        time_unix_nano,
        start_time_unix_nano: 0,
        attributes: BTreeMap::new(),
        value,
    }
}

/// A point carrying a single point-level attribute. Used to prove that
/// point attributes do NOT split a series.
fn point_with_attr(
    time_unix_nano: u64,
    value: f64,
    attr_key: &str,
    attr_value: &str,
) -> MetricPoint {
    let mut attributes = BTreeMap::new();
    attributes.insert(attr_key.to_string(), attr_value.to_string());
    MetricPoint {
        time_unix_nano,
        start_time_unix_nano: 0,
        attributes,
        value,
    }
}

/// A gauge whose only resource attribute is `service.name`. The series
/// identity under test is the full label set, so `service.name` is the
/// label that distinguishes one service's series from another's.
fn gauge(metric_name: &str, service: &str, points: Vec<MetricPoint>) -> Metric {
    let mut resource = BTreeMap::new();
    resource.insert("service.name".to_string(), service.to_string());
    Metric {
        name: MetricName::new(metric_name),
        description: "test gauge".to_string(),
        unit: "1".to_string(),
        kind: MetricKind::Gauge,
        points,
        resource_attributes: resource,
    }
}

fn temp_base(test_name: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    path.push(format!("pulse-v1-ident-{test_name}-{pid}-{nanos}"));
    fs::create_dir_all(&path).expect("mkdir");
    path.push("store");
    path
}

fn cleanup(base: &std::path::Path) {
    if let Some(dir) = base.parent() {
        let _ = fs::remove_dir_all(dir);
    }
}

/// Group a flat `query` result into series by `service.name`.
///
/// `query` returns a flat `Vec<(Metric, MetricPoint)>` that may carry
/// rows from more than one series when a name is multi-service. Each
/// row carries its own series' `Metric` (and thus its
/// `resource_attributes`). We group by the row's
/// `resource_attributes["service.name"]` to reconstruct the distinct
/// series for assertion. Ordering across series is NOT assumed: rows are
/// keyed into a `BTreeMap` by service, so the assertions are
/// order-insensitive across series.
fn series_by_service(rows: &[(Metric, MetricPoint)]) -> BTreeMap<String, Vec<MetricPoint>> {
    let mut grouped: BTreeMap<String, Vec<MetricPoint>> = BTreeMap::new();
    for (metric, p) in rows {
        let service = metric
            .resource_attributes
            .get("service.name")
            .cloned()
            .unwrap_or_default();
        grouped.entry(service).or_default().push(p.clone());
    }
    grouped
}

/// The set of distinct `service.name` values present in a flat result.
fn services_present(rows: &[(Metric, MetricPoint)]) -> BTreeSet<String> {
    rows.iter()
        .filter_map(|(m, _)| m.resource_attributes.get("service.name").cloned())
        .collect()
}

// --------------------------------------------------------------------
// AC-1 (US-01 walking skeleton) — two services, two series
//
// Tenant "acme-prod". Ingest `http_requests_total` for checkout (value
// 10 at t=100), then for cart (value 20 at t=100). A single query of
// the name returns BOTH series, each wearing its own service.name and
// carrying its own value. Neither overwrote the other.
// --------------------------------------------------------------------

#[test]
fn walking_skeleton_two_services_return_two_distinct_series() {
    // @walking_skeleton @driving_port US-01
    let base = temp_base("ws_two_services");
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");
    let acme = tenant("acme-prod");

    store
        .ingest(
            &acme,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![point(100, 10.0)],
            )]),
        )
        .expect("ingest checkout");
    store
        .ingest(
            &acme,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "cart",
                vec![point(100, 20.0)],
            )]),
        )
        .expect("ingest cart");

    let rows = store
        .query(&acme, &name("http_requests_total"), TimeRange::all())
        .expect("query");

    let series = series_by_service(&rows);
    assert_eq!(
        series.keys().cloned().collect::<BTreeSet<_>>(),
        BTreeSet::from(["cart".to_string(), "checkout".to_string()]),
        "two distinct services must come back as two distinct series"
    );

    let checkout = series.get("checkout").expect("checkout series present");
    assert_eq!(checkout.len(), 1, "checkout has one point");
    assert_eq!(
        checkout[0].value, 10.0,
        "checkout point keeps its own value"
    );

    let cart = series.get("cart").expect("cart series present");
    assert_eq!(cart.len(), 1, "cart has one point");
    assert_eq!(cart[0].value, 20.0, "cart point keeps its own value");

    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-2 — three services under one name return three distinct series
// --------------------------------------------------------------------

#[test]
fn three_services_return_three_distinct_series() {
    // US-01
    let base = temp_base("three_services");
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");
    let acme = tenant("acme-prod");

    for (service, value) in [("checkout", 10.0), ("cart", 20.0), ("search", 30.0)] {
        store
            .ingest(
                &acme,
                MetricBatch::with_metrics(vec![gauge(
                    "http_requests_total",
                    service,
                    vec![point(100, value)],
                )]),
            )
            .expect("ingest");
    }

    let rows = store
        .query(&acme, &name("http_requests_total"), TimeRange::all())
        .expect("query");

    assert_eq!(
        services_present(&rows),
        BTreeSet::from([
            "cart".to_string(),
            "checkout".to_string(),
            "search".to_string(),
        ]),
        "three distinct services must come back as three distinct series"
    );

    let series = series_by_service(&rows);
    assert_eq!(series.get("checkout").map(|p| p[0].value), Some(10.0));
    assert_eq!(series.get("cart").map(|p| p[0].value), Some(20.0));
    assert_eq!(series.get("search").map(|p| p[0].value), Some(30.0));

    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-3 (edge) — identical full label set across two ingests merges
//
// Same service.name="checkout", a point at t=100 then a point at t=200.
// One checkout series with both points, ascending. Distinct identity
// must not become accidental duplication when label sets are equal.
// --------------------------------------------------------------------

#[test]
fn identical_label_set_across_two_ingests_merges_into_one_series() {
    // US-01
    let base = temp_base("identical_merge");
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");
    let acme = tenant("acme-prod");

    store
        .ingest(
            &acme,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![point(100, 1.0)],
            )]),
        )
        .expect("ingest t=100");
    store
        .ingest(
            &acme,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![point(200, 2.0)],
            )]),
        )
        .expect("ingest t=200");

    let rows = store
        .query(&acme, &name("http_requests_total"), TimeRange::all())
        .expect("query");

    let series = series_by_service(&rows);
    assert_eq!(
        series.len(),
        1,
        "an identical label set must merge into one series, not duplicate"
    );
    let checkout = series.get("checkout").expect("checkout series present");
    assert_eq!(checkout.len(), 2, "both points present in one series");
    assert_eq!(checkout[0].time_unix_nano, 100, "points ascending by time");
    assert_eq!(checkout[1].time_unix_nano, 200, "points ascending by time");

    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-4 (edge) — point attributes do NOT split a series
//
// One service.name="checkout" metric with two points whose
// point-level http.route differ ("/a" and "/b"). One checkout series,
// both points present, each keeping its own http.route.
// --------------------------------------------------------------------

#[test]
fn differing_point_attributes_do_not_split_a_series() {
    // US-01
    let base = temp_base("point_attrs");
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");
    let acme = tenant("acme-prod");

    store
        .ingest(
            &acme,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![
                    point_with_attr(100, 1.0, "http.route", "/a"),
                    point_with_attr(200, 2.0, "http.route", "/b"),
                ],
            )]),
        )
        .expect("ingest");

    let rows = store
        .query(&acme, &name("http_requests_total"), TimeRange::all())
        .expect("query");

    let series = series_by_service(&rows);
    assert_eq!(
        series.len(),
        1,
        "point-level attributes must not split the checkout series"
    );
    let checkout = series.get("checkout").expect("checkout series present");
    assert_eq!(checkout.len(), 2, "both points present in one series");

    let routes: BTreeSet<String> = checkout
        .iter()
        .filter_map(|p| p.attributes.get("http.route").cloned())
        .collect();
    assert_eq!(
        routes,
        BTreeSet::from(["/a".to_string(), "/b".to_string()]),
        "each point keeps its own http.route point attribute"
    );

    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-5 (US-02) — distinct series survive snapshot + drop + reopen
// --------------------------------------------------------------------

#[test]
fn distinct_series_survive_snapshot_and_reopen() {
    // US-02
    let base = temp_base("survive_snapshot");
    let acme = tenant("acme-prod");
    {
        let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        store
            .ingest(
                &acme,
                MetricBatch::with_metrics(vec![gauge(
                    "http_requests_total",
                    "checkout",
                    vec![point(100, 10.0)],
                )]),
            )
            .expect("ingest checkout");
        store
            .ingest(
                &acme,
                MetricBatch::with_metrics(vec![gauge(
                    "http_requests_total",
                    "cart",
                    vec![point(100, 20.0)],
                )]),
            )
            .expect("ingest cart");
        store.snapshot().expect("snapshot");
    }

    let reopened = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    let rows = reopened
        .query(&acme, &name("http_requests_total"), TimeRange::all())
        .expect("query");

    let series = series_by_service(&rows);
    assert_eq!(
        series.keys().cloned().collect::<BTreeSet<_>>(),
        BTreeSet::from(["cart".to_string(), "checkout".to_string()]),
        "both distinct series must survive snapshot + reopen"
    );
    assert_eq!(series.get("checkout").map(|p| p[0].value), Some(10.0));
    assert_eq!(series.get("cart").map(|p| p[0].value), Some(20.0));

    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-6 (US-02 edge) — distinct series survive a WAL-only reopen
//
// No snapshot: recovery rebuilds the series by replaying the WAL only.
// This proves replay, not just snapshot, honours full-label-set
// identity.
// --------------------------------------------------------------------

#[test]
fn distinct_series_survive_wal_only_reopen() {
    // US-02
    let base = temp_base("survive_wal_only");
    let acme = tenant("acme-prod");
    {
        let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        store
            .ingest(
                &acme,
                MetricBatch::with_metrics(vec![gauge(
                    "http_requests_total",
                    "checkout",
                    vec![point(100, 10.0)],
                )]),
            )
            .expect("ingest checkout");
        store
            .ingest(
                &acme,
                MetricBatch::with_metrics(vec![gauge(
                    "http_requests_total",
                    "cart",
                    vec![point(100, 20.0)],
                )]),
            )
            .expect("ingest cart");
        // No snapshot here: drop forces a pure WAL-replay recovery.
    }

    let reopened = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    let rows = reopened
        .query(&acme, &name("http_requests_total"), TimeRange::all())
        .expect("query");

    let series = series_by_service(&rows);
    assert_eq!(
        series.keys().cloned().collect::<BTreeSet<_>>(),
        BTreeSet::from(["cart".to_string(), "checkout".to_string()]),
        "both distinct series must be rebuilt by WAL replay"
    );
    assert_eq!(series.get("checkout").map(|p| p[0].value), Some(10.0));
    assert_eq!(series.get("cart").map(|p| p[0].value), Some(20.0));

    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-7 (US-02 boundary) — a re-ingest after reopen joins the recovered
// series by full label set, not name alone
//
// Open, ingest checkout(t=100) + cart(t=100), snapshot, reopen, then
// ingest checkout(t=200). Checkout has two points; cart still one; no
// third series.
// --------------------------------------------------------------------

#[test]
fn reingest_after_reopen_joins_the_recovered_series() {
    // US-02
    let base = temp_base("reingest_after_reopen");
    let acme = tenant("acme-prod");
    {
        let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open 1");
        store
            .ingest(
                &acme,
                MetricBatch::with_metrics(vec![gauge(
                    "http_requests_total",
                    "checkout",
                    vec![point(100, 10.0)],
                )]),
            )
            .expect("ingest checkout");
        store
            .ingest(
                &acme,
                MetricBatch::with_metrics(vec![gauge(
                    "http_requests_total",
                    "cart",
                    vec![point(100, 20.0)],
                )]),
            )
            .expect("ingest cart");
        store.snapshot().expect("snapshot");
    }

    let reopened = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("reopen");
    reopened
        .ingest(
            &acme,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![point(200, 30.0)],
            )]),
        )
        .expect("re-ingest checkout after reopen");

    let rows = reopened
        .query(&acme, &name("http_requests_total"), TimeRange::all())
        .expect("query");

    let series = series_by_service(&rows);
    assert_eq!(
        series.keys().cloned().collect::<BTreeSet<_>>(),
        BTreeSet::from(["cart".to_string(), "checkout".to_string()]),
        "the re-ingest must not create a third series"
    );

    let checkout = series.get("checkout").expect("checkout series present");
    assert_eq!(
        checkout.len(),
        2,
        "the re-ingest joined the recovered checkout series"
    );
    assert_eq!(checkout[0].time_unix_nano, 100, "recovered point first");
    assert_eq!(
        checkout[1].time_unix_nano, 200,
        "new point appended, ascending"
    );

    let cart = series.get("cart").expect("cart series present");
    assert_eq!(
        cart.len(),
        1,
        "the checkout re-ingest did not land in the cart series"
    );
    assert_eq!(cart[0].time_unix_nano, 100);

    cleanup(&base);
}

// --------------------------------------------------------------------
// AC-8 — no cross-service overwrite
//
// After the second service ingests, the first service's series still
// carries its OWN service.name and its OWN point value. Asserted
// explicitly: today the overwrite leaves both rows wearing the
// last-ingested service's labels.
// --------------------------------------------------------------------

#[test]
fn second_ingest_does_not_overwrite_first_services_labels() {
    // US-01
    let base = temp_base("no_overwrite");
    let store = FileBackedMetricStore::open(&base, Box::new(NoopRecorder)).expect("open");
    let acme = tenant("acme-prod");

    store
        .ingest(
            &acme,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "checkout",
                vec![point(100, 10.0)],
            )]),
        )
        .expect("ingest checkout");
    store
        .ingest(
            &acme,
            MetricBatch::with_metrics(vec![gauge(
                "http_requests_total",
                "cart",
                vec![point(100, 20.0)],
            )]),
        )
        .expect("ingest cart");

    let rows = store
        .query(&acme, &name("http_requests_total"), TimeRange::all())
        .expect("query");

    // The point that came from checkout (value 10) must still be paired
    // with a Metric wearing service.name "checkout", not "cart".
    let checkout_row = rows
        .iter()
        .find(|(_, p)| p.value == 10.0)
        .expect("the checkout point (value 10) is present");
    assert_eq!(
        checkout_row.0.resource_attributes.get("service.name"),
        Some(&"checkout".to_string()),
        "the checkout point must keep its own service.name, not be overwritten by cart"
    );

    let cart_row = rows
        .iter()
        .find(|(_, p)| p.value == 20.0)
        .expect("the cart point (value 20) is present");
    assert_eq!(
        cart_row.0.resource_attributes.get("service.name"),
        Some(&"cart".to_string()),
        "the cart point carries its own service.name"
    );

    cleanup(&base);
}
