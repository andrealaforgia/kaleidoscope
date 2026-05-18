// Kaleidoscope self-observe — Strata → OTLP-JSON NDJSON writer
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

//! `StrataToOtlpJsonWriter` — emits each Strata
//! `MetricsRecorder` event as one line of OTLP-JSON metrics
//! data on a generic `Write`. Fixed-array shape, third
//! instance of that family after Lumen and Ray. With this
//! commit the rule-of-three threshold is reached on the
//! fixed-array side; a refactor lifting the shared
//! serialization structs into one module is the very next
//! commit.

use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use serde::Serialize;
use strata::MetricsRecorder as StrataRecorder;

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

pub struct StrataToOtlpJsonWriter<W: Write + Send + Sync> {
    inner: Mutex<W>,
    scope_name: String,
}

impl<W: Write + Send + Sync> StrataToOtlpJsonWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner: Mutex::new(inner),
            scope_name: "kaleidoscope.strata".to_string(),
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

impl<W: Write + Send + Sync> StrataRecorder for StrataToOtlpJsonWriter<W> {
    fn record_ingest(&self, tenant: &TenantId, profile_count: usize) {
        self.emit(tenant, "strata.ingest.count", profile_count as u64);
    }

    fn record_query(&self, tenant: &TenantId, matched_count: usize) {
        self.emit(tenant, "strata.query.count", matched_count as u64);
    }
}
