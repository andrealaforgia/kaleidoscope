// Kaleidoscope self-observe — shared OTLP-JSON fixed-array writer
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

//! Shared OTLP-JSON serialization for the fixed-array writer
//! family.
//!
//! ## What is shared
//!
//! Three writers — `LumenToOtlpJsonWriter`,
//! `RayToOtlpJsonWriter`, `StrataToOtlpJsonWriter` — produce
//! NDJSON lines whose `ResourceMetrics` envelopes carry
//! exactly one point attribute (`tenant_id`), one Sum metric
//! kind, and an integer value encoded as `asInt`. The
//! per-event differences are only:
//!
//! - The scope name (`kaleidoscope.lumen` / `.ray` / `.strata`)
//! - The metric name (`<domain>.<event>.count`)
//! - The integer value
//! - The tenant id
//!
//! Before this module existed each writer carried its own copy
//! of the OTLP-JSON serialization structs and a near-identical
//! `emit` method. With three instances of the shape the
//! rule-of-three threshold for extraction was reached. This
//! module is the extraction.
//!
//! ## What stays per-writer
//!
//! Each domain writer keeps its own struct (so the type system
//! tracks which crate's `MetricsRecorder` trait it implements)
//! and forwards every `record_*` call to [`emit_fixed_sum_int`].

use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
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

/// Build and append a single NDJSON OTLP-JSON line carrying a
/// `Sum` metric with an integer value, one `tenant_id` point
/// attribute, and the supplied scope + metric name. The line
/// is timestamped at call time using `SystemTime::now()`.
///
/// Errors during serialization or write are swallowed: this is
/// a best-effort observability sink. The on-disk side effect is
/// "either a complete line gets appended, or nothing does".
pub fn emit_fixed_sum_int<W: Write + Send + Sync>(
    writer: &Mutex<W>,
    scope_name: &str,
    tenant: &TenantId,
    metric_name: &str,
    value: u64,
) {
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
            scope: OtlpScope { name: scope_name },
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
        if let Ok(mut writer) = writer.lock() {
            let _ = writer.write_all(line.as_bytes());
            let _ = writer.write_all(b"\n");
            let _ = writer.flush();
        }
    }
}
