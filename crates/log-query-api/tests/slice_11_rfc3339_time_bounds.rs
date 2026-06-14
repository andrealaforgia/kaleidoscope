// Kaleidoscope log-query-api — slice 11 RFC3339-or-unix time-bounds
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

//! FIX-B.2 — `/api/v1/logs` accepts `start`/`end` as EITHER RFC3339 OR
//! unix seconds, yielding the SAME records for the equivalent instant; an
//! unparseable bound is a 400 naming both formats without echoing the raw
//! value. `2024-05-20T00:00:00Z` == unix `1_716_163_200`;
//! `2024-05-20T00:00:45Z` == unix `1_716_163_245`.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;

use common::{call, open_durable_store, record, records_array, seed, tenant};
use lumen::LogStore;

#[tokio::test]
async fn rfc3339_and_unix_windows_over_the_same_instant_agree() {
    let (store, _base) = open_durable_store("rfc3339-bounds");
    let t = tenant("acme-prod");
    seed(
        &store,
        &t,
        vec![record(1_716_163_210, "checkout", "checkout: started")],
    );
    let store: Arc<dyn LogStore + Send + Sync> = store;

    let unix_router = log_query_api::router(store.clone(), Some(t.clone()));
    let (unix_status, unix_body) = call(
        unix_router,
        common::logs_request("1716163200", "1716163245"),
    )
    .await;

    let rfc_router = log_query_api::router(store, Some(t));
    let (rfc_status, rfc_body) = call(
        rfc_router,
        common::logs_request("2024-05-20T00:00:00Z", "2024-05-20T00:00:45Z"),
    )
    .await;

    assert_eq!(unix_status, StatusCode::OK, "unix-seconds window is 200");
    assert_eq!(rfc_status, StatusCode::OK, "RFC3339 window is 200");
    assert_eq!(
        unix_body, rfc_body,
        "the equivalent instant yields the SAME records in either notation"
    );
    assert_eq!(
        records_array(&rfc_body).len(),
        1,
        "the seeded record is in the window"
    );
}

#[tokio::test]
async fn an_unparseable_bound_is_a_400_naming_both_formats_without_echo() {
    let (store, _base) = open_durable_store("rfc3339-bad");
    let t = tenant("acme-prod");
    let store: Arc<dyn LogStore + Send + Sync> = store;
    let router = log_query_api::router(store, Some(t));

    let (status, body) = call(router, common::logs_request("notatimestamp", "1716163245")).await;

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
