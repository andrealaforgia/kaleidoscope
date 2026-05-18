// Kaleidoscope self-observe — Sluice → OTLP-JSON NDJSON writer
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

//! `SluiceToOtlpJsonWriter` — emits each Sluice
//! `MetricsRecorder` event as one line of OTLP-JSON metrics
//! data on a generic `Write`. Mirrors
//! [`crate::CinderToOtlpJsonWriter`] in shape and locking.
//!
//! Uses the `Vec<OtlpAttr>` shape (rather than the Lumen
//! writer's fixed-array shape) because `enqueue` carries an
//! `accepted` point attribute beyond `tenant_id`. The same
//! "three similar lines beat a premature abstraction"
//! reasoning that justified the parallel Cinder module applies
//! again here. When a third Vec-based writer lands (Augur or
//! Ray), that is the right moment to factor a shared OTLP-JSON
//! serialization module.

use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use serde::Serialize;
use sluice::MetricsRecorder as SluiceRecorder;

#[derive(Serialize)]
struct OtlpResourceMetrics<'a> {
    resource: OtlpResource<'a>,
    #[serde(rename = "scopeMetrics")]
    scope_metrics: [OtlpScopeMetrics<'a>; 1],
}

#[derive(Serialize)]
struct OtlpResource<'a> {
    attributes: Vec<OtlpAttr<'a>>,
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
    attributes: Vec<OtlpAttr<'a>>,
    #[serde(rename = "timeUnixNano")]
    time_unix_nano: String,
    #[serde(rename = "asInt")]
    as_int: String,
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

pub struct SluiceToOtlpJsonWriter<W: Write + Send + Sync> {
    inner: Mutex<W>,
    scope_name: String,
}

impl<W: Write + Send + Sync> SluiceToOtlpJsonWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner: Mutex::new(inner),
            scope_name: "kaleidoscope.sluice".to_string(),
        }
    }

    fn emit(&self, tenant: &TenantId, metric_name: &str, value: u64, extra_attrs: &[(&str, &str)]) {
        let now_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let time_str = now_ns.to_string();
        let value_str = value.to_string();

        let mut point_attrs: Vec<OtlpAttr> = Vec::with_capacity(1 + extra_attrs.len());
        point_attrs.push(OtlpAttr {
            key: "tenant_id",
            value: OtlpAttrValue {
                string_value: tenant.0.as_str(),
            },
        });
        for (k, v) in extra_attrs {
            point_attrs.push(OtlpAttr {
                key: k,
                value: OtlpAttrValue { string_value: v },
            });
        }

        let payload = OtlpResourceMetrics {
            resource: OtlpResource {
                attributes: vec![OtlpAttr {
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
                        aggregation_temporality: 2, // CUMULATIVE
                        is_monotonic: true,
                        data_points: [OtlpNumberPoint {
                            attributes: point_attrs,
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

impl<W: Write + Send + Sync> SluiceRecorder for SluiceToOtlpJsonWriter<W> {
    fn record_enqueue(&self, tenant: &TenantId, accepted: bool) {
        let label = if accepted { "true" } else { "false" };
        self.emit(tenant, "sluice.enqueue.count", 1, &[("accepted", label)]);
    }

    fn record_dequeue(&self, tenant: &TenantId) {
        self.emit(tenant, "sluice.dequeue.count", 1, &[]);
    }

    fn record_ack(&self, tenant: &TenantId) {
        self.emit(tenant, "sluice.ack.count", 1, &[]);
    }

    fn record_nack(&self, tenant: &TenantId) {
        self.emit(tenant, "sluice.nack.count", 1, &[]);
    }
}
