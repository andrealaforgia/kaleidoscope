//! Test doubles for integration tests.
//!
//! Per `docs/feature/aperture/design/component-design.md > Test
//! doubles`, [`RecordingSink`] is the seam US-AP-03's "custom OtlpSink
//! plugs in without crate-level changes" UAT writes against. It is the
//! smallest possible witness that the trait IS the integration surface.

// SCAFFOLD: true
// Status: DISTILL test-double surface. Unlike the production-stub
// modules, the test doubles are FUNCTIONAL at DISTILL — the slice
// tests rely on `RecordingSink` to capture handed-off records. DELIVER
// keeps this surface and additionally lands a `stderr_capture` symbol
// here per `wave-decisions.md > D6`.

use std::pin::Pin;
use std::sync::Mutex;

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
    ///
    /// DELIVER keeps this as the seam; the underlying mutex is the
    /// trivial implementation detail.
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
            self.inner
                .lock()
                .expect("recording-sink mutex poisoned")
                .push(record);
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
