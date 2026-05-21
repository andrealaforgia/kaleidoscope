// Kaleidoscope Ray — OTLP-shaped span types
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

//! OTLP-shaped span types at the trait boundary.
//!
//! Field set mirrors `opentelemetry-proto::trace::v1::Span`
//! exactly. The v1 columnar adapter round-trips every field
//! byte-stable.

use std::collections::BTreeMap;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Hand-rolled lowercase-hex encode/decode for the fixed-width
/// byte-array identifiers. Kept in-crate (no `hex` / `serde_with`
/// crate) to match the project's hand-rolled-over-dependency
/// posture (cf. the hand-rolled ISO 8601 in `kaleidoscope-cli`).
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push(char::from_digit((b >> 4) as u32, 16).unwrap());
            s.push(char::from_digit((b & 0x0f) as u32, 16).unwrap());
        }
        s
    }

    /// Decodes lowercase or uppercase hex into exactly `N` bytes.
    /// Errors on wrong length or a non-hex character.
    pub fn decode<const N: usize>(s: &str) -> Result<[u8; N], String> {
        if s.len() != N * 2 {
            return Err(format!("expected {} hex chars, got {}", N * 2, s.len()));
        }
        let mut out = [0u8; N];
        let bytes = s.as_bytes();
        for (i, slot) in out.iter_mut().enumerate() {
            let hi = (bytes[i * 2] as char)
                .to_digit(16)
                .ok_or_else(|| format!("non-hex char at position {}", i * 2))?;
            let lo = (bytes[i * 2 + 1] as char)
                .to_digit(16)
                .ok_or_else(|| format!("non-hex char at position {}", i * 2 + 1))?;
            *slot = ((hi << 4) | lo) as u8;
        }
        Ok(out)
    }
}

/// W3C trace context — 128 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TraceId(pub [u8; 16]);

impl Serialize for TraceId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for TraceId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = TraceId;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a 32-character hex string")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<TraceId, E> {
                hex::decode::<16>(v).map(TraceId).map_err(E::custom)
            }
        }
        d.deserialize_str(V)
    }
}

/// W3C span context — 64 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpanId(pub [u8; 8]);

impl Serialize for SpanId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for SpanId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = SpanId;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a 16-character hex string")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<SpanId, E> {
                hex::decode::<8>(v).map(SpanId).map_err(E::custom)
            }
        }
        d.deserialize_str(V)
    }
}

/// Service name. Stable key for the `(tenant, service)` index.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ServiceName(pub String);

impl ServiceName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// OTLP span kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpanKind {
    Unspecified,
    Internal,
    Server,
    Client,
    Producer,
    Consumer,
}

/// OTLP span status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StatusCode {
    Unset,
    Ok,
    Error,
}

/// OTLP span status (code + description).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpanStatus {
    pub code: StatusCode,
    pub message: String,
}

impl Default for SpanStatus {
    fn default() -> Self {
        Self {
            code: StatusCode::Unset,
            message: String::new(),
        }
    }
}

/// One event recorded inside a span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpanEvent {
    pub time_unix_nano: u64,
    pub name: String,
    pub attributes: BTreeMap<String, String>,
}

/// Link to another span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpanLink {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub attributes: BTreeMap<String, String>,
}

/// One OTLP span. Field set mirrors
/// `opentelemetry-proto::trace::v1::Span` plus the carrying
/// resource attributes (hoisted into v1's batch level later).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    /// `None` for trace roots.
    pub parent_span_id: Option<SpanId>,
    pub name: String,
    pub kind: SpanKind,
    pub start_time_unix_nano: u64,
    pub end_time_unix_nano: u64,
    pub status: SpanStatus,
    /// Span-level attributes (e.g. `http.route`,
    /// `db.statement`).
    pub attributes: BTreeMap<String, String>,
    /// Resource attributes (e.g. `service.name`,
    /// `service.version`).
    pub resource_attributes: BTreeMap<String, String>,
    pub events: Vec<SpanEvent>,
    pub links: Vec<SpanLink>,
}

impl Span {
    /// Convenience accessor — pulls `service.name` from the
    /// resource attributes. Returns an empty string if missing
    /// (matches OTel collector behaviour).
    pub fn service_name(&self) -> &str {
        self.resource_attributes
            .get("service.name")
            .map(String::as_str)
            .unwrap_or("")
    }
}

/// A batch of spans, all belonging to one tenant.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpanBatch {
    pub spans: Vec<Span>,
}

impl SpanBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_spans(spans: Vec<Span>) -> Self {
        Self { spans }
    }

    pub fn push(&mut self, span: Span) {
        self.spans.push(span);
    }

    pub fn len(&self) -> usize {
        self.spans.len()
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }
}

/// Half-open time range `[start, end)` in nanoseconds since
/// the Unix epoch. A span matches when
/// `start <= start_time_unix_nano < end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRange {
    pub start_unix_nano: u64,
    pub end_unix_nano: u64,
}

impl TimeRange {
    pub fn new(start_unix_nano: u64, end_unix_nano: u64) -> Self {
        Self {
            start_unix_nano,
            end_unix_nano,
        }
    }

    pub fn all() -> Self {
        Self::new(0, u64::MAX)
    }

    pub fn contains(&self, t: u64) -> bool {
        t >= self.start_unix_nano && t < self.end_unix_nano
    }
}
