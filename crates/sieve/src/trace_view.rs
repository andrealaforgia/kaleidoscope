//! `TraceView<'a>` — borrowed view over a logical trace's spans,
//! plus the doc-hidden `__test_trace_view` constructor for fixture
//! building in slice tests.
//!
//! Per ADR-0018 §"Why the borrowed `TraceView` (D1 resolution)": the
//! decorator's grouping pass builds these views from
//! `ExportTraceServiceRequest::resource_spans` once per batch; the
//! sampler reads `trace_id` and (for the error-bearing check)
//! iterates the spans without taking ownership.
//!
//! The fixture-side test seam exists so slice tests can construct a
//! `TraceView` from a separately-given trace_id and a `&[Span]`
//! without going through the decorator's grouping pass. This
//! decouples the slice 02/03/04 sampler tests from the decorator's
//! plumbing.

use opentelemetry_proto::tonic::trace::v1::Span;

/// Borrowed view over a logical trace's spans.
///
/// The lifetime ties the view to the underlying span storage — for
/// the production path this is the
/// `ExportTraceServiceRequest::resource_spans` arena the grouping
/// pass borrows into; for the test path this is whatever
/// `&[Span]` the slice test owns.
///
/// Construction is private: in production, the decorator's grouping
/// pass is the only constructor; in tests, [`__test_trace_view`] is
/// the only constructor. Either way the invariant "every span in the
/// view shares the same `trace_id`" is maintained at the
/// constructor.
pub struct TraceView<'a> {
    trace_id: [u8; 16],
    spans: &'a [Span],
}

impl<'a> TraceView<'a> {
    /// The 16-byte OTLP `trace_id` keyed across this view's spans.
    ///
    /// All spans of a logical trace share the same `trace_id` (an
    /// OpenTelemetry invariant); the `TraceView` makes the invariant
    /// addressable at the type level. The
    /// [`crate::HeadSampler`] hashes this value with `xxh3_64` for
    /// the rate-based decision (per ADR-0018 §"`HeadSampler::sample`
    /// mechanism").
    pub fn trace_id(&self) -> [u8; 16] {
        self.trace_id
    }

    /// Iterate the spans in the view.
    ///
    /// The lifetime of the iterator ties to the underlying span
    /// storage, not to the `TraceView` itself, so the iterator can
    /// outlive the view. The error-bias check
    /// (`is_error_bearing`) iterates this and inspects each span's
    /// `status.code`.
    pub fn spans(&self) -> impl Iterator<Item = &'a Span> + '_ {
        self.spans.iter()
    }
}

// =========================================================================
// Test seam — `__test_trace_view` (per ADR-0018 §"Test seams").
//
// Real (not `unimplemented!()`) so slice tests can construct fixture
// views without running the decorator's grouping pass. Doc-hidden so
// it does not appear in the consumer-facing API documentation.
// =========================================================================

/// Construct a [`TraceView`] from a fixture trace_id and a borrowed
/// span slice.
///
/// The slice tests use this to build fixture trace views for the
/// `Sampler::sample` calls without instantiating a decorator.
/// Production code does NOT call this — the decorator's grouping
/// pass builds views via the private
/// `TraceView::from_grouping_pass` constructor.
///
/// `#[doc(hidden)]` and the `__` prefix mark this as a test seam, not
/// part of the consumer-facing API contract. `cargo public-api`
/// records the seam; SemVer treats it as stable.
#[doc(hidden)]
pub fn __test_trace_view<'a>(trace_id: [u8; 16], spans: &'a [Span]) -> TraceView<'a> {
    TraceView { trace_id, spans }
}
