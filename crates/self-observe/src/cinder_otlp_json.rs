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

//! `CinderToOtlpJsonWriter` — emits each Cinder `MetricsRecorder`
//! event as one line of OTLP-JSON metrics data on a generic
//! `Write`.
//!
//! Sibling of `lumen_otlp_json.rs` (the Lumen-side OTLP-JSON
//! writer) and `cinder_bridge.rs` (the in-process Pulse sink for
//! the same three Cinder events). Public surface locked by
//! ADR-0039 §1; per-event emission contract locked by ADR-0039 §2.
//!
//! ## Shape
//!
//! Each call produces a single NDJSON line. The line is one
//! `ResourceMetrics`-rooted message in the OTLP-JSON encoding
//! defined by the OpenTelemetry specification. A sidecar process
//! that consumes the stream can wrap it in a `MetricsData`
//! envelope and POST it to any real OTLP/HTTP collector.
//!
//! ## Per-event contract (ADR-0039 §2)
//!
//! - `record_place(tenant, tier)`     → `cinder.place.count`,
//!   `asInt="1"`, point attrs `[tenant_id, tier]`
//! - `record_migrate(tenant, f, t)`   → `cinder.migrate.count`,
//!   `asInt="1"`, point attrs `[tenant_id, from, to]`
//! - `record_evaluate(tenant, n)`     → `cinder.evaluate.migrated.count`,
//!   `asInt=n.to_string()`, point attrs `[tenant_id]`
//!
//! ## Atomicity (DISCUSS D6)
//!
//! Writer holds a `Mutex<W>`. Each emission acquires the guard
//! once and performs the triple `write_all(body) +
//! write_all(b"\n") + flush` inside the critical section. This is
//! the NDJSON-validity defence against concurrent emissions
//! interleaving.
//!
//! ## Best-effort posture (DISCUSS D5)
//!
//! Serialisation failure, write failure, and `Mutex<W>` poisoning
//! are all silently swallowed. Same posture as
//! `lumen_otlp_json.rs:182-189`.
//!
//! ## Duplication
//!
//! The OTLP-JSON envelope serde structs are duplicated from
//! `lumen_otlp_json.rs` per DISCUSS D7 (rule of three not yet
//! reached). The single structural divergence from the Lumen
//! envelope is `OtlpNumberPoint.attributes`: `Vec<OtlpAttr<'a>>`
//! here vs `[OtlpAttr<'a>; 1]` there, because Cinder's per-event
//! attribute cardinality is non-uniform (place: 2, migrate: 3,
//! evaluate: 1). See ADR-0039 §5 DD2.

use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis::TenantId;
use cinder::{MetricsRecorder as CinderRecorder, Tier};
use serde::Serialize;

// --------------------------------------------------------------------
// OTLP-JSON shape — hand-rolled minimal subset.
// Duplicated from `lumen_otlp_json.rs` per DISCUSS D7. The single
// divergence is `OtlpNumberPoint.attributes` — see module doc.
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
    // DIVERGES from Lumen: Vec<OtlpAttr> (not [OtlpAttr; 1]) per
    // ADR-0039 §5 DD2. Cinder per-event point-attribute cardinality
    // is non-uniform (place: 2, migrate: 3, evaluate: 1).
    attributes: Vec<OtlpAttr<'a>>,
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
// Tier → wire-format string (DISCUSS D3). Single source of truth for
// the lowercase ASCII serialisation; both the `tier` attribute on
// place points and the `from`/`to` attributes on migrate points must
// agree. Centralising the mapping here makes that impossible to break
// by accident (mirror of `cinder_bridge.rs:109-115`).
// --------------------------------------------------------------------

fn tier_lowercase(tier: Tier) -> &'static str {
    match tier {
        Tier::Hot => "hot",
        Tier::Warm => "warm",
        Tier::Cold => "cold",
    }
}

// --------------------------------------------------------------------
// Writer
// --------------------------------------------------------------------

/// Bridge: implements `cinder::MetricsRecorder`, writes one OTLP-
/// JSON `ResourceMetrics` line per event to the inner writer.
///
/// Public surface locked by ADR-0039 §1.
pub struct CinderToOtlpJsonWriter<W: Write + Send + Sync> {
    inner: Mutex<W>,
    scope_name: String,
}

impl<W: Write + Send + Sync> CinderToOtlpJsonWriter<W> {
    /// Construct a writer wrapping the inner sink.
    pub fn new(inner: W) -> Self {
        Self {
            inner: Mutex::new(inner),
            scope_name: "kaleidoscope.cinder".to_string(),
        }
    }

    /// Emit one OTLP-JSON ResourceMetrics line. The `value` is the
    /// already-stringified `asInt` payload (because `record_evaluate`
    /// passes `migrated.to_string()`, not `"1"`). The `point_attrs`
    /// vector is the per-event-shaped point-attribute set.
    fn emit(
        &self,
        tenant: &TenantId,
        metric_name: &str,
        value: &str,
        point_attrs: Vec<OtlpAttr<'_>>,
    ) {
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
                metrics: [OtlpMetric {
                    name: metric_name,
                    sum: OtlpSum {
                        // 2 = AGGREGATION_TEMPORALITY_CUMULATIVE
                        aggregation_temporality: 2,
                        is_monotonic: true,
                        data_points: [OtlpNumberPoint {
                            attributes: point_attrs,
                            time_unix_nano: time_str,
                            as_int: value.to_string(),
                        }],
                    },
                }],
            }],
        };
        if let Ok(mut line) = serde_json::to_string(&payload) {
            // ADR-0039 §8 cross-writer atomicity: combine body + `\n`
            // into one buffer so the inner `write_all` issues a single
            // `write(2)` syscall. Under POSIX O_APPEND, a single
            // `write(2)` smaller than PIPE_BUF (4096) is atomic with
            // respect to other appenders sharing the same file
            // description. Without this, three separate writes (body,
            // newline, flush) can interleave across writers on macOS
            // and produce empty lines or torn records.
            line.push('\n');
            if let Ok(mut writer) = self.inner.lock() {
                let _ = writer.write_all(line.as_bytes());
                let _ = writer.flush();
            }
        }
    }
}

impl<W: Write + Send + Sync> CinderRecorder for CinderToOtlpJsonWriter<W> {
    fn record_place(&self, tenant: &TenantId, tier: Tier) {
        let attrs = vec![
            OtlpAttr {
                key: "tenant_id",
                value: OtlpAttrValue {
                    string_value: tenant.0.as_str(),
                },
            },
            OtlpAttr {
                key: "tier",
                value: OtlpAttrValue {
                    string_value: tier_lowercase(tier),
                },
            },
        ];
        self.emit(tenant, "cinder.place.count", "1", attrs);
    }

    fn record_migrate(&self, tenant: &TenantId, from: Tier, to: Tier) {
        let attrs = vec![
            OtlpAttr {
                key: "tenant_id",
                value: OtlpAttrValue {
                    string_value: tenant.0.as_str(),
                },
            },
            OtlpAttr {
                key: "from",
                value: OtlpAttrValue {
                    string_value: tier_lowercase(from),
                },
            },
            OtlpAttr {
                key: "to",
                value: OtlpAttrValue {
                    string_value: tier_lowercase(to),
                },
            },
        ];
        self.emit(tenant, "cinder.migrate.count", "1", attrs);
    }

    fn record_evaluate(&self, tenant: &TenantId, migrated: usize) {
        let value = migrated.to_string();
        let attrs = vec![OtlpAttr {
            key: "tenant_id",
            value: OtlpAttrValue {
                string_value: tenant.0.as_str(),
            },
        }];
        self.emit(tenant, "cinder.evaluate.migrated.count", &value, attrs);
    }
}
