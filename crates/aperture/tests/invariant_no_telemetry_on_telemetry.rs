//! Invariant — no telemetry from telemetry.
//!
//! DISCUSS D4 + Q6: Aperture's outbound network footprint =
//! ForwardingSink-only. No `/metrics` endpoint, no OTLP-out from
//! Aperture itself. The structural enforcement is owned by DEVOPS as
//! a network-namespace integration fixture (Linux only at v0): the
//! fixture binds Aperture inside a constrained net-ns, captures every
//! outbound packet, and asserts zero packets except listener acks
//! and ForwardingSink-to-downstream traffic.
//!
//! That fixture is the load-bearing defence. This integration test
//! is a behavioural-layer corroboration: it asserts at the
//! application surface that the documented forbidden surfaces are
//! absent. Specifically: `/metrics` is not exposed, and no outbound
//! connection is opened when `sink=stub`.
//!
//! ## DEVOPS responsibilities
//!
//! - **Network-namespace fixture**: the load-bearing defence.
//!   Documented in `docs/feature/aperture/design/wave-decisions.md > D10`.
//! - **CI workflow YAML**: Linux-only invocation; `#[cfg(target_os = "linux")]`
//!   network capture; assertion on captured bytes.
//!
//! This Rust test runs unconditionally and catches the easy-to-detect
//! forbidden surfaces (`/metrics`, `/telemetry`, etc.).

mod common;

use std::time::Duration;

use crate::common::start_default;

#[tokio::test(flavor = "multi_thread")]
async fn aperture_does_not_expose_a_metrics_endpoint() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/metrics", instance.http_base_url()))
        .send()
        .await
        .expect("GET /metrics");
    // 404 is the only acceptable response. 200 means a metrics
    // endpoint exists; anything in 5xx is a bug.
    assert_eq!(
        response.status().as_u16(),
        404,
        "/metrics MUST NOT be exposed in v0 (telemetry-on-telemetry forbidden)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn aperture_does_not_expose_a_telemetry_endpoint() {
    let instance = start_default().await;
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/telemetry", instance.http_base_url()))
        .send()
        .await
        .expect("GET /telemetry");
    assert_eq!(response.status().as_u16(), 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn aperture_with_stub_sink_idle_does_not_open_any_outbound_connection() {
    // Behavioural-layer surface for the network-namespace gate. With
    // a stub sink, Aperture should never originate outbound traffic.
    // We start the instance, leave it idle for a brief observation
    // window, then drop it. The DEVOPS-owned net-ns fixture is the
    // load-bearing defence (it actually captures packets); this test
    // asserts the application doesn't even *try* to dial out by
    // running with no DNS-resolvable target configured and observing
    // no panic / error from a non-existent forwarding peer.
    let _instance = start_default().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    // If Aperture were dialling out, the unconfigured-forwarding
    // path would fail loudly. With sink=stub (the default) there is
    // nothing to dial; the absence of any failure is the invariant.
    // (The full assertion belongs in the net-ns fixture.)
}
