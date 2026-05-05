//! Driven adapters — concrete `OtlpSink` implementations.
//!
//! See `docs/feature/aperture/design/component-design.md > Sinks` for
//! the design contract.
//!
//! Slice 01 lights up `StubSink`. Slice 06 lands `ForwardingSink`.

use std::pin::Pin;

use crate::app::summarise_record;
use crate::observability::event;
use crate::ports::{OtlpSink, Probe, ProbeError, SinkError, SinkRecord};

/// `StubSink` — writes one structured stderr line per accepted record
/// (`event=sink_accepted sink=stub`) and returns `Ok(())`. Useful for
/// smoke-testing fixtures and CI; the v0 default sink kind.
#[derive(Debug, Default)]
pub struct StubSink;

impl OtlpSink for StubSink {
    fn accept<'a>(
        &'a self,
        record: SinkRecord,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), SinkError>> + Send + 'a>> {
        Box::pin(async move {
            emit_sink_accepted("stub", &record);
            Ok(())
        })
    }
}

/// Emit the `event=sink_accepted` line with the per-signal count field
/// name. The closed v0 vocabulary uses signal-specific count fields:
/// `record_count` for logs (Slice 01), `span_count` for traces (Slice
/// 03), `data_point_count` for metrics (Slice 04). `tracing::info!`
/// fixes field names at compile time, so the per-signal call sites are
/// the natural shape.
pub(crate) fn emit_sink_accepted(sink: &'static str, record: &SinkRecord) {
    let summary = summarise_record(record);
    let service_name = summary.resource_service_name.unwrap_or("");
    let count = summary.count as u64;
    match record {
        SinkRecord::Logs(_) => tracing::info!(
            event = event::SINK_ACCEPTED,
            sink = sink,
            signal = summary.signal,
            record_count = count,
            "resource.service.name" = service_name,
        ),
        SinkRecord::Traces(_) => tracing::info!(
            event = event::SINK_ACCEPTED,
            sink = sink,
            signal = summary.signal,
            span_count = count,
            "resource.service.name" = service_name,
        ),
        SinkRecord::Metrics(_) => tracing::info!(
            event = event::SINK_ACCEPTED,
            sink = sink,
            signal = summary.signal,
            data_point_count = count,
            "resource.service.name" = service_name,
        ),
    }
}

impl Probe for StubSink {
    fn probe<'a>(
        &'a self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), ProbeError>> + Send + 'a>> {
        // No external dependency. Probe is trivially Ok.
        Box::pin(async { Ok(()) })
    }
}
