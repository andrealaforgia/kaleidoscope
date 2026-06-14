// Kaleidoscope query-api — slice 09 RFC3339-or-unix time-bounds acceptance
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

//! FIX-B.2 — `/api/v1/query_range` accepts `start`/`end` as EITHER RFC3339
//! OR unix seconds, and yields the SAME result for the equivalent instant.
//! An unparseable bound is a 400 whose reason names both accepted formats
//! and never echoes the raw value.
//!
//! `2024-05-20T00:00:00Z` is exactly unix `1_716_163_200`;
//! `2024-05-20T00:00:45Z` is exactly unix `1_716_163_245`. The two
//! requests below ask for the same window in the two notations and must
//! return byte-identical success bodies.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;

use common::{
    call, gauge, open_durable_store, point, prism_accepts_success, query_range_request,
    result_series, secs_to_nanos, tenant,
};
use pulse::{MetricBatch, MetricStore};

#[tokio::test]
async fn rfc3339_and_unix_windows_over_the_same_instant_agree() {
    let (store, _base) = open_durable_store("rfc3339-bounds");
    let t = tenant("acme-prod");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "process_cpu_utilization",
                "checkout",
                vec![point(secs_to_nanos(1_716_163_210), 0.42, &[])],
            )]),
        )
        .expect("seed durable store");

    let store: Arc<dyn MetricStore + Send + Sync> = store;

    // The same window in unix seconds and in RFC3339.
    let unix_router = query_api::router(store.clone(), Some(t.clone()), None);
    let (unix_status, unix_body) = call(
        unix_router,
        query_range_request("process_cpu_utilization", "1716163200", "1716163245"),
    )
    .await;

    let rfc_router = query_api::router(store, Some(t), None);
    let (rfc_status, rfc_body) = call(
        rfc_router,
        query_range_request(
            "process_cpu_utilization",
            "2024-05-20T00:00:00Z",
            "2024-05-20T00:00:45Z",
        ),
    )
    .await;

    assert_eq!(unix_status, StatusCode::OK, "unix-seconds window is 200");
    assert_eq!(rfc_status, StatusCode::OK, "RFC3339 window is 200");
    assert!(prism_accepts_success(&unix_body), "unix body is success");
    assert!(prism_accepts_success(&rfc_body), "RFC3339 body is success");
    assert_eq!(
        unix_body, rfc_body,
        "the equivalent instant yields the SAME result in either notation"
    );
    assert_eq!(
        result_series(&rfc_body).len(),
        1,
        "the seeded point is in the window"
    );
}

#[tokio::test]
async fn an_unparseable_bound_is_a_400_naming_both_formats_without_echo() {
    let (store, _base) = open_durable_store("rfc3339-bad");
    let t = tenant("acme-prod");
    let store: Arc<dyn MetricStore + Send + Sync> = store;
    let router = query_api::router(store, Some(t), None);

    let (status, body) = call(
        router,
        query_range_request("process_cpu_utilization", "notatimestamp", "1716163245"),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "unparseable bound is 400");
    let error = body
        .get("error")
        .and_then(|value| value.as_str())
        .expect("error envelope carries a string reason");
    assert!(error.contains("RFC3339"), "reason names RFC3339: {error}");
    assert!(error.contains("unix"), "reason names unix seconds: {error}");
    assert!(
        !error.contains("notatimestamp"),
        "reason must not echo the raw value: {error}"
    );
}
