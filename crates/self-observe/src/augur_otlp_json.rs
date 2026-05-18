// Kaleidoscope self-observe — Augur → OTLP-JSON NDJSON writer
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

//! `AugurToOtlpJsonWriter` — emits each Augur
//! `MetricsRecorder` event as one or two lines of OTLP-JSON
//! metrics data on a generic `Write`.
//!
//! ## Why two writer shapes
//!
//! Augur is the first bridge that emits multiple OTLP metrics
//! per source event:
//!
//! - `record_observation(tenant)` → one line, `augur.observation.count`,
//!   Sum, `asInt=1`. Fixed-array shape like Lumen / Ray.
//! - `record_anomaly(tenant, score)` → two lines, both for the
//!   same resource and scope:
//!     - `augur.anomaly.count`, Sum, `asInt=1`
//!     - `augur.anomaly.score`, Gauge, `asDouble=<score>`
//!
//! The two-line emission for anomaly follows the same mental
//! model as the other writers ("one line per concrete OTLP
//! metric") rather than rolling both into a single `metrics`
//! array inside one ResourceMetrics envelope. Operators
//! correlate the pair by tenant + timestamp; a sidecar that
//! POSTs to a real collector can either forward both lines
//! independently or batch them. Both shapes are valid OTLP.
//!
//! ## The `asDouble` numeric point
//!
//! This is also the first writer that needs an OTLP-JSON
//! `asDouble` data-point variant (Augur's score is `f64`).
//! `asInt` is encoded as a string in OTLP-JSON because uint64
//! does not fit a JSON number safely; `asDouble` is a real JSON
//! number. The two number-point structs are parallel because
//! the JSON shape genuinely differs.

use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use augur::MetricsRecorder as AugurRecorder;
use serde::Serialize;

#[derive(Serialize)]
struct OtlpResourceMetrics<'a, P: Serialize> {
    resource: OtlpResource<'a>,
    #[serde(rename = "scopeMetrics")]
    scope_metrics: [OtlpScopeMetrics<'a, P>; 1],
}

#[derive(Serialize)]
struct OtlpResource<'a> {
    attributes: [OtlpAttr<'a>; 1],
}

#[derive(Serialize)]
struct OtlpScopeMetrics<'a, P: Serialize> {
    scope: OtlpScope<'a>,
    metrics: [OtlpMetric<'a, P>; 1],
}

#[derive(Serialize)]
struct OtlpScope<'a> {
    name: &'a str,
}

#[derive(Serialize)]
#[serde(untagged)]
enum OtlpMetric<'a, P: Serialize> {
    Sum {
        name: &'a str,
        sum: OtlpAggregation<'a, P>,
    },
    Gauge {
        name: &'a str,
        gauge: OtlpAggregation<'a, P>,
    },
}

#[derive(Serialize)]
struct OtlpAggregation<'a, P: Serialize> {
    #[serde(
        rename = "aggregationTemporality",
        skip_serializing_if = "Option::is_none"
    )]
    aggregation_temporality: Option<u8>,
    #[serde(rename = "isMonotonic", skip_serializing_if = "Option::is_none")]
    is_monotonic: Option<bool>,
    #[serde(rename = "dataPoints")]
    data_points: [P; 1],
    #[serde(skip)]
    _phantom: std::marker::PhantomData<&'a ()>,
}

#[derive(Serialize)]
struct OtlpIntPoint<'a> {
    attributes: [OtlpAttr<'a>; 1],
    #[serde(rename = "timeUnixNano")]
    time_unix_nano: String,
    #[serde(rename = "asInt")]
    as_int: String,
}

#[derive(Serialize)]
struct OtlpDoublePoint<'a> {
    attributes: [OtlpAttr<'a>; 1],
    #[serde(rename = "timeUnixNano")]
    time_unix_nano: String,
    #[serde(rename = "asDouble")]
    as_double: f64,
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

pub struct AugurToOtlpJsonWriter<W: Write + Send + Sync> {
    inner: Mutex<W>,
    scope_name: String,
}

impl<W: Write + Send + Sync> AugurToOtlpJsonWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner: Mutex::new(inner),
            scope_name: "kaleidoscope.augur".to_string(),
        }
    }

    fn write_line<S: Serialize>(&self, payload: &S) {
        if let Ok(line) = serde_json::to_string(payload) {
            if let Ok(mut writer) = self.inner.lock() {
                let _ = writer.write_all(line.as_bytes());
                let _ = writer.write_all(b"\n");
                let _ = writer.flush();
            }
        }
    }

    fn emit_sum_int(&self, tenant: &TenantId, metric_name: &str, value: u64) {
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
                metrics: [OtlpMetric::Sum {
                    name: metric_name,
                    sum: OtlpAggregation {
                        aggregation_temporality: Some(2),
                        is_monotonic: Some(true),
                        data_points: [OtlpIntPoint {
                            attributes: [OtlpAttr {
                                key: "tenant_id",
                                value: OtlpAttrValue {
                                    string_value: tenant.0.as_str(),
                                },
                            }],
                            time_unix_nano: time_str,
                            as_int: value_str,
                        }],
                        _phantom: std::marker::PhantomData,
                    },
                }],
            }],
        };
        self.write_line(&payload);
    }

    fn emit_gauge_double(&self, tenant: &TenantId, metric_name: &str, value: f64) {
        let now_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let time_str = now_ns.to_string();
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
                metrics: [OtlpMetric::Gauge {
                    name: metric_name,
                    gauge: OtlpAggregation {
                        aggregation_temporality: None,
                        is_monotonic: None,
                        data_points: [OtlpDoublePoint {
                            attributes: [OtlpAttr {
                                key: "tenant_id",
                                value: OtlpAttrValue {
                                    string_value: tenant.0.as_str(),
                                },
                            }],
                            time_unix_nano: time_str,
                            as_double: value,
                        }],
                        _phantom: std::marker::PhantomData,
                    },
                }],
            }],
        };
        self.write_line(&payload);
    }
}

impl<W: Write + Send + Sync> AugurRecorder for AugurToOtlpJsonWriter<W> {
    fn record_observation(&self, tenant: &TenantId) {
        self.emit_sum_int(tenant, "augur.observation.count", 1);
    }

    fn record_anomaly(&self, tenant: &TenantId, score: f64) {
        self.emit_sum_int(tenant, "augur.anomaly.count", 1);
        self.emit_gauge_double(tenant, "augur.anomaly.score", score);
    }
}
