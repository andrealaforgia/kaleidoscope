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
//! `Write`. After the rule-of-three refactor that landed
//! alongside the Strata bridge, the serialization plumbing now
//! lives in [`crate::otlp_json_fixed`]; this module keeps only
//! the per-domain shape: trait impl, scope name, metric names.

use std::io::Write;
use std::sync::Mutex;

use aegis::TenantId;
use lumen::MetricsRecorder as LumenRecorder;

use crate::otlp_json_fixed::emit_fixed_sum_int;

const SCOPE_NAME: &str = "kaleidoscope.lumen";

/// Bridge: implements `lumen::MetricsRecorder`, writes one OTLP-
/// JSON `ResourceMetrics` line per event to the inner writer.
pub struct LumenToOtlpJsonWriter<W: Write + Send + Sync> {
    inner: Mutex<W>,
}

impl<W: Write + Send + Sync> LumenToOtlpJsonWriter<W> {
    /// Construct a writer wrapping the inner sink.
    pub fn new(inner: W) -> Self {
        Self {
            inner: Mutex::new(inner),
        }
    }
}

impl<W: Write + Send + Sync> LumenRecorder for LumenToOtlpJsonWriter<W> {
    fn record_ingest(&self, tenant: &TenantId, record_count: usize) {
        emit_fixed_sum_int(
            &self.inner,
            SCOPE_NAME,
            tenant,
            "lumen.ingest.count",
            record_count as u64,
        );
    }

    fn record_query(&self, tenant: &TenantId, matched_count: usize) {
        emit_fixed_sum_int(
            &self.inner,
            SCOPE_NAME,
            tenant,
            "lumen.query.count",
            matched_count as u64,
        );
    }
}
