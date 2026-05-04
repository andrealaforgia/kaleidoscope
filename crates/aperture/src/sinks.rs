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
            let summary = summarise_record(&record);
            let service_name = summary.resource_service_name.unwrap_or("");
            let count = summary.count as u64;
            // Slice 01 only exercises logs; Slice 03 and Slice 04 land
            // the traces/metrics branches with their distinctive
            // `span_count` and `data_point_count` field names.
            tracing::info!(
                event = event::SINK_ACCEPTED,
                sink = "stub",
                signal = summary.signal,
                record_count = count,
                "resource.service.name" = service_name,
            );
            Ok(())
        })
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
