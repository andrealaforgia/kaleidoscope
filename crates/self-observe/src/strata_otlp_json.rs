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
//! data on a generic `Write`. After the rule-of-three refactor,
//! the serialization plumbing lives in
//! [`crate::otlp_json_fixed`]; this module keeps only the
//! per-domain shape.

use std::io::Write;
use std::sync::Mutex;

use aegis::TenantId;
use strata::MetricsRecorder as StrataRecorder;

use crate::otlp_json_fixed::emit_fixed_sum_int;

const SCOPE_NAME: &str = "kaleidoscope.strata";

pub struct StrataToOtlpJsonWriter<W: Write + Send + Sync> {
    inner: Mutex<W>,
}

impl<W: Write + Send + Sync> StrataToOtlpJsonWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner: Mutex::new(inner),
        }
    }
}

impl<W: Write + Send + Sync> StrataRecorder for StrataToOtlpJsonWriter<W> {
    fn record_ingest(&self, tenant: &TenantId, profile_count: usize) {
        emit_fixed_sum_int(
            &self.inner,
            SCOPE_NAME,
            tenant,
            "strata.ingest.count",
            profile_count as u64,
        );
    }

    fn record_query(&self, tenant: &TenantId, matched_count: usize) {
        emit_fixed_sum_int(
            &self.inner,
            SCOPE_NAME,
            tenant,
            "strata.query.count",
            matched_count as u64,
        );
    }
}
