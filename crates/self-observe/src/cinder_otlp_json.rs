// Kaleidoscope self-observe — Cinder → OTLP-JSON NDJSON writer
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

//! `CinderToOtlpJsonWriter` — emits each Cinder
//! `MetricsRecorder` event as one line of OTLP-JSON metrics
//! data on a generic `Write`. Mirrors
//! [`crate::LumenToOtlpJsonWriter`] in shape and locking.
//!
//! ## Why a separate writer rather than a shared one
//!
//! Lumen events carry one point attribute (`tenant_id`). Cinder
//! events carry up to three (`tenant_id`, `tier`, plus `from` /
//! `to` for migrate). The Lumen writer uses fixed-size arrays
//! for zero-allocation hot-path emission; Cinder needs the
//! flexibility of `Vec<OtlpAttr>`. Rather than retrofit a `Vec`
//! into the Lumen writer and risk perturbing its passing tests,
//! this module carries a parallel set of serialization structs.
//! Three similar lines beat a premature abstraction.

use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{MetricsRecorder as CinderRecorder, Tier};
use serde::Serialize;

// --------------------------------------------------------------------
// OTLP-JSON shape — variable-attribute version.
// --------------------------------------------------------------------

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

// --------------------------------------------------------------------
// Writer
// --------------------------------------------------------------------

pub struct CinderToOtlpJsonWriter<W: Write + Send + Sync> {
    inner: Mutex<W>,
    scope_name: String,
}

impl<W: Write + Send + Sync> CinderToOtlpJsonWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner: Mutex::new(inner),
            scope_name: "kaleidoscope.cinder".to_string(),
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

fn tier_label(tier: Tier) -> &'static str {
    match tier {
        Tier::Hot => "hot",
        Tier::Warm => "warm",
        Tier::Cold => "cold",
    }
}

impl<W: Write + Send + Sync> CinderRecorder for CinderToOtlpJsonWriter<W> {
    fn record_place(&self, tenant: &TenantId, tier: Tier) {
        self.emit(
            tenant,
            "cinder.place.count",
            1,
            &[("tier", tier_label(tier))],
        );
    }

    fn record_migrate(&self, tenant: &TenantId, from: Tier, to: Tier) {
        self.emit(
            tenant,
            "cinder.migrate.count",
            1,
            &[("from", tier_label(from)), ("to", tier_label(to))],
        );
    }

    fn record_evaluate(&self, tenant: &TenantId, migrated: usize) {
        self.emit(
            tenant,
            "cinder.evaluate.migrated.count",
            migrated as u64,
            &[],
        );
    }
}
