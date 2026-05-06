//! `SamplingSink<S, N>` — the `OtlpSink + Probe` decorator that adds
//! head-based sampling on the `Traces` variant and forwards `Logs` /
//! `Metrics` unchanged.
//!
//! Per ADR-0021: generic over the inner sink type `S` and the
//! sampler type `N`; consumes Aperture's existing `OtlpSink +
//! Probe` traits; no Aperture-side trait amendment.
//!
//! ## DELIVER state — slice 06 (final)
//!
//! - [`SamplingSink::new`] stores the inner sink, the sampler, and a
//!   fresh [`Counters`] aggregator behind `Arc`. The Tokio summary
//!   timer task is spawned on the ambient runtime per ADR-0020 §2;
//!   the timer ticks every `SIEVE_SUMMARY_TICK_MS` (default
//!   `60_000`).
//! - The `OtlpSink::accept` impl routes per variant: `Logs` and
//!   `Metrics` are forwarded to the inner sink unchanged (per Q6 +
//!   ADR-0021 §1); `Traces` are grouped by `trace_id`, the sampler
//!   is asked per trace, the matching counter is incremented, and a
//!   DEBUG `target = "sieve"` event is emitted for every decision.
//!   The kept-traces-only [`ExportTraceServiceRequest`] is forwarded
//!   to the inner sink.
//! - The `Probe::probe` impl delegates to the inner sink (per
//!   ADR-0021 §6).
//! - [`__test_summary_tick_now`] fires the snapshot-and-emit-INFO
//!   path synchronously per ADR-0020 §6; the integration test seam
//!   bypasses the timer entirely.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use aperture::ports::{OtlpSink, Probe, ProbeError, SinkError, SinkRecord};
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span};

use crate::aggregator::{
    parse_summary_tick_ms_from_env, Counters, SummaryTask, DEFAULT_SUMMARY_TICK_MS,
};
use crate::decision::Decision;
use crate::observability::{
    emit_debug_dropped, emit_debug_kept_error_bearing, emit_debug_kept_sampled, emit_summary,
};
use crate::sampler::{is_error_bearing, Sampler};
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
    /// can read them concurrently with the hot path. Slice 06 wires
    /// the increments at the keep / drop branches and the
    /// timer-driven snapshot.
    counters: Arc<Counters>,

    /// The configured non-error rate, surfaced for the periodic INFO
    /// summary. Captured at construction time (the sampler's rate is
    /// the ground truth; this field caches it so the timer task and
    /// the test seam can render the summary without a trait-bounded
    /// callback into `Sampler`).
    rate: f64,

    /// The Tokio timer task that ticks every `SIEVE_SUMMARY_TICK_MS`
    /// (default `60_000`) and emits the periodic INFO summary.
    /// Cancelled on Drop; per ADR-0020 §3 the cancel is sync and the
    /// JoinHandle is abandoned so Drop runs in sync context without
    /// `await`. The field is held only for its `Drop` side effect
    /// (cancel-on-drop), not read directly.
    #[allow(dead_code)]
    summary_task: SummaryTask,
}

impl<S, N> SamplingSink<S, N>
where
    S: OtlpSink + Probe,
    N: Sampler,
{
    /// Wrap the inner sink with the given sampler.
    ///
    /// The constructor stores the inner sink, the sampler, and a
    /// fresh [`Counters`] aggregator behind `Arc` so the timer task
    /// can read state concurrently with the hot path. The Tokio
    /// summary timer is spawned on the ambient runtime per ADR-0020
    /// §2; the configured rate is captured for the periodic INFO
    /// summary.
    ///
    /// **Rate capture**: the rate carried by the periodic summary is
    /// read from the sampler at construction time. At v0 the only
    /// concrete `Sampler` is [`crate::HeadSampler`], whose
    /// [`crate::HeadSampler::rate`] surface returns the configured
    /// rate; the constructor uses an `Any` downcast to read it
    /// without altering the public `Sampler` trait surface (locked at
    /// ADR-0018 §"Public surface (final list)"). For a future
    /// `Sampler` impl that does not downcast to `HeadSampler`, the
    /// summary's `rate` field carries `f64::NAN` — operators see the
    /// configured-rate field as "unknown" rather than seeing a stale
    /// value. The downcast is bounded by `N: 'static` (which
    /// `Sampler: 'static` already provides).
    pub fn new(inner: S, sampler: N) -> Self {
        let rate = read_rate_from_sampler(&sampler);
        let counters = Arc::new(Counters::new());
        let interval_ms = parse_summary_tick_ms_from_env().unwrap_or(DEFAULT_SUMMARY_TICK_MS);
        let summary_task = SummaryTask::spawn(Arc::clone(&counters), rate, interval_ms);
        Self {
            inner: Arc::new(inner),
            sampler: Arc::new(sampler),
            counters,
            rate,
            summary_task,
        }
    }
}

/// Read the configured rate from the sampler via an `Any` downcast to
/// the only v0 concrete sampler ([`crate::HeadSampler`]).
///
/// Returns `f64::NAN` if the sampler is not a `HeadSampler` — the
/// summary's `rate` field then carries NaN, which operators see as
/// "unknown rate". This is honest: the v0 `Sampler` trait does not
/// expose a rate accessor (ADR-0018 keeps the trait at one method),
/// so the rate is only knowable when the concrete type is
/// `HeadSampler`. A future Sampler impl that wants to surface its
/// rate adds itself to the downcast match below.
///
/// Pulled out as a free function so unit tests can pin both the
/// `HeadSampler` branch (returns the configured rate) and the
/// fallback branch (returns NaN for an unknown sampler).
fn read_rate_from_sampler<N: Sampler + 'static>(sampler: &N) -> f64 {
    let any: &dyn std::any::Any = sampler;
    if let Some(head) = any.downcast_ref::<crate::HeadSampler>() {
        return head.rate();
    }
    f64::NAN
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
    /// Slice 06: every per-trace decision increments the appropriate
    /// counter (`record_kept_error_bearing` / `record_kept_sampled` /
    /// `record_dropped`) AND emits a DEBUG tracing event with
    /// `target = "sieve"`. The DEBUG event vocabulary is locked at
    /// ADR-0020 §5 + the slice-06 brief.
    async fn accept_traces(&self, request: ExportTraceServiceRequest) -> Result<(), SinkError> {
        let kept_trace_ids = self.decide_kept_trace_ids(&request);
        let filtered = filter_request_by_trace_ids(request, &kept_trace_ids);
        self.inner.accept(SinkRecord::Traces(filtered)).await
    }

    /// Compute the set of trace_ids that the sampler keeps for this
    /// request. Spans whose `trace_id` is not the canonical 16-byte
    /// length are skipped (defensive; the OTLP wire contract
    /// requires 16 bytes).
    ///
    /// For each trace, this is the single point that:
    /// 1. Asks the sampler for a `Decision`.
    /// 2. Computes the `KeepReason` (`ErrorBearing` if any span has
    ///    `status.code == ERROR`, `Sampled` otherwise).
    /// 3. Increments the appropriate counter on `self.counters`.
    /// 4. Emits the matching DEBUG tracing event.
    ///
    /// The single-point shape keeps the three observability surfaces
    /// (counters, DEBUG events, kept-set) aligned by construction —
    /// a refactor that splits them risks the three drifting apart.
    fn decide_kept_trace_ids(&self, request: &ExportTraceServiceRequest) -> HashSet<[u8; 16]> {
        let groups = group_spans_by_trace_id(request);
        let mut kept: HashSet<[u8; 16]> = HashSet::with_capacity(groups.len());
        for (trace_id, spans) in &groups {
            let view = TraceView::from_grouping_pass(*trace_id, spans.as_slice());
            let decision = self.sampler.sample(&view);
            self.record_decision(*trace_id, &view, decision);
            if matches!(decision, Decision::Keep) {
                kept.insert(*trace_id);
            }
        }
        kept
    }

    /// Record a per-trace decision into the counters and emit the
    /// matching DEBUG tracing event. Slice 06's observability
    /// vocabulary lives entirely in this function — every keep / drop
    /// branch routes through here.
    ///
    /// The `KeepReason` is computed on the spot via
    /// [`crate::sampler::is_error_bearing`] (the same predicate the
    /// sampler uses for its error-bias rule). Per ADR-0018
    /// §"Internal layout": "Free function `is_error_bearing(spans) ->
    /// bool` (kept `pub(crate)` so the decorator can call it without
    /// going through the trait)."
    fn record_decision(&self, trace_id: [u8; 16], view: &TraceView<'_>, decision: Decision) {
        match decision {
            Decision::Keep => {
                if is_error_bearing(view.spans()) {
                    self.counters.record_kept_error_bearing();
                    emit_debug_kept_error_bearing(trace_id);
                } else {
                    self.counters.record_kept_sampled();
                    emit_debug_kept_sampled(trace_id, self.rate);
                }
            }
            Decision::Drop => {
                self.counters.record_dropped();
                emit_debug_dropped(trace_id, self.rate);
            }
        }
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
// =========================================================================

/// Fire the periodic summary synchronously, without waiting for the
/// timer.
///
/// Snapshots the counters (resetting them in the process, matching
/// the periodic timer's behaviour exactly) and emits ONE INFO event
/// with `target = "sieve"` carrying the same field set the timer
/// task emits.
///
/// `#[doc(hidden)]` and the `__` prefix mark this as a test seam. The
/// slice-06 integration test calls this, then asserts the captured
/// `target = "sieve"` INFO event carries the expected field set.
#[doc(hidden)]
pub fn __test_summary_tick_now<S, N>(sink: &SamplingSink<S, N>)
where
    S: OtlpSink + Probe,
    N: Sampler,
{
    let (kept, kept_err, dropped) = sink.counters.snapshot_and_reset();
    emit_summary(kept, kept_err, dropped, sink.rate);
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

    // ---------------------------------------------------------------------
    // Behaviour 3: rate capture via `Any` downcast.
    //
    // Pins the two branches of `read_rate_from_sampler`:
    // - Branch A: the concrete sampler is `HeadSampler` → returns the
    //   configured rate.
    // - Branch B: the concrete sampler is something else (the
    //   `AllowListSampler` test double) → returns `f64::NAN`.
    //
    // A mutation that swaps the downcast target type, removes the
    // downcast, or returns the wrong fallback is caught by the
    // contrast between the two cases.
    //
    // Port-to-port at domain scope per Mandate 2: `read_rate_from_sampler`
    // is a pure free function whose signature IS its driving port.
    // Calling it directly from a test IS port-to-port testing.
    // ---------------------------------------------------------------------

    #[test]
    fn read_rate_from_sampler_returns_configured_rate_for_head_sampler() {
        let head = crate::HeadSampler::new(0.42).expect("rate 0.42 is in [0.0, 1.0]");
        let rate = super::read_rate_from_sampler(&head);
        assert_eq!(
            rate, 0.42,
            "read_rate_from_sampler must surface the HeadSampler's configured rate"
        );
    }

    #[test]
    fn read_rate_from_sampler_returns_nan_for_a_non_head_sampler() {
        let custom = AllowListSampler { keep: Vec::new() };
        let rate = super::read_rate_from_sampler(&custom);
        assert!(
            rate.is_nan(),
            "read_rate_from_sampler must return NaN for a sampler that is not HeadSampler; got {rate}"
        );
    }
}
