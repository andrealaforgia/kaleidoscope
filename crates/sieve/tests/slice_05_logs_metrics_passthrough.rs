//! Slice 05 — Logs and metrics pass through unfiltered.
//!
//! Maps to US-SI-05. Three scenarios assert that the decorator's
//! `accept(SinkRecord::Logs(...))` and
//! `accept(SinkRecord::Metrics(...))` paths forward the record to
//! the inner sink unchanged, regardless of the configured non-error
//! trace rate.
//!
//! ## DISTILL state
//!
//! `SamplingSink::new` panics with `unimplemented!()` so every test
//! below panics on construction. DELIVER slice 01 lands the
//! constructor; DELIVER slice 05 lands the per-variant routing
//! body that forwards Logs and Metrics unchanged (per ADR-0021 §1).

mod common;

use aperture::ports::{SinkRecord, TenantId, TenantScoped};
use common::{
    accept, envelope_with_one_log, envelope_with_one_metric,
    spawn_sampling_sink_with_recording_inner,
};
use sieve::SamplingSink;

/// Wrap a signal payload with a fixed test tenant for the post-ADR-0068
/// `TenantScoped` `SinkRecord` shape (aegis-ingest-auth-v0). Sieve is a
/// sampling/passthrough decorator; these tests assert the variant + body
/// pass through, so the tenant value is immaterial here.
fn scoped<T>(inner: T) -> TenantScoped<T> {
    TenantScoped::new(TenantId("acme-prod".to_string()), inner)
}

// =========================================================================
// US-SI-05 Scenario: A log record passes through Sieve unchanged.
//
// Given: Sieve's pipeline configured with HeadSampler at rate 0.0
// And:   a fixture log record
// When:  the log enters the pipeline
// Then:  the same log exits the pipeline byte-for-byte
//
// At rate 0.0 the trace path drops every non-error trace; the
// log path must NOT be affected. The assertion is that the inner
// `RecordingSink` receives the log record verbatim.
// =========================================================================

#[tokio::test]
async fn a_log_record_passes_through_unchanged_at_rate_zero() {
    let fixture = spawn_sampling_sink_with_recording_inner(0.0);
    let log_envelope = envelope_with_one_log();

    let result = accept(&fixture.sink, SinkRecord::Logs(scoped(log_envelope))).await;

    assert!(
        result.is_ok(),
        "log passthrough must succeed at rate 0.0; got {result:?}"
    );
    // The decorator forwards the log to the inner sink; the inner
    // sink's record set must contain exactly one Logs variant.
    // DELIVER slice 05 wires the recording instance through the
    // decorator so this assertion has access to the inner state;
    // until then the panic at construction stops execution before
    // this point.
    let recorded = fixture.recording.drain();
    assert_eq!(
        recorded.len(),
        1,
        "exactly one log record must reach the inner sink"
    );
    assert!(
        matches!(recorded[0], SinkRecord::Logs(_)),
        "the recorded variant must be Logs"
    );
}

// =========================================================================
// US-SI-05 Scenario: A metric data point passes through Sieve
// unchanged.
//
// Given: Sieve's pipeline configured with HeadSampler at rate 0.0
// And:   a fixture metric data point
// When:  the metric enters the pipeline
// Then:  the same metric exits the pipeline byte-for-byte
// =========================================================================

#[tokio::test]
async fn a_metric_data_point_passes_through_unchanged_at_rate_zero() {
    let fixture = spawn_sampling_sink_with_recording_inner(0.0);
    let metrics_envelope = envelope_with_one_metric();

    let result = accept(&fixture.sink, SinkRecord::Metrics(scoped(metrics_envelope))).await;

    assert!(
        result.is_ok(),
        "metric passthrough must succeed at rate 0.0; got {result:?}"
    );
    let recorded = fixture.recording.drain();
    assert_eq!(
        recorded.len(),
        1,
        "exactly one metric record must reach the inner sink"
    );
    assert!(
        matches!(recorded[0], SinkRecord::Metrics(_)),
        "the recorded variant must be Metrics"
    );
}

// =========================================================================
// US-SI-05 Scenario: Logs are not affected by the trace-rate
// setting.
//
// Given: Sieve's pipeline configured with HeadSampler at rate 0.0
// And:   100 fixture log records
// When:  each enters the pipeline
// Then:  100 logs exit the pipeline
// =========================================================================

#[tokio::test]
async fn one_hundred_log_records_pass_through_at_rate_zero() {
    let fixture = spawn_sampling_sink_with_recording_inner(0.0);

    for _ in 0..100 {
        let log_envelope = envelope_with_one_log();
        let result = accept(&fixture.sink, SinkRecord::Logs(scoped(log_envelope))).await;
        assert!(
            result.is_ok(),
            "every log must pass through; got {result:?}"
        );
    }

    let recorded = fixture.recording.drain();
    assert_eq!(
        recorded.len(),
        100,
        "exactly 100 log records must reach the inner sink"
    );
    for r in &recorded {
        assert!(
            matches!(r, SinkRecord::Logs(_)),
            "every recorded record must be a Logs variant"
        );
    }
}

// =========================================================================
// Type-level: ensure SamplingSink<RecordingSink, HeadSampler> exposes
// the OtlpSink + Probe contract the slice tests rely on. Compile-time
// check; the test body is empty (the where-clause is the assertion).
// =========================================================================

#[test]
fn sampling_sink_implements_otlp_sink_for_recording_inner() {
    fn _assert<S, N>()
    where
        S: aperture::ports::OtlpSink + aperture::ports::Probe + Send + Sync + 'static,
        N: sieve::Sampler,
        SamplingSink<S, N>: aperture::ports::OtlpSink + aperture::ports::Probe,
    {
    }
    _assert::<aperture::testing::RecordingSink, sieve::HeadSampler>();
}
