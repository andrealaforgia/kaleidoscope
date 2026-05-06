//! Invariant — single `spark::init` per process.
//!
//! Maps to US-SP-02's `GlobalAlreadyInitialised` UAT and
//! `shared-artifacts-registry.md > otel_global_provider`. Per
//! ADR-0015 §2 + §3: this test is its own `[[test]]`-declared binary
//! with **a single `#[test]` function**, so the binary's process runs
//! exactly two `init` calls — the first succeeds, the second returns
//! the `GlobalAlreadyInitialised` variant. No other test in this
//! binary touches the OTel global state, so the assertion is
//! deterministic.
//!
//! The other three error-path variants (`MissingRequiredAttribute`,
//! `InvalidEndpoint`, `ExporterInitFailed`) live in
//! `slice_02_init_error_paths.rs` because they do not depend on
//! global state — multiple `#[test]` functions can share that binary
//! without sequencing concerns.
//!
//! ## RED-on-day-one
//!
//! `spark::init` panics with `unimplemented!()` at DISTILL. This test
//! panics at the first `init` call. DELIVER replaces the panic with
//! the AtomicBool CAS + provider-set logic when Slice 02 lands.

mod common;

use spark::{init, SparkConfig, SparkError};

use crate::common::{
    spawn_aperture_with_recording_sink, CANONICAL_SERVICE_NAME, CANONICAL_TENANT_ID,
};

/// US-SP-02 UAT: "spark::init refuses a second call in the same
/// process." Per ADR-0015 §1: the Spark-internal AtomicBool catches
/// the Spark-called-twice case; both calls produce the same Err
/// variant.
#[tokio::test(flavor = "multi_thread")]
async fn developer_calls_init_twice_in_same_process_and_second_call_returns_global_already_initialised(
) {
    let aperture = spawn_aperture_with_recording_sink().await;

    let _first = init(
        SparkConfig::for_service(CANONICAL_SERVICE_NAME)
            .require_tenant_id()
            .with_tenant_id(CANONICAL_TENANT_ID)
            .with_endpoint(aperture.grpc_endpoint()),
    )
    .expect("first init must succeed");

    let second = init(
        SparkConfig::for_service("another-service-name")
            .require_tenant_id()
            .with_tenant_id(CANONICAL_TENANT_ID)
            .with_endpoint(aperture.grpc_endpoint()),
    );

    assert!(
        matches!(second, Err(SparkError::GlobalAlreadyInitialised)),
        "second init in the same process must return GlobalAlreadyInitialised; got: {:?}",
        second.err()
    );
}
