//! `Framing` — the wire framing the caller asserts the bytes were received under.
//!
//! Per ADR-0001 the enum is `#[non_exhaustive]` so future framings (e.g.
//! `OtlpJson` if the OpenTelemetry spec ever stabilises a JSON framing for
//! transport, or new gRPC variants) ship in minor versions without breaking
//! consumer pattern-matching.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Framing {
    /// OTLP/HTTP/protobuf framing — the body is one serialised
    /// `ExportFooServiceRequest` protobuf message.
    HttpProtobuf,
    /// OTLP/gRPC framing — the body is a length-prefixed protobuf message.
    /// In v0 the harness validates the message bytes themselves; the gRPC
    /// length prefix is the caller's responsibility to strip before invoking
    /// the harness.
    GrpcProtobuf,
}
