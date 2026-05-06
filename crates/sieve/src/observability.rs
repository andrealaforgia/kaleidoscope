//! Observability vocabulary — DEBUG per-decision events and the INFO
//! periodic summary, all with `target = "sieve"`.
//!
//! Per ADR-0020 §5 + DISCUSS Q8: the rendered messages and the
//! structured field set are the operator-facing contract. Once
//! operators write dashboards against this vocabulary, changing
//! field names is a breaking change.
//!
//! ## DELIVER state — slice 06
//!
//! Every helper is a real `tracing::debug!` / `tracing::info!`
//! emission. The helpers are kept `pub(crate)` so the decorator
//! (DEBUG events) and the aggregator's snapshot path (INFO summary)
//! call them through one named function each — the names ARE the
//! contract that mutation testing exercises.

#![allow(dead_code)]

/// The tracing target every Sieve event uses. Locked at DISCUSS Q8
/// + ADR-0020 §5.
pub(crate) const SIEVE_TRACING_TARGET: &str = "sieve";

/// Emit a DEBUG event for a kept error-bearing trace.
///
/// Field set: `trace_id=<hex>`. Message: "kept (error-bearing)".
pub(crate) fn emit_debug_kept_error_bearing(trace_id: [u8; 16]) {
    tracing::debug!(
        target: SIEVE_TRACING_TARGET,
        trace_id = format_trace_id(trace_id),
        "kept (error-bearing)"
    );
}

/// Emit a DEBUG event for a kept rate-sampled trace.
///
/// Field set: `trace_id=<hex>`, `rate=<f64>`. Message: "kept
/// (sampled, rate={rate:.2})".
pub(crate) fn emit_debug_kept_sampled(trace_id: [u8; 16], rate: f64) {
    tracing::debug!(
        target: SIEVE_TRACING_TARGET,
        trace_id = format_trace_id(trace_id),
        rate = rate,
        "kept (sampled, rate={rate:.2})"
    );
}

/// Emit a DEBUG event for a dropped trace.
///
/// Field set: `trace_id=<hex>`, `rate=<f64>`. Message: "dropped
/// (rate={rate:.2})".
pub(crate) fn emit_debug_dropped(trace_id: [u8; 16], rate: f64) {
    tracing::debug!(
        target: SIEVE_TRACING_TARGET,
        trace_id = format_trace_id(trace_id),
        rate = rate,
        "dropped (rate={rate:.2})"
    );
}

/// Emit the periodic INFO summary.
///
/// Field set per ADR-0020 §5: `kept`, `kept_error_bearing`,
/// `kept_sampled`, `dropped`, `rate`. Message follows the wording
/// locked at slice-06's brief: "kept N traces (E error-bearing, S
/// sampled at R rate), dropped M traces over the last summary
/// window".
///
/// Called by the timer task on every tick AND by the
/// [`crate::__test_summary_tick_now`] test seam.
pub(crate) fn emit_summary(kept_total: u64, kept_error_bearing: u64, dropped: u64, rate: f64) {
    let kept_sampled = kept_total.saturating_sub(kept_error_bearing);
    tracing::info!(
        target: SIEVE_TRACING_TARGET,
        kept = kept_total,
        kept_error_bearing = kept_error_bearing,
        kept_sampled = kept_sampled,
        dropped = dropped,
        rate = rate,
        "sieve: kept {kept_total} traces ({kept_error_bearing} error-bearing, \
         {kept_sampled} sampled at {rate:.2} rate), dropped {dropped} traces \
         over the last summary window"
    );
}

/// Render a 16-byte trace_id as a lowercase hex string.
///
/// Operator-facing rendering: dashboards and log lines surface the
/// trace_id verbatim so an operator can correlate against the
/// upstream OTel collector / backend.
fn format_trace_id(trace_id: [u8; 16]) -> String {
    let mut out = String::with_capacity(32);
    for byte in trace_id {
        let hi = HEX_DIGITS[(byte >> 4) as usize];
        let lo = HEX_DIGITS[(byte & 0x0F) as usize];
        out.push(hi as char);
        out.push(lo as char);
    }
    out
}

/// Lowercase hex digit lookup. Pulled out as a constant so the
/// formatter has no arithmetic mutation surface beyond the literal
/// table.
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

#[cfg(test)]
mod tests {
    //! Unit tests for the trace_id formatter and the four event
    //! emitters.
    //!
    //! Port-to-port at domain scope per Mandate 2: `format_trace_id`
    //! and the four `emit_*` helpers are pure free functions whose
    //! signatures ARE their driving ports. Calling them directly from
    //! a test IS port-to-port testing.
    //!
    //! Tracing events are captured via
    //! `tracing::subscriber::with_default`, which installs a
    //! thread-local subscriber for the duration of the closure. This
    //! is the canonical Tokio-pattern for scoped subscribers (the
    //! integration tests use a global subscriber, gated by
    //! `serial_test`; the unit tests stay thread-local so they can
    //! run in parallel without colliding).
    //!
    //! Test budget:
    //! - `format_trace_id`: 1 distinct behaviour × 2 = 2 unit tests
    //!   max. The behaviour is "render every byte as two lowercase
    //!   hex digits, in big-endian order".
    //! - `emit_*`: 4 distinct behaviours (one per emitter), each
    //!   exercised by one test that captures the event and asserts on
    //!   the level + message + field set.

    use super::*;
    use std::sync::{Arc, Mutex};

    /// In-memory subscriber that captures every event whose `target`
    /// matches `SIEVE_TRACING_TARGET`. Drives the unit tests below.
    struct CapturingSubscriber {
        events: Arc<Mutex<Vec<CapturedEvent>>>,
    }

    #[derive(Debug, Clone)]
    struct CapturedEvent {
        level: tracing::Level,
        message: String,
        rate: Option<f64>,
        trace_id: Option<String>,
        kept: Option<u64>,
        kept_error_bearing: Option<u64>,
        kept_sampled: Option<u64>,
        dropped: Option<u64>,
    }

    impl CapturingSubscriber {
        fn new() -> (Self, Arc<Mutex<Vec<CapturedEvent>>>) {
            let events = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    events: Arc::clone(&events),
                },
                events,
            )
        }
    }

    impl tracing::Subscriber for CapturingSubscriber {
        fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }
        fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}
        fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}
        fn event(&self, event: &tracing::Event<'_>) {
            if event.metadata().target() != SIEVE_TRACING_TARGET {
                return;
            }
            let mut visitor = FieldVisitor::default();
            event.record(&mut visitor);
            self.events.lock().unwrap().push(CapturedEvent {
                level: *event.metadata().level(),
                message: visitor.message.unwrap_or_default(),
                rate: visitor.rate,
                trace_id: visitor.trace_id,
                kept: visitor.kept,
                kept_error_bearing: visitor.kept_error_bearing,
                kept_sampled: visitor.kept_sampled,
                dropped: visitor.dropped,
            });
        }
        fn enter(&self, _span: &tracing::span::Id) {}
        fn exit(&self, _span: &tracing::span::Id) {}
    }

    #[derive(Default)]
    struct FieldVisitor {
        message: Option<String>,
        rate: Option<f64>,
        trace_id: Option<String>,
        kept: Option<u64>,
        kept_error_bearing: Option<u64>,
        kept_sampled: Option<u64>,
        dropped: Option<u64>,
    }

    impl tracing::field::Visit for FieldVisitor {
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            match field.name() {
                "message" => self.message = Some(value.to_owned()),
                "trace_id" => self.trace_id = Some(value.to_owned()),
                _ => {}
            }
        }
        fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
            match field.name() {
                "kept" => self.kept = Some(value),
                "kept_error_bearing" => self.kept_error_bearing = Some(value),
                "kept_sampled" => self.kept_sampled = Some(value),
                "dropped" => self.dropped = Some(value),
                _ => {}
            }
        }
        fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
            if field.name() == "rate" {
                self.rate = Some(value);
            }
        }
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                self.message = Some(format!("{value:?}"));
            }
        }
    }

    /// Run `body` with a scoped capturing subscriber. Returns every
    /// `target = "sieve"` event the closure emitted. Uses
    /// `tracing::subscriber::with_default` so the subscriber is
    /// thread-local (no cross-test serialisation needed).
    fn capture_events_during<F: FnOnce()>(body: F) -> Vec<CapturedEvent> {
        let (subscriber, events) = CapturingSubscriber::new();
        tracing::subscriber::with_default(subscriber, body);
        let captured = events.lock().unwrap().clone();
        captured
    }

    #[test]
    fn format_trace_id_renders_each_byte_as_two_lowercase_hex_digits() {
        // The single behaviour is exercised across the boundary bytes
        // (0x00, 0x0F, 0xF0, 0xFF) and a typical mixed pattern. Any
        // mutation that perturbs the digit table, the shift, or the
        // mask is caught by at least one of these cases.
        let cases: &[(&str, [u8; 16], &str)] = &[
            (
                "all zeros render as thirty-two zero digits",
                [0u8; 16],
                "00000000000000000000000000000000",
            ),
            (
                "all 0xFF render as thirty-two 'f' digits",
                [0xFFu8; 16],
                "ffffffffffffffffffffffffffffffff",
            ),
            (
                "ascending bytes render in big-endian order",
                [
                    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, //
                    0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
                ],
                "000102030405060708090a0b0c0d0e0f",
            ),
            (
                "high-nibble boundary 0xF0 renders as 'f0'",
                [
                    0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                ],
                "f0000000000000000000000000000000",
            ),
            (
                "low-nibble boundary 0x0F renders as '0f'",
                [
                    0x0F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                ],
                "0f000000000000000000000000000000",
            ),
        ];

        for (label, input, expected) in cases {
            assert_eq!(
                format_trace_id(*input),
                *expected,
                "case {label}: expected {expected}"
            );
        }
    }

    // =====================================================================
    // Event-emitter tests — pin the four emit_* helpers' wire output.
    //
    // Each test captures events through a thread-local subscriber and
    // asserts:
    // - exactly one event was captured (no spurious extra events)
    // - the level matches the contract (DEBUG for per-decision, INFO
    //   for summary)
    // - the message contains the contract-locked substring
    // - the structured field set is correct
    //
    // A mutation that replaces an emitter body with `()` (cargo-mutants'
    // canonical "remove this function" mutation) is caught by the
    // "exactly one event" assertion.
    // =====================================================================

    #[test]
    fn emit_debug_kept_error_bearing_emits_one_debug_event_with_kept_error_bearing_message() {
        let trace_id = [0x42u8; 16];
        let events = capture_events_during(|| {
            emit_debug_kept_error_bearing(trace_id);
        });
        assert_eq!(
            events.len(),
            1,
            "exactly one DEBUG event must be emitted; got {events:?}"
        );
        assert_eq!(events[0].level, tracing::Level::DEBUG);
        assert!(
            events[0].message.contains("kept (error-bearing)"),
            "message must contain \"kept (error-bearing)\"; got {:?}",
            events[0].message
        );
        assert_eq!(
            events[0].trace_id.as_deref(),
            Some("42424242424242424242424242424242"),
            "trace_id field must be the hex-rendered fixture"
        );
    }

    #[test]
    fn emit_debug_kept_sampled_emits_one_debug_event_with_kept_sampled_message_and_rate() {
        let trace_id = [0x77u8; 16];
        let events = capture_events_during(|| {
            emit_debug_kept_sampled(trace_id, 0.25);
        });
        assert_eq!(
            events.len(),
            1,
            "exactly one DEBUG event must be emitted; got {events:?}"
        );
        assert_eq!(events[0].level, tracing::Level::DEBUG);
        assert!(
            events[0].message.contains("kept (sampled"),
            "message must contain \"kept (sampled\"; got {:?}",
            events[0].message
        );
        assert_eq!(
            events[0].rate,
            Some(0.25),
            "rate field must equal the configured value"
        );
        assert_eq!(
            events[0].trace_id.as_deref(),
            Some("77777777777777777777777777777777"),
        );
    }

    #[test]
    fn emit_debug_dropped_emits_one_debug_event_with_dropped_message_and_rate() {
        let trace_id = [0xABu8; 16];
        let events = capture_events_during(|| {
            emit_debug_dropped(trace_id, 0.0);
        });
        assert_eq!(
            events.len(),
            1,
            "exactly one DEBUG event must be emitted; got {events:?}"
        );
        assert_eq!(events[0].level, tracing::Level::DEBUG);
        assert!(
            events[0].message.contains("dropped"),
            "message must contain \"dropped\"; got {:?}",
            events[0].message
        );
        assert_eq!(events[0].rate, Some(0.0));
        assert_eq!(
            events[0].trace_id.as_deref(),
            Some("abababababababababababababababab"),
        );
    }

    #[test]
    fn emit_summary_emits_one_info_event_with_summary_window_message_and_full_field_set() {
        let events = capture_events_during(|| {
            // 5 kept (3 error-bearing, so 2 sampled), 4 dropped, rate
            // 0.10. The sampled count is derived as 5 - 3 = 2.
            emit_summary(5, 3, 4, 0.10);
        });
        assert_eq!(
            events.len(),
            1,
            "exactly one INFO event must be emitted; got {events:?}"
        );
        assert_eq!(events[0].level, tracing::Level::INFO);
        assert!(
            events[0].message.contains("summary window"),
            "message must contain \"summary window\"; got {:?}",
            events[0].message
        );
        assert_eq!(events[0].kept, Some(5), "kept field must be kept_total");
        assert_eq!(
            events[0].kept_error_bearing,
            Some(3),
            "kept_error_bearing field must be the recorded count"
        );
        assert_eq!(
            events[0].kept_sampled,
            Some(2),
            "kept_sampled = kept_total - kept_error_bearing = 5 - 3 = 2"
        );
        assert_eq!(events[0].dropped, Some(4));
        assert_eq!(events[0].rate, Some(0.10));
    }

    #[test]
    fn emit_summary_saturates_kept_sampled_to_zero_when_error_bearing_exceeds_kept_total() {
        // The cross-counter race in `Counters::snapshot_and_reset` can
        // produce `kept_error_bearing > kept_total` if a record lands
        // between the three swaps. The render must saturate (not
        // underflow) — we use `saturating_sub`. Pin that behaviour.
        let events = capture_events_during(|| {
            emit_summary(2, 5, 0, 0.5);
        });
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].kept_sampled,
            Some(0),
            "saturating_sub(2 - 5) = 0; the renderer must not underflow"
        );
    }
}
