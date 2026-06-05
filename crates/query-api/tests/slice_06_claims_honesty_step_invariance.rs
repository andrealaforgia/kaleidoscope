// Kaleidoscope query-api — slice 06 claims-honesty-pass-v0 behaviour suite
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

//! Slice 06 — claims-honesty-pass-v0 US-05: `query_range` `step` invariance.
//!
//! Feature: `claims-honesty-pass-v0`, DESIGN flag #1 resolved DOCUMENT
//! (ADR-0062): `GET /api/v1/query_range` returns the raw in-window stored
//! points; `step` is a reserved, accepted-but-not-honoured parameter at
//! v0, NOT a Prometheus stepped grid.
//!
//! The observable that pins the documented boundary is INVARIANCE under
//! `step`: for a fixed `query`/`start`/`end`, two distinct `step` values
//! (`15s`, `60s`) AND the omitted-`step` case all return BYTE-IDENTICAL
//! output. This is the verifier's black-box; under DOCUMENT it passes
//! today and documents the honest contract. It is therefore NOT
//! `#[ignore]`d (it guards against regression now).
//!
//! ADR-0062 NOTE: a FUTURE stepped-grid feature will INTENTIONALLY retire
//! this assertion (two `step` values would then produce DIFFERENT,
//! correctly-stepped output). DISTILL/DELIVER for that future feature must
//! delete this test deliberately — a then-failing assertion here is
//! PLANNED, not a regression.

mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};

use common::{call, gauge, open_durable_store, point, secs_to_nanos, tenant};
use pulse::{MetricBatch, MetricStore};

/// Build a `query_range` request with an explicit `step` value.
fn request_with_step(query: &str, start: &str, end: &str, step: &str) -> Request<Body> {
    let uri = format!("/api/v1/query_range?query={query}&start={start}&end={end}&step={step}");
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("build request")
}

/// Build a `query_range` request with NO `step` parameter at all.
fn request_without_step(query: &str, start: &str, end: &str) -> Request<Body> {
    let uri = format!("/api/v1/query_range?query={query}&start={start}&end={end}");
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("build request")
}

/// @driving_port @US-05 @property
///
/// Given a metric over a fixed window with several in-range points,
/// When the operator queries it with `step=15s`, then `step=60s`, then
/// with `step` omitted entirely,
/// Then all three responses are BYTE-IDENTICAL — because `step` is
/// accepted-but-not-honoured at v0 (ADR-0062): raw stored points are
/// returned, never a `step`-driven grid. This is the honest contract the
/// corrected README now describes; the black-box verifier agrees with the
/// prose with zero gap.
#[tokio::test]
async fn step_is_not_honoured_two_step_values_and_omitted_step_return_identical_output() {
    let (store, _base) = open_durable_store("step-invariance");
    let t = tenant("northwind");
    store
        .ingest(
            &t,
            MetricBatch::with_metrics(vec![gauge(
                "checkout_requests_total",
                "checkout",
                vec![
                    point(secs_to_nanos(1_716_200_000), 1.0, &[]),
                    point(secs_to_nanos(1_716_200_015), 2.0, &[]),
                    point(secs_to_nanos(1_716_200_030), 3.0, &[]),
                    point(secs_to_nanos(1_716_200_045), 4.0, &[]),
                ],
            )]),
        )
        .expect("seed durable store");

    let query = "checkout_requests_total";
    let start = "1716200000";
    let end = "1716200060";

    // A fresh router per call (the store is cheap to share via Arc clone).
    let store = store as Arc<dyn MetricStore + Send + Sync>;

    let (s15, body15) = call(
        query_api::router(Arc::clone(&store), Some(t.clone()), None),
        request_with_step(query, start, end, "15s"),
    )
    .await;
    let (s60, body60) = call(
        query_api::router(Arc::clone(&store), Some(t.clone()), None),
        request_with_step(query, start, end, "60s"),
    )
    .await;
    let (s_none, body_none) = call(
        query_api::router(Arc::clone(&store), Some(t.clone()), None),
        request_without_step(query, start, end),
    )
    .await;

    assert_eq!(s15, StatusCode::OK);
    assert_eq!(s60, StatusCode::OK);
    assert_eq!(s_none, StatusCode::OK);

    // INVARIANCE under `step`: the canonical JSON encodings are identical.
    // `serde_json::Value`'s `to_string` is a deterministic canonical form,
    // so equal Values render to identical bytes.
    assert_eq!(
        body15, body60,
        "step=15s and step=60s must return identical output — `step` is \
         accepted-but-not-honoured at v0 (ADR-0062), so no grid re-sampling \
         occurs"
    );
    assert_eq!(
        body15, body_none,
        "the omitted-`step` case must return the same raw points as any \
         explicit `step` — the parameter is inert at v0"
    );
    assert_eq!(
        body15.to_string(),
        body60.to_string(),
        "byte-identical canonical rendering for two distinct `step` values"
    );
}
