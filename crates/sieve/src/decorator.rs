//! `SamplingSink<S, N>` — the `OtlpSink + Probe` decorator that adds
//! head-based sampling on the `Traces` variant and forwards `Logs` /
//! `Metrics` unchanged.
//!
//! Per ADR-0021: generic over the inner sink type `S` and the
//! sampler type `N`; consumes Aperture's existing `OtlpSink +
//! Probe` traits; no Aperture-side trait amendment.
//!
//! ## Slice 05 state
//!
//! - [`SamplingSink::new`] stores the inner sink and the sampler in
//!   `Arc`s and constructs a fresh [`Counters`] aggregator. The
//!   timer task that consumes the counters is slice 06's territory;
//!   slice 05 leaves the counters as zero-initialised state and does
//!   not spawn the task.
//! - The `OtlpSink::accept` impl routes per variant: `Logs` and
//!   `Metrics` are forwarded to the inner sink unchanged (per Q6 +
//!   ADR-0021 §1); `Traces` are grouped by trace_id, the sampler is
//!   asked per trace, and a kept-traces-only
//!   [`ExportTraceServiceRequest`] is forwarded to the inner sink.
//! - The `Probe::probe` impl delegates to the inner sink (per
//!   ADR-0021 §6).
//! - The per-decision DEBUG events and the counter increments inside
//!   `accept_traces` are slice 06's territory; this slice closes the
//!   routing and the kept-trace forwarding only.
//! - The [`__test_summary_tick_now`] test seam is still
//!   `unimplemented!()` (slice 06 lands its body).

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use aperture::ports::{OtlpSink, Probe, ProbeError, SinkError, SinkRecord};
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span};

use crate::aggregator::Counters;
use crate::decision::Decision;
use crate::sampler::Sampler;
use crate::trace_view::TraceView;

/// `OtlpSink + Probe` decorator adding head-based sampling on the
/// `Traces` variant; forwards `Logs` / `Metrics` unchanged per
/// DISCUSS Q6.
///
/// Generic over the inner sink type `S` (so the test path uses
/// concrete types and the production path uses `Arc<dyn OtlpSink>`
/// via Aperture's existing pattern) and the sampler type `N` (so
/// `HeadSampler` is the v0 concrete and a future tail sampler can
/// slot in without reshape).
///
/// Constructed via [`SamplingSink::new`]. Slice 06 will spawn the
/// periodic summary task at construction on the ambient Tokio runtime
/// (per ADR-0020 §2); slice 05 leaves the counters present and zero.
pub struct SamplingSink<S, N>
where
    S: OtlpSink + Probe,
    N: Sampler,
{
    /// The inner sink the decorator wraps. Held in an `Arc` so the
    /// timer task (slice 06) can hold a clone without bounding the
    /// decorator's lifetime to the runtime's tick cadence.
    inner: Arc<S>,

    /// The sampler the decorator consults for `Traces` records.
    sampler: Arc<N>,

    /// The aggregator's counters. Held in an `Arc` so the timer task
    /// (slice 06) can read them concurrently with the hot path.
    /// Slice 05 holds them as zero-initialised state; slice 06 wires
    /// the increments and the timer-driven snapshot.
    #[allow(dead_code)]
    counters: Arc<Counters>,
}

impl<S, N> SamplingSink<S, N>
where
    S: OtlpSink + Probe,
    N: Sampler,
{
    /// Wrap the inner sink with the given sampler.
    ///
    /// The constructor stores the inner sink, the sampler, and a
    /// fresh [`Counters`] aggregator behind `Arc` so the slice-06
    /// timer task can read state concurrently with the hot path.
    /// Slice 05 does not spawn the task; slice 06 lands the spawn,
    /// the join handle, and the cancellation token.
    pub fn new(inner: S, sampler: N) -> Self {
        Self {
            inner: Arc::new(inner),
            sampler: Arc::new(sampler),
            counters: Arc::new(Counters::new()),
        }
    }
}

// =========================================================================
// `OtlpSink` and `Probe` impls — the integration point with Aperture.
// =========================================================================

impl<S, N> OtlpSink for SamplingSink<S, N>
where
    S: OtlpSink + Probe,
    N: Sampler,
{
    fn accept<'a>(
        &'a self,
        record: SinkRecord,
    ) -> Pin<Box<dyn Future<Output = Result<(), SinkError>> + Send + 'a>> {
        Box::pin(async move {
            match record {
                // Per ADR-0021 §1: traces are grouped by trace_id,
                // the sampler is asked per trace, and a
                // kept-traces-only envelope is forwarded.
                SinkRecord::Traces(req) => self.accept_traces(req).await,
                // Per Q6 + ADR-0021 §1: logs pass through to the
                // inner sink unchanged. The decorator does not
                // unpack the envelope.
                SinkRecord::Logs(req) => self.inner.accept(SinkRecord::Logs(req)).await,
                // Per Q6 + ADR-0021 §1: metrics pass through to the
                // inner sink unchanged.
                SinkRecord::Metrics(req) => self.inner.accept(SinkRecord::Metrics(req)).await,
                // `SinkRecord` is `#[non_exhaustive]`. A future
                // Aperture-side variant will pass through this arm
                // verbatim — the right v0 posture is "Sieve only
                // mutates Traces; every other variant passes through
                // unchanged".
                other => self.inner.accept(other).await,
            }
        })
    }
}

impl<S, N> Probe for SamplingSink<S, N>
where
    S: OtlpSink + Probe,
    N: Sampler,
{
    fn probe<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<(), ProbeError>> + Send + 'a>> {
        // Per ADR-0021 §6: Sieve has no external dependency to probe;
        // the only external dependency is the inner sink. Delegation
        // is the contract.
        Box::pin(async move { self.inner.probe().await })
    }
}

impl<S, N> SamplingSink<S, N>
where
    S: OtlpSink + Probe,
    N: Sampler,
{
    /// Group spans of the incoming `ExportTraceServiceRequest` by
    /// `trace_id`, ask the sampler for a `Decision` per trace, and
    /// forward a kept-traces-only envelope to the inner sink.
    ///
    /// Per ADR-0021 §1: the grouping pass is one allocation per
    /// call (the `HashMap<[u8; 16], Vec<&Span>>`); the rebuild
    /// filters spans within each `ResourceSpans` / `ScopeSpans`. An
    /// entirely-dropped `ResourceSpans` is omitted.
    ///
    /// Slice 05 lands the routing and the kept-trace forwarding;
    /// slice 06 will add the per-decision DEBUG events and the
    /// counter increments at the keep / drop branches.
    async fn accept_traces(&self, request: ExportTraceServiceRequest) -> Result<(), SinkError> {
        let kept_trace_ids = self.decide_kept_trace_ids(&request);
        let filtered = filter_request_by_trace_ids(request, &kept_trace_ids);
        self.inner.accept(SinkRecord::Traces(filtered)).await
    }

    /// Compute the set of trace_ids that the sampler keeps for this
    /// request. Spans whose `trace_id` is not the canonical 16-byte
    /// length are skipped (defensive; the OTLP wire contract
    /// requires 16 bytes).
    fn decide_kept_trace_ids(&self, request: &ExportTraceServiceRequest) -> HashSet<[u8; 16]> {
        let groups = group_spans_by_trace_id(request);
        let mut kept: HashSet<[u8; 16]> = HashSet::with_capacity(groups.len());
        for (trace_id, spans) in &groups {
            let view = TraceView::from_grouping_pass(*trace_id, spans.as_slice());
            if matches!(self.sampler.sample(&view), Decision::Keep) {
                kept.insert(*trace_id);
            }
        }
        kept
    }
}

/// Walk an `ExportTraceServiceRequest`, copying each span into a
/// `Vec<Span>` keyed by its 16-byte `trace_id`.
///
/// Spans whose `trace_id` is not exactly 16 bytes are skipped
/// (defensive against malformed input; the OTLP wire contract
/// requires 16 bytes per `Span::trace_id`).
///
/// The sampler reads `trace_id` and iterates spans through
/// [`crate::TraceView`]; `TraceView` borrows a `&[Span]` so the
/// grouping pass owns a `Vec<Span>` per trace and the sampler reads
/// it through the borrowed view. Cloning each span here is the v0
/// shape; ADR-0021 §"Cons" notes a future optimisation can pool the
/// allocation.
fn group_spans_by_trace_id(request: &ExportTraceServiceRequest) -> HashMap<[u8; 16], Vec<Span>> {
    let mut groups: HashMap<[u8; 16], Vec<Span>> = HashMap::new();
    for resource_spans in &request.resource_spans {
        for scope_spans in &resource_spans.scope_spans {
            for span in &scope_spans.spans {
                if let Ok(trace_id) = <[u8; 16]>::try_from(span.trace_id.as_slice()) {
                    groups.entry(trace_id).or_default().push(span.clone());
                }
            }
        }
    }
    groups
}

/// Rebuild the `ExportTraceServiceRequest` keeping only the spans
/// whose `trace_id` is in `kept`. A `ScopeSpans` whose span list
/// becomes empty after filtering is omitted; a `ResourceSpans` whose
/// scope-span list becomes empty after filtering is omitted.
fn filter_request_by_trace_ids(
    request: ExportTraceServiceRequest,
    kept: &HashSet<[u8; 16]>,
) -> ExportTraceServiceRequest {
    let resource_spans = request
        .resource_spans
        .into_iter()
        .filter_map(|resource_spans| filter_resource_spans(resource_spans, kept))
        .collect();
    ExportTraceServiceRequest { resource_spans }
}

fn filter_resource_spans(
    resource_spans: ResourceSpans,
    kept: &HashSet<[u8; 16]>,
) -> Option<ResourceSpans> {
    let scope_spans: Vec<ScopeSpans> = resource_spans
        .scope_spans
        .into_iter()
        .filter_map(|scope_spans| filter_scope_spans(scope_spans, kept))
        .collect();
    if scope_spans.is_empty() {
        return None;
    }
    Some(ResourceSpans {
        resource: resource_spans.resource,
        scope_spans,
        schema_url: resource_spans.schema_url,
    })
}

fn filter_scope_spans(scope_spans: ScopeSpans, kept: &HashSet<[u8; 16]>) -> Option<ScopeSpans> {
    let spans: Vec<Span> = scope_spans
        .spans
        .into_iter()
        .filter(|span| span_is_kept(span, kept))
        .collect();
    if spans.is_empty() {
        return None;
    }
    Some(ScopeSpans {
        scope: scope_spans.scope,
        spans,
        schema_url: scope_spans.schema_url,
    })
}

fn span_is_kept(span: &Span, kept: &HashSet<[u8; 16]>) -> bool {
    match <[u8; 16]>::try_from(span.trace_id.as_slice()) {
        Ok(trace_id) => kept.contains(&trace_id),
        Err(_) => false,
    }
}

// =========================================================================
// Test seam — `__test_summary_tick_now` (per ADR-0018 §"Test seams"
// + ADR-0020 §6).
//
// Fires the snapshot-and-emit-INFO path synchronously, bypassing the
// Tokio timer entirely. Slice-06 uses this so the assertion does not
// depend on wall-clock time.
//
// At slice 05 the body is still `unimplemented!()`; slice 06 replaces
// it with a snapshot of the counters and a call into
// `observability::emit_summary`.
// =========================================================================

/// Fire the periodic summary synchronously, without waiting for the
/// timer.
///
/// `#[doc(hidden)]` and the `__` prefix mark this as a test seam. The
/// slice-06 integration test calls this, then asserts the captured
/// `target = "sieve"` INFO event carries the expected field set.
#[doc(hidden)]
pub fn __test_summary_tick_now<S, N>(_sink: &SamplingSink<S, N>)
where
    S: OtlpSink + Probe,
    N: Sampler,
{
    unimplemented!("__test_summary_tick_now lands at DELIVER slice 06");
}

#[cfg(test)]
mod tests {
    //! Unit tests for the trace-handling helpers.
    //!
    //! Port-to-port at the decorator's `accept` driving port: every
    //! test enters through `<SamplingSink as OtlpSink>::accept` and
    //! observes the records that reach the inner sink. The free
    //! functions `group_spans_by_trace_id`, `filter_request_by_trace_ids`,
    //! `span_is_kept`, and friends are exercised indirectly — the
    //! decorator is their only caller.
    //!
    //! Slice 05 lands the trace routing and the kept-trace filtering;
    //! the per-decision DEBUG events and counter increments are slice
    //! 06's territory and are NOT asserted here.
    //!
    //! Test budget: the trace path has two distinct behaviours —
    //! "forward kept-only traces to inner" and "drop malformed
    //! trace_ids defensively". Two parametrised tests exercise the
    //! full decision matrix.
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};

    use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
    use opentelemetry_proto::tonic::trace::v1::status::StatusCode;
    use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span, Status};

    use crate::decision::Decision;
    use crate::sampler::Sampler;
    use crate::trace_view::TraceView;
    use crate::SamplingSink;

    use aperture::ports::{OtlpSink, Probe, ProbeError, SinkError, SinkRecord};

    // ---------------------------------------------------------------------
    // Test doubles: a recording inner sink and a deterministic sampler
    // keyed by an explicit allow-list of trace_ids.
    //
    // Both doubles live at the hexagonal port boundary (the inner sink
    // is an `OtlpSink + Probe` impl; the sampler is a `Sampler` impl).
    // No mocks of internal types.
    // ---------------------------------------------------------------------

    struct RecordingInner {
        records: Mutex<Vec<SinkRecord>>,
    }

    impl RecordingInner {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                records: Mutex::new(Vec::new()),
            })
        }

        fn drain(&self) -> Vec<SinkRecord> {
            std::mem::take(&mut *self.records.lock().unwrap())
        }
    }

    struct RecordingHandle {
        inner: Arc<RecordingInner>,
    }

    impl OtlpSink for RecordingHandle {
        fn accept<'a>(
            &'a self,
            record: SinkRecord,
        ) -> Pin<Box<dyn Future<Output = Result<(), SinkError>> + Send + 'a>> {
            let inner = Arc::clone(&self.inner);
            Box::pin(async move {
                inner.records.lock().unwrap().push(record);
                Ok(())
            })
        }
    }

    impl Probe for RecordingHandle {
        fn probe<'a>(
            &'a self,
        ) -> Pin<Box<dyn Future<Output = Result<(), ProbeError>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }
    }

    /// Sampler that keeps a trace iff its `trace_id` is in the
    /// allow-list. Deterministic and side-effect-free; the test
    /// fixtures construct an allow-list per scenario.
    struct AllowListSampler {
        keep: Vec<[u8; 16]>,
    }

    impl Sampler for AllowListSampler {
        fn sample(&self, trace: &TraceView<'_>) -> Decision {
            if self.keep.contains(&trace.trace_id()) {
                Decision::Keep
            } else {
                Decision::Drop
            }
        }
    }

    // ---------------------------------------------------------------------
    // Fixture builders.
    // ---------------------------------------------------------------------

    fn span_for(trace_id_byte: u8) -> Span {
        Span {
            trace_id: vec![trace_id_byte; 16],
            span_id: vec![0; 8],
            trace_state: String::new(),
            parent_span_id: Vec::new(),
            flags: 0,
            name: "fixture".to_string(),
            kind: 0,
            start_time_unix_nano: 0,
            end_time_unix_nano: 0,
            attributes: Vec::new(),
            dropped_attributes_count: 0,
            events: Vec::new(),
            dropped_events_count: 0,
            links: Vec::new(),
            dropped_links_count: 0,
            status: Some(Status {
                message: String::new(),
                code: StatusCode::Ok as i32,
            }),
        }
    }

    fn span_with_short_trace_id() -> Span {
        let mut s = span_for(0xAA);
        s.trace_id = vec![0; 8]; // not 16 bytes
        s
    }

    fn envelope_with_spans(spans: Vec<Span>) -> ExportTraceServiceRequest {
        ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: None,
                scope_spans: vec![ScopeSpans {
                    scope: None,
                    spans,
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        }
    }

    fn build_sink(
        keep: Vec<[u8; 16]>,
    ) -> (
        SamplingSink<RecordingHandle, AllowListSampler>,
        Arc<RecordingInner>,
    ) {
        let inner = RecordingInner::new();
        let handle = RecordingHandle {
            inner: Arc::clone(&inner),
        };
        let sampler = AllowListSampler { keep };
        let sink = SamplingSink::new(handle, sampler);
        (sink, inner)
    }

    fn collect_kept_trace_id_first_bytes(record: &SinkRecord) -> Vec<u8> {
        let SinkRecord::Traces(req) = record else {
            panic!("expected Traces variant; got {record:?}");
        };
        let mut bytes = Vec::new();
        for resource_spans in &req.resource_spans {
            for scope_spans in &resource_spans.scope_spans {
                for span in &scope_spans.spans {
                    if let Some(b) = span.trace_id.first() {
                        bytes.push(*b);
                    }
                }
            }
        }
        bytes.sort_unstable();
        bytes
    }

    // ---------------------------------------------------------------------
    // Behaviour 1: the decorator forwards exactly the kept traces.
    //
    // Asserts the join of the grouping pass, the per-trace decision,
    // and the kept-only rebuild: the inner sink sees only spans whose
    // trace_id is in the sampler's allow-list, and a request with no
    // kept traces still produces one Traces record on the inner sink
    // (an empty envelope) — slice 06's observability path is what
    // surfaces "the entire batch was dropped"; the kept-only forward
    // contract on the wire is "always one envelope per inbound
    // envelope".
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn accept_traces_forwards_only_spans_whose_trace_id_is_kept() {
        // Three fixture trace_ids, two allowed; the third (0x33) is
        // exercised implicitly by its absence from the allow-list.
        let id_kept_a = [0x11; 16];
        let id_kept_b = [0x22; 16];

        let (sink, inner) = build_sink(vec![id_kept_a, id_kept_b]);

        let envelope = envelope_with_spans(vec![
            span_for(0x11),
            span_for(0x22),
            span_for(0x33),
            span_for(0x11), // second span on the kept-A trace
        ]);

        sink.accept(SinkRecord::Traces(envelope))
            .await
            .expect("accept must succeed");

        let recorded = inner.drain();
        assert_eq!(recorded.len(), 1, "exactly one Traces record reaches inner");
        let kept_first_bytes = collect_kept_trace_id_first_bytes(&recorded[0]);
        assert_eq!(
            kept_first_bytes,
            vec![0x11, 0x11, 0x22],
            "only spans with trace_id in the allow-list reach the inner sink"
        );
    }

    #[tokio::test]
    async fn accept_traces_drops_all_spans_when_allow_list_is_empty() {
        let (sink, inner) = build_sink(vec![]);
        let envelope = envelope_with_spans(vec![span_for(0x11), span_for(0x22)]);

        sink.accept(SinkRecord::Traces(envelope))
            .await
            .expect("accept must succeed");

        let recorded = inner.drain();
        assert_eq!(
            recorded.len(),
            1,
            "the decorator forwards exactly one envelope per inbound envelope"
        );
        let kept_first_bytes = collect_kept_trace_id_first_bytes(&recorded[0]);
        assert!(
            kept_first_bytes.is_empty(),
            "no spans reach the inner sink when the allow-list is empty; got {kept_first_bytes:?}"
        );
    }

    #[tokio::test]
    async fn accept_traces_keeps_every_span_when_all_trace_ids_are_in_allow_list() {
        let id_a = [0x11; 16];
        let id_b = [0x22; 16];
        let (sink, inner) = build_sink(vec![id_a, id_b]);
        let envelope = envelope_with_spans(vec![
            span_for(0x11),
            span_for(0x22),
            span_for(0x11),
            span_for(0x22),
        ]);

        sink.accept(SinkRecord::Traces(envelope))
            .await
            .expect("accept must succeed");

        let recorded = inner.drain();
        let kept_first_bytes = collect_kept_trace_id_first_bytes(&recorded[0]);
        assert_eq!(
            kept_first_bytes,
            vec![0x11, 0x11, 0x22, 0x22],
            "all four spans reach the inner sink when all trace_ids are kept"
        );
    }

    // ---------------------------------------------------------------------
    // Behaviour 2: malformed trace_id (not exactly 16 bytes) is treated
    // defensively — the span is excluded from the grouping pass and
    // does not reach the inner sink even if the allow-list is
    // permissive.
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn accept_traces_drops_spans_with_non_16_byte_trace_id() {
        // Allow-list permits the well-formed span.
        let id_well_formed = [0xAA; 16];
        let (sink, inner) = build_sink(vec![id_well_formed]);

        let envelope = envelope_with_spans(vec![
            span_for(0xAA),             // 16-byte trace_id, kept
            span_with_short_trace_id(), // 8-byte trace_id, defensively dropped
        ]);

        sink.accept(SinkRecord::Traces(envelope))
            .await
            .expect("accept must succeed");

        let recorded = inner.drain();
        let kept_first_bytes = collect_kept_trace_id_first_bytes(&recorded[0]);
        assert_eq!(
            kept_first_bytes,
            vec![0xAA],
            "only the well-formed span reaches inner; the malformed trace_id is dropped"
        );
    }

    // ---------------------------------------------------------------------
    // Probe: delegation to the inner sink (per ADR-0021 §6).
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn probe_delegates_to_inner_sink() {
        let (sink, _inner) = build_sink(vec![]);
        sink.probe()
            .await
            .expect("inner RecordingHandle's probe always returns Ok");
    }
}
