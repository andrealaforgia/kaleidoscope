//! Observability vocabulary — DEBUG per-decision events and the INFO
//! periodic summary, all with `target = "sieve"`.
//!
//! Per ADR-0020 §5 + DISCUSS Q8: the rendered messages and the
//! structured field set are the operator-facing contract. Once
//! operators write dashboards against this vocabulary, changing
//! field names is a breaking change.
//!
//! ## DISTILL state
//!
//! Function bodies panic with `unimplemented!()` until DELIVER. The
//! signatures are locked here so the decorator's `accept_traces`
//! body can compile against them at every slice.

#![allow(dead_code)]

/// The tracing target every Sieve event uses. Locked at DISCUSS Q8
/// + ADR-0020 §5.
pub(crate) const SIEVE_TRACING_TARGET: &str = "sieve";

/// Emit a DEBUG event for a kept error-bearing trace.
///
/// Field set: `decision="kept"`, `reason="error_bearing"`,
/// `trace_id=<hex>`. Message contains "kept (error-bearing)".
pub(crate) fn emit_debug_kept_error_bearing(_trace_id: [u8; 16]) {
    unimplemented!("DEBUG kept (error-bearing) event lands at DELIVER slice 06");
}

/// Emit a DEBUG event for a kept rate-sampled trace.
///
/// Field set: `decision="kept"`, `reason="rate_kept"`,
/// `trace_id=<hex>`, `rate=<f64>`. Message contains "kept (sampled,
/// rate=…)".
pub(crate) fn emit_debug_kept_sampled(_trace_id: [u8; 16], _rate: f64) {
    unimplemented!("DEBUG kept (sampled) event lands at DELIVER slice 06");
}

/// Emit a DEBUG event for a dropped trace.
///
/// Field set: `decision="dropped"`, `reason="rate_dropped"`,
/// `trace_id=<hex>`, `rate=<f64>`. Message contains "dropped".
pub(crate) fn emit_debug_dropped(_trace_id: [u8; 16], _rate: f64) {
    unimplemented!("DEBUG dropped event lands at DELIVER slice 06");
}

/// Emit the periodic INFO summary.
///
/// Field set per ADR-0020 §5: `kept`, `kept_error_bearing`,
/// `kept_sampled`, `dropped`, `rate`. Message follows the wording
/// locked at slice-06's brief.
///
/// Called by the timer task on every tick AND by the
/// [`crate::__test_summary_tick_now`] test seam.
pub(crate) fn emit_summary(_kept_total: u64, _kept_error_bearing: u64, _dropped: u64, _rate: f64) {
    unimplemented!("INFO summary event lands at DELIVER slice 06");
}
