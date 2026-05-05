//! Probe gold-test runner — ADR-0010 layer-3 behavioural enforcement
//! of the Earned-Trust probe contract.
//!
//! The Earned-Trust contract (Principle 12; ADR-0007) is enforced by
//! three semantically-orthogonal layers:
//!
//!   | Layer        | Mechanism                                      |
//!   |--------------|------------------------------------------------|
//!   | Subtype      | Composition root requires `OtlpSink + Probe`   |
//!   | Structural   | xtask AST-walk asserts every `impl OtlpSink`   |
//!   |              | also has a matching `impl Probe`               |
//!   | Behavioural  | This file: starts a wiremock that lies, calls  |
//!   |              | `Probe::probe()`, asserts the lie is caught    |
//!
//! A maintainer who silently replaces the `Probe` body with `Ok(())`
//! is caught by this file's assertions: a no-op probe issues zero
//! HTTP traffic against the fixture and the wire-traffic assertion
//! fails. A maintainer who only does the OPTIONS preflight and
//! returns success on 200 is caught by the lying-fixture scenario:
//! 200 OPTIONS + 503 POST must surface as `Refused`.
//!
//! These tests enter at the smallest port-to-port surface that
//! exposes the network behaviour: `aperture::testing::
//! forwarding_sink_probe_for_gold_test()` returns the concrete
//! `Arc<dyn Probe>`; the test calls `.probe().await` directly.
//! Going through `aperture::spawn` would couple the assertion to
//! the listener-binding path and add false negatives.

use std::time::Duration;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use aperture::testing::forwarding_sink_probe_for_gold_test;

// =========================================================================
// The catalogued v0 substrate lie: 200 OPTIONS / 503 POST
// =========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn probe_refuses_when_downstream_returns_200_options_but_503_post() {
    // The catalogued v0 substrate lie: a downstream that answers OK to
    // the OPTIONS preflight but rejects the actual POST. ADR-0007's
    // behavioural-layer enforcement target.
    let downstream = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&downstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&downstream)
        .await;

    let probe = forwarding_sink_probe_for_gold_test(downstream.uri(), Duration::from_secs(2));
    let result = probe.probe().await;

    assert!(
        result.is_err(),
        "probe must refuse the 200/503 lie; got: {result:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn probe_actually_issues_an_options_request() {
    // The behavioural-layer assertion that catches a `Probe { Ok(()) }`
    // no-op replacement: the fixture records every request; if the
    // probe issues no HTTP traffic, this assertion fires.
    let downstream = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&downstream)
        .await;

    let probe = forwarding_sink_probe_for_gold_test(downstream.uri(), Duration::from_secs(2));
    let _ = probe.probe().await;

    let received = downstream.received_requests().await.unwrap_or_default();
    let options_count = received
        .iter()
        .filter(|r| r.method == http::Method::OPTIONS && r.url.path() == "/v1/logs")
        .count();
    assert_eq!(
        options_count, 1,
        "probe must issue exactly one OPTIONS preflight; received: {received:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn probe_issues_post_when_options_returns_200_so_the_lie_is_caught() {
    // The behavioural assertion that catches a "stop after OPTIONS=2xx"
    // shortcut: when OPTIONS=200, the probe MUST also POST so that the
    // catalogued lie scenario can refuse. The fixture records the POST
    // even though it returns 503 (the previous test asserts the
    // refusal; this one asserts the wire traffic).
    let downstream = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&downstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&downstream)
        .await;

    let probe = forwarding_sink_probe_for_gold_test(downstream.uri(), Duration::from_secs(2));
    let _ = probe.probe().await;

    let received = downstream.received_requests().await.unwrap_or_default();
    let options_count = received
        .iter()
        .filter(|r| r.method == http::Method::OPTIONS && r.url.path() == "/v1/logs")
        .count();
    let post_count = received
        .iter()
        .filter(|r| r.method == http::Method::POST && r.url.path() == "/v1/logs")
        .count();
    assert_eq!(options_count, 1, "expected one OPTIONS preflight");
    assert_eq!(
        post_count, 1,
        "probe MUST issue a POST when OPTIONS=200 — this is the lie-detector",
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn probe_issues_post_when_options_returns_405_to_reach_otel_compatible_downstreams() {
    // Symmetric to the test above for the 404/405 fallback case: an
    // OTel-compatible downstream that doesn't implement OPTIONS still
    // accepts OTLP POSTs. The probe must reach the POST to verify.
    let downstream = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(405))
        .mount(&downstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&downstream)
        .await;

    let probe = forwarding_sink_probe_for_gold_test(downstream.uri(), Duration::from_secs(2));
    let result = probe.probe().await;

    assert!(
        result.is_ok(),
        "probe should succeed via degraded POST when OPTIONS=405; got: {result:?}"
    );
    let received = downstream.received_requests().await.unwrap_or_default();
    let post_count = received
        .iter()
        .filter(|r| r.method == http::Method::POST && r.url.path() == "/v1/logs")
        .count();
    assert_eq!(
        post_count, 1,
        "probe MUST POST after OPTIONS=405 to verify OTel-compatibility",
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn probe_short_circuits_on_options_204_and_does_not_post() {
    // 204 is the formal preflight-OK response. RFC 9110 specifies
    // 204 No Content as the canonical OPTIONS response when the server
    // confirms acceptability without a body. Aperture treats 204 as
    // sufficient evidence and skips the degraded POST. Pinned here so
    // a mutation that tightens the success condition (e.g. requires
    // both stages unconditionally) is caught against the wire-traffic
    // count.
    let downstream = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/v1/logs"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&downstream)
        .await;
    // Deliberately NO POST mock: any POST would return 404 from
    // wiremock's default. If the probe POSTed, the wire-traffic count
    // assertion below would observe it.

    let probe = forwarding_sink_probe_for_gold_test(downstream.uri(), Duration::from_secs(2));
    let result = probe.probe().await;

    assert!(
        result.is_ok(),
        "OPTIONS=204 alone must be sufficient; got: {result:?}"
    );
    let received = downstream.received_requests().await.unwrap_or_default();
    let post_count = received
        .iter()
        .filter(|r| r.method == http::Method::POST && r.url.path() == "/v1/logs")
        .count();
    assert_eq!(
        post_count, 0,
        "OPTIONS=204 must short-circuit; no POST should be issued",
    );
}
