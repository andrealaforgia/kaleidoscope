// Kaleidoscope self-observe — Lumen → OTLP-JSON NDJSON writer
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

//! `LumenToOtlpJsonWriter` — emits each Lumen `MetricsRecorder`
//! event as one line of OTLP-JSON metrics data on a generic
//! `Write`.
//!
//! ## Shape
//!
//! Each call produces a single NDJSON line. The line is one
//! `ResourceMetrics`-rooted message in the OTLP-JSON encoding
//! defined by the OpenTelemetry specification. A sidecar
//! process that consumes the stream can wrap it in a
//! `MetricsData` envelope and POST it to any real OTLP/HTTP
//! collector.
//!
//! ## Field set
//!
//! v1 emits the minimal subset needed for an OTLP collector
//! to ingest the metric: `resource.attributes`, `scopeMetrics`
//! (with a single Kaleidoscope-named scope),
//! `metrics[].name`, `metrics[].sum.dataPoints[].asInt`,
//! `metrics[].sum.dataPoints[].timeUnixNano`,
//! `metrics[].sum.dataPoints[].attributes` (carrying the tenant
//! id). `tenant_id` lives as a resource attribute AND as a
//! point attribute; collectors disagree on which one they
//! prefer, and emitting both is the safer interop choice.
//!
//! ## Locking
//!
//! The writer is `Send + Sync` so it fits Lumen's recorder
//! bounds. Internally it wraps the `W` in a `Mutex` because
//! `lumen::MetricsRecorder` methods take `&self` and there is
//! no concurrency guarantee that two threads will not call
//! `record_ingest` simultaneously.

use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use lumen::MetricsRecorder as LumenRecorder;
use serde::Serialize;

// --------------------------------------------------------------------
// OTLP-JSON shape — hand-rolled minimal subset.
// --------------------------------------------------------------------

#[derive(Serialize)]
struct OtlpResourceMetrics<'a> {
    resource: OtlpResource<'a>,
    #[serde(rename = "scopeMetrics")]
    scope_metrics: [OtlpScopeMetrics<'a>; 1],
}

#[derive(Serialize)]
struct OtlpResource<'a> {
    attributes: [OtlpAttr<'a>; 1],
}

#[derive(Serialize)]
struct OtlpScopeMetrics<'a> {
    scope: OtlpScope<'a>,
    metrics: [OtlpMetric<'a>; 1],
}

#[derive(Serialize)]
struct OtlpScope<'a> {
    name: &'a str,
}

#[derive(Serialize)]
struct OtlpMetric<'a> {
    name: &'a str,
    sum: OtlpSum<'a>,
}

#[derive(Serialize)]
struct OtlpSum<'a> {
    #[serde(rename = "aggregationTemporality")]
    aggregation_temporality: u8,
    #[serde(rename = "isMonotonic")]
    is_monotonic: bool,
    #[serde(rename = "dataPoints")]
    data_points: [OtlpNumberPoint<'a>; 1],
}

#[derive(Serialize)]
struct OtlpNumberPoint<'a> {
    attributes: [OtlpAttr<'a>; 1],
    #[serde(rename = "timeUnixNano")]
    time_unix_nano: String, // OTLP-JSON encodes uint64 as a string
    #[serde(rename = "asInt")]
    as_int: String, // ditto
}

#[derive(Serialize)]
struct OtlpAttr<'a> {
    key: &'a str,
    value: OtlpAttrValue<'a>,
}

#[derive(Serialize)]
struct OtlpAttrValue<'a> {
    #[serde(rename = "stringValue")]
    string_value: &'a str,
}

// --------------------------------------------------------------------
// Writer
// --------------------------------------------------------------------

/// Bridge: implements `lumen::MetricsRecorder`, writes one OTLP-
/// JSON `ResourceMetrics` line per event to the inner writer.
pub struct LumenToOtlpJsonWriter<W: Write + Send + Sync> {
    inner: Mutex<W>,
    scope_name: String,
}

impl<W: Write + Send + Sync> LumenToOtlpJsonWriter<W> {
    /// Construct a writer wrapping the inner sink.
    pub fn new(inner: W) -> Self {
        Self {
            inner: Mutex::new(inner),
            scope_name: "kaleidoscope.lumen".to_string(),
        }
    }

    fn emit(&self, tenant: &TenantId, metric_name: &str, value: u64) {
        let now_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let time_str = now_ns.to_string();
        let value_str = value.to_string();
        let payload = OtlpResourceMetrics {
            resource: OtlpResource {
                attributes: [OtlpAttr {
                    key: "tenant_id",
                    value: OtlpAttrValue {
                        string_value: tenant.0.as_str(),
                    },
                }],
            },
            scope_metrics: [OtlpScopeMetrics {
                scope: OtlpScope {
                    name: &self.scope_name,
                },
                metrics: [OtlpMetric {
                    name: metric_name,
                    sum: OtlpSum {
                        // 2 = AGGREGATION_TEMPORALITY_CUMULATIVE
                        aggregation_temporality: 2,
                        is_monotonic: true,
                        data_points: [OtlpNumberPoint {
                            attributes: [OtlpAttr {
                                key: "tenant_id",
                                value: OtlpAttrValue {
                                    string_value: tenant.0.as_str(),
                                },
                            }],
                            time_unix_nano: time_str,
                            as_int: value_str,
                        }],
                    },
                }],
            }],
        };
        if let Ok(line) = serde_json::to_string(&payload) {
            if let Ok(mut writer) = self.inner.lock() {
                let _ = writer.write_all(line.as_bytes());
                let _ = writer.write_all(b"\n");
                let _ = writer.flush();
            }
        }
    }
}

impl<W: Write + Send + Sync> LumenRecorder for LumenToOtlpJsonWriter<W> {
    fn record_ingest(&self, tenant: &TenantId, record_count: usize) {
        self.emit(tenant, "lumen.ingest.count", record_count as u64);
    }

    fn record_query(&self, tenant: &TenantId, matched_count: usize) {
        self.emit(tenant, "lumen.query.count", matched_count as u64);
    }
}
