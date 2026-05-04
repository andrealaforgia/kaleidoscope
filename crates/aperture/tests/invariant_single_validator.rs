//! Invariant — single validator per signal.
//!
//! The structural enforcement of DISCUSS D3 ("every accepted byte
//! sequence flows through exactly one `otlp_conformance_harness::validate_*`
//! call") is owned by DEVOPS as an `xtask` AST walk: the static check
//! reads `crates/aperture/src/**` and counts call sites of
//! `validate_logs/traces/metrics`, asserting at most one per signal.
//! That is the load-bearing defence; it catches a second call site
//! the moment it is added to the source tree.
//!
//! This integration test is a behavioural-layer corroboration of the
//! same invariant: the binary, at runtime, produces exactly one
//! request_received-then-sink_accepted pair per accepted request. If
//! a future maintainer added a parallel validation gate (a wrapper, a
//! second harness call), the duplicate hand-off would be visible
//! either as duplicate `sink_accepted` events or as a record passed
//! to the sink twice. This test asserts the runtime invariant
//! complements the AST check.
//!
//! ## DEVOPS responsibilities
//!
//! - **AST-walk check** (`xtask single-validator-per-signal`): the
//!   primary defence. Documented in
//!   `docs/feature/aperture/design/wave-decisions.md > D10`.
//! - **Workflow YAML**: invocation in CI pre-merge.
//!
//! This Rust test sits alongside as the runtime corroboration; it
//! runs every `cargo test --all-targets` so a build that breaks the
//! invariant fails locally before reaching CI.

mod common;

use std::time::Duration;

use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use prost::Message;
use tonic::transport::Channel;

use crate::common::{encode_logs_request, start_default, wait_for};

#[tokio::test(flavor = "multi_thread")]
async fn one_export_produces_exactly_one_record_in_the_sink() {
    let instance = start_default().await;
    let channel = Channel::from_shared(instance.grpc_endpoint())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = LogsServiceClient::new(channel);
    let req = ExportLogsServiceRequest::decode(&encode_logs_request("payments-api", 1)[..])
        .expect("encoder produced valid bytes");
    let _ = client.export(req).await;
    wait_for(|| !instance.sink.is_empty(), Duration::from_secs(2)).await;
    // Allow time for any phantom duplicate hand-off to also land.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let recorded = instance.sink.drain();
    assert_eq!(
        recorded.len(),
        1,
        "exactly one record per accepted request — duplicate validators \
         would surface as duplicate sink hand-offs"
    );
}
