// Kaleidoscope self-observe — Ray → OTLP-JSON NDJSON writer
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

//! `RayToOtlpJsonWriter` — emits each Ray `MetricsRecorder`
//! event as one line of OTLP-JSON metrics data on a generic
//! `Write`.
//!
//! ## Shape choice
//!
//! Ray's events carry only `tenant_id` as a point attribute
//! (no tier topology like Cinder, no `accepted` flag like
//! Sluice). The fixed-size `[OtlpAttr; 1]` shape from the
//! Lumen writer applies cleanly, so this writer is a near-twin
//! of `LumenToOtlpJsonWriter` with different metric names and
//! a `kaleidoscope.ray` scope.
//!
//! ## Pending abstraction
//!
//! With this commit there are two writers with the fixed-array
//! shape (Lumen, Ray) and two with the variable Vec shape
//! (Cinder, Sluice). The previous narrative entry promised
//! that a third Vec instance would trigger an extraction.
//! Ray instead lands as the second fixed-array instance, so
//! the rule-of-three threshold is not yet reached on either
//! side. When the next Augur or Strata bridge lands, that may
//! be the moment.

use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use ray::MetricsRecorder as RayRecorder;
use serde::Serialize;

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

pub struct RayToOtlpJsonWriter<W: Write + Send + Sync> {
    inner: Mutex<W>,
    scope_name: String,
}

impl<W: Write + Send + Sync> RayToOtlpJsonWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner: Mutex::new(inner),
            scope_name: "kaleidoscope.ray".to_string(),
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

impl<W: Write + Send + Sync> RayRecorder for RayToOtlpJsonWriter<W> {
    fn record_ingest(&self, tenant: &TenantId, span_count: usize) {
        self.emit(tenant, "ray.ingest.count", span_count as u64);
    }

    fn record_query(&self, tenant: &TenantId, matched_count: usize) {
        self.emit(tenant, "ray.query.count", matched_count as u64);
    }
}
