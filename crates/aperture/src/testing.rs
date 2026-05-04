//! Test doubles for integration tests.
//!
//! Per `docs/feature/aperture/design/component-design.md > Test
//! doubles`, [`RecordingSink`] is the seam US-AP-03's "custom OtlpSink
//! plugs in without crate-level changes" UAT writes against. It is the
//! smallest possible witness that the trait IS the integration surface.
//!
//! The [`stderr_capture`] helper subscribes a layer to the production
//! `tracing-subscriber` registry that records every event emitted while
//! the supplied closure runs. Integration tests use it to assert
//! against the closed v0 event vocabulary without parsing JSON out of
//! file descriptors.

use std::pin::Pin;
use std::sync::Mutex;

use crate::observability::CapturedEvent;
use crate::ports::{OtlpSink, Probe, ProbeError, SinkError, SinkRecord};

/// In-memory sink: every accepted record is appended to a vector.
///
/// `RecordingSink` is the test double the integration tests use to
/// observe what Aperture's application core hands off through the
/// `OtlpSink` trait. It records the records — nothing else.
pub struct RecordingSink {
    inner: Mutex<Vec<SinkRecord>>,
}

impl Default for RecordingSink {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordingSink {
    /// Construct an empty recorder.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Vec::new()),
        }
    }

    /// Snapshot of every record accepted so far. Clones the records
    /// out from under the mutex; tests assert against the snapshot.
    pub fn drain(&self) -> Vec<SinkRecord> {
        let mut g = self.inner.lock().expect("recording-sink mutex poisoned");
        std::mem::take(&mut *g)
    }

    /// Number of records accepted so far without removing them.
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("recording-sink mutex poisoned")
            .len()
    }

    /// Convenience predicate; equivalent to `len() == 0`.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl OtlpSink for RecordingSink {
    fn accept<'a>(
        &'a self,
        record: SinkRecord,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), SinkError>> + Send + 'a>> {
        Box::pin(async move {
            // A `RecordingSink` substitutes for `StubSink` at the
            // hexagonal seam (DISTILL D2). The integration tests for
            // Slice 01 assert against the production-bound stderr line
            // `event=sink_accepted sink=stub` — so the recording sink
            // emits the same line shape on accept. This keeps the
            // hexagonal substitution observable.
            //
            // Compute the summary BEFORE moving the record into the
            // recorder; the borrow on `record` is released by the time
            // `push` runs.
            let (signal, service_name, count) = {
                let summary = crate::app::summarise_record(&record);
                (
                    summary.signal,
                    summary.resource_service_name.unwrap_or("").to_string(),
                    summary.count as u64,
                )
            };
            self.inner
                .lock()
                .expect("recording-sink mutex poisoned")
                .push(record);
            // Slice 01 only exercises the logs path; Slices 03/04 will
            // land the traces/metrics field-name variants. The stderr
            // line shape mirrors `StubSink::accept` exactly so the
            // hexagonal substitution at the trait seam is observable.
            tracing::info!(
                event = crate::observability::event::SINK_ACCEPTED,
                sink = "stub",
                signal = signal,
                record_count = count,
                "resource.service.name" = service_name,
            );
            Ok(())
        })
    }
}

impl Probe for RecordingSink {
    fn probe<'a>(
        &'a self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), ProbeError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

// =========================================================================
// stderr capture seam
// =========================================================================

/// A captured tracing event observed during a [`stderr_capture`] call.
#[derive(Debug, Clone)]
pub struct StderrEvent {
    pub level: String,
    pub event: String,
    pub fields: serde_json::Value,
}

impl From<CapturedEvent> for StderrEvent {
    fn from(c: CapturedEvent) -> Self {
        Self {
            level: c.level,
            event: c.event,
            fields: c.fields,
        }
    }
}

/// Run the supplied async closure with a fresh capture layer
/// subscribed to the tracing registry. Returns the closure's value
/// alongside every event the closure emitted.
///
/// The capture layer is process-global; concurrent captures are not
/// supported (the integration test harness runs sequentially under
/// `RUST_TEST_THREADS=1`).
pub async fn stderr_capture<F, Fut, R>(f: F) -> (R, Vec<StderrEvent>)
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = R>,
{
    crate::observability::begin_capture();
    let value = f().await;
    let captured = crate::observability::end_capture();
    let events = captured.into_iter().map(StderrEvent::from).collect();
    (value, events)
}
