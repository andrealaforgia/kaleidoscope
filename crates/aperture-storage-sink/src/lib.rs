// Kaleidoscope aperture-storage-sink — the storage OtlpSink
// Copyright (C) 2026 The Kaleidoscope authors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with this program. If not, see <https://www.gnu.org/licenses/>.

//! # `aperture-storage-sink`
//!
//! A third [`OtlpSink`](aperture::ports::OtlpSink) (sibling of
//! `StubSink` and `ForwardingSink`, ADR-0007) that persists accepted
//! OTLP records into the durable Kaleidoscope pillars. Slice 01 wires
//! **logs to lumen**; traces (ray) and metrics (pulse) land in slices
//! 02 / 03 behind the same constructor shape.
//!
//! aperture itself gains no pillar dependency: the dependency arrow
//! points from this crate to aperture (the port), never back (DD2). A
//! host composition binary (`kaleidoscope-gateway`) opens the
//! `FileBacked*Store`s and injects the [`StorageSink`] through
//! aperture's `spawn(config, Arc<dyn OtlpSink>)` seam.
//!
//! ## Slice 01 surface
//!
//! - [`StorageSinkConfig`] carries the optional `default_tenant`
//!   (DD3 / ADR-0041 Decision 2).
//! - [`StorageSink::with_log_store`] constructs a logs-only sink from a
//!   shared [`lumen::FileBackedLogStore`] (DD4). The sink holds each
//!   signal's store as an `Option<Arc<...>>`, so slices 02 / 03 add
//!   `with_trace_store` / `with_metric_store` builders without breaking
//!   the logs-only entry point.
//! - [`StorageSink`] implements [`OtlpSink`](aperture::ports::OtlpSink)
//!   and [`Probe`](aperture::ports::Probe).
//!
//! ## Translation, tenancy, atomicity
//!
//! The OTLP-to-lumen field mapping lives in [`translate`]. Translation
//! runs to completion before any ingest; a malformed byte-array
//! identifier refuses the entire accept and writes nothing (DD7). The
//! tenant is resolved once per accept from the first resource's
//! `tenant.id` attribute, falling back to `default_tenant`, else the
//! accept is refused (DD3).

#![forbid(unsafe_code)]

mod translate;

use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::Arc;

use aegis::TenantId;
use aperture::ports::{OtlpSink, Probe, ProbeError, SinkError, SinkRecord};
use lumen::{FileBackedLogStore, LogBatch, LogRecord, LogStore, SeverityNumber};
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;

use crate::translate::{resolve_tenant_id, translate_logs};

/// The reserved tenant the [`Probe`] writes its sentinel record under,
/// so an active write check never collides with a real tenant's data
/// (DD5).
const PROBE_TENANT: &str = "__aperture_storage_sink_probe__";

/// Configuration for a [`StorageSink`]. Slice 01 carries only the
/// optional default tenant (DD3); later slices may grow it additively.
#[derive(Debug, Clone)]
pub struct StorageSinkConfig {
    /// The tenant a record is filed under when no `tenant.id` resource
    /// attribute resolves it. `None` means fail-closed: an unresolvable
    /// tenant refuses the accept and writes nothing.
    default_tenant: Option<String>,
}

impl StorageSinkConfig {
    /// Configure a default tenant. Records without a `tenant.id`
    /// resource attribute are filed under this tenant.
    pub fn with_default_tenant(tenant: impl Into<String>) -> Self {
        Self {
            default_tenant: Some(tenant.into()),
        }
    }

    /// Configure no default tenant (fail-closed). A record without a
    /// resolvable `tenant.id` is refused and nothing is written.
    pub fn no_default_tenant() -> Self {
        Self {
            default_tenant: None,
        }
    }
}

/// The storage sink. Holds each signal's durable store as an
/// `Option<Arc<...>>` so the logs-only constructor works with just the
/// log store wired; slices 02 / 03 add the trace / metric handles
/// without a breaking change (DD4).
#[derive(Debug)]
pub struct StorageSink {
    config: StorageSinkConfig,
    log_store: Option<Arc<FileBackedLogStore>>,
}

impl StorageSink {
    /// Construct a logs-only sink from a shared
    /// [`lumen::FileBackedLogStore`] (DD4). Traces and metrics are not
    /// wired in this slice; their `accept` arms are honest no-ops that
    /// emit a `signal_not_yet_wired` event (slices 02 / 03 wire them).
    pub fn with_log_store(log_store: Arc<FileBackedLogStore>, config: StorageSinkConfig) -> Self {
        Self {
            config,
            log_store: Some(log_store),
        }
    }

    /// Resolve the tenant for an accept (DD3): the `tenant.id` resource
    /// attribute when present, else the configured `default_tenant`,
    /// else a refusal naming the missing-tenant rule.
    fn resolve_tenant(&self, request: &ExportLogsServiceRequest) -> Result<TenantId, SinkError> {
        if let Some(explicit) = resolve_tenant_id(request) {
            return Ok(TenantId(explicit));
        }
        if let Some(default_tenant) = &self.config.default_tenant {
            return Ok(TenantId(default_tenant.clone()));
        }
        Err(SinkError::Internal {
            message: "no tenant: record carries no tenant.id resource attribute and no \
                      default_tenant is configured; refusing per ADR-0041 Decision 2"
                .to_string(),
        })
    }

    /// Translate and persist a logs request (section 6.1 / DD7). The
    /// whole request is translated before any ingest; a malformed
    /// identifier refuses the accept and writes nothing.
    fn accept_logs(&self, request: &ExportLogsServiceRequest) -> Result<(), SinkError> {
        let store = self.require_log_store()?;
        let tenant = self.resolve_tenant(request)?;
        let records = translate_logs(request).map_err(|e| SinkError::Internal {
            message: format!("log translation refused: {e}"),
        })?;
        ingest_logs(store.as_ref(), &tenant, records)
    }

    /// The wired log store, or an internal error if a logs request
    /// reaches a sink built without one (cannot happen via the slice-01
    /// constructor, but the arm keeps the contract total).
    fn require_log_store(&self) -> Result<&Arc<FileBackedLogStore>, SinkError> {
        self.log_store.as_ref().ok_or_else(|| SinkError::Internal {
            message: "no log store wired into this StorageSink".to_string(),
        })
    }

    /// Active write check (DD5): ingest a single sentinel record under
    /// the reserved probe tenant, then take a snapshot. The ingest
    /// forces a WAL append; the snapshot forces a fresh `File::create`
    /// inside the `pillar_root`. A `pillar_root` that opened but is not
    /// writable (read-only mount, full disk) fails the snapshot's
    /// directory-level create — the open WAL file descriptor would
    /// otherwise let an append-only check pass even on a read-only
    /// directory — so the snapshot is what genuinely catches the
    /// catalogued "opens but is not writable" substrate lie.
    fn probe_log_store(&self) -> Result<(), ProbeError> {
        let store = self
            .log_store
            .as_ref()
            .ok_or_else(|| ProbeError::Unreachable {
                endpoint: "lumen".to_string(),
                reason: "no log store wired into this StorageSink".to_string(),
            })?;
        let probe_batch = LogBatch::with_records(vec![probe_record()]);
        store
            .ingest(&TenantId(PROBE_TENANT.to_string()), probe_batch)
            .map_err(|e| probe_unreachable(format!("probe write check failed: {e}")))?;
        store
            .snapshot()
            .map_err(|e| probe_unreachable(format!("probe snapshot check failed: {e}")))
    }
}

/// Build the `ProbeError::Unreachable` the probe returns when the
/// lumen pillar_root is not writable. Named so the two probe failure
/// arms share one constructor.
fn probe_unreachable(reason: String) -> ProbeError {
    ProbeError::Unreachable {
        endpoint: "lumen".to_string(),
        reason,
    }
}

/// Persist the translated records under the resolved tenant. A lumen
/// persistence failure maps to `SinkError::Internal` (DD6).
fn ingest_logs(
    store: &FileBackedLogStore,
    tenant: &TenantId,
    records: Vec<LogRecord>,
) -> Result<(), SinkError> {
    store
        .ingest(tenant, LogBatch::with_records(records))
        .map(|_| ())
        .map_err(|e| SinkError::Internal {
            message: format!("lumen ingest failed: {e}"),
        })
}

/// The sentinel record the probe ingests. A fixed, recognisable shape
/// so an operator inspecting the reserved probe tenant sees why the
/// record exists.
fn probe_record() -> LogRecord {
    LogRecord {
        observed_time_unix_nano: 0,
        severity_number: SeverityNumber::UNSPECIFIED,
        severity_text: String::new(),
        body: "aperture-storage-sink probe write check".to_string(),
        attributes: BTreeMap::new(),
        resource_attributes: BTreeMap::new(),
        trace_id: None,
        span_id: None,
    }
}

impl OtlpSink for StorageSink {
    fn accept<'a>(
        &'a self,
        record: SinkRecord,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), SinkError>> + Send + 'a>> {
        Box::pin(async move {
            match record {
                SinkRecord::Logs(request) => self.accept_logs(&request),
                // Slice 01 is logs-only. Traces and metrics are accepted
                // (Ok, so the gateway does not reject them) but not yet
                // persisted; the event makes the gap observable. Slices
                // 02 / 03 replace these arms with real translation +
                // ingest into ray / pulse.
                SinkRecord::Traces(_) => {
                    emit_signal_not_yet_wired("traces");
                    Ok(())
                }
                SinkRecord::Metrics(_) => {
                    emit_signal_not_yet_wired("metrics");
                    Ok(())
                }
                // `SinkRecord` is `#[non_exhaustive]`. A signal variant
                // this sink does not recognise is refused rather than
                // silently accepted, so a future additive variant
                // cannot be dropped on the floor unnoticed.
                other => Err(SinkError::Internal {
                    message: format!("unrecognised SinkRecord variant: {other:?}"),
                }),
            }
        })
    }
}

impl Probe for StorageSink {
    fn probe<'a>(
        &'a self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), ProbeError>> + Send + 'a>> {
        Box::pin(async move { self.probe_log_store() })
    }
}

/// Emit the `signal_not_yet_wired` warn line for a signal whose pillar
/// is not wired in this slice and return the signal name it logged.
/// Returning the name (rather than `()`) makes the emission observable:
/// a unit test asserts the returned signal, so a mutation that drops
/// the body is caught (an empty body cannot return the right name).
fn emit_signal_not_yet_wired(signal: &'static str) -> &'static str {
    tracing::warn!(
        event = "signal_not_yet_wired",
        sink = "storage",
        signal = signal,
        "accepted but not persisted: this signal's pillar lands in a later slice",
    );
    signal
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------
    // StorageSinkConfig — the two constructors set the default_tenant
    // field as named. The acceptance suite drives both through the port
    // (default-tenant fallback and the no-default refusal), but pinning
    // the field directly kills an `Ok(Default::default())` mutation on
    // either constructor that would silently swap Some<->None.
    // -------------------------------------------------------------------

    #[test]
    fn with_default_tenant_carries_the_tenant() {
        let config = StorageSinkConfig::with_default_tenant("acme");
        assert_eq!(config.default_tenant, Some("acme".to_string()));
    }

    #[test]
    fn no_default_tenant_carries_none() {
        let config = StorageSinkConfig::no_default_tenant();
        assert_eq!(config.default_tenant, None);
    }

    // -------------------------------------------------------------------
    // probe_record — pin the sentinel body so a mutation that empties
    // it (the only field an operator reads to recognise the probe
    // write) is caught. The acceptance suite asserts the probe Ok/Err
    // verdict through the port but never inspects the sentinel.
    // -------------------------------------------------------------------

    #[test]
    fn probe_record_carries_a_recognisable_body() {
        let record = probe_record();
        assert_eq!(record.body, "aperture-storage-sink probe write check");
    }

    // -------------------------------------------------------------------
    // not-yet-wired signals (slice 01 is logs-only) — the traces /
    // metrics arms emit the observability event and accept (Ok, so the
    // gateway does not reject them). The acceptance suite is logs-only,
    // so the emit helper is reachable only here. Pinning the emit's
    // returned signal name kills a `replace emit body` mutation.
    // -------------------------------------------------------------------

    #[test]
    fn emit_signal_not_yet_wired_returns_the_signal_name() {
        assert_eq!(emit_signal_not_yet_wired("traces"), "traces");
        assert_eq!(emit_signal_not_yet_wired("metrics"), "metrics");
    }
}
