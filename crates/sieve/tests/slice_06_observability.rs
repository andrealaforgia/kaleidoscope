//! Slice 06 — Sampling decision observability.
//!
//! Maps to US-SI-06. Three scenarios:
//!
//! - A kept error trace emits exactly one DEBUG event with
//!   `target = "sieve"` and a message containing
//!   "kept (error-bearing)".
//! - A dropped non-error trace at rate 0.0 emits exactly one DEBUG
//!   event with `target = "sieve"` and a message containing
//!   "dropped".
//! - The periodic INFO summary fires once per
//!   `__test_summary_tick_now` call with `target = "sieve"`,
//!   carrying `kept`, `dropped`, and `rate` fields.
//!
//! ## DISTILL state
//!
//! `SamplingSink::new`, `SamplingSink::accept`, and
//! `__test_summary_tick_now` panic with `unimplemented!()`. Every
//! test below panics on construction or first call. DELIVER slice
//! 06 lands the observability vocabulary and the test seam body.
//!
//! ## Process-global state
//!
//! The capture mechanism is process-global; tests within this
//! binary serialise via `#[serial_test::serial]` so the
//! `CAPTURED_EVENTS` buffer is clean for each test body. The
//! `SIEVE_NON_ERROR_TRACE_RATE` and `SIEVE_SUMMARY_TICK_MS` env
//! vars are read at construction; tests that drive `from_env` must
//! also serialise (the env var is process-global).

mod common;

use aperture::ports::SinkRecord;
use common::{
    accept, capture_sieve_events, envelope_with_one_trace, expect_sieve_event_with_message,
    fixture_error_trace, fixture_ok_trace, fixture_trace_id,
    spawn_sampling_sink_with_recording_inner,
};
use serial_test::serial;
use sieve::__test_summary_tick_now;

// =========================================================================
// US-SI-06 Scenario: A kept error trace emits a DEBUG event.
//
// Given: Sieve's pipeline with the tracing subscriber capturing
//        events for target="sieve"
// When:  the sampler returns Decision::Keep for an error-bearing
//        trace
// Then:  exactly one DEBUG event is emitted with target="sieve" and
//        a message containing "kept (error-bearing)"
// =========================================================================

#[tokio::test]
#[serial]
async fn a_kept_error_trace_emits_a_debug_kept_error_bearing_event() {
    let _serial = common::acquire_test_serial();
    let capture = capture_sieve_events();

    let fixture = spawn_sampling_sink_with_recording_inner(0.0);
    let trace_id = fixture_trace_id(601);
    let spans = fixture_error_trace(trace_id);
    let envelope = envelope_with_one_trace(spans);
    let _ = accept(&fixture.sink, SinkRecord::Traces(envelope)).await;

    let events = capture.events();
    let event = expect_sieve_event_with_message(&events, "kept (error-bearing)");
    assert_eq!(
        event.level, "DEBUG",
        "per-decision events must fire at DEBUG (not INFO or higher)"
    );
}

// =========================================================================
// US-SI-06 Scenario: A dropped non-error trace emits a DEBUG event.
//
// Given: Sieve's pipeline at rate 0.0
// When:  the sampler returns Decision::Drop for a non-error trace
// Then:  exactly one DEBUG event is emitted with target="sieve" and
//        a message containing "dropped"
// =========================================================================

#[tokio::test]
#[serial]
async fn a_dropped_non_error_trace_emits_a_debug_dropped_event() {
    let _serial = common::acquire_test_serial();
    let capture = capture_sieve_events();

    let fixture = spawn_sampling_sink_with_recording_inner(0.0);
    let trace_id = fixture_trace_id(602);
    let spans = fixture_ok_trace(trace_id);
    let envelope = envelope_with_one_trace(spans);
    let _ = accept(&fixture.sink, SinkRecord::Traces(envelope)).await;

    let events = capture.events();
    let event = expect_sieve_event_with_message(&events, "dropped");
    assert_eq!(
        event.level, "DEBUG",
        "per-decision events must fire at DEBUG (not INFO or higher)"
    );
}

// =========================================================================
// US-SI-06 Scenario: A periodic summary aggregates the decisions
// over the window.
//
// Given: Sieve's pipeline running with mixed traces
// When:  the summary tick fires (synchronously via the test seam)
// Then:  exactly one INFO event is emitted with target="sieve"
//        containing "kept" and "dropped" counts and the configured
//        rate
//
// Per ADR-0020 §6: the test seam fires the snapshot path
// synchronously without waiting for the timer.
// =========================================================================

#[tokio::test]
#[serial]
async fn periodic_summary_emits_info_event_with_counts_and_rate() {
    let _serial = common::acquire_test_serial();
    let capture = capture_sieve_events();

    let fixture = spawn_sampling_sink_with_recording_inner(0.5);

    // Submit a mixed batch: one error trace, one non-error trace.
    // Slice 03's deterministic hashing sends the non-error trace
    // either way at rate 0.5; for the summary assertion only the
    // shape of the resulting event matters, not the exact
    // partition.
    let error_trace_id = fixture_trace_id(701);
    let ok_trace_id = fixture_trace_id(702);
    let mut spans = fixture_error_trace(error_trace_id);
    spans.extend(fixture_ok_trace(ok_trace_id));
    let envelope = envelope_with_one_trace(spans);
    let _ = accept(&fixture.sink, SinkRecord::Traces(envelope)).await;

    // Fire the summary synchronously.
    __test_summary_tick_now(&fixture.sink);

    let events = capture.events();
    let summary = expect_sieve_event_with_message(&events, "summary window");
    assert_eq!(
        summary.level, "INFO",
        "summary event must fire at INFO (operator default visibility)"
    );
    assert!(
        summary.fields.contains_key("kept"),
        "summary must carry a `kept` field"
    );
    assert!(
        summary.fields.contains_key("dropped"),
        "summary must carry a `dropped` field"
    );
    assert!(
        summary.fields.contains_key("rate"),
        "summary must carry a `rate` field for operator confirmation"
    );
    let rate = summary
        .field_f64("rate")
        .expect("rate field must be a float");
    assert!(
        (rate - 0.5).abs() < f64::EPSILON,
        "summary rate must equal the configured rate; got {rate}"
    );
}

// =========================================================================
// `from_env` happy path: unset env var → default rate 0.1.
//
// US-SI-06 brief: "an unset var produces the default 0.1".
// =========================================================================

#[test]
#[serial]
fn head_sampler_from_env_defaults_to_zero_point_one_when_var_is_unset() {
    let _serial = common::acquire_test_serial();
    // Force the env var unset for this test scope.
    std::env::remove_var(sieve::sampler_env_for_tests());
    let sampler =
        sieve::HeadSampler::from_env().expect("default rate must construct a sampler successfully");
    assert!(
        (sampler.rate() - 0.1).abs() < f64::EPSILON,
        "default rate must be 0.1; got {}",
        sampler.rate()
    );
}

// =========================================================================
// `from_env` error path: non-numeric value → RateUnparseable.
// =========================================================================

#[test]
#[serial]
fn head_sampler_from_env_rejects_non_numeric_value() {
    let _serial = common::acquire_test_serial();
    std::env::set_var(sieve::sampler_env_for_tests(), "not-a-number");
    let result = sieve::HeadSampler::from_env();
    std::env::remove_var(sieve::sampler_env_for_tests());

    let err = result.expect_err("non-numeric env value must be rejected");
    assert!(
        matches!(err, sieve::SieveConfigError::RateUnparseable { .. }),
        "expected RateUnparseable, got {err:?}"
    );
}

// =========================================================================
// `from_env` error path: out-of-range value → RateOutOfRange.
// =========================================================================

#[test]
#[serial]
fn head_sampler_from_env_rejects_out_of_range_value() {
    let _serial = common::acquire_test_serial();
    std::env::set_var(sieve::sampler_env_for_tests(), "1.5");
    let result = sieve::HeadSampler::from_env();
    std::env::remove_var(sieve::sampler_env_for_tests());

    let err = result.expect_err("out-of-range env value must be rejected");
    assert!(
        matches!(err, sieve::SieveConfigError::RateOutOfRange { .. }),
        "expected RateOutOfRange, got {err:?}"
    );
}
